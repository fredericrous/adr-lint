//! The TypeScript import graph of a repository (ADR-0056): files as nodes,
//! `import` and `export … from` declarations as edges, resolved the way
//! `tsconfig.json` resolves them. A literal dynamic `import("x")` is an edge
//! that layers and `only_in` read and cycles do not.

mod config;
mod files;
mod jsonc;
mod resolve;
mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use adr_lint::config::FILE;

use crate::check::Finding;
use crate::coverage::Failure;

use super::cycles::{self, Edges};
use super::{line_column, Request, CYCLE_RULE, DEPENDENCY_RULE, INTERFACE_RULE};
use config::Config;
use resolve::{Resolution, Resolver};
use scan::{Form, Import};

/// What an edge leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    File(String),
    /// A bare package specifier, kept for `only_in`.
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    from: String,
    target: Target,
    form: Form,
    offset: usize,
    specifier: String,
}

/// Something the graph could not follow: where it was written, if it was
/// written in a file, and what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Warning {
    place: Option<(String, usize)>,
    text: String,
}

#[derive(Debug, Default)]
struct TypeScriptGraph {
    edges: Vec<Edge>,
    warnings: Vec<Warning>,
}

impl TypeScriptGraph {
    /// Resolves `imports` among `nodes`. `exists_on_disk` answers for files
    /// that are not nodes.
    fn build(
        nodes: &[String],
        imports: &[Import],
        config: &Config,
        exists_on_disk: &dyn Fn(&str) -> bool,
    ) -> TypeScriptGraph {
        let known: BTreeSet<String> = nodes.iter().cloned().collect();
        let resolver = Resolver {
            config,
            nodes: &known,
        };
        let mut graph = TypeScriptGraph::default();
        for import in imports.iter().filter(|import| known.contains(&import.file)) {
            let target = match resolver.resolve(&import.file, &import.specifier, exists_on_disk) {
                Resolution::File(file) => Target::File(file),
                Resolution::External(package) => Target::External(package),
                Resolution::Left => continue,
                Resolution::Unresolved => {
                    graph.warnings.push(Warning {
                        place: Some((import.file.clone(), import.offset)),
                        text: format!("cannot resolve `{}`", import.specifier),
                    });
                    continue;
                }
            };
            graph.edges.push(Edge {
                from: import.file.clone(),
                target,
                form: import.form,
                offset: import.offset,
                specifier: import.specifier.clone(),
            });
        }
        graph
    }

    /// File-level edges that count for cycles: static ones between two
    /// different files.
    fn cycle_edges(&self) -> Edges {
        let mut edges = Edges::new();
        for edge in &self.edges {
            let Target::File(to) = &edge.target else {
                continue;
            };
            if edge.form == Form::Static && *to != edge.from {
                edges
                    .entry((edge.from.clone(), to.clone()))
                    .or_default()
                    .insert((edge.from.clone(), edge.offset, edge.specifier.clone()));
            }
        }
        edges
    }
}

/// The graph of the repository under `root`.
fn graph_of(program: &str, root: &Path, named: &[String]) -> Result<TypeScriptGraph, Failure> {
    let nodes = files::nodes(root, named)?;
    if nodes.is_empty() {
        return Ok(TypeScriptGraph::default());
    }
    let imports = scan::scan(program, root, &nodes)?;
    let (config, notes) = config::load(root);
    let mut graph =
        TypeScriptGraph::build(&nodes, &imports, &config, &|path| root.join(path).is_file());
    graph
        .warnings
        .extend(notes.into_iter().map(|text| Warning { place: None, text }));
    Ok(graph)
}

/// Warnings printed before the count line: this many, then `+N more`.
const WARNINGS_SHOWN: usize = 5;

/// The contract findings of the TypeScript graph: layers, `only_in` and
/// cycles among files.
pub(super) fn findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let selected = |rule: &str| request.rule.is_none_or(|wanted| wanted == rule);
    let layers =
        selected(DEPENDENCY_RULE) && contract.layers.iter().any(|layer| !layer.paths.is_empty());
    let packages =
        selected(INTERFACE_RULE) && contract.only_in.iter().any(|list| !list.imports.is_empty());
    let cycles = contract.check_cycles && selected(CYCLE_RULE);
    if !(layers || packages || cycles) {
        return Ok(Vec::new());
    }
    let graph = graph_of(request.ast_grep, Path::new("."), request.checked)?;
    let checked: BTreeSet<String> = request
        .checked
        .iter()
        .map(|path| super::clean(path))
        .collect();
    let wanted = |file: &str| request.widen_typescript || checked.contains(file);
    let mut sources = Sources::default();
    let mut shown = Vec::new();
    for warning in graph
        .warnings
        .iter()
        .filter(|warning| warning.place.as_ref().is_none_or(|(file, _)| wanted(file)))
    {
        let place = match &warning.place {
            Some((file, offset)) => format!("{file}:{}: ", sources.line(file, *offset)?),
            None => String::new(),
        };
        shown.push(format!("adr-lint: warning: {place}{}", warning.text));
    }
    for line in shown.iter().take(WARNINGS_SHOWN) {
        eprintln!("{line}");
    }
    if shown.len() > WARNINGS_SHOWN {
        eprintln!("adr-lint: warning: +{} more", shown.len() - WARNINGS_SHOWN);
    }
    let mut findings = Vec::new();
    if layers {
        findings.extend(layer_findings(request, &graph, &wanted, &mut sources)?);
    }
    if packages {
        findings.extend(package_findings(request, &graph, &wanted, &mut sources)?);
    }
    if cycles {
        findings.extend(cycle_findings(&graph, &wanted, &mut sources)?);
    }
    Ok(findings)
}

fn finding(
    rule: &str,
    file: &str,
    offset: usize,
    detail: String,
    sources: &mut Sources,
) -> Result<Finding, Failure> {
    let (line, col) = sources.position(file, offset)?;
    Ok(Finding {
        path: file.to_string(),
        line,
        col,
        rule: rule.to_string(),
        pattern_id: rule.to_string(),
        detail,
        explained: true,
    })
}

/// `architecture.dependency-rule` findings: a file importing a file of a
/// later layer.
fn layer_findings(
    request: &Request,
    graph: &TypeScriptGraph,
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let contract = request.contract;
    let layer_of = |file: &str| {
        contract
            .layer_index_of_path(file)
            .map_err(|cause| Failure::CouldNotRun(cause.to_string()))
    };
    let mut findings = Vec::new();
    for edge in graph.edges.iter().filter(|edge| wanted(&edge.from)) {
        let Target::File(to) = &edge.target else {
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
            "`{}` (layer `{}`) imports `{}` from `{to}` (layer `{}`); a layer may \
             import only itself and the layers before it, and `{}` is declared \
             after `{}` at {FILE}:{}",
            edge.from,
            from_layer.name,
            edge.specifier,
            to_layer.name,
            to_layer.name,
            from_layer.name,
            to_layer.line,
        );
        findings.push(finding(
            DEPENDENCY_RULE,
            &edge.from,
            edge.offset,
            detail,
            sources,
        )?);
    }
    Ok(findings)
}

/// `boundaries.own-the-interface` findings: a listed package imported from a
/// file outside the entry's `paths`.
fn package_findings(
    request: &Request,
    graph: &TypeScriptGraph,
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let mut findings = Vec::new();
    for edge in graph.edges.iter().filter(|edge| wanted(&edge.from)) {
        let Target::External(specifier) = &edge.target else {
            continue;
        };
        for list in &request.contract.only_in {
            if list.paths.is_empty()
                || !list.covers_package(specifier)
                || list.allows_path(&edge.from)
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
                "`{specifier}` is imported in `{}`, outside {allowed} where the \
                 `only_in` entry at {FILE}:{} allows it",
                edge.from, list.line,
            );
            findings.push(finding(
                INTERFACE_RULE,
                &edge.from,
                edge.offset,
                detail,
                sources,
            )?);
        }
    }
    Ok(findings)
}

/// `components.acyclic-dependencies` findings for cycles among files.
fn cycle_findings(
    graph: &TypeScriptGraph,
    wanted: &dyn Fn(&str) -> bool,
    sources: &mut Sources,
) -> Result<Vec<Finding>, Failure> {
    let mut findings = Vec::new();
    for component in cycles::components(&graph.cycle_edges()) {
        let detail = cycle_detail(&component, sources)?;
        for sites in component.edges.values() {
            let mut files = BTreeSet::new();
            for (file, offset, _) in sites {
                if wanted(file) && files.insert(file.as_str()) {
                    findings.push(finding(CYCLE_RULE, file, *offset, detail.clone(), sources)?);
                }
            }
        }
    }
    Ok(findings)
}

fn cycle_detail(component: &cycles::Component, sources: &mut Sources) -> Result<String, Failure> {
    let mut lines = vec!["these files import each other:".to_string()];
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
                Some((file, offset, _)) => format!("{file}:{}", sources.line(file, *offset)?),
                None => String::new(),
            };
            lines.push(format!("  {} -> {} ({at})", pair[0], pair[1]));
        }
    }
    Ok(lines.join("\n"))
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

    fn line(&mut self, file: &str, offset: usize) -> Result<usize, Failure> {
        self.position(file, offset).map(|(line, _)| line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(file: &str, specifier: &str, form: Form) -> Import {
        Import {
            file: file.to_string(),
            offset: 0,
            specifier: specifier.to_string(),
            form,
        }
    }

    fn nodes(files: &[&str]) -> Vec<String> {
        files.iter().map(|file| (*file).to_string()).collect()
    }

    #[test]
    fn a_dynamic_import_is_an_edge_but_not_a_cycle_edge() {
        let graph = TypeScriptGraph::build(
            &nodes(&["a.ts", "b.ts"]),
            &[
                import("a.ts", "./b", Form::Dynamic),
                import("b.ts", "./a", Form::Static),
            ],
            &Config::default(),
            &|_| false,
        );
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].form, Form::Dynamic);
        let cycle_edges = graph.cycle_edges();
        assert_eq!(
            cycle_edges.keys().collect::<Vec<_>>(),
            [&("b.ts".to_string(), "a.ts".to_string())]
        );
        assert!(cycles::components(&cycle_edges).is_empty());
    }

    #[test]
    fn packages_are_kept_and_unresolved_imports_warn_once() {
        let graph = TypeScriptGraph::build(
            &nodes(&["a.ts"]),
            &[
                import("a.ts", "effect", Form::Static),
                import("a.ts", "./gone", Form::Static),
                import("a.ts", "./styles.css", Form::Static),
                import("ignored.ts", "./a", Form::Static),
            ],
            &Config::default(),
            &|_| false,
        );
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(
            graph.edges[0].target,
            Target::External("effect".to_string())
        );
        assert_eq!(graph.warnings.len(), 1);
        assert_eq!(graph.warnings[0].text, "cannot resolve `./gone`");
    }

    #[test]
    fn a_ring_between_two_files_is_one_component() {
        let graph = TypeScriptGraph::build(
            &nodes(&["a.ts", "b.ts"]),
            &[
                import("a.ts", "./b", Form::Static),
                import("b.ts", "./a.js", Form::Static),
            ],
            &Config::default(),
            &|_| false,
        );
        let found = cycles::components(&graph.cycle_edges());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members, ["a.ts", "b.ts"]);
        assert_eq!(found[0].cycles, [["a.ts", "b.ts", "a.ts"]]);
    }
}
