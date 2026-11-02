//! The Go import graph of a repository (ADR-0056): packages as nodes, import
//! specs as edges. Each `go.mod` is one module and a directory belongs to its
//! nearest `go.mod`; a package is a directory holding non-test `.go` files
//! (generated ones included, `//go:build ignore` ones left out). An import
//! path under the module path of the importing module is an edge to that
//! package directory; every other path, another module of the repository
//! included, is external, kept for `only_in`. The compiler refuses import
//! cycles, so there is no cycle check.

mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use adr_lint::config::FILE;

use crate::check::Finding;
use crate::coverage::Failure;

use super::files::listed;
use super::{finding, Request, Scope, Sources, DEPENDENCY_RULE, INTERFACE_RULE};
use scan::Import;

/// A `go.mod`: the directory it sits in (`""` for the root) and the path on
/// its `module` line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Module {
    directory: String,
    path: String,
}

/// What an edge leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// The directory of a package of the same module.
    Package(String),
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    file: String,
    /// The directory of the importing package.
    from: String,
    target: Target,
    offset: usize,
    written: String,
}

/// The package files and the modules they belong to.
#[derive(Debug, Default)]
struct Packages {
    modules: Vec<Module>,
    /// File to the index in `modules` of its module.
    files: BTreeMap<String, usize>,
}

fn directory_of(file: &str) -> &str {
    file.rsplit_once('/').map_or("", |(directory, _)| directory)
}

/// The package name a directory is known by in findings.
fn package_key(directory: &str) -> &str {
    if directory.is_empty() {
        "."
    } else {
        directory
    }
}

/// The path on the `module` line of a `go.mod`.
fn module_path(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("module")?;
        if !rest.starts_with([' ', '\t', '"', '`']) {
            return None;
        }
        let path = rest.split("//").next().unwrap_or("").trim();
        let path = path.trim_matches(['"', '`']);
        (!path.is_empty()).then(|| path.to_string())
    })
}

/// Whether the `//go:build` constraint before the package clause is exactly
/// `ignore`.
fn is_build_ignored(text: &str) -> bool {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("package ") || line == "package" {
            return false;
        }
        if let Some(constraint) = line.strip_prefix("//go:build") {
            if constraint.starts_with([' ', '\t']) && constraint.trim() == "ignore" {
                return true;
            }
        }
    }
    false
}

fn is_package_file(path: &str) -> bool {
    let mut segments = path.split('/');
    let name = segments.next_back().unwrap_or("");
    name.ends_with(".go")
        && !name.ends_with("_test.go")
        && !name.starts_with(['_', '.'])
        && !segments.any(|segment| segment == "vendor")
}

/// The modules and package files of the repository under `root`, plus the
/// `named` files that exist.
fn discover(root: &Path, named: &[String]) -> Result<Packages, Failure> {
    let mut all: BTreeSet<String> = listed(root)?.into_iter().collect();
    all.extend(named.iter().map(|path| super::clean(path)));
    let existing: Vec<String> = all
        .into_iter()
        .filter(|path| root.join(path).is_file())
        .collect();
    let mut packages = Packages::default();
    for path in existing
        .iter()
        .filter(|path| path.as_str() == "go.mod" || path.ends_with("/go.mod"))
    {
        let text = std::fs::read_to_string(root.join(path))
            .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))?;
        let Some(module) = module_path(&text) else {
            return Err(Failure::CouldNotRun(format!(
                "{path}: no parsable `module` line"
            )));
        };
        packages.modules.push(Module {
            directory: directory_of(path).to_string(),
            path: module,
        });
    }
    for path in existing.iter().filter(|path| is_package_file(path)) {
        let directory = directory_of(path);
        let owner = packages
            .modules
            .iter()
            .enumerate()
            .filter(|(_, module)| {
                module.directory.is_empty()
                    || directory == module.directory
                    || directory.starts_with(&format!("{}/", module.directory))
            })
            .max_by_key(|(_, module)| module.directory.len())
            .map(|(index, _)| index);
        let Some(owner) = owner else {
            continue;
        };
        let text = std::fs::read_to_string(root.join(path))
            .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))?;
        if !is_build_ignored(&text) {
            packages.files.insert(path.clone(), owner);
        }
    }
    Ok(packages)
}

fn build(packages: &Packages, imports: &[Import]) -> Vec<Edge> {
    let directories: BTreeMap<&str, usize> = packages
        .files
        .iter()
        .map(|(file, module)| (directory_of(file), *module))
        .collect();
    let mut edges = Vec::new();
    for import in imports {
        let Some(&owner) = packages.files.get(&import.file) else {
            continue;
        };
        let module = &packages.modules[owner];
        let from = directory_of(&import.file);
        let inside = if import.path == module.path {
            Some("")
        } else {
            import
                .path
                .strip_prefix(&format!("{}/", module.path))
                .filter(|rest| !rest.is_empty())
        };
        let target = match inside {
            Some(rest) => {
                let directory = match (module.directory.is_empty(), rest.is_empty()) {
                    (true, _) => rest.to_string(),
                    (false, true) => module.directory.clone(),
                    (false, false) => format!("{}/{rest}", module.directory),
                };
                match directories.get(directory.as_str()) {
                    Some(&found) if found == owner => Target::Package(directory),
                    Some(_) => Target::External(import.path.clone()),
                    None => continue,
                }
            }
            None => Target::External(import.path.clone()),
        };
        if target == Target::Package(from.to_string()) {
            continue;
        }
        edges.push(Edge {
            file: import.file.clone(),
            from: from.to_string(),
            target,
            offset: import.offset,
            written: import.path.clone(),
        });
    }
    edges
}

/// The contract findings of the Go graph: layers and `only_in`.
pub(super) fn findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let selected = |rule: &str| request.rule.is_none_or(|wanted| wanted == rule);
    let layers =
        selected(DEPENDENCY_RULE) && contract.layers.iter().any(|layer| !layer.paths.is_empty());
    let packages = selected(INTERFACE_RULE)
        && contract
            .only_in
            .iter()
            .any(|list| !(list.imports.is_empty() || list.paths.is_empty()));
    if !(layers || packages) {
        return Ok(Vec::new());
    }
    let root = Path::new(".");
    let named: Vec<String> = request
        .checked
        .iter()
        .chain(request.checked_generated)
        .cloned()
        .collect();
    let discovered = discover(root, &named)?;
    if discovered.files.is_empty() {
        return Ok(Vec::new());
    }
    let files: Vec<String> = discovered.files.keys().cloned().collect();
    let imports = scan::scan(request.ast_grep, root, &files)?;
    let edges = build(&discovered, &imports);
    let checked: BTreeSet<String> = named.iter().map(|path| super::clean(path)).collect();
    let wanted = |file: &str| request.go_scope == Scope::Everything || checked.contains(file);
    let mut sources = Sources::default();
    let mut findings = Vec::new();
    if layers {
        findings.extend(layer_findings(request, &edges, &wanted, &mut sources)?);
    }
    if packages {
        findings.extend(package_findings(request, &edges, &wanted, &mut sources)?);
    }
    Ok(findings)
}

/// `architecture.dependency-rule` findings: a package importing a package of
/// a later layer.
fn layer_findings(
    request: &Request,
    edges: &[Edge],
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let layer_of = |directory: &str| {
        contract
            .layer_index_of_path(package_key(directory))
            .map_err(|cause| Failure::CouldNotRun(cause.to_string()))
    };
    let mut findings = Vec::new();
    for edge in edges.iter().filter(|edge| wanted(&edge.file)) {
        let Target::Package(to) = &edge.target else {
            continue;
        };
        let (Some(from_index), Some(to_index)) = (layer_of(&edge.from)?, layer_of(to)?) else {
            continue;
        };
        if to_index <= from_index {
            continue;
        }
        let (from_layer, to_layer) = (&contract.layers[from_index], &contract.layers[to_index]);
        let detail = format!(
            "`{}` (layer `{}`) imports `{}` from `{}` (layer `{}`); a layer may \
             import only itself and the layers before it, and `{}` is declared \
             after `{}` at {FILE}:{}",
            package_key(&edge.from),
            from_layer.name,
            edge.written,
            package_key(to),
            to_layer.name,
            to_layer.name,
            from_layer.name,
            to_layer.line,
        );
        findings.push(finding(
            DEPENDENCY_RULE,
            &edge.file,
            edge.offset,
            detail,
            sources,
        )?);
    }
    Ok(findings)
}

/// `boundaries.own-the-interface` findings: a listed import path imported
/// from a package outside the entry's `paths`.
fn package_findings(
    request: &Request,
    edges: &[Edge],
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let mut findings = Vec::new();
    for edge in edges.iter().filter(|edge| wanted(&edge.file)) {
        let Target::External(path) = &edge.target else {
            continue;
        };
        for list in &request.contract.only_in {
            if list.paths.is_empty()
                || !list.covers_package(path)
                || list.allows_path(package_key(&edge.from))
            {
                continue;
            }
            let allowed = list
                .paths
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let detail = format!(
                "`{path}` is imported in `{}`, outside {allowed} where the \
                 `only_in` entry at {FILE}:{} allows it",
                package_key(&edge.from),
                list.line,
            );
            findings.push(finding(
                INTERFACE_RULE,
                &edge.file,
                edge.offset,
                detail,
                sources,
            )?);
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_line_gives_the_path() {
        assert_eq!(
            module_path("// c\nmodule example.com/app // x\n\ngo 1.22\n").as_deref(),
            Some("example.com/app")
        );
        assert_eq!(
            module_path("module \"example.com/app\"\n").as_deref(),
            Some("example.com/app")
        );
        assert_eq!(module_path("modulex y\n"), None);
    }

    #[test]
    fn only_a_constraint_of_exactly_ignore_leaves_a_file_out() {
        assert!(is_build_ignored("//go:build ignore\n\npackage main\n"));
        assert!(!is_build_ignored("//go:build linux\n\npackage main\n"));
        assert!(!is_build_ignored(
            "//go:build ignore && linux\npackage main\n"
        ));
        assert!(!is_build_ignored("package main\n//go:build ignore\n"));
    }

    #[test]
    fn test_vendor_and_underscore_files_are_not_package_files() {
        assert!(is_package_file("a/b.go"));
        assert!(is_package_file("a/zz_generated.deepcopy.go"));
        assert!(!is_package_file("a/b_test.go"));
        assert!(!is_package_file("vendor/x/b.go"));
        assert!(!is_package_file("a/_b.go"));
    }
}
