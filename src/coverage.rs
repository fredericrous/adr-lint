//! `adr-lint coverage`: every active code constraint says how it is
//! checked, and the map names a mechanism for each exact or candidate
//! class it lists (guidance.code-rules-checked-by-fit).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::aval;
use crate::checked_by::{self, Check, Class};
use crate::reviewed_by;

const AVAL_PROGRAM_VARIABLE: &str = "ADR_LINT_AVAL";
const DEFAULT_AVAL_PROGRAM: &str = "aval";
const DEFAULT_MAP: &str = "map/rules.toml";

pub(crate) const USAGE: &str = "\
usage: adr-lint coverage [-C <dir>] [--map <path>] [--json]

Reads the active governs:code constraints from `aval --json rules --level
constraint --governs code --full` and checks that each has a `Checked by:`
line, that each exact or candidate class on it has a mechanism in the map,
that a judge-only rule has exactly one `Reviewed by:` line naming known
profiles and any other rule has none, and that every id in the map is an
active rule.

flags:
  -C <dir>        run aval in <dir> and read <dir>/map/rules.toml (default .)
  --map <path>    the rules map (default map/rules.toml under -C)
  --json          print the problems as a JSON array of {rule, problem}
  -h, --help      this text

environment:
  ADR_LINT_AVAL   the aval program (default: aval from PATH)

exit status:
  0  no problems
  1  problems, one `<rule-id>: <problem>` line each on stdout
  2  usage error
  3  could not run (aval missing, failing, timing out or not printing the
     expected JSON; the map unreadable)
";

/// What a rule body says about itself, as aval printed it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RuleBody {
    pub(crate) id: String,
    pub(crate) body: String,
}

/// The mechanisms the map names for one rule.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Mechanisms {
    pub(crate) exact: Option<String>,
    pub(crate) candidate: Option<String>,
}

pub(crate) type RuleMap = BTreeMap<String, Mechanisms>;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Problem {
    pub(crate) rule: String,
    pub(crate) problem: String,
}

/// Why the command stopped before it could judge.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    Usage(String),
    CouldNotRun(String),
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    dir: PathBuf,
    map: Option<PathBuf>,
    json: bool,
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options {
        dir: PathBuf::from("."),
        map: None,
        json: false,
    };
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let mut value_of = |flag: &str, attached: Option<&str>| match attached {
            Some(value) if !value.is_empty() => Ok(value.to_string()),
            _ => remaining
                .next()
                .cloned()
                .ok_or_else(|| Failure::Usage(format!("`{flag}` needs a value"))),
        };
        if arg == "--json" {
            options.json = true;
        } else if arg == "-C" || (arg.starts_with("-C") && !arg.starts_with("--")) {
            options.dir = PathBuf::from(value_of("-C", arg.strip_prefix("-C"))?);
        } else if arg == "--map" || arg.starts_with("--map=") {
            options.map = Some(PathBuf::from(value_of(
                "--map",
                arg.strip_prefix("--map="),
            )?));
        } else if arg.starts_with('-') {
            return Err(Failure::Usage(format!("unknown flag `{arg}`")));
        } else {
            return Err(Failure::Usage(format!("unexpected argument `{arg}`")));
        }
    }
    Ok(options)
}

/// Runs the command; the printed problems are the `Ok` value.
pub(crate) fn run(args: &[String]) -> Result<Vec<Problem>, Failure> {
    let options = parse_options(args)?;
    let problems = execute(&options)?;
    if options.json {
        println!("{}", problems_as_json(&problems));
    } else {
        for problem in &problems {
            println!("{}: {}", problem.rule, problem.problem);
        }
    }
    Ok(problems)
}

fn execute(options: &Options) -> Result<Vec<Problem>, Failure> {
    let program =
        std::env::var(AVAL_PROGRAM_VARIABLE).unwrap_or_else(|_| DEFAULT_AVAL_PROGRAM.to_string());
    let printed = aval::code_constraints(&program, &options.dir).map_err(Failure::CouldNotRun)?;
    let rules = rules_from_json(&printed).map_err(Failure::CouldNotRun)?;
    let map_path = options
        .map
        .clone()
        .unwrap_or_else(|| options.dir.join(DEFAULT_MAP));
    let map_text = std::fs::read_to_string(&map_path).map_err(|cause| {
        Failure::CouldNotRun(format!("cannot read map {}: {cause}", map_path.display()))
    })?;
    let map = map_from_toml(&map_text)
        .map_err(|cause| Failure::CouldNotRun(format!("map {}: {cause}", map_path.display())))?;
    Ok(check(&rules, &map))
}

/// The active rules of aval's `{"rules": [...]}` output.
pub(crate) fn rules_from_json(printed: &str) -> Result<Vec<RuleBody>, String> {
    let document: serde_json::Value =
        serde_json::from_str(printed).map_err(|cause| format!("aval printed no JSON: {cause}"))?;
    let entries = document
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .ok_or("aval JSON has no `rules` array")?;
    let mut rules = Vec::new();
    for entry in entries {
        let text = |field: &str| entry.get(field).and_then(serde_json::Value::as_str);
        let id = text("id").ok_or("an aval rule has no `id` string")?;
        let active = entry
            .get("active")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("aval rule `{id}` has no `active` boolean"))?;
        let body = text("body").ok_or_else(|| format!("aval rule `{id}` has no `body`"))?;
        if active {
            rules.push(RuleBody {
                id: id.to_string(),
                body: body.to_string(),
            });
        }
    }
    Ok(rules)
}

/// `[rules."<id>"]` tables with optional `exact` and `candidate` strings.
pub(crate) fn map_from_toml(text: &str) -> Result<RuleMap, String> {
    let document: toml::Table = text.parse().map_err(|cause| format!("{cause}"))?;
    let mut map = RuleMap::new();
    let Some(rules) = document.get("rules") else {
        return Ok(map);
    };
    let rules = rules.as_table().ok_or("`rules` is not a table")?;
    for (id, entry) in rules {
        let entry = entry
            .as_table()
            .ok_or_else(|| format!("`rules.{id}` is not a table"))?;
        let mechanism = |field: &str| match entry.get(field) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(|text| Some(text.to_string()))
                .ok_or_else(|| format!("`rules.{id}.{field}` is not a string")),
        };
        map.insert(
            id.clone(),
            Mechanisms {
                exact: mechanism("exact")?,
                candidate: mechanism("candidate")?,
            },
        );
    }
    Ok(map)
}

/// The problems of the live rules against the map, in rule order and then
/// map order.
pub(crate) fn check(rules: &[RuleBody], map: &RuleMap) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut problem = |rule: &str, text: String| {
        problems.push(Problem {
            rule: rule.to_string(),
            problem: text,
        });
    };
    for rule in rules {
        match checked_by::parse(&rule.body) {
            Err(cause) => problem(&rule.id, cause.to_string()),
            Ok(checks) => {
                for Check { class, .. } in &checks {
                    let mapped = map.get(&rule.id);
                    let mechanism = match class {
                        Class::Exact => mapped.and_then(|found| found.exact.as_ref()),
                        Class::Candidate => mapped.and_then(|found| found.candidate.as_ref()),
                        Class::Judge => continue,
                    };
                    if mechanism.is_none() {
                        problem(
                            &rule.id,
                            format!("{} class has no mechanism in the map", class.name()),
                        );
                    }
                }
                let judge_only = checks.iter().all(|found| found.class == Class::Judge);
                match (reviewed_by::parse(&rule.body), judge_only) {
                    (Err(cause), _) => problem(&rule.id, cause.to_string()),
                    (Ok(None), true) => problem(
                        &rule.id,
                        "judge-only rule has no `**Reviewed by:**` line".to_string(),
                    ),
                    (Ok(Some(_)), false) => problem(
                        &rule.id,
                        "rule with an exact or candidate class has a `**Reviewed by:**` line"
                            .to_string(),
                    ),
                    (Ok(None), false) | (Ok(Some(_)), true) => {}
                }
            }
        }
    }
    let active: BTreeSet<&str> = rules.iter().map(|rule| rule.id.as_str()).collect();
    for id in map.keys().filter(|id| !active.contains(id.as_str())) {
        problem(id, "in the map but not an active rule".to_string());
    }
    problems
}

fn problems_as_json(problems: &[Problem]) -> serde_json::Value {
    problems
        .iter()
        .map(|found| serde_json::json!({ "rule": found.rule, "problem": found.problem }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, body: &str) -> RuleBody {
        RuleBody {
            id: id.to_string(),
            body: body.to_string(),
        }
    }

    fn mapped(exact: Option<&str>, candidate: Option<&str>) -> Mechanisms {
        Mechanisms {
            exact: exact.map(str::to_string),
            candidate: candidate.map(str::to_string),
        }
    }

    #[test]
    fn problems_of_a_named_table() {
        let rules = vec![
            rule("a.judged", "**Checked by:** judge.\n**Reviewed by:** unix."),
            rule("b.exact", "**Checked by:** exact (fmt), judge."),
            rule("c.candidate", "**Checked by:** candidate (grep)."),
            rule("d.silent", "no line"),
            rule("e.odd", "**Checked by:** vibes."),
        ];
        let mut map = RuleMap::new();
        map.insert("b.exact".into(), mapped(Some("cargo-fmt"), None));
        map.insert("c.candidate".into(), mapped(Some("only exact"), None));
        map.insert("z.gone".into(), mapped(None, None));
        let found: Vec<(String, String)> = check(&rules, &map)
            .into_iter()
            .map(|p| (p.rule, p.problem))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    "c.candidate".into(),
                    "candidate class has no mechanism in the map".into()
                ),
                (
                    "d.silent".into(),
                    "no `**Checked by:**` line in the rule body".into()
                ),
                ("e.odd".into(), "unknown class `vibes`".into()),
                ("z.gone".into(), "in the map but not an active rule".into()),
            ]
        );
    }

    #[test]
    fn reviewed_by_line_follows_the_checked_by_classes() {
        let rules = vec![
            rule("a.bare", "**Checked by:** judge."),
            rule(
                "b.nobody",
                "**Checked by:** judge.\n**Reviewed by:** nobody.",
            ),
            rule(
                "c.exact",
                "**Checked by:** exact (fmt).\n**Reviewed by:** unix.",
            ),
            rule(
                "d.mixed",
                "**Checked by:** candidate (grep), judge (fit).\n**Reviewed by:** tui.",
            ),
            rule(
                "e.fine",
                "**Checked by:** judge.\n**Reviewed by:** unix, tui.",
            ),
            rule(
                "f.twice",
                "**Checked by:** judge.\n**Reviewed by:** unix.\n**Reviewed by:** tui.",
            ),
        ];
        let mut map = RuleMap::new();
        map.insert("c.exact".into(), mapped(Some("fmt"), None));
        map.insert("d.mixed".into(), mapped(None, Some("grep")));
        let found: Vec<(String, String)> = check(&rules, &map)
            .into_iter()
            .map(|p| (p.rule, p.problem))
            .collect();
        let exact_or_candidate =
            "rule with an exact or candidate class has a `**Reviewed by:**` line";
        assert_eq!(
            found,
            vec![
                (
                    "a.bare".into(),
                    "judge-only rule has no `**Reviewed by:**` line".into()
                ),
                (
                    "b.nobody".into(),
                    "unknown `**Reviewed by:**` profile `nobody`".into()
                ),
                ("c.exact".into(), exact_or_candidate.into()),
                ("d.mixed".into(), exact_or_candidate.into()),
                (
                    "f.twice".into(),
                    "more than one `**Reviewed by:**` line".into()
                ),
            ]
        );
    }

    #[test]
    fn inactive_rules_are_not_rules() {
        let printed = r#"{"rules":[
            {"id":"a","active":true,"body":"**Checked by:** judge."},
            {"id":"b","active":false,"body":""}]}"#;
        let rules = rules_from_json(printed).expect("decodes");
        assert_eq!(rules, vec![rule("a", "**Checked by:** judge.")]);
        assert!(rules_from_json("not json").is_err());
        assert!(rules_from_json("{}").is_err());
    }

    #[test]
    fn map_reads_mechanisms_and_rejects_wrong_types() {
        let map =
            map_from_toml("[rules.\"a.b\"]\nexact = \"x\"\nmisses = \"y\"\n").expect("parses");
        assert_eq!(map["a.b"], mapped(Some("x"), None));
        assert!(map_from_toml("[rules.\"a.b\"]\nexact = 3\n").is_err());
        assert!(map_from_toml("= broken").is_err());
    }

    #[test]
    fn options_parse_both_value_forms() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let options = parse_options(&args(&["-C", "d", "--map=m.toml", "--json"])).unwrap();
        assert_eq!(options.dir, PathBuf::from("d"));
        assert_eq!(options.map, Some(PathBuf::from("m.toml")));
        assert!(options.json);
        assert_eq!(
            parse_options(&args(&["-Cd"])).unwrap().dir,
            PathBuf::from("d")
        );
        assert!(parse_options(&args(&["--bogus"])).is_err());
        assert!(parse_options(&args(&["-C"])).is_err());
    }
}
