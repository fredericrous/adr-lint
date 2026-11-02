//! One ast-grep scan over every Python file: the `import` and `from … import`
//! statements, and literal `importlib.import_module("x")` calls, each with
//! its byte offset. A statement is flattened to one `Import` per module it
//! names.

use std::path::Path;

use crate::check::{ast_grep_failure, ast_grep_lines};
use crate::coverage::Failure;

use super::super::scan::clean;

/// How an import is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graph) enum Form {
    /// `import a.b`, `from a import b`.
    Static,
    /// `importlib.import_module("a.b")` with a literal name.
    Dynamic,
}

/// One module an import names: `from ..a import b` is level 2, module `a`,
/// name `b`; `from a import *` and `import a` have no name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::graph) struct Import {
    pub(in crate::graph) file: String,
    pub(in crate::graph) offset: usize,
    pub(in crate::graph) form: Form,
    pub(in crate::graph) level: usize,
    pub(in crate::graph) module: String,
    pub(in crate::graph) name: Option<String>,
}

impl Import {
    /// The import as the programmer wrote it, for a finding's detail.
    pub(in crate::graph) fn written(&self) -> String {
        let dots = ".".repeat(self.level);
        match &self.name {
            Some(name) if self.level > 0 || !self.module.is_empty() => {
                format!("from {dots}{} import {name}", self.module)
            }
            _ => format!("{dots}{}", self.module),
        }
    }
}

const RULES: &str = "id: graph.import\nlanguage: Python\nrule:\n  kind: import_statement\n\
---\nid: graph.from\nlanguage: Python\nrule:\n  kind: import_from_statement\n\
---\nid: graph.dynamic\nlanguage: Python\nrule:\n  pattern: importlib.import_module($$$ARGS)\n";

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
        let file = clean(file);
        let statements = match rule {
            "graph.dynamic" => dynamic(matched),
            "graph.from" => from_statement(matched),
            "graph.import" => import_statement(matched),
            unknown => {
                return Err(Failure::CouldNotRun(format!(
                    "adr-lint has no handler for its own ast-grep rule `{unknown}` (a bug in adr-lint)"
                )));
            }
        };
        found.extend(statements.into_iter().map(|statement| Import {
            file: file.clone(),
            offset,
            ..statement
        }));
    }
    found.sort_by(|first, second| {
        (&first.file, first.offset, &first.module, &first.name).cmp(&(
            &second.file,
            second.offset,
            &second.module,
            &second.name,
        ))
    });
    found.dedup();
    Ok(found)
}

/// A token of the statement text: a word, a punctuation character, or a
/// string literal's content.
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
        if character == '#' {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
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
        } else if character.is_alphanumeric() || character == '_' {
            let mut word = String::new();
            while index < characters.len()
                && (characters[index].is_alphanumeric() || characters[index] == '_')
            {
                word.push(characters[index]);
                index += 1;
            }
            out.push(Token::Word(word));
        } else {
            if !character.is_whitespace() && character != '\\' {
                out.push(Token::Punct(character));
            }
            index += 1;
        }
    }
    out
}

fn statement(level: usize, module: String, name: Option<String>, form: Form) -> Import {
    Import {
        file: String::new(),
        offset: 0,
        form,
        level,
        module,
        name,
    }
}

/// The dotted name at the start of `tokens`: words joined by dots, up to
/// the first token that is neither.
fn dotted(tokens: &[Token]) -> String {
    let mut name = String::new();
    for token in tokens {
        match token {
            Token::Word(word) if word != "as" => name.push_str(word),
            Token::Punct('.') => name.push('.'),
            Token::Word(_) | Token::Punct(_) | Token::Str(_) => break,
        }
    }
    name
}

/// `import a.b, c as d`: one import per comma-separated name.
fn import_statement(text: &str) -> Vec<Import> {
    let found = tokens(text);
    let Some((Token::Word(keyword), rest)) = found.split_first() else {
        return Vec::new();
    };
    if keyword != "import" {
        return Vec::new();
    }
    rest.split(|token| *token == Token::Punct(','))
        .map(dotted)
        .filter(|name| !name.is_empty())
        .map(|name| statement(0, name, None, Form::Static))
        .collect()
}

/// `from ..a import (b, c as d)`: one import per imported name, or one for
/// `*`.
fn from_statement(text: &str) -> Vec<Import> {
    let found = tokens(text);
    let Some((Token::Word(keyword), rest)) = found.split_first() else {
        return Vec::new();
    };
    if keyword != "from" {
        return Vec::new();
    }
    let level = rest
        .iter()
        .take_while(|token| **token == Token::Punct('.'))
        .count();
    let Some(split) = rest
        .iter()
        .position(|token| matches!(token, Token::Word(word) if word == "import"))
    else {
        return Vec::new();
    };
    let module = dotted(&rest[level..split]);
    let names = &rest[split + 1..];
    if names.contains(&Token::Punct('*')) {
        return vec![statement(level, module, None, Form::Static)];
    }
    names
        .split(|token| *token == Token::Punct(','))
        .filter_map(|group| {
            group.iter().find_map(|token| match token {
                Token::Word(word) => Some(word.clone()),
                Token::Punct(_) | Token::Str(_) => None,
            })
        })
        .map(|name| statement(level, module.clone(), Some(name), Form::Static))
        .collect()
}

/// `importlib.import_module("a.b")`; nothing for a computed or relative name.
fn dynamic(text: &str) -> Vec<Import> {
    let found = tokens(text);
    match found.as_slice() {
        [Token::Word(library), Token::Punct('.'), Token::Word(function), Token::Punct('('), Token::Str(name), Token::Punct(')' | ','), ..]
            if library == "importlib"
                && function == "import_module"
                && !name.is_empty()
                && !name.starts_with('.') =>
        {
            vec![statement(0, name.clone(), None, Form::Dynamic)]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(imports: Vec<Import>) -> Vec<String> {
        imports.iter().map(Import::written).collect()
    }

    #[test]
    fn import_statements_name_each_module() {
        assert_eq!(
            written(import_statement("import a.b, c as d")),
            ["a.b", "c"]
        );
    }

    #[test]
    fn from_statements_name_each_imported_name() {
        assert_eq!(
            written(from_statement(
                "from ..a import (\n    b,  # note\n    c as d,\n)"
            )),
            ["from ..a import b", "from ..a import c"]
        );
        assert_eq!(
            written(from_statement("from . import x")),
            ["from . import x"]
        );
        assert_eq!(written(from_statement("from a.b import *")), ["a.b"]);
        assert_eq!(
            written(from_statement("from .a import \\\n  b")),
            ["from .a import b"]
        );
    }

    #[test]
    fn only_a_literal_absolute_import_module_names_a_module() {
        assert_eq!(
            written(dynamic("importlib.import_module(\"a.b\")")),
            ["a.b"]
        );
        assert!(dynamic("importlib.import_module(name)").is_empty());
        assert!(dynamic("importlib.import_module(\".x\", __package__)").is_empty());
    }
}
