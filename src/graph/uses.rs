//! A `use` declaration's text expanded to one leaf per imported path: brace
//! groups flattened, `as` aliases and `*` globs kept, a trailing `self`
//! dropped.

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Leaf {
    pub(super) segments: Vec<String>,
    pub(super) glob: bool,
    pub(super) alias: Option<String>,
    /// Byte offset in the declaration's text of the leaf's first token.
    pub(super) offset: usize,
}

pub(super) fn expand(text: &str) -> Vec<Leaf> {
    let mut cursor = Cursor { text, position: 0 };
    cursor.skip_trivia();
    if cursor.eat_word("pub") {
        cursor.skip_trivia();
        if cursor.peek() == Some(b'(') {
            while let Some(next) = cursor.rest().chars().next() {
                cursor.position += next.len_utf8();
                if next == ')' {
                    break;
                }
            }
        }
    }
    cursor.skip_trivia();
    if !cursor.eat_word("use") {
        return Vec::new();
    }
    cursor.tree(&[])
}

struct Cursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.position).copied()
    }

    fn skip_trivia(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.position += rest.len() - trimmed.len();
            if trimmed.starts_with("//") {
                self.position += trimmed.find('\n').unwrap_or(trimmed.len());
            } else if trimmed.starts_with("/*") {
                self.position += trimmed.find("*/").map_or(trimmed.len(), |end| end + 2);
            } else {
                return;
            }
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let rest = self.rest();
        let boundary = rest
            .get(word.len()..)
            .and_then(|after| after.chars().next())
            .is_none_or(|next| !is_identifier_character(next));
        if rest.starts_with(word) && boundary {
            self.position += word.len();
            true
        } else {
            false
        }
    }

    fn identifier(&mut self) -> String {
        let rest = self.rest();
        let length = rest
            .find(|character: char| !is_identifier_character(character))
            .unwrap_or(rest.len());
        self.position += length;
        rest[..length].to_string()
    }

    fn tree(&mut self, prefix: &[String]) -> Vec<Leaf> {
        self.skip_trivia();
        let start = self.position;
        let mut segments = prefix.to_vec();
        if self.rest().starts_with("::") {
            self.position += 2;
        }
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(b'{') => {
                    self.position += 1;
                    return self.group(&segments);
                }
                Some(b'*') => {
                    self.position += 1;
                    return vec![Leaf {
                        segments,
                        glob: true,
                        alias: None,
                        offset: start,
                    }];
                }
                _ => {}
            }
            let name = self.identifier();
            if name.is_empty() {
                return Vec::new();
            }
            segments.push(name);
            self.skip_trivia();
            if self.rest().starts_with("::") {
                self.position += 2;
                continue;
            }
            let alias = if self.eat_word("as") {
                self.skip_trivia();
                Some(self.identifier())
            } else {
                None
            };
            if segments.len() > 1 && segments.last().is_some_and(|last| last == "self") {
                segments.pop();
            }
            return vec![Leaf {
                segments,
                glob: false,
                alias,
                offset: start,
            }];
        }
    }

    fn group(&mut self, prefix: &[String]) -> Vec<Leaf> {
        let mut leaves = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                None => return leaves,
                Some(b'}') => {
                    self.position += 1;
                    return leaves;
                }
                Some(b',') => self.position += 1,
                Some(_) => {
                    let before = self.position;
                    leaves.extend(self.tree(prefix));
                    if self.position == before {
                        self.position += self.rest().chars().next().map_or(1, char::len_utf8);
                    }
                }
            }
        }
    }
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '#'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(text: &str) -> Vec<String> {
        expand(text)
            .iter()
            .map(|leaf| {
                let path = leaf.segments.join("::");
                if leaf.glob {
                    format!("{path}::*")
                } else {
                    path
                }
            })
            .collect()
    }

    #[test]
    fn brace_groups_are_expanded_to_one_leaf_per_path() {
        assert_eq!(
            written("use crate::{b::x, c::{y, z as w}, d::*};"),
            ["crate::b::x", "crate::c::y", "crate::c::z", "crate::d::*"]
        );
        assert_eq!(
            written("pub(crate) use super::a::{self, B};"),
            ["super::a", "super::a::B"]
        );
        assert_eq!(written("use ::std::fs;"), ["std::fs"]);
    }

    #[test]
    fn a_multi_byte_character_in_a_group_does_not_panic() {
        assert_eq!(written("use a::{→, b};"), ["a::b"]);
        assert_eq!(written("use a::{b, → c};"), ["a::b", "a::c"]);
        assert_eq!(written("pub(crate →) use a::b;"), ["a::b"]);
    }

    #[test]
    fn an_alias_is_the_bound_name() {
        assert_eq!(expand("use a::b as c;")[0].alias.as_deref(), Some("c"));
        assert_eq!(expand("use a::b;")[0].alias, None);
    }

    #[test]
    fn a_leaf_offset_points_at_its_first_token() {
        let text = "use crate::{\n    b::x,\n    c::y,\n};";
        let leaves = expand(text);
        assert_eq!(&text[leaves[1].offset..leaves[1].offset + 4], "c::y");
    }
}
