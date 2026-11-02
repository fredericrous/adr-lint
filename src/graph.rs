//! The Rust module graph of a repository and the findings of its
//! `[architecture]` contract (ADR-0056): an import from a later layer, a
//! parent or sibling glob import, a crate used outside the modules its
//! `only_in` entry names, and a cycle among sibling modules; the same for
//! the TypeScript import graph, file by file.

mod cycles;
mod scan;
mod tree;
mod typescript;
mod uses;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use adr_lint::config::FILE;
use adr_lint::contract::Contract;

use crate::check::Finding;
use crate::coverage::Failure;
use cycles::{Edges, Site};
use scan::clean;
use tree::{Graph, Import, Node, Reached};

pub(crate) const DEPENDENCY_RULE: &str = "architecture.dependency-rule";
pub(crate) const INTERFACE_RULE: &str = "boundaries.own-the-interface";
pub(crate) const CYCLE_RULE: &str = "components.acyclic-dependencies";
pub(crate) const CONTRACT_RULES: [&str; 3] = [DEPENDENCY_RULE, INTERFACE_RULE, CYCLE_RULE];

pub(crate) struct Request<'a> {
    pub(crate) ast_grep: &'a str,
    pub(crate) contract: &'a Contract,
    /// The checked files, relative to the current directory.
    pub(crate) checked: &'a [String],
    /// Report on every file, not only the checked ones: the contract or the
    /// shape of the module tree may have changed.
    pub(crate) widen: bool,
    /// `widen`, or a `tsconfig*.json` was checked: it changes how TypeScript
    /// imports resolve.
    pub(crate) widen_typescript: bool,
    /// Only this rule's findings, when set.
    pub(crate) rule: Option<&'a str>,
}

pub(crate) fn contract_findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let mut findings = rust_findings(request)?;
    findings.extend(typescript::findings(request)?);
    Ok(findings)
}

fn rust_findings(request: &Request) -> Result<Vec<Finding>, Failure> {
    let root = Path::new(".");
    let workspace = tree::discover(root)?;
    if workspace.rust_files.is_empty() {
        return Ok(Vec::new());
    }
    let mut external_roots = BTreeSet::new();
    for only_in in &request.contract.only_in {
        for entry in &only_in.imports {
            let first = entry.text.split("::").next().unwrap_or("");
            if is_crate_name(first) {
                external_roots.insert(first.to_string());
            }
        }
    }
    for package in &workspace.packages {
        if let Some(library) = &package.library {
            external_roots.insert(library.name.clone());
        }
    }
    let facts = scan::scan(
        request.ast_grep,
        root,
        &workspace.rust_files,
        &external_roots,
    )?;
    let graph = Graph::build(&workspace, &facts);
    let checked: BTreeSet<String> = request.checked.iter().map(|path| clean(path)).collect();
    let widen = request.widen
        || checked.iter().any(|file| {
            facts
                .get(file)
                .is_some_and(|found| !found.modules.is_empty())
        });
    let mut run = Run {
        request,
        graph: &graph,
        checked,
        widen,
        sources: BTreeMap::new(),
        findings: Vec::new(),
    };
    let resolved = run.resolve();
    run.dependency_rule(&resolved)?;
    run.own_the_interface(&resolved)?;
    run.cycles(&resolved)?;
    Ok(run.findings)
}

/// The 1-based line and column of a byte offset in `text`.
fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let before = &bytes[..offset.min(bytes.len())];
    let line = 1 + before.iter().filter(|byte| **byte == b'\n').count();
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let column = 1 + String::from_utf8_lossy(&before[line_start..])
        .chars()
        .count();
    (line, column)
}

fn is_crate_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
}

/// An import and the module it leads to, when that is another module of
/// the repository.
struct Resolved<'a> {
    from: Node,
    import: &'a Import,
    target: Option<Node>,
}

struct Run<'a> {
    request: &'a Request<'a>,
    graph: &'a Graph,
    checked: BTreeSet<String>,
    widen: bool,
    sources: BTreeMap<String, String>,
    findings: Vec<Finding>,
}

impl<'a> Run<'a> {
    fn resolve(&self) -> Vec<Resolved<'a>> {
        let mut resolved = Vec::new();
        for (tree_index, tree) in self.graph.trees.iter().enumerate() {
            for (module_index, module) in tree.modules.iter().enumerate() {
                let from = Node {
                    tree: tree_index,
                    module: module_index,
                };
                for import in module.uses.iter().chain(&module.paths) {
                    let target = match self.graph.reach(from, &import.segments) {
                        Reached::Module(node, _) if node != from => Some(node),
                        Reached::Module(..) | Reached::Outside => None,
                    };
                    resolved.push(Resolved {
                        from,
                        import,
                        target,
                    });
                }
            }
        }
        resolved
    }

    fn wanted(&self, file: &str) -> bool {
        self.widen || self.checked.contains(file)
    }

    fn selects(&self, rule: &str) -> bool {
        self.request.rule.is_none_or(|wanted| wanted == rule)
    }

    fn layer_of(&self, node: Node) -> Result<Option<usize>, Failure> {
        let module = self.graph.module(node);
        let contract = self.request.contract;
        let layer = match contract.layer_of_module(&module.key) {
            Some(layer) => Some(layer),
            None => contract
                .layer_of_path(&module.file)
                .map_err(|cause| Failure::CouldNotRun(cause.to_string()))?,
        };
        Ok(layer.and_then(|found| {
            contract
                .layers
                .iter()
                .position(|candidate| std::ptr::eq(candidate, found))
        }))
    }

    fn position(&mut self, file: &str, offset: usize) -> Result<(usize, usize), Failure> {
        if !self.sources.contains_key(file) {
            let text = std::fs::read_to_string(file)
                .map_err(|cause| Failure::CouldNotRun(format!("cannot read {file}: {cause}")))?;
            self.sources.insert(file.to_string(), text);
        }
        Ok(line_column(&self.sources[file], offset))
    }

    fn push(
        &mut self,
        rule: &str,
        file: &str,
        offset: usize,
        detail: String,
    ) -> Result<(), Failure> {
        let (line, col) = self.position(file, offset)?;
        self.findings.push(Finding {
            path: file.to_string(),
            line,
            col,
            rule: rule.to_string(),
            pattern_id: rule.to_string(),
            detail,
            explained: true,
        });
        Ok(())
    }

    fn dependency_rule(&mut self, resolved: &[Resolved]) -> Result<(), Failure> {
        if !self.selects(DEPENDENCY_RULE) {
            return Ok(());
        }
        let request = self.request;
        let graph = self.graph;
        let layers = &request.contract.layers;
        let wanted: Vec<&Resolved> = resolved
            .iter()
            .filter(|found| self.wanted(&found.import.file))
            .collect();
        for found in wanted {
            let from_layer = self.layer_of(found.from)?;
            let written = found.import.written();
            if let (Some(from_index), Some(target)) = (from_layer, found.target) {
                if let Some(to_index) = self.layer_of(target)?.filter(|to| *to > from_index) {
                    let detail = format!(
                        "`{}` (layer `{}`) imports `{written}` from `{}` (layer `{}`); \
                         a layer may import only itself and the layers before it, and `{}` \
                         is declared after `{}` at {FILE}:{}",
                        graph.key(found.from),
                        layers[from_index].name,
                        graph.key(target),
                        layers[to_index].name,
                        layers[to_index].name,
                        layers[from_index].name,
                        layers[to_index].line,
                    );
                    self.push(
                        DEPENDENCY_RULE,
                        &found.import.file,
                        found.import.offset,
                        detail,
                    )?;
                }
            }
            let parent_glob = found.import.glob
                && found
                    .import
                    .segments
                    .first()
                    .is_some_and(|first| first == "super");
            if let (true, Some(from_index)) = (parent_glob, from_layer) {
                let detail = format!(
                    "`use {written};` in `{}` (layer `{}`, {FILE}:{}) imports everything a \
                     parent or sibling module has in scope, which the layer order cannot \
                     check: name what is imported",
                    graph.key(found.from),
                    layers[from_index].name,
                    layers[from_index].line,
                );
                self.push(
                    DEPENDENCY_RULE,
                    &found.import.file,
                    found.import.offset,
                    detail,
                )?;
            }
        }
        Ok(())
    }

    fn own_the_interface(&mut self, resolved: &[Resolved]) -> Result<(), Failure> {
        if !self.selects(INTERFACE_RULE) {
            return Ok(());
        }
        let request = self.request;
        let graph = self.graph;
        let wanted: Vec<&Resolved> = resolved
            .iter()
            .filter(|found| self.wanted(&found.import.file))
            .collect();
        for found in wanted {
            let written = found.import.segments.join("::");
            let target_key = found.target.map(|target| graph.key(target));
            let importer = graph.key(found.from);
            for list in &request.contract.only_in {
                let covered =
                    list.covers(&written) || target_key.is_some_and(|key| list.covers(key));
                if covered && !list.allows(importer, &found.import.file) {
                    let places: Vec<&str> = list
                        .modules
                        .iter()
                        .chain(&list.paths)
                        .map(|entry| entry.text.as_str())
                        .collect();
                    let detail = format!(
                        "`{written}` is imported in `{importer}`, outside {} where the \
                         `only_in` entry at {FILE}:{} allows it",
                        places.join(", "),
                        list.line,
                    );
                    self.push(
                        INTERFACE_RULE,
                        &found.import.file,
                        found.import.offset,
                        detail,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn cycles(&mut self, resolved: &[Resolved]) -> Result<(), Failure> {
        if !self.request.contract.check_cycles || !self.selects(CYCLE_RULE) {
            return Ok(());
        }
        let graph = self.graph;
        let mut groups: BTreeMap<(usize, usize), Edges> = BTreeMap::new();
        for found in resolved {
            let Some(target) = found.target.filter(|node| node.tree == found.from.tree) else {
                continue;
            };
            let from_chain = graph.chain(found.from);
            let to_chain = graph.chain(target);
            let common = from_chain
                .iter()
                .zip(&to_chain)
                .take_while(|(first, second)| first == second)
                .count();
            if common == 0 || common == from_chain.len() || common == to_chain.len() {
                continue;
            }
            let tree = found.from.tree;
            let child_key = |module: usize| graph.key(Node { tree, module }).to_string();
            groups
                .entry((tree, from_chain[common - 1]))
                .or_default()
                .entry((child_key(from_chain[common]), child_key(to_chain[common])))
                .or_default()
                .insert((
                    found.import.file.clone(),
                    found.import.offset,
                    found.import.written(),
                ));
        }
        for ((tree, parent), edges) in groups {
            let parent_key = graph.key(Node {
                tree,
                module: parent,
            });
            for component in cycles::components(&edges) {
                let detail = self.cycle_detail(parent_key, &component)?;
                for sites in component.edges.values() {
                    let mut files = BTreeSet::new();
                    for (file, offset, _) in sites {
                        if self.wanted(file) && files.insert(file.as_str()) {
                            self.push(CYCLE_RULE, file, *offset, detail.clone())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn cycle_detail(
        &mut self,
        parent_key: &str,
        component: &cycles::Component,
    ) -> Result<String, Failure> {
        let mut lines = vec![format!(
            "these modules import each other, all inside `{parent_key}`:"
        )];
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
                let site: Option<&Site> = component
                    .edges
                    .get(&(pair[0].clone(), pair[1].clone()))
                    .and_then(|sites| sites.first());
                let at = match site {
                    Some((file, offset, _)) => {
                        let (line, _) = self.position(file, *offset)?;
                        format!("{file}:{line}")
                    }
                    None => String::new(),
                };
                lines.push(format!("  {} -> {} ({at})", pair[0], pair[1]));
            }
        }
        Ok(lines.join("\n"))
    }
}
