//! `adr-lint ruling-record --check`: validates a file of ruling records, the
//! JSONL a reviewer's rulings on candidates are written in (docs/formats.md,
//! decisions ADR-0044).

use crate::coverage::Failure;

pub(crate) const USAGE: &str = "\
usage: adr-lint ruling-record --check <file>

Reads a JSON Lines file of `adr-lint.ruling/1` records (docs/formats.md) and
checks every line: a JSON object with the format token, all fields present
and well formed (`tree` and `candidate` are 64 lowercase hex digits, `ruling`
is `violation` or `fine`, `tool` is `amont-agent` or `relais`, `at` is an
RFC 3339 UTC time). It prints nothing when the file is valid, and the first
bad line number with the reason to stderr otherwise.

flags:
  --check <file>  the file to validate (also --check=<file>)
  -h, --help      this text

exit status:
  0  every line is a valid record
  1  a line is not; the first one is named
  2  usage error
  3  the file cannot be read
";

const RULING_FORMAT: &str = "adr-lint.ruling/1";
const TEXT_FIELDS: [&str; 10] = [
    "review",
    "repo",
    "rule",
    "pattern_id",
    "pattern",
    "clause",
    "tool",
    "ruling",
    "at",
    "format",
];
const HEX_FIELDS: [&str; 2] = ["tree", "candidate"];
const TOOLS: [&str; 2] = ["amont-agent", "relais"];
const RULINGS: [&str; 2] = ["violation", "fine"];
const HEX_LENGTH: usize = 64;

/// Reads the file named by `--check` and returns the first bad line as
/// `line <n>: <why>`, or `None` when every line is valid.
pub(crate) fn run(args: &[String]) -> Result<Option<String>, Failure> {
    let path = parse_options(args)?;
    Ok(read_checked(&path)?.err())
}

/// Reads the file at `path` once and checks that same text: `Ok(text)` when
/// every line is valid, `Err("line <n>: <why>")` for the first bad line.
pub(crate) fn read_checked(path: &str) -> Result<Result<String, String>, Failure> {
    let bytes = std::fs::read(path)
        .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))?;
    // A file that reads but is not UTF-8 holds a bad line, not an unreadable
    // file: name the first such line, as for any other bad line.
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(not_utf8) => {
            let valid_up_to = not_utf8.utf8_error().valid_up_to();
            let line = not_utf8.as_bytes()[..valid_up_to]
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count()
                + 1;
            return Ok(Err(format!("line {line}: not UTF-8")));
        }
    };
    Ok(match first_bad_line(&text) {
        Some(problem) => Err(problem),
        None => Ok(text),
    })
}

fn parse_options(args: &[String]) -> Result<String, Failure> {
    let mut path = None;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let (flag, attached) = match arg.split_once('=') {
            Some((flag, attached_text)) if flag.starts_with("--") => (flag, Some(attached_text)),
            _ => (arg.as_str(), None),
        };
        match flag {
            "--check" => {
                let given = attached
                    .map(str::to_string)
                    .or_else(|| remaining.next().cloned())
                    .filter(|given| !given.is_empty())
                    .ok_or_else(|| Failure::Usage("`--check` needs a file".to_string()))?;
                path = Some(given);
            }
            _ if arg.starts_with('-') => {
                return Err(Failure::Usage(format!("unknown flag `{arg}`")))
            }
            _ => return Err(Failure::Usage(format!("unexpected argument `{arg}`"))),
        }
    }
    path.ok_or_else(|| Failure::Usage("`--check <file>` is required".to_string()))
}

fn first_bad_line(text: &str) -> Option<String> {
    text.lines().enumerate().find_map(|(index, line)| {
        check_line(line)
            .err()
            .map(|why| format!("line {}: {why}", index + 1))
    })
}

fn check_line(line: &str) -> Result<(), String> {
    let document: serde_json::Value =
        serde_json::from_str(line).map_err(|cause| format!("not JSON: {cause}"))?;
    let record = document
        .as_object()
        .ok_or_else(|| "not a JSON object".to_string())?;
    let text_of = |field: &str| match record.get(field) {
        None => Err(format!("missing field `{field}`")),
        Some(serde_json::Value::String(text)) => Ok(text.as_str()),
        Some(_) => Err(format!("field `{field}` is not a string")),
    };
    let format = text_of("format")?;
    if format != RULING_FORMAT {
        return Err(format!(
            "unknown format token `{format}`, want `{RULING_FORMAT}`"
        ));
    }
    for field in TEXT_FIELDS.iter().chain(&HEX_FIELDS) {
        text_of(field)?;
    }
    for field in HEX_FIELDS {
        if !is_content_id(text_of(field)?) {
            return Err(format!("`{field}` is not 64 lowercase hex digits"));
        }
    }
    let tool = text_of("tool")?;
    if !TOOLS.contains(&tool) {
        return Err(format!("`tool` is `{tool}`, want amont-agent or relais"));
    }
    let ruling = text_of("ruling")?;
    if !RULINGS.contains(&ruling) {
        return Err(format!("`ruling` is `{ruling}`, want violation or fine"));
    }
    let at = text_of("at")?;
    if !is_utc_timestamp(at) {
        return Err(format!("`at` is `{at}`, not an RFC 3339 UTC time"));
    }
    Ok(())
}

fn is_content_id(text: &str) -> bool {
    text.len() == HEX_LENGTH
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, with the ranges of a real date and time.
fn is_utc_timestamp(text: &str) -> bool {
    let Some(body) = text.strip_suffix('Z') else {
        return false;
    };
    let (whole, fraction) = match body.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (body, None),
    };
    if fraction
        .is_some_and(|digits| digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let shape = b"0000-00-00T00:00:00";
    let fits_shape = whole.len() == shape.len()
        && whole.bytes().zip(shape).all(|(found, want)| {
            if *want == b'0' {
                found.is_ascii_digit()
            } else {
                found == *want
            }
        });
    if !fits_shape {
        return false;
    }
    let number = |range: std::ops::Range<usize>| whole[range].parse::<u32>().unwrap_or(u32::MAX);
    (1..=12).contains(&number(5..7))
        && (1..=31).contains(&number(8..10))
        && number(11..13) <= 23
        && number(14..16) <= 59
        && number(17..19) <= 60
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn record(edit: impl Fn(&mut serde_json::Value)) -> String {
        let mut value = serde_json::json!({
            "format": "adr-lint.ruling/1",
            "review": "agent-1",
            "tool": "relais",
            "repo": "adr-lint",
            "tree": TREE,
            "candidate": TREE,
            "rule": "names.reveal-intent",
            "pattern_id": "names.reveal-intent/rust-weak-name",
            "pattern": "a short name",
            "ruling": "fine",
            "clause": "a very short name is acceptable in a tiny scope",
            "at": "2026-10-24T12:00:00Z",
        });
        edit(&mut value);
        value.to_string()
    }

    #[test]
    fn each_line_of_the_table_is_judged_as_named() {
        let cases: Vec<(&str, String, Option<&str>)> = vec![
            ("a fine record", record(|_| {}), None),
            (
                "a violation with fractional seconds",
                record(|v| {
                    v["ruling"] = "violation".into();
                    v["at"] = "2026-10-24T12:00:00.25Z".into();
                }),
                None,
            ),
            (
                "an unknown format token",
                record(|v| v["format"] = "adr-lint.ruling/2".into()),
                Some("unknown format token"),
            ),
            (
                "a missing field",
                record(|v| {
                    v.as_object_mut().expect("object").remove("clause");
                }),
                Some("missing field `clause`"),
            ),
            (
                "a ruling that is neither value",
                record(|v| v["ruling"] = "maybe".into()),
                Some("`ruling` is `maybe`"),
            ),
            (
                "a short tree",
                record(|v| v["tree"] = "abc".into()),
                Some("`tree` is not 64 lowercase hex"),
            ),
            (
                "an upper-case candidate",
                record(|v| v["candidate"] = TREE.to_uppercase().into()),
                Some("`candidate` is not 64 lowercase hex"),
            ),
            (
                "an unknown tool",
                record(|v| v["tool"] = "other".into()),
                Some("`tool` is `other`"),
            ),
            (
                "a local time",
                record(|v| v["at"] = "2026-10-24T12:00:00+02:00".into()),
                Some("not an RFC 3339 UTC time"),
            ),
            (
                "a month 13",
                record(|v| v["at"] = "2026-13-24T12:00:00Z".into()),
                Some("not an RFC 3339 UTC time"),
            ),
            (
                "a field of the wrong type",
                record(|v| v["review"] = 7.into()),
                Some("`review` is not a string"),
            ),
            (
                "not an object",
                "[1]".to_string(),
                Some("not a JSON object"),
            ),
            ("not JSON", "{".to_string(), Some("not JSON")),
            ("an empty line", String::new(), Some("not JSON")),
        ];
        for (name, line, expected) in cases {
            match (check_line(&line), expected) {
                (Ok(()), None) => {}
                (Err(why), Some(part)) => assert!(why.contains(part), "{name}: {why}"),
                (found, _) => panic!("{name}: {found:?}"),
            }
        }
    }

    #[test]
    fn the_first_bad_line_is_named_by_number() {
        let good = record(|_| {});
        let text = format!("{good}\n{good}\n{{\n{good}\n");
        let found = first_bad_line(&text).expect("a bad line");
        assert!(found.starts_with("line 3: "), "{found}");
        assert_eq!(first_bad_line(&format!("{good}\n{good}\n")), None);
    }

    #[test]
    fn options_need_check_with_a_file() {
        let args = |list: &[&str]| -> Vec<String> { list.iter().map(|a| a.to_string()).collect() };
        assert_eq!(
            parse_options(&args(&["--check", "f"])).ok(),
            Some("f".into())
        );
        assert_eq!(parse_options(&args(&["--check=f"])).ok(), Some("f".into()));
        for bad in [&[][..], &["--check"], &["--bogus"], &["f"]] {
            assert!(
                matches!(parse_options(&args(bad)), Err(Failure::Usage(_))),
                "{bad:?}"
            );
        }
    }
}
