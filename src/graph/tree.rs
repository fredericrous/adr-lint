//! The packages of a repository and the module tree of each of their
//! targets, built from the scan; `Graph::reach` follows a path through the
//! tree, through `use` re-exports, and from a binary into its own library.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::scan::{clean, FileFacts};
use super::uses::{self, Leaf};
use crate::coverage::Failure;

/// How many re-exports one path may be followed through.
const REEXPORT_DEPTH: usize = 8;

pub(super) struct Workspace {
    pub(super) rust_files: Vec<String>,
    pub(super) packages: Vec<Package>,
}

pub(super) struct Package {
    pub(super) library: Option<Target>,
    pub(super) binaries: Vec<Target>,
}

pub(super) struct Target {
    pub(super) name: String,
    pub(super) root_file: String,
}

pub(super) fn discover(root: &Path) -> Result<Workspace, Failure> {
    let mut rust_files = Vec::new();
    let mut manifests = Vec::new();
    walk(root, "", &mut rust_files, &mut manifests)?;
    rust_files.sort();
    let known: BTreeSet<&str> = rust_files.iter().map(String::as_str).collect();
    let mut packages = Vec::new();
    for manifest in manifests {
        let path = root.join(&manifest);
        let text = std::fs::read_to_string(&path)
            .map_err(|cause| Failure::CouldNotRun(format!("cannot read {manifest}: {cause}")))?;
        let Ok(table) = text.parse::<toml::Table>() else {
            continue;
        };
        let directory = manifest
            .strip_suffix("Cargo.toml")
            .unwrap_or("")
            .to_string();
        if let Some(package) = package_of(&table, &directory, &known) {
            packages.push(package);
        }
    }
    Ok(Workspace {
        rust_files,
        packages,
    })
}

fn walk(
    root: &Path,
    relative: &str,
    rust_files: &mut Vec<String>,
    manifests: &mut Vec<String>,
) -> Result<(), Failure> {
    let directory = root.join(relative);
    let unreadable = |cause: std::io::Error| {
        Failure::CouldNotRun(format!("cannot read {}: {cause}", directory.display()))
    };
    for entry in std::fs::read_dir(&directory).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{relative}{name}");
        let kind = entry.file_type().map_err(unreadable)?;
        if kind.is_dir() {
            if !matches!(name.as_str(), "target" | "node_modules" | "tests") {
                walk(root, &format!("{path}/"), rust_files, manifests)?;
            }
        } else if name == "Cargo.toml" {
            manifests.push(path);
        } else if name.ends_with(".rs") {
            rust_files.push(path);
        }
    }
    Ok(())
}

fn package_of(table: &toml::Table, directory: &str, known: &BTreeSet<&str>) -> Option<Package> {
    let package_name = table.get("package")?.get("name")?.as_str()?;
    let at = |relative: &str| clean(&format!("{directory}{relative}"));
    let library_table = table.get("lib");
    let library_path = library_table
        .and_then(|lib| lib.get("path"))
        .and_then(toml::Value::as_str)
        .map_or_else(|| at("src/lib.rs"), at);
    let library = known.contains(library_path.as_str()).then(|| Target {
        name: library_table
            .and_then(|lib| lib.get("name"))
            .and_then(toml::Value::as_str)
            .map_or_else(|| package_name.replace('-', "_"), str::to_string),
        root_file: library_path,
    });
    let mut binaries: BTreeMap<String, Target> = BTreeMap::new();
    let mut add = |name: String, file: String| {
        if known.contains(file.as_str()) && !binaries.values().any(|bin| bin.root_file == file) {
            binaries.insert(
                file.clone(),
                Target {
                    name,
                    root_file: file,
                },
            );
        }
    };
    if let Some(declared) = table.get("bin").and_then(toml::Value::as_array) {
        for bin in declared {
            let name = bin
                .get("name")
                .and_then(toml::Value::as_str)
                .unwrap_or(package_name);
            let path = bin
                .get("path")
                .and_then(toml::Value::as_str)
                .map_or_else(|| at("src/main.rs"), at);
            add(name.to_string(), path);
        }
    }
    add(package_name.to_string(), at("src/main.rs"));
    let prefix = at("src/bin/");
    for file in known.iter().filter(|file| file.starts_with(&prefix)) {
        let rest = &file[prefix.len()..];
        if let Some(stem) = rest.strip_suffix(".rs").filter(|stem| !stem.contains('/')) {
            add(stem.to_string(), (*file).to_string());
        }
        if let Some(name) = rest
            .strip_suffix("/main.rs")
            .filter(|name| !name.contains('/'))
        {
            add(name.to_string(), (*file).to_string());
        }
    }
    let binaries: Vec<Target> = binaries.into_values().collect();
    (library.is_some() || !binaries.is_empty()).then_some(Package { library, binaries })
}

/// A module of one target, named by its `crate::` path.
pub(super) struct Module {
    pub(super) key: String,
    pub(super) file: String,
    pub(super) parent: Option<usize>,
    pub(super) children: BTreeMap<String, usize>,
    pub(super) uses: Vec<Import>,
    pub(super) paths: Vec<Import>,
}

/// One `use` leaf or fully qualified path, where it is written.
pub(super) struct Import {
    pub(super) segments: Vec<String>,
    pub(super) glob: bool,
    pub(super) alias: Option<String>,
    pub(super) file: String,
    pub(super) offset: usize,
}

impl Import {
    pub(super) fn written(&self) -> String {
        let path = self.segments.join("::");
        if self.glob {
            format!("{path}::*")
        } else {
            path
        }
    }
}

pub(super) struct Tree {
    pub(super) modules: Vec<Module>,
    /// The library this target's own name resolves into, for a binary.
    pub(super) library: Option<(String, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Node {
    pub(super) tree: usize,
    pub(super) module: usize,
}

pub(super) struct Graph {
    pub(super) trees: Vec<Tree>,
}

/// Where a path leads.
pub(super) enum Reached {
    /// The deepest module the path names, and whether every segment was a
    /// module (the rest of the path is then not an item of it).
    Module(Node, bool),
    /// A crate or a name that is not a module of this repository.
    Outside,
}

impl Graph {
    pub(super) fn build(workspace: &Workspace, facts: &BTreeMap<String, FileFacts>) -> Graph {
        let mut trees = Vec::new();
        let known: BTreeSet<&str> = workspace.rust_files.iter().map(String::as_str).collect();
        for package in &workspace.packages {
            let library_index = package.library.as_ref().map(|library| {
                trees.push(build_tree(library, facts, &known, None));
                (library.name.clone(), trees.len() - 1)
            });
            for binary in &package.binaries {
                trees.push(build_tree(binary, facts, &known, library_index.clone()));
            }
        }
        Graph { trees }
    }

    pub(super) fn module(&self, node: Node) -> &Module {
        &self.trees[node.tree].modules[node.module]
    }

    pub(super) fn key(&self, node: Node) -> &str {
        &self.module(node).key
    }

    /// The modules from the root to `node`, both included.
    pub(super) fn chain(&self, node: Node) -> Vec<usize> {
        let mut chain = vec![node.module];
        let mut current = node.module;
        while let Some(parent) = self.trees[node.tree].modules[current].parent {
            chain.push(parent);
            current = parent;
        }
        chain.reverse();
        chain
    }

    /// Follows `segments` as written in module `from`.
    pub(super) fn reach(&self, from: Node, segments: &[String]) -> Reached {
        self.reach_within(from, segments, 0)
    }

    fn reach_within(&self, from: Node, segments: &[String], depth: usize) -> Reached {
        let Some(first) = segments.first() else {
            return Reached::Module(from, true);
        };
        let tree = &self.trees[from.tree];
        match first.as_str() {
            "crate" => self.descend(
                Node {
                    tree: from.tree,
                    module: 0,
                },
                &segments[1..],
                depth,
            ),
            "self" => self.descend(from, &segments[1..], depth),
            "super" => self.descend(from, segments, depth),
            name => {
                if tree.modules[from.module].children.contains_key(name) {
                    self.descend(from, segments, depth)
                } else if let Some((library_name, library_tree)) = &tree.library {
                    if library_name == name {
                        self.descend(
                            Node {
                                tree: *library_tree,
                                module: 0,
                            },
                            &segments[1..],
                            depth,
                        )
                    } else {
                        Reached::Outside
                    }
                } else {
                    Reached::Outside
                }
            }
        }
    }

    fn descend(&self, start: Node, segments: &[String], depth: usize) -> Reached {
        let mut current = start;
        for segment in segments {
            let module = self.module(current);
            if segment == "self" {
                continue;
            }
            if segment == "super" {
                match module.parent {
                    Some(parent) => current.module = parent,
                    None => return Reached::Outside,
                }
                continue;
            }
            if let Some(child) = module.children.get(segment) {
                current.module = *child;
                continue;
            }
            if depth < REEXPORT_DEPTH {
                if let Some(Reached::Module(node, consumed)) =
                    self.through_use(current, segment, depth + 1)
                {
                    if consumed {
                        current = node;
                        continue;
                    }
                    return Reached::Module(node, false);
                }
            }
            return Reached::Module(current, false);
        }
        Reached::Module(current, true)
    }

    /// Where the name `name`, brought into `node` by a `use`, leads. Any
    /// `use` counts, not only `pub use`: a child sees its parent's private
    /// imports.
    fn through_use(&self, node: Node, name: &str, depth: usize) -> Option<Reached> {
        let import = self
            .module(node)
            .uses
            .iter()
            .find(|import| import.bound_name() == Some(name))?;
        Some(self.reach_within(node, &import.segments, depth))
    }
}

impl Import {
    fn bound_name(&self) -> Option<&str> {
        if self.glob {
            return None;
        }
        let name = self
            .alias
            .as_deref()
            .or_else(|| self.segments.last().map(String::as_str))?;
        (name != "_").then_some(name)
    }
}

fn is_test_file(path: &str) -> bool {
    path.split('/')
        .any(|part| part == "tests" || part == "tests.rs")
}

fn parent_directory(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(directory, _)| directory.to_string())
}

fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    }
}

fn build_tree(
    target: &Target,
    facts: &BTreeMap<String, FileFacts>,
    known: &BTreeSet<&str>,
    library: Option<(String, usize)>,
) -> Tree {
    let mut tree = Tree {
        modules: vec![Module {
            key: "crate".to_string(),
            file: target.root_file.clone(),
            parent: None,
            children: BTreeMap::new(),
            uses: Vec::new(),
            paths: Vec::new(),
        }],
        library,
    };
    let mut loaded = BTreeSet::from([target.root_file.clone()]);
    load_file(
        &mut tree,
        facts,
        known,
        &mut loaded,
        0,
        &target.root_file,
        &parent_directory(&target.root_file),
    );
    tree
}

/// Fills module `file_module`, written in `file`, with its imports and its
/// child modules; `directory` is where its out-of-line children live.
fn load_file(
    tree: &mut Tree,
    facts: &BTreeMap<String, FileFacts>,
    known: &BTreeSet<&str>,
    loaded: &mut BTreeSet<String>,
    file_module: usize,
    file: &str,
    directory: &str,
) {
    let empty = FileFacts::default();
    let found = facts.get(file).unwrap_or(&empty);
    let mut modules = found.modules.clone();
    modules.sort_by_key(|item| item.start);
    let mut open: Vec<(usize, usize, String)> =
        vec![(file_module, usize::MAX, directory.to_string())];
    let mut ranges: Vec<(usize, usize, usize)> = Vec::new();
    let mut out_of_line: Vec<(usize, String, String)> = Vec::new();
    for item in &modules {
        while open.len() > 1 && open.last().is_some_and(|(_, end, _)| *end <= item.start) {
            open.pop();
        }
        let Some((owner, _, owner_directory)) = open.last().cloned() else {
            continue;
        };
        let children_directory = join(&owner_directory, &item.name);
        if item.inline {
            let id = add_child(tree, owner, &item.name, file);
            ranges.push((item.start, item.end, id));
            open.push((id, item.end, children_directory));
        } else {
            let candidates = [
                join(&owner_directory, &format!("{}.rs", item.name)),
                join(&children_directory, "mod.rs"),
            ];
            if let Some(child_file) = candidates
                .into_iter()
                .find(|candidate| known.contains(candidate.as_str()) && !is_test_file(candidate))
            {
                if loaded.insert(child_file.clone()) {
                    let id = add_child(tree, owner, &item.name, &child_file);
                    out_of_line.push((id, child_file, children_directory));
                }
            }
        }
    }
    let owner_of = |offset: usize| {
        ranges
            .iter()
            .filter(|(start, end, _)| *start <= offset && offset < *end)
            .max_by_key(|(start, _, _)| *start)
            .map_or(file_module, |(_, _, id)| *id)
    };
    for found_use in &found.uses {
        for leaf in uses::expand(&found_use.text) {
            let Leaf {
                segments,
                glob,
                alias,
                offset,
            } = leaf;
            tree.modules[owner_of(found_use.start)].uses.push(Import {
                segments,
                glob,
                alias,
                file: file.to_string(),
                offset: found_use.start + offset,
            });
        }
    }
    for found_path in &found.paths {
        let segments: Vec<String> = found_path.text.split("::").map(str::to_string).collect();
        if segments.iter().all(|segment| is_plain_identifier(segment)) {
            tree.modules[owner_of(found_path.start)].paths.push(Import {
                segments,
                glob: false,
                alias: None,
                file: file.to_string(),
                offset: found_path.start,
            });
        }
    }
    for (id, child_file, child_directory) in out_of_line {
        load_file(
            tree,
            facts,
            known,
            loaded,
            id,
            &child_file,
            &child_directory,
        );
    }
}

fn is_plain_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
}

fn add_child(tree: &mut Tree, parent: usize, name: &str, file: &str) -> usize {
    let id = tree.modules.len();
    let key = format!("{}::{name}", tree.modules[parent].key);
    tree.modules.push(Module {
        key,
        file: file.to_string(),
        parent: Some(parent),
        children: BTreeMap::new(),
        uses: Vec::new(),
        paths: Vec::new(),
    });
    tree.modules[parent].children.insert(name.to_string(), id);
    id
}
