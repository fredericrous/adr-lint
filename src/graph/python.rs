//! The Python import graph of a repository (ADR-0056): modules as nodes,
//! `import` and `from … import` statements as edges, resolved against the
//! modules under the uv workspace members' roots. Cycles are reported among
//! sibling modules at every package depth, each sibling package folded with
//! its descendants, as the Rust graph does. A literal
//! `importlib.import_module("x")` is an edge that cycles do not read.
//!
//! An import of `a.b.c` makes no edge to `a` or `a.b`, and a re-export in an
//! `__init__.py` is not followed. A name whose top-level package is outside
//! the roots is an external edge, kept for `only_in`.

mod roots;
mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use adr_lint::config::FILE;

use crate::check::Finding;
use crate::coverage::Failure;

use super::cycles::{self, Edges};
use super::{cycle_detail, finding, Request, Sources, CYCLE_RULE, DEPENDENCY_RULE, INTERFACE_RULE};
use roots::Module;
use scan::{Form, Import};

/// What an edge leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Module(String),
    /// The dotted name of a package outside the roots, kept for `only_in`.
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    file: String,
    from: String,
    target: Target,
    form: Form,
    offset: usize,
    written: String,
}

/// The dotted name `import` names, relative imports resolved against `own`.
fn full_name(own: &Module, import: &Import) -> Option<String> {
    let mut base: Vec<&str> = Vec::new();
    if import.level > 0 {
        base = own.name.split('.').collect();
        if !own.package {
            base.pop();
        }
        for _ in 1..import.level {
            base.pop()?;
        }
        if base.is_empty() {
            return None;
        }
    }
    let mut full = base.join(".");
    if !import.module.is_empty() {
        if !full.is_empty() {
            full.push('.');
        }
        full.push_str(&import.module);
    }
    (!full.is_empty()).then_some(full)
}

/// What `import` leads to: one of `modules`, or a package outside the roots.
fn target(modules: &BTreeMap<&str, &Module>, own: &Module, import: &Import) -> Option<Target> {
    let full = full_name(own, import)?;
    if let Some(name) = &import.name {
        let submodule = format!("{full}.{name}");
        if modules.contains_key(submodule.as_str()) {
            return Some(Target::Module(submodule));
        }
    }
    if modules.contains_key(full.as_str()) {
        return Some(Target::Module(full));
    }
    let top_level = full.split('.').next().unwrap_or("");
    (import.level == 0 && !modules.contains_key(top_level)).then_some(Target::External(full))
}

fn build(modules: &[Module], imports: &[Import]) -> Vec<Edge> {
    let by_name: BTreeMap<&str, &Module> = modules
        .iter()
        .map(|module| (module.name.as_str(), module))
        .collect();
    let by_file: BTreeMap<&str, &Module> = modules
        .iter()
        .map(|module| (module.file.as_str(), module))
        .collect();
    let mut edges = Vec::new();
    for import in imports {
        let Some(own) = by_file.get(import.file.as_str()) else {
            continue;
        };
        let Some(target) = target(&by_name, own, import) else {
            continue;
        };
        if target == Target::Module(own.name.clone()) {
            continue;
        }
        edges.push(Edge {
            file: import.file.clone(),
            from: own.name.clone(),
            target,
            form: import.form,
            offset: import.offset,
            written: import.written(),
        });
    }
    edges
}

/// The edges that count for cycles, between the subtrees of two siblings,
/// grouped by their parent (`""` for the top-level packages).
fn cycle_groups(edges: &[Edge]) -> BTreeMap<String, Edges> {
    let mut groups: BTreeMap<String, Edges> = BTreeMap::new();
    for edge in edges.iter().filter(|edge| edge.form == Form::Static) {
        let Target::Module(target) = &edge.target else {
            continue;
        };
        let from: Vec<&str> = edge.from.split('.').collect();
        let to: Vec<&str> = target.split('.').collect();
        let common = from
            .iter()
            .zip(&to)
            .take_while(|(first, second)| first == second)
            .count();
        if common == from.len() || common == to.len() {
            continue;
        }
        groups
            .entry(from[..common].join("."))
            .or_default()
            .entry((from[..=common].join("."), to[..=common].join(".")))
            .or_default()
            .insert((edge.file.clone(), edge.offset, edge.written.clone()));
    }
    groups
}

/// The contract findings of the Python graph: layers, `only_in` and cycles
/// among modules.
pub(super) fn findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let selected = |rule: &str| request.rule.is_none_or(|wanted| wanted == rule);
    let layers = selected(DEPENDENCY_RULE)
        && contract
            .layers
            .iter()
            .any(|layer| !layer.modules.is_empty());
    let packages = selected(INTERFACE_RULE)
        && contract.only_in.iter().any(|list| {
            !(list.imports.is_empty() || list.modules.is_empty() && list.paths.is_empty())
        });
    let cycles = contract.check_cycles && selected(CYCLE_RULE);
    if !(layers || packages || cycles) {
        return Ok(Vec::new());
    }
    let root = Path::new(".");
    let modules = roots::modules(root, request.checked)?;
    if modules.is_empty() {
        return Ok(Vec::new());
    }
    let files: Vec<String> = modules.iter().map(|module| module.file.clone()).collect();
    let imports = scan::scan(request.ast_grep, root, &files)?;
    let edges = build(&modules, &imports);
    let checked: BTreeSet<String> = request
        .checked
        .iter()
        .map(|path| super::clean(path))
        .collect();
    let wanted = |file: &str| request.widen_python || checked.contains(file);
    let mut sources = Sources::default();
    let mut findings = Vec::new();
    if layers {
        findings.extend(layer_findings(request, &edges, &wanted, &mut sources)?);
    }
    if packages {
        findings.extend(package_findings(request, &edges, &wanted, &mut sources)?);
    }
    if cycles {
        findings.extend(cycle_findings(&edges, &wanted, &mut sources)?);
    }
    Ok(findings)
}

/// `architecture.dependency-rule` findings: a module importing a module of a
/// later layer.
fn layer_findings(
    request: &Request,
    edges: &[Edge],
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let layer_of = |module: &str| {
        contract.layer_of_module(module).and_then(|found| {
            contract
                .layers
                .iter()
                .position(|candidate| std::ptr::eq(candidate, found))
        })
    };
    let mut findings = Vec::new();
    for edge in edges.iter().filter(|edge| wanted(&edge.file)) {
        let Target::Module(to) = &edge.target else {
            continue;
        };
        let (Some(from_index), Some(to_index)) = (layer_of(&edge.from), layer_of(to)) else {
            continue;
        };
        if to_index <= from_index {
            continue;
        }
        let (from_layer, to_layer) = (&contract.layers[from_index], &contract.layers[to_index]);
        let detail = format!(
            "`{}` (layer `{}`) imports `{}` from `{to}` (layer `{}`); a layer may \
             import only itself and the layers before it, and `{}` is declared \
             after `{}` at {FILE}:{}",
            edge.from,
            from_layer.name,
            edge.written,
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

/// `boundaries.own-the-interface` findings: a listed package imported from a
/// module outside the entry's `modules` and `paths`.
fn package_findings(
    request: &Request,
    edges: &[Edge],
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let mut findings = Vec::new();
    for edge in edges.iter().filter(|edge| wanted(&edge.file)) {
        let Target::External(package) = &edge.target else {
            continue;
        };
        for list in &request.contract.only_in {
            if (list.modules.is_empty() && list.paths.is_empty())
                || !list.covers(package)
                || list.allows(&edge.from, &edge.file)
            {
                continue;
            }
            let allowed = list
                .modules
                .iter()
                .chain(&list.paths)
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let detail = format!(
                "`{package}` is imported in `{}`, outside {allowed} where the \
                 `only_in` entry at {FILE}:{} allows it",
                edge.from, list.line,
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

/// `components.acyclic-dependencies` findings for cycles among Python
/// modules.
fn cycle_findings(
    edges: &[Edge],
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let mut findings = Vec::new();
    for (parent, group) in cycle_groups(edges) {
        for component in cycles::components(&group) {
            let place = if parent.is_empty() {
                "among the top-level packages".to_string()
            } else {
                format!("all inside `{parent}`")
            };
            let detail = cycle_detail(
                format!("these modules import each other, {place}:"),
                &component,
                sources,
            )?;
            for sites in component.edges.values() {
                let mut seen = BTreeSet::new();
                for (file, offset, _) in sites {
                    if wanted(file) && seen.insert(file.as_str()) {
                        findings.push(finding(CYCLE_RULE, file, *offset, detail.clone(), sources)?);
                    }
                }
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(name: &str, package: bool) -> Module {
        Module {
            name: name.to_string(),
            file: format!("{}.py", name.replace('.', "/")),
            package,
        }
    }

    fn import(file: &str, level: usize, module: &str, name: Option<&str>, form: Form) -> Import {
        Import {
            file: file.to_string(),
            offset: 0,
            form,
            level,
            module: module.to_string(),
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn a_name_is_a_submodule_when_one_exists_else_the_package() {
        let modules = [
            module("pkg", true),
            module("pkg.a", false),
            module("pkg.b", false),
        ];
        let imports = [
            import("pkg/b.py", 0, "pkg", Some("a"), Form::Static),
            import("pkg/b.py", 0, "pkg", Some("thing"), Form::Static),
            import("pkg/a.py", 1, "", Some("b"), Form::Static),
            import("pkg/a.py", 2, "", Some("b"), Form::Static),
        ];
        let edges = build(&modules, &imports);
        let found: Vec<(&str, &str)> = edges
            .iter()
            .map(|edge| match &edge.target {
                Target::Module(to) | Target::External(to) => (edge.from.as_str(), to.as_str()),
            })
            .collect();
        assert_eq!(
            found,
            [("pkg.b", "pkg.a"), ("pkg.b", "pkg"), ("pkg.a", "pkg.b")]
        );
    }

    #[test]
    fn a_dynamic_import_is_an_edge_but_not_a_cycle_edge() {
        let modules = [module("pkg.a", false), module("pkg.b", false)];
        let imports = [
            import("pkg/a.py", 0, "pkg.b", None, Form::Dynamic),
            import("pkg/b.py", 0, "pkg.a", None, Form::Static),
        ];
        let edges = build(&modules, &imports);
        assert_eq!(edges.len(), 2);
        let groups = cycle_groups(&edges);
        assert!(cycles::components(&groups["pkg"]).is_empty());
    }

    #[test]
    fn sibling_packages_fold_with_their_descendants() {
        let modules = [
            module("pkg.adaptive", false),
            module("pkg.sources", true),
            module("pkg.sources.errors", false),
            module("pkg.sources.boursorama", false),
        ];
        let imports = [
            import(
                "pkg/adaptive.py",
                0,
                "pkg.sources.errors",
                None,
                Form::Static,
            ),
            import(
                "pkg/sources/boursorama.py",
                0,
                "pkg.adaptive",
                None,
                Form::Static,
            ),
        ];
        let groups = cycle_groups(&build(&modules, &imports));
        let found = cycles::components(&groups["pkg"]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members, ["pkg.adaptive", "pkg.sources"]);
    }
}
