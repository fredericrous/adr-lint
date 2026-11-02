//! The TypeScript import graph of a repository (ADR-0056): files as nodes,
//! `import` and `export … from` declarations as edges, resolved the way
//! `tsconfig.json` resolves them. A literal dynamic `import("x")` is an edge
//! that layers and `only_in` read and cycles do not. Only the cycle check is
//! reported so far.

mod config;
mod files;
mod jsonc;
mod resolve;
mod scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::check::Finding;
use crate::coverage::Failure;

use super::cycles::{self, Edges};
use super::{line_column, Request, CYCLE_RULE};
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

/// `components.acyclic-dependencies` findings for cycles among files.
pub(super) fn cycle_findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let selected = request.rule.is_none_or(|wanted| wanted == CYCLE_RULE);
    if !request.contract.check_cycles || !selected {
        return Ok(Vec::new());
    }
    let graph = graph_of(request.ast_grep, Path::new("."), request.checked)?;
    let mut sources = Sources::default();
    for warning in &graph.warnings {
        let place = match &warning.place {
            Some((file, offset)) => format!("{file}:{}: ", sources.line(file, *offset)?),
            None => String::new(),
        };
        eprintln!("adr-lint: warning: {place}{}", warning.text);
    }
    let checked: BTreeSet<String> = request
        .checked
        .iter()
        .map(|path| super::clean(path))
        .collect();
    let mut findings = Vec::new();
    for component in cycles::components(&graph.cycle_edges()) {
        let detail = cycle_detail(&component, &mut sources)?;
        for sites in component.edges.values() {
            let mut files = BTreeSet::new();
            for (file, offset, _) in sites {
                if (request.widen || checked.contains(file)) && files.insert(file.as_str()) {
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
