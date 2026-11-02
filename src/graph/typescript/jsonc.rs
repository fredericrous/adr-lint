//! JSON with comments and trailing commas, as `tsconfig.json` is written,
//! reduced to plain JSON for serde_json. A comment marker inside a string
//! (`"a//b"`, `"~/*"`) is text, so the scan tracks strings and escapes.

/// `text` without `//` and `/* */` comments and without commas that are
/// followed only by whitespace, comments and a closing `}` or `]`.
pub(super) fn to_json(text: &str) -> String {
    let without_comments = strip_comments(text);
    drop_trailing_commas(&without_comments)
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                out.push(character);
                out.push_str(&string_rest(&mut chars));
            }
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        out.push('\n');
                    }
                    if previous == '*' && skipped == '/' {
                        break;
                    }
                    previous = skipped;
                }
                out.push(' ');
            }
            _ => out.push(character),
        }
    }
    out
}

/// The rest of a string whose opening quote was already read, through its
/// closing quote, honouring backslash escapes.
fn string_rest(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut rest = String::new();
    while let Some(character) = chars.next() {
        rest.push(character);
        match character {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    rest.push(escaped);
                }
            }
            '"' => break,
            _ => {}
        }
    }
    rest
}

fn drop_trailing_commas(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '"' {
            out.push(character);
            index += 1;
            while index < characters.len() {
                let inside = characters[index];
                out.push(inside);
                index += 1;
                if inside == '\\' && index < characters.len() {
                    out.push(characters[index]);
                    index += 1;
                } else if inside == '"' {
                    break;
                }
            }
            continue;
        }
        if character == ',' {
            let next = characters[index + 1..]
                .iter()
                .find(|following| !following.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                index += 1;
                continue;
            }
        }
        out.push(character);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(&to_json(text)).expect("plain JSON after stripping")
    }

    #[test]
    fn a_glob_with_a_slash_and_a_star_is_not_a_comment() {
        let config = parsed(r#"{ "paths": { "~/*": ["./app/*"] } }"#);
        assert_eq!(config["paths"]["~/*"][0], "./app/*");
    }

    #[test]
    fn a_double_slash_inside_a_string_stays() {
        assert_eq!(parsed(r#"{ "a//b": "c//d" }"#)["a//b"], "c//d");
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert_eq!(parsed(r#"{ "k": "x\" // y" }"#)["k"], "x\" // y");
    }

    #[test]
    fn line_and_block_comments_go() {
        let config = parsed("{\n // one\n \"a\": 1, /* two\n three */ \"b\": 2 }");
        assert_eq!(config["a"], 1);
        assert_eq!(config["b"], 2);
    }

    #[test]
    fn trailing_commas_go_in_objects_and_arrays() {
        let config = parsed("{ \"a\": [1, 2, ], \"b\": {\"c\": 3,}, // end\n }");
        assert_eq!(config["a"][1], 2);
        assert_eq!(config["b"]["c"], 3);
    }

    #[test]
    fn a_comma_inside_a_string_before_a_bracket_stays() {
        assert_eq!(parsed(r#"{ "a": ",}" }"#)["a"], ",}");
    }
}
