//! The `**Reviewed by:**` line of a rule body: the code-review profiles
//! that rule on a judge-only rule (`work.code-review-profiles`).

const MARKER: &str = "**Reviewed by:**";

/// The profiles a `**Reviewed by:**` line may name.
pub(crate) const KNOWN_PROFILES: [&str; 8] = [
    "backend",
    "language",
    "platform",
    "unix",
    "tui",
    "react",
    "architect",
    "ui-design",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    UnknownProfile(String),
    Empty,
    MoreThanOneLine,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownProfile(profile) => {
                write!(out, "unknown `{MARKER}` profile `{profile}`")
            }
            ParseError::Empty => write!(out, "`{MARKER}` line has an empty profile entry"),
            ParseError::MoreThanOneLine => write!(out, "more than one `{MARKER}` line"),
        }
    }
}

/// The profiles of the body's single `**Reviewed by:**` line, or `None`
/// when the body has no such line.
pub(crate) fn parse(body: &str) -> Result<Option<Vec<String>>, ParseError> {
    let mut lines = body
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix(MARKER));
    let Some(line) = lines.next() else {
        return Ok(None);
    };
    if lines.next().is_some() {
        return Err(ParseError::MoreThanOneLine);
    }
    let line = line.trim();
    let line = line.strip_suffix('.').unwrap_or(line);
    let mut profiles = Vec::new();
    for name in line.split(',').map(str::trim) {
        if name.is_empty() {
            return Err(ParseError::Empty);
        }
        if !KNOWN_PROFILES.contains(&name) {
            return Err(ParseError::UnknownProfile(name.to_string()));
        }
        profiles.push(name.to_string());
    }
    Ok(Some(profiles))
}

#[cfg(test)]
mod tests {
    use super::*;

    type Parsed = Result<Option<Vec<String>>, ParseError>;

    #[test]
    fn named_table_of_lines() {
        let table: Vec<(&str, &str, Parsed)> = vec![
            ("no line", "prose only", Ok(None)),
            (
                "two profiles",
                "**Checked by:** judge.\n\n**Reviewed by:** unix, tui.",
                Ok(Some(vec!["unix".into(), "tui".into()])),
            ),
            (
                "unknown profile",
                "**Reviewed by:** nobody.",
                Err(ParseError::UnknownProfile("nobody".into())),
            ),
            ("empty", "**Reviewed by:** .", Err(ParseError::Empty)),
            (
                "twice",
                "**Reviewed by:** unix.\n**Reviewed by:** tui.",
                Err(ParseError::MoreThanOneLine),
            ),
        ];
        for (name, body, expected) in table {
            assert_eq!(parse(body), expected, "{name}");
        }
    }
}
