//! One ast-grep scan over every Rust file: the `use` declarations, the `mod`
//! items, the fully qualified paths and the items that follow a
//! `#[cfg(test)]` attribute, each with its byte range.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::check::{ast_grep_lines, normalize_path};
use crate::coverage::Failure;

const ITEM_KINDS: [&str; 17] = [
    "mod_item",
    "use_declaration",
    "function_item",
    "impl_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "const_item",
    "static_item",
    "type_item",
    "macro_invocation",
    "macro_definition",
    "extern_crate_declaration",
    "foreign_mod_item",
    "union_item",
    "expression_statement",
    "let_declaration",
];

#[derive(Debug, Default)]
pub(super) struct FileFacts {
    pub(super) uses: Vec<Found>,
    pub(super) modules: Vec<ModItem>,
    pub(super) paths: Vec<Found>,
}

#[derive(Debug, Clone)]
pub(super) struct Found {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) text: String,
}

#[derive(Debug, Clone)]
pub(super) struct ModItem {
    pub(super) name: String,
    pub(super) start: usize,
    pub(super) end: usize,
    /// `mod x { … }` rather than `mod x;`.
    pub(super) inline: bool,
}

/// Scans `files` (relative to `root`) once. `external_roots` are the crate
/// names whose fully qualified paths are collected beside `crate::`, `self::`
/// and `super::`.
pub(super) fn scan(
    program: &str,
    root: &Path,
    files: &[String],
    external_roots: &BTreeSet<String>,
) -> Result<BTreeMap<String, FileFacts>, Failure> {
    let rules = rules(external_roots);
    let rule_texts: Vec<&str> = vec![rules.as_str()];
    let mut raw: BTreeMap<String, Raw> = BTreeMap::new();
    for line in ast_grep_lines(program, &rule_texts, files, root)? {
        let document: serde_json::Value = serde_json::from_str(&line).map_err(|cause| {
            crate::check::ast_grep_failure(&format!(
                "ast-grep printed output that is not JSON: {cause}"
            ))
        })?;
        let text = |pointer: &str| {
            document
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
        };
        let number = |pointer: &str| {
            document
                .pointer(pointer)
                .and_then(serde_json::Value::as_u64)
                .and_then(|found| usize::try_from(found).ok())
        };
        let (Some(file), Some(rule), Some(found_text), Some(start), Some(end)) = (
            text("/file"),
            text("/ruleId"),
            text("/text"),
            number("/range/byteOffset/start"),
            number("/range/byteOffset/end"),
        ) else {
            return Err(crate::check::ast_grep_failure(
                "ast-grep printed JSON without a file, rule id, text or byte range",
            ));
        };
        let found = Found {
            start,
            end,
            text: found_text.to_string(),
        };
        let entry = raw.entry(clean(file)).or_default();
        match rule {
            "graph.use" => entry.uses.push(found),
            "graph.mod" => entry.modules.push(found),
            "graph.path" => entry.paths.push(found),
            "graph.cfg-test" => entry.test_ranges.push((start, end)),
            _ => {}
        }
    }
    Ok(raw
        .into_iter()
        .map(|(file, raw)| (file, raw.into_facts()))
        .collect())
}

/// A repository-relative path with `/` separators and no `./` prefix.
pub(super) fn clean(path: &str) -> String {
    let normalized = normalize_path(path);
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}

fn rules(external_roots: &BTreeSet<String>) -> String {
    let mut roots = vec!["crate".to_string(), "self".to_string(), "super".to_string()];
    roots.extend(external_roots.iter().cloned());
    let kinds: String = ITEM_KINDS
        .iter()
        .map(|kind| format!("        - kind: {kind}\n"))
        .collect();
    format!(
        "id: graph.use
language: Rust
rule:
  kind: use_declaration
---
id: graph.mod
language: Rust
rule:
  kind: mod_item
---
id: graph.path
language: Rust
rule:
  any:
    - kind: scoped_identifier
    - kind: scoped_type_identifier
  regex: '^({})::'
  not:
    inside:
      kind: use_declaration
      stopBy: end
---
id: graph.cfg-test
language: Rust
rule:
  all:
    - any:
{kinds}    - inside:
        any:
          - kind: source_file
          - kind: declaration_list
    - not:
        any:
          - kind: attribute_item
          - kind: line_comment
          - kind: block_comment
    - follows:
        kind: attribute_item
        regex: 'cfg\\(\\s*(all\\(\\s*)?test\\b'
        stopBy:
          not:
            kind: attribute_item
",
        roots.join("|")
    )
}

#[derive(Default)]
struct Raw {
    uses: Vec<Found>,
    modules: Vec<Found>,
    paths: Vec<Found>,
    test_ranges: Vec<(usize, usize)>,
}

impl Raw {
    fn into_facts(self) -> FileFacts {
        let in_test = |found: &Found| {
            self.test_ranges
                .iter()
                .any(|(start, end)| found.start >= *start && found.end <= *end)
        };
        let uses = self
            .uses
            .iter()
            .filter(|found| !in_test(found))
            .cloned()
            .collect();
        let modules = self
            .modules
            .iter()
            .filter(|found| !in_test(found))
            .filter_map(|found| {
                module_name(&found.text).map(|name| ModItem {
                    name,
                    start: found.start,
                    end: found.end,
                    inline: found.text.trim_end().ends_with('}'),
                })
            })
            .collect();
        let mut paths: Vec<Found> = self
            .paths
            .iter()
            .filter(|found| !in_test(found))
            .cloned()
            .collect();
        paths.sort_by_key(|found| (found.start, std::cmp::Reverse(found.end)));
        let mut outermost: Vec<Found> = Vec::new();
        for found in paths {
            if outermost.last().is_none_or(|last| found.start >= last.end) {
                outermost.push(found);
            }
        }
        FileFacts {
            uses,
            modules,
            paths: outermost,
        }
    }
}

fn module_name(text: &str) -> Option<String> {
    let mut words = text
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty());
    words.find(|word| *word == "mod")?;
    words.next().map(str::to_string)
}
