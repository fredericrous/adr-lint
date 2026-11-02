//! One ast-grep scan over every TypeScript file: the `import` declarations
//! (`import type` and `import x = require(…)` included), the
//! `export … from` declarations and the dynamic `import(…)` calls, each with
//! its byte offset and the specifier it names.

use std::path::Path;

use crate::check::{ast_grep_failure, ast_grep_lines};
use crate::coverage::Failure;

use super::super::scan::clean;

/// How an import is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) enum Form {
    /// `import`, `export … from`, `import x = require(…)`.
    Static,
    /// `import("x")` with a literal specifier.
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::graph) struct Import {
    pub(in crate::graph) file: String,
    pub(in crate::graph) offset: usize,
    pub(in crate::graph) specifier: String,
    pub(in crate::graph) form: Form,
}

const RULE_KINDS: [(&str, &str); 3] = [
    ("graph.import", "kind: import_statement"),
    (
        "graph.export",
        "kind: export_statement\n  has:\n    field: source\n    kind: string",
    ),
    (
        "graph.dynamic",
        "kind: call_expression\n  has:\n    field: function\n    kind: import",
    ),
];

fn rules() -> String {
    let mut documents = Vec::new();
    for language in ["TypeScript", "Tsx"] {
        for (id, rule) in RULE_KINDS {
            documents.push(format!("id: {id}\nlanguage: {language}\nrule:\n  {rule}\n"));
        }
    }
    documents.join("---\n")
}

/// Scans `files` (relative to `root`) once and returns their imports,
/// sorted by file and offset.
pub(in crate::graph) fn scan(
    program: &str,
    root: &Path,
    files: &[String],
) -> Result<Vec<Import>, Failure> {
    let rules = rules();
    let rule_texts: Vec<&str> = vec![rules.as_str()];
    let mut found = Vec::new();
    for line in ast_grep_lines(program, &rule_texts, files, root)? {
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
        let (Some(file), Some(rule), Some(matched), Some(offset)) = (
            text("/file"),
            text("/ruleId"),
            text("/text"),
            document
                .pointer("/range/byteOffset/start")
                .and_then(serde_json::Value::as_u64)
                .and_then(|start| usize::try_from(start).ok()),
        ) else {
            return Err(ast_grep_failure(
                "ast-grep printed JSON without a file, rule id, text or byte range",
            ));
        };
        let (form, specifier) = match rule {
            "graph.dynamic" => (Form::Dynamic, dynamic_specifier(matched)),
            "graph.import" | "graph.export" => (Form::Static, static_specifier(matched)),
            unknown => {
                return Err(Failure::CouldNotRun(format!(
                "adr-lint has no handler for its own ast-grep rule `{unknown}` (a bug in adr-lint)"
            )))
            }
        };
        if let Some(specifier) = specifier {
            found.push(Import {
                file: clean(file),
                offset,
                specifier,
                form,
            });
        }
    }
    found.sort_by(|first, second| {
        (&first.file, first.offset, &first.specifier).cmp(&(
            &second.file,
            second.offset,
            &second.specifier,
        ))
    });
    found.dedup();
    Ok(found)
}

/// A token of the statement text that matters here: a word, a punctuation
/// character, or a string literal's content.
#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Punct(char),
    Str(String),
}

fn tokens(text: &str) -> Vec<Token> {
    let characters: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '/' && characters.get(index + 1) == Some(&'/') {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
        } else if character == '/' && characters.get(index + 1) == Some(&'*') {
            index += 2;
            while index < characters.len()
                && !(characters[index] == '*' && characters.get(index + 1) == Some(&'/'))
            {
                index += 1;
            }
            index += 2;
        } else if character == '"' || character == '\'' {
            let mut content = String::new();
            index += 1;
            while index < characters.len() && characters[index] != character {
                if characters[index] == '\\' && index + 1 < characters.len() {
                    index += 1;
                }
                content.push(characters[index]);
                index += 1;
            }
            index += 1;
            out.push(Token::Str(content));
        } else if character.is_alphanumeric() || character == '_' || character == '$' {
            let mut word = String::new();
            while index < characters.len()
                && (characters[index].is_alphanumeric()
                    || characters[index] == '_'
                    || characters[index] == '$')
            {
                word.push(characters[index]);
                index += 1;
            }
            out.push(Token::Word(word));
        } else {
            if !character.is_whitespace() {
                out.push(Token::Punct(character));
            }
            index += 1;
        }
    }
    out
}

/// The module named by an import or export declaration: the string after
/// `from`, after `require(`, or straight after `import`.
fn static_specifier(text: &str) -> Option<String> {
    let found = tokens(text);
    found.iter().enumerate().find_map(|(index, token)| {
        let Token::Str(content) = token else {
            return None;
        };
        let before = index.checked_sub(1).map(|previous| &found[previous]);
        match before {
            Some(Token::Word(word)) if word == "from" || word == "import" => Some(content.clone()),
            Some(Token::Punct('(')) => Some(content.clone()),
            _ => None,
        }
    })
}

/// The specifier of `import("x")`; `None` for anything computed.
fn dynamic_specifier(text: &str) -> Option<String> {
    let found = tokens(text);
    match found.as_slice() {
        [Token::Word(keyword), Token::Punct('('), Token::Str(content), Token::Punct(')' | ','), ..]
            if keyword == "import" =>
        {
            Some(content.clone())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_name_their_module() {
        let cases = [
            ("import a from \"./a\";", Some("./a")),
            ("import type { A } from './a';", Some("./a")),
            ("import \"./side\";", Some("./side")),
            ("import { \"x-y\" as z } from \"./w\";", Some("./w")),
            ("import fs = require(\"node:fs\");", Some("node:fs")),
            ("export * from \"./b\";", Some("./b")),
            ("export { a } from './b' // c", Some("./b")),
            (
                "import a from \"./a\" with { type: \"json\" };",
                Some("./a"),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(static_specifier(text).as_deref(), expected, "{text}");
        }
    }

    #[test]
    fn only_a_literal_dynamic_import_names_a_module() {
        assert_eq!(dynamic_specifier("import(\"./a\")").as_deref(), Some("./a"));
        assert_eq!(
            dynamic_specifier("import ('./a', {})").as_deref(),
            Some("./a")
        );
        assert_eq!(dynamic_specifier("import(name)"), None);
        assert_eq!(dynamic_specifier("import(\"./a\" + x)"), None);
        assert_eq!(dynamic_specifier("import(`./a`)"), None);
    }
}
