//! `adr-lint learn`: reports what reviewers' `fine` rulings say about each
//! pattern (ADR-0044 phase 3). It reads ruling records and writes nothing.

use std::collections::BTreeMap;

use crate::coverage::Failure;
use crate::ruling;

pub(crate) const USAGE: &str = "\
usage: adr-lint learn [--since <YYYY-MM-DD>] --from <file>...

Reads files of `adr-lint.ruling/1` records (docs/formats.md) and reports, per
pattern id, how many rulings were `violation` and how many `fine`, and the
`fine` clauses that recur. It involves no model and never writes anything:
a proposal is printed, not applied.

Records are keyed by (review, candidate); the last record for a key wins, in
file order and then in the order the files are given. `--since` drops records
whose `at` is earlier than that UTC date.

output, on stdout:
  <pattern_id>: violation <n>, fine <n>
    fine x<count>: <clause>            a clause recurring at least 5 times
  proposal: narrow <pattern_id> or clarify <rule> body
                                       a clause recurring at least 5 times
    ruled: tree <tree> candidate <id>  each ruling behind that clause: the
                                       ruled lines to draw a fixture from
  retire candidate: <rule> (<fine> of <total> rulings fine, <percent>%)
                                       a rule with at least 50 rulings of
                                       which more than 90% are `fine`

flags:
  --from <file>...   ruling-record files (repeatable; every following
                     non-flag word is one more file)
  --since <date>     only records at or after this date (also --since=<date>)
  -h, --help         this text

exit status:
  0  the report was printed
  2  usage error
  3  a file cannot be read or holds an invalid record (nothing is printed)
";

const RECURRING_CLAUSE_COUNT: usize = 5;
const RETIRE_MIN_RULINGS: usize = 50;
const RETIRE_FINE_PERCENT_ABOVE: usize = 90;
const DATE_LENGTH: usize = 10;

struct Options {
    since: Option<String>,
    files: Vec<String>,
}

struct Ruling {
    tree: String,
    candidate: String,
    rule: String,
    pattern_id: String,
    fine: bool,
    clause: String,
}

pub(crate) fn run(args: &[String]) -> Result<String, Failure> {
    let options = parse_options(args)?;
    let mut texts = Vec::new();
    for path in &options.files {
        let text = ruling::read_checked(path)?
            .map_err(|problem| Failure::CouldNotRun(format!("{path}: {problem}")))?;
        texts.push(text);
    }
    let rulings = latest_rulings(&texts, options.since.as_deref());
    Ok(report(&rulings))
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let mut since = None;
    let mut files = Vec::new();
    let mut reading_files = false;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let (flag, attached) = match arg.split_once('=') {
            Some((flag, attached_text)) if flag.starts_with("--") => (flag, Some(attached_text)),
            _ => (arg.as_str(), None),
        };
        match flag {
            "--since" => {
                let given = attached
                    .map(str::to_string)
                    .or_else(|| remaining.next().cloned())
                    .ok_or_else(|| Failure::Usage("`--since` needs a date".to_string()))?;
                if !is_date(&given) {
                    return Err(Failure::Usage(format!(
                        "`--since` is `{given}`, want a date as YYYY-MM-DD"
                    )));
                }
                since = Some(given);
                reading_files = false;
            }
            "--from" => {
                reading_files = true;
                if let Some(file) = attached {
                    files.push(file.to_string());
                }
            }
            _ if arg.starts_with('-') => {
                return Err(Failure::Usage(format!("unknown flag `{arg}`")))
            }
            _ if reading_files => files.push(arg.clone()),
            _ => return Err(Failure::Usage(format!("unexpected argument `{arg}`"))),
        }
    }
    if files.iter().any(String::is_empty) {
        return Err(Failure::Usage("`--from` needs a file".to_string()));
    }
    if files.is_empty() {
        return Err(Failure::Usage("`--from <file>...` is required".to_string()));
    }
    Ok(Options { since, files })
}

fn is_date(text: &str) -> bool {
    let shape = b"0000-00-00";
    let fits_shape = text.len() == DATE_LENGTH
        && text.bytes().zip(shape).all(|(found, want)| {
            if *want == b'0' {
                found.is_ascii_digit()
            } else {
                found == *want
            }
        });
    if !fits_shape {
        return false;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().unwrap_or(u32::MAX);
    (1..=12).contains(&number(5..7)) && (1..=31).contains(&number(8..10))
}

/// The last record per (review, candidate), over texts already checked by
/// `ruling::read_checked`, in the order given; records before `since` are
/// dropped first.
fn latest_rulings(texts: &[String], since: Option<&str>) -> Vec<Ruling> {
    let mut by_key: BTreeMap<(String, String), Ruling> = BTreeMap::new();
    for line in texts.iter().flat_map(|text| text.lines()) {
        let Ok(serde_json::Value::Object(record)) = serde_json::from_str(line) else {
            continue;
        };
        let text_of = |field: &str| {
            record
                .get(field)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let at = text_of("at");
        if since.is_some_and(|date| at.get(..DATE_LENGTH).is_none_or(|day| day < date)) {
            continue;
        }
        by_key.insert(
            (text_of("review"), text_of("candidate")),
            Ruling {
                tree: text_of("tree"),
                candidate: text_of("candidate"),
                rule: text_of("rule"),
                pattern_id: text_of("pattern_id"),
                fine: text_of("ruling") == "fine",
                clause: text_of("clause"),
            },
        );
    }
    by_key.into_values().collect()
}

#[derive(Default)]
struct PatternTally {
    rule: String,
    violations: usize,
    fine: usize,
    /// Each `fine` clause, with the (tree, candidate) of the rulings that
    /// quoted it: the ruled lines a narrower pattern's fixture is drawn from.
    fine_clauses: BTreeMap<String, Vec<(String, String)>>,
}

fn report(rulings: &[Ruling]) -> String {
    let mut patterns: BTreeMap<&str, PatternTally> = BTreeMap::new();
    let mut rules: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for ruling in rulings {
        let tally = patterns.entry(&ruling.pattern_id).or_default();
        tally.rule.clone_from(&ruling.rule);
        let rule_tally = rules.entry(&ruling.rule).or_default();
        rule_tally.0 += 1;
        if ruling.fine {
            tally.fine += 1;
            rule_tally.1 += 1;
            tally
                .fine_clauses
                .entry(ruling.clause.clone())
                .or_default()
                .push((ruling.tree.clone(), ruling.candidate.clone()));
        } else {
            tally.violations += 1;
        }
    }
    let mut lines = Vec::new();
    for (pattern_id, tally) in &patterns {
        lines.push(format!(
            "{pattern_id}: violation {}, fine {}",
            tally.violations, tally.fine
        ));
        let recurring: Vec<_> = tally
            .fine_clauses
            .iter()
            .filter(|(_, ruled)| ruled.len() >= RECURRING_CLAUSE_COUNT)
            .collect();
        for (clause, ruled) in &recurring {
            lines.push(format!("  fine x{}: {clause}", ruled.len()));
        }
        if !recurring.is_empty() {
            lines.push(format!(
                "proposal: narrow {pattern_id} or clarify {} body",
                tally.rule
            ));
            for (tree, candidate) in recurring.iter().flat_map(|(_, ruled)| ruled.iter()) {
                lines.push(format!("  ruled: tree {tree} candidate {candidate}"));
            }
        }
    }
    for (rule, (total, fine)) in &rules {
        if *total >= RETIRE_MIN_RULINGS && fine * 100 > RETIRE_FINE_PERCENT_ABOVE * total {
            lines.push(format!(
                "retire candidate: {rule} ({fine} of {total} rulings fine, {}%)",
                fine * 100 / total
            ));
        }
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}
