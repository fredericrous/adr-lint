//! `adr-lint check`: the exact ast-grep rules embedded in the binary, run
//! over the named, listed or staged files (guidance.code-rules-checked-by-fit).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{ErrorKind, IsTerminal, Write};
use std::path::Path;

use crate::aval;
use crate::candidates::normalize;
use crate::coverage::Failure;
use crate::process;

const AST_GREP_VARIABLE: &str = "ADR_LINT_AST_GREP";
const DEFAULT_AST_GREP: &str = "ast-grep";
/// The one ast-grep the embedded rules and their fixtures were written for,
/// written once in the repository-root file the Makefile and CI read too.
const AST_GREP_VERSION: &str = include_str!("../AST_GREP_VERSION");
const INSTALL_URL: &str = "https://github.com/ast-grep/ast-grep/releases";
const AVAL_VARIABLE: &str = "ADR_LINT_AVAL";
const DEFAULT_AVAL: &str = "aval";
const FILES_VARIABLE: &str = "AMONT_FILES";
const WAIVER_RULE: &str = "adr-lint.waiver-without-reason";
const WAIVER_RULE_STATEMENT: &str =
    "a waiver names no reason: write it as `adr-lint: allow <rule-id> -- <reason>`";
const WAIVER_MARKER: &str = "adr-lint: allow";
pub(crate) const SOURCE_EXTENSIONS: [&str; 7] = ["rs", "ts", "mts", "cts", "tsx", "go", "py"];
/// Files handed to one ast-grep run, so argv stays short.
const FILES_PER_RUN: usize = 200;

macro_rules! embedded {
    ($rule:literal, $language:literal) => {
        (
            $rule,
            include_str!(concat!("../rules/", $rule, "/", $language, ".yml")),
        )
    };
}

/// The ast-grep rule files by the aval rule they check. Each file holds one
/// or more rules whose ids start with `<aval rule id>/`.
const EMBEDDED: [(&str, &str); 12] = [
    embedded!("errors.never-swallowed", "typescript"),
    embedded!("errors.never-swallowed", "tsx"),
    embedded!("general.no-disabled-safety", "python"),
    embedded!("names.reveal-intent", "rust"),
    embedded!("names.reveal-intent", "typescript"),
    embedded!("names.reveal-intent", "tsx"),
    embedded!("names.reveal-intent", "go"),
    embedded!("names.reveal-intent", "python"),
    embedded!("tests.first-properties", "rust"),
    embedded!("tests.first-properties", "typescript"),
    embedded!("tests.first-properties", "tsx"),
    embedded!("tests.first-properties", "go"),
];

pub(crate) const USAGE: &str = "\
usage: adr-lint check [--rule <id>] [--json] [--no-color] [--] [PATH…]

Runs the exact ast-grep rules embedded in this binary over the files named,
else the newline-separated files in $AMONT_FILES, else the staged files
(`git diff --cached --name-only -z`). A path named in argv or in $AMONT_FILES
that is gone from the working tree and that git reports as deleted is skipped.
A directory is searched for source files (.rs .ts .mts .cts .tsx .go .py).

Findings print to stdout as `path:line:col: rule-id: statement`, the rule's
one-line statement as aval says it, grouped under one header line per rule that
ends with an `aval rule <id>` line; the count is the last line on stderr. A finding is waived by a comment on its line or the
line above: `adr-lint: allow <rule-id> -- <reason>`; a waiver with no reason is
itself a finding of rule adr-lint.waiver-without-reason.

flags:
  --rule <id>   only this rule (also --rule=<id>)
  --json        print the findings as a JSON array of
                {path, line, col, rule, message, detail, text}, and nothing
                else; message is the statement, detail the ast-grep message,
                text the finding's source line, whitespace trimmed and
                collapsed to single spaces (as candidate ids are computed)
  --no-color    no colour (also off when stdout is not a terminal, NO_COLOR is
                set, or TERM=dumb)
  -h, --help    this text

environment:
  ADR_LINT_AST_GREP  the ast-grep program (default: ast-grep from PATH); it
                     must be exactly the version in AST_GREP_VERSION
  ADR_LINT_AVAL      the aval program (default: aval from PATH), asked for
                     the rule statements when there are findings
  AMONT_FILES        the files to check, one per line

exit status:
  0  no findings (also a closed stdout)
  1  findings
  2  usage error (unknown flag, `--rule` with an unknown id)
  3  could not run (ast-grep or aval missing, failing, of another version or
     not printing the expected JSON; git failing; a named path or a directory
     entry that cannot be read)
";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    rule: Option<String>,
    json: bool,
    no_color: bool,
    paths: Vec<String>,
}

/// How the text report is dressed, chosen once from the terminal inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Ansi,
    Plain,
}

/// Everything `check` reads from the process environment, read once.
#[derive(Debug)]
struct Environment {
    ast_grep: String,
    aval: String,
    files: Option<String>,
    style: Style,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Finding {
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) col: usize,
    pub(crate) rule: String,
    /// The ast-grep rule id (`<rule>/<pattern>`): stable across message edits.
    pub(crate) pattern_id: String,
    /// The ast-grep message; the statement comes from aval.
    pub(crate) detail: String,
}

/// Where a comment excuses a finding the rule alone cannot tell apart.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum CommentExemption {
    SameLine,
    SameLineOrAbove,
}

/// A match as ast-grep printed it, before waivers and comments.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) finding: Finding,
    end_byte: usize,
    exemption: Option<CommentExemption>,
}

fn known_rules() -> BTreeSet<&'static str> {
    EMBEDDED
        .iter()
        .map(|(rule, _)| *rule)
        .chain([WAIVER_RULE])
        .collect()
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options {
        rule: None,
        json: false,
        no_color: false,
        paths: Vec::new(),
    };
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        if arg == "--" {
            options.paths.extend(remaining.by_ref().cloned());
        } else if arg == "--json" {
            options.json = true;
        } else if arg == "--no-color" {
            options.no_color = true;
        } else if arg == "--rule" || arg.starts_with("--rule=") {
            let value = match arg.strip_prefix("--rule=") {
                Some(attached) => Some(attached.to_string()),
                None => remaining.next().cloned(),
            };
            let id = value
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Failure::Usage("`--rule` needs a rule id".to_string()))?;
            if !known_rules().contains(id.as_str()) {
                return Err(Failure::Usage(format!("unknown rule `{id}`")));
            }
            options.rule = Some(id);
        } else if arg.starts_with('-') && arg != "-" {
            return Err(Failure::Usage(format!("unknown flag `{arg}`")));
        } else {
            options.paths.push(arg.clone());
        }
    }
    Ok(options)
}

fn read_environment(options: &Options) -> Environment {
    let no_color_variable = std::env::var("NO_COLOR").is_ok_and(|value| !value.is_empty());
    let dumb_terminal = std::env::var("TERM").is_ok_and(|value| value == "dumb");
    let colored = std::io::stdout().is_terminal()
        && !no_color_variable
        && !dumb_terminal
        && !options.no_color;
    Environment {
        ast_grep: std::env::var(AST_GREP_VARIABLE).unwrap_or_else(|_| DEFAULT_AST_GREP.to_string()),
        aval: std::env::var(AVAL_VARIABLE).unwrap_or_else(|_| DEFAULT_AVAL.to_string()),
        files: std::env::var(FILES_VARIABLE).ok(),
        style: if colored { Style::Ansi } else { Style::Plain },
    }
}

/// Runs the command, prints the report and returns how many findings it
/// holds (0 when stdout was closed before the report could be read).
pub(crate) fn run(args: &[String]) -> Result<usize, Failure> {
    let options = parse_options(args)?;
    let environment = read_environment(&options);
    let rule_files = selected_rule_files(options.rule.as_deref());
    let wants_waiver_findings = options
        .rule
        .as_deref()
        .is_none_or(|rule| rule == WAIVER_RULE);
    let files = source_files(&options, &environment)?;
    if !rule_files.is_empty() {
        check_version(&environment.ast_grep)?;
    }
    let matches = ast_grep_matches(&environment.ast_grep, &rule_files, &files, Path::new("."))?;
    let mut sources: HashMap<String, Vec<String>> = HashMap::new();
    for path in &files {
        sources.insert(normalize_path(path), read_lines(path)?);
    }
    let mut findings = Vec::new();
    for found in matches {
        let lines = sources.get(&found.finding.path).ok_or_else(|| {
            Failure::CouldNotRun(format!(
                "ast-grep reported {}, which was not asked for",
                found.finding.path
            ))
        })?;
        if !is_excused(&found, lines) && !is_waived(&found.finding, lines) {
            findings.push(found.finding);
        }
    }
    if wants_waiver_findings {
        for (path, lines) in &sources {
            findings.extend(waivers_without_reason(path, lines));
        }
    }
    findings
        .sort_by(|a, b| (&a.rule, &a.path, a.line, a.col).cmp(&(&b.rule, &b.path, b.line, b.col)));
    findings.dedup();
    let statements = rule_statements(&environment.aval, &findings)?;
    let rendered = if options.json {
        render_json(&findings, &statements, &sources)
    } else {
        render_text(&findings, &statements, environment.style)
    };
    match std::io::stdout().write_all(rendered.as_bytes()) {
        Ok(()) => {}
        Err(cause) if cause.kind() == ErrorKind::BrokenPipe => return Ok(0),
        Err(cause) => {
            return Err(Failure::CouldNotRun(format!(
                "cannot write to stdout: {cause}"
            )))
        }
    }
    if !findings.is_empty() {
        let rules: BTreeSet<&str> = findings.iter().map(|found| found.rule.as_str()).collect();
        eprintln!(
            "to waive a finding, put `adr-lint: allow <rule-id> -- <reason>` on its line or the line above"
        );
        eprintln!("{} findings in {} rules", findings.len(), rules.len());
    }
    Ok(findings.len())
}

/// The embedded rule texts of the selected aval rule (all when none).
fn selected_rule_files(rule: Option<&str>) -> Vec<&'static str> {
    EMBEDDED
        .iter()
        .filter(|(id, _)| rule.is_none_or(|wanted| wanted == *id))
        .map(|(_, text)| *text)
        .collect()
}

/// The ast-grep program to run: `$ADR_LINT_AST_GREP`, else `ast-grep`.
pub(crate) fn ast_grep_program() -> String {
    std::env::var(AST_GREP_VARIABLE).unwrap_or_else(|_| DEFAULT_AST_GREP.to_string())
}

pub(crate) fn ast_grep_failure(cause: &str) -> Failure {
    let first = cause.lines().next().unwrap_or("").trim();
    Failure::CouldNotRun(format!("{first}; {}", install_hint()))
}

fn expected_version() -> String {
    format!("ast-grep {}", AST_GREP_VERSION.trim())
}

fn install_hint() -> String {
    format!(
        "install ast-grep {} with `brew install ast-grep` or from {INSTALL_URL}",
        AST_GREP_VERSION.trim()
    )
}

/// The one-line statement of each rule, as aval says it; asked only when
/// there are findings to print.
fn rule_statements(
    aval_program: &str,
    findings: &[Finding],
) -> Result<BTreeMap<String, String>, Failure> {
    let mut statements = BTreeMap::new();
    if findings.is_empty() {
        return Ok(statements);
    }
    let printed =
        aval::code_constraints(aval_program, Path::new(".")).map_err(Failure::CouldNotRun)?;
    statements.extend(statements_from_json(&printed).map_err(Failure::CouldNotRun)?);
    statements.insert(WAIVER_RULE.to_string(), WAIVER_RULE_STATEMENT.to_string());
    for found in findings {
        if !statements.contains_key(&found.rule) {
            return Err(Failure::CouldNotRun(format!(
                "aval has no statement for rule `{}`",
                found.rule
            )));
        }
    }
    Ok(statements)
}

/// The `statement` of each active rule in aval's `{"rules": [...]}` output.
fn statements_from_json(printed: &str) -> Result<BTreeMap<String, String>, String> {
    let document: serde_json::Value =
        serde_json::from_str(printed).map_err(|cause| format!("aval printed no JSON: {cause}"))?;
    let entries = document
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .ok_or("aval JSON has no `rules` array")?;
    let mut statements = BTreeMap::new();
    for entry in entries {
        let text = |field: &str| entry.get(field).and_then(serde_json::Value::as_str);
        let id = text("id").ok_or("an aval rule has no `id` string")?;
        if entry.get("active").and_then(serde_json::Value::as_bool) == Some(false) {
            continue;
        }
        let statement =
            text("statement").ok_or_else(|| format!("aval rule `{id}` has no `statement`"))?;
        statements.insert(id.to_string(), statement.to_string());
    }
    Ok(statements)
}

pub(crate) fn check_version(program: &str) -> Result<(), Failure> {
    let printed = process::run(program, &["--version"], Path::new("."), process::DEADLINE)
        .map_err(|cause| ast_grep_failure(&cause.into_message()))?;
    let version = printed.trim();
    let expected = expected_version();
    if version == expected {
        Ok(())
    } else {
        Err(ast_grep_failure(&format!(
            "`{program} --version` printed `{version}`, expected `{expected}`"
        )))
    }
}

/// The files to check: the operands, else `$AMONT_FILES`, else the staged
/// set; directories expanded, anything that is not a source file dropped.
fn source_files(options: &Options, environment: &Environment) -> Result<Vec<String>, Failure> {
    let listed: Vec<String> = if !options.paths.is_empty() {
        options.paths.clone()
    } else if let Some(files) = &environment.files {
        files
            .split('\n')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        staged_files()?
    };
    let mut files = BTreeSet::new();
    let mut deleted: Option<BTreeSet<String>> = None;
    for name in listed {
        if !Path::new(&name).exists() {
            let known = deleted.get_or_insert_with(git_deleted_files);
            if known.contains(name.strip_prefix("./").unwrap_or(&name)) {
                continue;
            }
        }
        collect_source_files(Path::new(&name), &mut files)?;
    }
    Ok(files.into_iter().collect())
}

/// The paths git reports as deleted from the working tree or staged for
/// deletion, relative to the current directory. Empty when git cannot run or
/// cannot say: a missing path is then an unreadable one.
fn git_deleted_files() -> BTreeSet<String> {
    let listings: [&[&str]; 2] = [
        &["ls-files", "--deleted", "-z"],
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=D",
            "--relative",
            "-z",
        ],
    ];
    listings
        .iter()
        .filter_map(|args| {
            process::run("git", args, Path::new("."), process::DEADLINE).ok() // a git that cannot answer reads as nothing deleted: the path then exits 3
        })
        .flat_map(|printed| {
            printed
                .split('\0')
                .filter(|name| !name.is_empty())
                .map(normalize_path)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn staged_files() -> Result<Vec<String>, Failure> {
    let printed = process::run(
        "git",
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ],
        Path::new("."),
        process::DEADLINE,
    )
    .map_err(|cause| Failure::CouldNotRun(cause.into_message()))?;
    Ok(printed
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

pub(crate) fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
}

fn unreadable(path: &Path, cause: &std::io::Error) -> Failure {
    Failure::CouldNotRun(format!("cannot read {}: {cause}", path.display()))
}

fn collect_source_files(path: &Path, files: &mut BTreeSet<String>) -> Result<(), Failure> {
    let metadata = std::fs::metadata(path).map_err(|cause| unreadable(path, &cause))?;
    if metadata.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|cause| unreadable(path, &cause))?;
        for entry in entries {
            let entry = entry.map_err(|cause| unreadable(path, &cause))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            collect_source_files(&entry.path(), files)?;
        }
    } else if metadata.is_file() && is_source_file(path) {
        files.insert(path.to_string_lossy().into_owned());
    }
    Ok(())
}

fn read_lines(path: &str) -> Result<Vec<String>, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))?;
    Ok(text.lines().map(str::to_string).collect())
}

/// Runs the rule files over `files` (paths relative to `dir`), from `dir`.
pub(crate) fn ast_grep_matches(
    program: &str,
    rule_files: &[&str],
    files: &[String],
    dir: &Path,
) -> Result<Vec<Match>, Failure> {
    if rule_files.is_empty() {
        return Ok(Vec::new());
    }
    let rules = rule_files.join("\n---\n");
    let mut matches = Vec::new();
    for chunk in files.chunks(FILES_PER_RUN) {
        let mut args = vec![
            "scan",
            "--inline-rules",
            rules.as_str(),
            "--json=stream",
            "--include-metadata",
            "--color",
            "never",
            "--",
        ];
        args.extend(chunk.iter().map(String::as_str));
        let printed = process::run(program, &args, dir, process::DEADLINE)
            .map_err(|cause| ast_grep_failure(&cause.into_message()))?;
        for line in printed.lines().filter(|line| !line.trim().is_empty()) {
            matches.push(parse_match(line).map_err(|cause| ast_grep_failure(&cause))?);
        }
    }
    Ok(matches)
}

/// A path with `/` separators, whatever the OS or ast-grep reported, so output
/// and path comparisons agree on every machine (docs/formats.md).
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// One line of `ast-grep scan --json=stream`.
fn parse_match(line: &str) -> Result<Match, String> {
    let missing = |what: &str| format!("ast-grep printed JSON without {what}");
    let document: serde_json::Value = serde_json::from_str(line)
        .map_err(|cause| format!("ast-grep printed output that is not JSON: {cause}"))?;
    let text = |pointer: &str| {
        document
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
    };
    let number = |pointer: &str| {
        document
            .pointer(pointer)
            .and_then(serde_json::Value::as_u64)
            .and_then(|found| usize::try_from(found).ok())
    };
    let path = text("/file").ok_or_else(|| missing("`file`"))?;
    let rule_id = text("/ruleId").ok_or_else(|| missing("`ruleId`"))?;
    let message = text("/message").ok_or_else(|| missing("`message`"))?;
    let line_zero = number("/range/start/line").ok_or_else(|| missing("a start line"))?;
    let column_zero = number("/range/start/column").ok_or_else(|| missing("a start column"))?;
    let end_byte = number("/range/byteOffset/end").ok_or_else(|| missing("a byte offset"))?;
    let exemption = match text("/metadata/commentExempt") {
        None => None,
        Some("same-line") => Some(CommentExemption::SameLine),
        Some("same-line-or-above") => Some(CommentExemption::SameLineOrAbove),
        Some(other) => {
            return Err(format!(
                "ast-grep printed an unknown commentExempt `{other}`"
            ))
        }
    };
    let rule = rule_id.split('/').next().unwrap_or(rule_id);
    Ok(Match {
        finding: Finding {
            path: normalize_path(path),
            line: line_zero + 1,
            col: column_zero + 1,
            rule: rule.to_string(),
            pattern_id: rule_id.to_string(),
            detail: message.to_string(),
        },
        end_byte,
        exemption,
    })
}

/// Whether a comment the rule alone cannot see excuses the match.
fn is_excused(found: &Match, lines: &[String]) -> bool {
    let Some(exemption) = found.exemption else {
        return false;
    };
    let same_line = end_line_of(found, lines)
        .is_some_and(|(index, offset)| has_comment_after(&lines[index], offset));
    let above = found
        .finding
        .line
        .checked_sub(2)
        .and_then(|index| lines.get(index))
        .is_some_and(|line| {
            is_comment_line(line) && !is_waiver_for_another_rule(line, &found.finding.rule)
        });
    match exemption {
        CommentExemption::SameLine => same_line,
        CommentExemption::SameLineOrAbove => same_line || above,
    }
}

/// The index of the line the match ends on and the byte offset in it, from
/// the match's absolute end byte.
fn end_line_of(found: &Match, lines: &[String]) -> Option<(usize, usize)> {
    let mut start_of_line = 0;
    for (index, line) in lines.iter().enumerate() {
        let after = start_of_line + line.len();
        if found.end_byte <= after && index + 1 >= found.finding.line {
            return Some((index, found.end_byte - start_of_line));
        }
        // `lines()` drops the terminator: one byte for \n.
        start_of_line = after + 1;
    }
    None
}

/// Whether a comment starts in the line from `offset` on; a `//` or `/*`
/// inside a string literal does not start one.
fn has_comment_after(line: &str, offset: usize) -> bool {
    let Some(rest) = line.get(offset..) else {
        return false;
    };
    let mut quote: Option<char> = None;
    let mut characters = rest.chars().peekable();
    while let Some(character) = characters.next() {
        match quote {
            Some(_) if character == '\\' => {
                characters.next();
            }
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'' | '`') => quote = Some(character),
            None if character == '/' && matches!(characters.peek(), Some('/' | '*')) => {
                return true
            }
            None => {}
        }
    }
    false
}

/// A waiver comment excuses only the rule it names.
fn is_waiver_for_another_rule(line: &str, rule: &str) -> bool {
    parse_waiver(line).is_some_and(|waiver| waiver.rule != rule)
}

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.ends_with("*/")
}

/// What a waiver comment says.
#[derive(Debug, PartialEq, Eq)]
struct Waiver {
    rule: String,
    reason: String,
    col: usize,
}

/// The waiver on a line, when a comment leader comes before the marker.
fn parse_waiver(line: &str) -> Option<Waiver> {
    let at = line.find(WAIVER_MARKER)?;
    let before = &line[..at];
    if !(before.contains("//") || before.contains('#') || before.contains("/*")) {
        return None;
    }
    let rest = line[at + WAIVER_MARKER.len()..].trim_start();
    let (rule, after_rule) = match rest.split_once(char::is_whitespace) {
        Some((rule, after)) => (rule, after.trim_start()),
        None => (rest, ""),
    };
    let reason = after_rule
        .strip_prefix("--")
        .map(|reason| reason.trim().trim_end_matches("*/").trim())
        .unwrap_or("");
    Some(Waiver {
        rule: rule.to_string(),
        reason: reason.to_string(),
        col: before.chars().count() + 1,
    })
}

/// Whether the finding's line or the line above carries a waiver for its
/// rule with a reason.
fn is_waived(finding: &Finding, lines: &[String]) -> bool {
    [Some(finding.line), finding.line.checked_sub(1)]
        .into_iter()
        .flatten()
        .filter(|number| *number >= 1)
        .filter_map(|number| lines.get(number - 1))
        .filter_map(|line| parse_waiver(line))
        .any(|waiver| waiver.rule == finding.rule && !waiver.reason.is_empty())
}

fn waivers_without_reason(path: &str, lines: &[String]) -> Vec<Finding> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_waiver(line)
                .filter(|waiver| waiver.reason.is_empty())
                .map(|waiver| Finding {
                    path: path.to_string(),
                    line: index + 1,
                    col: waiver.col,
                    rule: WAIVER_RULE.to_string(),
                    pattern_id: WAIVER_RULE.to_string(),
                    detail: WAIVER_RULE_STATEMENT.to_string(),
                })
        })
        .collect()
}

fn statement_of<'a>(statements: &'a BTreeMap<String, String>, rule: &str) -> &'a str {
    statements.get(rule).map_or("", String::as_str)
}

fn render_json(
    findings: &[Finding],
    statements: &BTreeMap<String, String>,
    sources: &HashMap<String, Vec<String>>,
) -> String {
    let array: Vec<serde_json::Value> = findings
        .iter()
        .map(|found| {
            serde_json::json!({
                "path": found.path,
                "line": found.line,
                "col": found.col,
                "rule": found.rule,
                "message": statement_of(statements, &found.rule),
                "detail": found.detail,
                "text": finding_text(found, sources),
            })
        })
        .collect();
    format!("{}\n", serde_json::Value::Array(array))
}

/// The finding's source line, normalized as `candidates` normalizes the text
/// of its ids.
fn finding_text(found: &Finding, sources: &HashMap<String, Vec<String>>) -> String {
    sources
        .get(&found.path)
        .and_then(|lines| found.line.checked_sub(1).and_then(|index| lines.get(index)))
        .map_or_else(String::new, |line| normalize(line))
}

fn render_text(
    findings: &[Finding],
    statements: &BTreeMap<String, String>,
    style: Style,
) -> String {
    let mut by_rule: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for found in findings {
        by_rule.entry(found.rule.as_str()).or_default().push(found);
    }
    let (bold, reset) = match style {
        Style::Ansi => ("\x1b[1m", "\x1b[0m"),
        Style::Plain => ("", ""),
    };
    let mut text = String::new();
    for (rule, group) in by_rule {
        text.push_str(&format!("{bold}rule {rule} ({}){reset}\n", group.len()));
        for found in group {
            text.push_str(&format!(
                "{}:{}:{}: {}: {}\n",
                found.path,
                found.line,
                found.col,
                found.rule,
                statement_of(statements, rule)
            ));
        }
        text.push_str(&format!("aval rule {rule}\n"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    fn finding(rule: &str, line: usize) -> Finding {
        Finding {
            path: "a.ts".to_string(),
            line,
            col: 1,
            rule: rule.to_string(),
            pattern_id: format!("{rule}/p"),
            detail: "m".to_string(),
        }
    }

    #[test]
    fn a_backslash_path_from_ast_grep_is_reported_with_slashes() {
        assert_eq!(normalize_path("src\\deep\\a.rs"), "src/deep/a.rs");
        assert_eq!(normalize_path("src/a.rs"), "src/a.rs");
        let line = r#"{"file":"src\\deep\\a.rs","ruleId":"r.x/rust","message":"m","range":{"start":{"line":0,"column":0},"byteOffset":{"end":3}}}"#;
        let parsed = parse_match(line).expect("parses");
        assert_eq!(parsed.finding.path, "src/deep/a.rs");
    }

    #[test]
    fn a_waiver_is_read_with_its_rule_and_reason() {
        let waiver = parse_waiver(
            "  it.skip(); // adr-lint: allow tests.first-properties -- flaky, owner: ci",
        );
        assert_eq!(
            waiver,
            Some(Waiver {
                rule: "tests.first-properties".to_string(),
                reason: "flaky, owner: ci".to_string(),
                col: 17,
            })
        );
        let block = parse_waiver("/* adr-lint: allow x.y -- because */").expect("waiver");
        assert_eq!(block.reason, "because");
        let bare = parse_waiver("# adr-lint: allow x.y --").expect("waiver");
        assert_eq!(bare.reason, "");
        let no_dashes = parse_waiver("# adr-lint: allow x.y because").expect("waiver");
        assert_eq!(no_dashes.reason, "");
    }

    #[test]
    fn the_marker_outside_a_comment_is_not_a_waiver() {
        assert_eq!(parse_waiver("let marker = \"adr-lint: allow\";"), None);
    }

    #[test]
    fn a_waiver_covers_its_line_and_the_one_below_only() {
        let source = lines("// adr-lint: allow r.one -- why\nbad();\nbad();");
        assert!(is_waived(&finding("r.one", 1), &source));
        assert!(is_waived(&finding("r.one", 2), &source));
        assert!(!is_waived(&finding("r.one", 3), &source));
        assert!(!is_waived(&finding("r.two", 2), &source));
        let reasonless = lines("// adr-lint: allow r.one --\nbad();");
        assert!(!is_waived(&finding("r.one", 2), &reasonless));
    }

    #[test]
    fn reasonless_waivers_are_findings_at_their_marker() {
        let source = lines("ok();\n  // adr-lint: allow r.one --");
        let found = waivers_without_reason("a.ts", &source);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].col), (2, 6));
        assert_eq!(found[0].rule, WAIVER_RULE);
    }

    #[test]
    fn a_comment_excuses_by_where_the_exemption_looks() {
        let source = lines("// why\nit.skip(\"a\", f);\nit.skip(\"b\", f); // why");
        let skip = |line: usize, end_byte: usize, exemption| Match {
            finding: finding("r", line),
            end_byte,
            exemption: Some(exemption),
        };
        // Line 2 starts at byte 7 and its call ends 15 bytes in; line 3
        // starts at byte 24.
        assert!(is_excused(
            &skip(2, 22, CommentExemption::SameLineOrAbove),
            &source
        ));
        assert!(!is_excused(
            &skip(2, 22, CommentExemption::SameLine),
            &source
        ));
        assert!(is_excused(
            &skip(3, 39, CommentExemption::SameLine),
            &source
        ));
    }

    #[test]
    fn rendering_groups_by_rule_and_ends_each_group_with_the_aval_line() {
        let findings = vec![finding("a.rule", 3), finding("b.rule", 4)];
        let statements = BTreeMap::from([
            ("a.rule".to_string(), "says a".to_string()),
            ("b.rule".to_string(), "says b".to_string()),
        ]);
        assert_eq!(
            render_text(&findings, &statements, Style::Plain),
            "rule a.rule (1)\na.ts:3:1: a.rule: says a\naval rule a.rule\n\
             rule b.rule (1)\na.ts:4:1: b.rule: says b\naval rule b.rule\n"
        );
        assert!(render_text(&findings, &statements, Style::Ansi).contains('\x1b'));
        let sources = HashMap::from([("a.ts".to_string(), lines("x\ny\n  let  a;\t"))]);
        assert_eq!(render_json(&[], &statements, &sources), "[]\n");
        let json = render_json(&findings[..1], &statements, &sources);
        assert!(json.contains("\"message\":\"says a\"") && json.contains("\"detail\":\"m\""));
        assert!(json.contains("\"text\":\"let a;\""));
    }

    #[test]
    fn a_comment_marker_inside_a_string_is_not_a_comment() {
        let call = "const p = effect.catchAll(() => Effect.void);";
        let offset = call.len() - 1;
        let with_url = format!("{call} log(\"http://x\");");
        assert!(!has_comment_after(&with_url, offset));
        assert!(has_comment_after(&format!("{call} // why"), offset));
        assert!(has_comment_after(
            &format!("{call} log(\"a\"); /* why */"),
            offset
        ));
        assert!(!has_comment_after(
            &format!("{call} log('//', `/*`);"),
            offset
        ));
    }

    #[test]
    fn a_waiver_for_another_rule_is_not_a_reason_to_skip() {
        let source = lines("// adr-lint: allow other.rule -- why\nit.skip(\"a\", f);");
        let skip = Match {
            finding: finding("r", 2),
            end_byte: 52,
            exemption: Some(CommentExemption::SameLineOrAbove),
        };
        assert!(!is_excused(&skip, &source));
        let plain = lines("// flaky, owner: ci\nit.skip(\"a\", f);");
        assert!(is_excused(
            &Match {
                end_byte: 35,
                ..skip
            },
            &plain
        ));
    }

    #[test]
    fn statements_come_from_active_aval_rules() {
        let printed = r#"{"rules":[{"id":"a.b","statement":"says","active":true},{"id":"c.d","statement":"x","active":false}]}"#;
        let statements = statements_from_json(printed).expect("valid");
        assert_eq!(statements.len(), 1);
        assert_eq!(statements["a.b"], "says");
        assert!(statements_from_json("{}").is_err());
    }

    #[test]
    fn options_follow_the_posix_shapes() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let parsed = parse_options(&args(&[
            "--rule=names.reveal-intent",
            "--json",
            "--",
            "--odd.rs",
        ]))
        .expect("valid");
        assert_eq!(parsed.rule.as_deref(), Some("names.reveal-intent"));
        assert!(parsed.json);
        assert_eq!(parsed.paths, vec!["--odd.rs".to_string()]);
        assert!(matches!(
            parse_options(&args(&["--bogus"])),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_options(&args(&["--rule", "no.such"])),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_options(&args(&["--rule"])),
            Err(Failure::Usage(_))
        ));
    }
}
