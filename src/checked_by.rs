//! The `**Checked by:**` line of a rule body: which classes of mechanism
//! check the rule, and the mechanism each names (ADR-0044).

const MARKER: &str = "**Checked by:**";

/// How a rule is checked (guidance.code-rules-checked-by-fit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Exact,
    Candidate,
    Judge,
}

impl Class {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Class::Exact => "exact",
            Class::Candidate => "candidate",
            Class::Judge => "judge",
        }
    }
}

/// One class on the line, with the text of its parenthesised mechanism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Check {
    pub(crate) class: Class,
    pub(crate) mechanism: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    MissingLine,
    UnknownClass(String),
    Malformed(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingLine => write!(out, "no `{MARKER}` line in the rule body"),
            ParseError::UnknownClass(class) => write!(out, "unknown class `{class}`"),
            ParseError::Malformed(item) => write!(out, "malformed `{MARKER}` item `{item}`"),
        }
    }
}

/// Parses the first `**Checked by:**` line of a rule body.
pub(crate) fn parse(body: &str) -> Result<Vec<Check>, ParseError> {
    let line = body
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(MARKER))
        .ok_or(ParseError::MissingLine)?;
    let line = line.trim();
    let line = line.strip_suffix('.').unwrap_or(line);
    split_top_level(line)
        .iter()
        .map(|item| parse_item(item.trim()))
        .collect()
}

/// Splits at commas outside parentheses.
fn split_top_level(line: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, character) in line.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&line[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    items.push(&line[start..]);
    items
}

fn parse_item(item: &str) -> Result<Check, ParseError> {
    let word_end = item
        .find(|character: char| !character.is_ascii_alphabetic())
        .unwrap_or(item.len());
    let (word, rest) = item.split_at(word_end);
    let class = match word {
        "exact" => Class::Exact,
        "candidate" => Class::Candidate,
        "judge" => Class::Judge,
        "" => return Err(ParseError::Malformed(item.to_string())),
        unknown => return Err(ParseError::UnknownClass(unknown.to_string())),
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(Check {
            class,
            mechanism: None,
        });
    }
    match rest
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
    {
        Some(mechanism) => Ok(Check {
            class,
            mechanism: Some(mechanism.to_string()),
        }),
        None => Err(ParseError::Malformed(item.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Parsed = Result<Vec<Check>, ParseError>;

    fn check(class: Class, mechanism: Option<&str>) -> Check {
        Check {
            class,
            mechanism: mechanism.map(str::to_string),
        }
    }

    #[test]
    fn named_table_of_lines() {
        let table: Vec<(&str, &str, Parsed)> = vec![
            (
                "single class",
                "prose\n\n**Checked by:** exact (cargo-fmt).",
                Ok(vec![check(Class::Exact, Some("cargo-fmt"))]),
            ),
            (
                "several classes",
                "**Checked by:** candidate (a sleep), judge (whether it is fine).",
                Ok(vec![
                    check(Class::Candidate, Some("a sleep")),
                    check(Class::Judge, Some("whether it is fine")),
                ]),
            ),
            (
                "bare judge",
                "**Checked by:** judge.",
                Ok(vec![check(Class::Judge, None)]),
            ),
            (
                "commas inside parentheses",
                "**Checked by:** exact (a, b, `f(x, y)`), judge.",
                Ok(vec![
                    check(Class::Exact, Some("a, b, `f(x, y)`")),
                    check(Class::Judge, None),
                ]),
            ),
            (
                "unknown class",
                "**Checked by:** vibes (feel).",
                Err(ParseError::UnknownClass("vibes".to_string())),
            ),
            (
                "missing line",
                "Just prose, no marker.",
                Err(ParseError::MissingLine),
            ),
            (
                "trailing punctuation without a period",
                "**Checked by:** judge",
                Ok(vec![check(Class::Judge, None)]),
            ),
            (
                "unbalanced mechanism",
                "**Checked by:** exact (oops.",
                Err(ParseError::Malformed("exact (oops".to_string())),
            ),
        ];
        for (name, body, expected) in table {
            assert_eq!(parse(body), expected, "{name}");
        }
    }
}
