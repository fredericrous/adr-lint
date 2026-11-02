//! One ast-grep scan over every Go package file: the import specs, each with
//! its byte offset and the import path it names.

use std::path::Path;

use crate::check::{ast_grep_failure, ast_grep_lines};
use crate::coverage::Failure;

use super::super::scan::clean;

/// One import spec: `"a"`, `alias "a"`, `_ "a"`, `. "a"` or `` `a` ``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::graph) struct Import {
    pub(in crate::graph) file: String,
    pub(in crate::graph) offset: usize,
    pub(in crate::graph) path: String,
}

const RULES: &str = "id: graph.import\nlanguage: Go\nrule:\n  kind: import_spec\n";

/// Scans `files` (relative to `root`) once and returns their imports,
/// sorted by file and offset.
pub(in crate::graph) fn scan(
    program: &str,
    root: &Path,
    files: &[String],
) -> Result<Vec<Import>, Failure> {
    let mut found = Vec::new();
    for line in ast_grep_lines(program, &[RULES], files, root)? {
        let document: serde_json::Value = serde_json::from_str(&line).map_err(|cause| {
            ast_grep_failure(&format!(
                "ast-grep printed output that is not JSON: {cause}"
            ))
        })?;
        let text = |pointer: &str| {
            document
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
        };
        let (Some(file), Some(matched), Some(offset)) = (
            text("/file"),
            text("/text"),
            document
                .pointer("/range/byteOffset/start")
                .and_then(serde_json::Value::as_u64)
                .and_then(|start| usize::try_from(start).ok()),
        ) else {
            return Err(ast_grep_failure(
                "ast-grep printed JSON without a file, text or byte range",
            ));
        };
        if let Some(path) = import_path(matched) {
            found.push(Import {
                file: clean(file),
                offset,
                path,
            });
        }
    }
    found.sort_by(|first, second| {
        (&first.file, first.offset, &first.path).cmp(&(&second.file, second.offset, &second.path))
    });
    found.dedup();
    Ok(found)
}

/// The path inside the string literal of an import spec, interpreted or raw.
fn import_path(spec: &str) -> Option<String> {
    let start = spec.find(['"', '`'])?;
    let quote = spec[start..].chars().next()?;
    let rest = &spec[start + 1..];
    let end = rest.find(quote)?;
    let path = &rest[..end];
    (!path.is_empty()).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_form_names_its_path() {
        for (spec, path) in [
            ("\"a/b\"", "a/b"),
            ("x \"a/b\"", "a/b"),
            ("_ \"github.com/lib/pq\"", "github.com/lib/pq"),
            (". \"a/b\"", "a/b"),
            ("`a/b`", "a/b"),
            ("alias `a/b`", "a/b"),
        ] {
            assert_eq!(import_path(spec).as_deref(), Some(path), "{spec}");
        }
        assert_eq!(import_path("x"), None);
    }
}
