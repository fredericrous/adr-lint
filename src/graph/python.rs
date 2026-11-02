//! The Python import graph of a repository (ADR-0056): modules as nodes,
//! `import` and `from … import` statements as edges, resolved against the
//! modules under the uv workspace members' roots. Cycles are reported among
//! sibling modules at every package depth, each sibling package folded with
//! its descendants, as the Rust graph does. A literal
//! `importlib.import_module("x")` is an edge that cycles do not read.
//!
//! An import of `a.b.c` makes no edge to `a` or `a.b`, and a re-export in an
//! `__init__.py` is not followed. A name outside the roots is not an edge
//! yet: layers and `only_in` read it in the next part.

mod roots;
mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::check::Finding;
use crate::coverage::Failure;

use super::cycles::{self, Edges};
use super::{line_column, Request, CYCLE_RULE};
use roots::Module;
use scan::{Form, Import};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    file: String,
    from: String,
    to: String,
    form: Form,
    offset: usize,
    written: String,
}

/// The module `import` leads to, when it is one of `modules`.
fn target(modules: &BTreeMap<&str, &Module>, own: &Module, import: &Import) -> Option<String> {
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
    if full.is_empty() {
        return None;
    }
    if let Some(name) = &import.name {
        let submodule = format!("{full}.{name}");
        if modules.contains_key(submodule.as_str()) {
            return Some(submodule);
        }
    }
    modules.contains_key(full.as_str()).then_some(full)
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
        let Some(to) = target(&by_name, own, import) else {
            continue;
        };
        if to == own.name {
            continue;
        }
        edges.push(Edge {
            file: import.file.clone(),
            from: own.name.clone(),
            to,
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
        let from: Vec<&str> = edge.from.split('.').collect();
        let to: Vec<&str> = edge.to.split('.').collect();
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

/// `components.acyclic-dependencies` findings for cycles among Python
/// modules.
pub(super) fn findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    if !request.contract.check_cycles || request.rule.is_some_and(|wanted| wanted != CYCLE_RULE) {
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
    for (parent, group) in cycle_groups(&edges) {
        for component in cycles::components(&group) {
            let detail = cycle_detail(&parent, &component, &mut sources)?;
            for sites in component.edges.values() {
                let mut seen = BTreeSet::new();
                for (file, offset, _) in sites {
                    if wanted(file) && seen.insert(file.as_str()) {
                        let (line, col) = sources.position(file, *offset)?;
                        findings.push(Finding {
                            path: file.clone(),
                            line,
                            col,
                            rule: CYCLE_RULE.to_string(),
                            pattern_id: CYCLE_RULE.to_string(),
                            detail: detail.clone(),
                            explained: true,
                        });
                    }
                }
            }
        }
    }
    Ok(findings)
}

/// File texts read once, to turn byte offsets into lines.
#[derive(Default)]
struct Sources(BTreeMap<String, String>);

impl Sources {
    fn position(&mut self, file: &str, offset: usize) -> Result<(usize, usize), Failure> {
        if !self.0.contains_key(file) {
            let text = std::fs::read_to_string(file)
                .map_err(|cause| Failure::CouldNotRun(format!("cannot read {file}: {cause}")))?;
            self.0.insert(file.to_string(), text);
        }
        Ok(line_column(&self.0[file], offset))
    }
}

fn cycle_detail(
    parent: &str,
    component: &cycles::Component,
    sources: &mut Sources,
) -> Result<String, Failure> {
    let place = if parent.is_empty() {
        "among the top-level packages".to_string()
    } else {
        format!("all inside `{parent}`")
    };
    let mut lines = vec![format!("these modules import each other, {place}:")];
    lines.extend(component.members.iter().map(|member| format!("  {member}")));
    for (index, cycle) in component.cycles.iter().enumerate() {
        lines.push(
            if index == 0 {
                "a shortest cycle, one import per step:"
            } else {
                "another cycle:"
            }
            .to_string(),
        );
        for pair in cycle.windows(2) {
            let at = match component
                .edges
                .get(&(pair[0].clone(), pair[1].clone()))
                .and_then(|sites| sites.first())
            {
                Some((file, offset, _)) => format!("{file}:{}", sources.position(file, *offset)?.0),
                None => String::new(),
            };
            lines.push(format!("  {} -> {} ({at})", pair[0], pair[1]));
        }
    }
    Ok(lines.join("\n"))
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
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
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
