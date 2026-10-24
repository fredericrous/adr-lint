//! `adr-lint candidates`: the candidate rules of the map, run over the files
//! a diff changes, reporting only the matches on lines the diff adds, for a
//! reviewer to rule on (guidance.code-rules-checked-by-fit).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::check;
use crate::coverage::Failure;
use crate::process;

const DEFAULT_MAX: usize = 40;
const DEPENDENCY_RULE: &str = "change.a-new-dependency-is-owned";
const DEPENDENCY_MESSAGE: &str = "a dependency added to a manifest: write beside it why the rungs above did not hold and what is now owned";
/// Files of this stem print the data the command was asked for.
const OUTPUT_STEMS: [&str; 3] = ["output", "out", "render"];
const STDOUT_RULE: &str = "cli.basics.stdout-is-data-stderr-is-talk";
const WAIT_RULE: &str = "tests.no-real-waits";
const TEST_DIRECTORIES: [&str; 4] = ["test", "tests", "__tests__", "spec"];

macro_rules! candidate {
    ($rule:literal, $language:literal) => {
        (
            $rule,
            include_str!(concat!(
                "../rules/candidates/",
                $rule,
                "/",
                $language,
                ".yml"
            )),
        )
    };
}

/// The ast-grep candidate rule files by the aval rule they mark lines for.
/// They live apart from `rules/<rule>/` so `adr-lint check` never reports one.
const EMBEDDED: [(&str, &str); 48] = [
    candidate!("cli.basics.stdout-is-data-stderr-is-talk", "go"),
    candidate!("cli.basics.stdout-is-data-stderr-is-talk", "python"),
    candidate!("cli.basics.stdout-is-data-stderr-is-talk", "rust"),
    candidate!("cli.basics.stdout-is-data-stderr-is-talk", "tsx"),
    candidate!("cli.basics.stdout-is-data-stderr-is-talk", "typescript"),
    candidate!("comments.accurate-or-deleted", "go"),
    candidate!("comments.accurate-or-deleted", "python"),
    candidate!("comments.accurate-or-deleted", "rust"),
    candidate!("comments.accurate-or-deleted", "tsx"),
    candidate!("comments.accurate-or-deleted", "typescript"),
    candidate!("dependencies.explicit", "go"),
    candidate!("dependencies.explicit", "rust"),
    candidate!("dependencies.explicit", "tsx"),
    candidate!("dependencies.explicit", "typescript"),
    candidate!("failures.expected-vs-defect", "go"),
    candidate!("failures.expected-vs-defect", "rust"),
    candidate!("failures.expected-vs-defect", "tsx"),
    candidate!("failures.expected-vs-defect", "typescript"),
    candidate!("functions.no-flag-arguments", "go"),
    candidate!("functions.no-flag-arguments", "python"),
    candidate!("functions.no-flag-arguments", "rust"),
    candidate!("functions.no-flag-arguments", "tsx"),
    candidate!("functions.no-flag-arguments", "typescript"),
    candidate!("functions.no-hidden-side-effects", "go"),
    candidate!("functions.no-hidden-side-effects", "rust"),
    candidate!("names.reveal-intent", "go"),
    candidate!("names.reveal-intent", "python"),
    candidate!("names.reveal-intent", "rust"),
    candidate!("names.reveal-intent", "tsx"),
    candidate!("names.reveal-intent", "typescript"),
    candidate!("reliability.every-wait-is-bounded", "go"),
    candidate!("reliability.every-wait-is-bounded", "rust"),
    candidate!("reliability.every-wait-is-bounded", "tsx"),
    candidate!("reliability.every-wait-is-bounded", "typescript"),
    candidate!("reliability.retries-are-bounded", "go"),
    candidate!("reliability.retries-are-bounded", "python"),
    candidate!("reliability.retries-are-bounded", "rust"),
    candidate!("reliability.retries-are-bounded", "tsx"),
    candidate!("reliability.retries-are-bounded", "typescript"),
    candidate!("tests.no-real-waits", "go"),
    candidate!("tests.no-real-waits", "python"),
    candidate!("tests.no-real-waits", "rust"),
    candidate!("tests.no-real-waits", "tsx"),
    candidate!("tests.no-real-waits", "typescript"),
    candidate!("types.exhaustive-matching", "rust"),
    candidate!("values.immutable-by-default", "go"),
    candidate!("values.immutable-by-default", "tsx"),
    candidate!("values.immutable-by-default", "typescript"),
];

pub(crate) const USAGE: &str = "\
usage: adr-lint candidates --base <ref> [--json] [--rule <id>] [--max <n>]

Runs the candidate ast-grep rules embedded in this binary over the files that
differ between the merge-base of HEAD and <ref> and HEAD, and prints only the
matches that start on a line the diff adds, for a reviewer to rule on. A
candidate is a line to look at, not a finding. A line added to a dependency
list of Cargo.toml, go.mod, package.json or pyproject.toml is a candidate of
change.a-new-dependency-is-owned.

Candidates print to stdout as `<id12> path:line:col: rule-id: pattern`,
grouped under one header line per rule that ends with an `aval rule <id>`
line. The id is the lowercase hex sha256 of the rule id, a NUL byte, the
repository-relative path, a NUL byte and the matched line's text with
whitespace collapsed: the same edit has the same id on any machine.

flags:
  --base <ref>  the ref the change is measured from (also --base=<ref>)
  --json        print a JSON array of {id, rule, pattern, path, line, col,
                text}, and nothing else; when --max cuts the list, the last
                element is {\"overflow\": {<rule-id>: <count>}}
  --rule <id>   only this rule (also --rule=<id>)
  --max <n>     keep the first n candidates (default 40), then say how many
                more each rule has
  -h, --help    this text

environment:
  ADR_LINT_AST_GREP  the ast-grep program (default: ast-grep from PATH); it
                     must be exactly the version in AST_GREP_VERSION

The command reads the committed diff and the files' text from the working
tree, so a changed file with uncommitted edits is refused (exit 3).

exit status:
  0  success, whether or not there are candidates (also a closed stdout)
  2  usage error (no --base, an unknown rule, a bad --max)
  3  could not run (git, ast-grep or the merge-base unreadable)
";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    base: String,
    json: bool,
    rule: Option<String>,
    max: usize,
}

/// One line the diff adds that a candidate rule marks.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Candidate {
    id: String,
    rule: String,
    pattern: String,
    path: String,
    line: usize,
    col: usize,
    text: String,
}

fn known_rules() -> BTreeSet<&'static str> {
    EMBEDDED
        .iter()
        .map(|(rule, _)| *rule)
        .chain([DEPENDENCY_RULE])
        .collect()
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let mut base = None;
    let mut json = false;
    let mut rule = None;
    let mut max = DEFAULT_MAX;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let (flag, attached) = match arg.split_once('=') {
            Some((flag, attached_text)) if flag.starts_with("--") => {
                (flag, Some(attached_text.to_string()))
            }
            _ => (arg.as_str(), None),
        };
        match flag {
            "--json" if attached.is_none() => json = true,
            "--base" | "--rule" | "--max" => {
                let option_argument = attached
                    .or_else(|| remaining.next().cloned())
                    .filter(|option_argument| !option_argument.is_empty())
                    .ok_or_else(|| Failure::Usage(format!("`{flag}` needs a value")))?;
                match flag {
                    "--base" if option_argument.starts_with('-') => {
                        return Err(Failure::Usage(format!("`{option_argument}` is not a ref")))
                    }
                    "--base" => base = Some(option_argument),
                    "--rule" if !known_rules().contains(option_argument.as_str()) => {
                        return Err(Failure::Usage(format!("unknown rule `{option_argument}`")))
                    }
                    "--rule" => rule = Some(option_argument),
                    _ => {
                        max = option_argument.parse().map_err(|_| {
                            Failure::Usage(format!(
                                "`--max` needs a number, not `{option_argument}`"
                            ))
                        })?;
                    }
                }
            }
            _ if arg.starts_with('-') => {
                return Err(Failure::Usage(format!("unknown flag `{arg}`")))
            }
            _ => return Err(Failure::Usage(format!("unexpected argument `{arg}`"))),
        }
    }
    let base = base.ok_or_else(|| Failure::Usage("`--base <ref>` is required".to_string()))?;
    Ok(Options {
        base,
        json,
        rule,
        max,
    })
}

/// Prints the candidates on the lines the diff adds.
pub(crate) fn run(args: &[String]) -> Result<(), Failure> {
    let options = parse_options(args)?;
    let root = PathBuf::from(
        git(Path::new("."), &["rev-parse", "--show-toplevel"])?
            .trim()
            .to_string(),
    );
    let merge_base = git(&root, &["merge-base", "HEAD", &options.base])?
        .trim()
        .to_string();
    let range = format!("{merge_base}...HEAD");
    let changed = git(
        &root,
        &[
            "-c",
            "core.quotePath=off",
            "diff",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
            &range,
        ],
    )?;
    let changed: Vec<String> = changed
        .split('\0')
        .filter(|name| !name.is_empty())
        .filter(|name| root.join(name).is_file())
        .map(str::to_string)
        .collect();
    refuse_uncommitted(&root, &changed)?;
    let patch = git(
        &root,
        &[
            "-c",
            "core.quotePath=off",
            "diff",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            &range,
        ],
    )?;
    let added = added_lines(&patch);
    let mut candidates = Vec::new();
    candidates.extend(scan(&options, &root, &changed, &added)?);
    if options.rule.as_deref().is_none_or(|r| r == DEPENDENCY_RULE) {
        candidates.extend(manifest_candidates(&root, &changed, &added)?);
    }
    candidates.sort_by(|a, b| {
        (&a.rule, &a.path, a.line, a.col, &a.id).cmp(&(&b.rule, &b.path, b.line, b.col, &b.id))
    });
    candidates.dedup_by(|a, b| (&a.id, &a.path, a.line, a.col) == (&b.id, &b.path, b.line, b.col));
    let (shown, overflow) = split_overflow(candidates, options.max);
    let rendered = if options.json {
        render_json(&shown, &overflow)
    } else {
        render_text(&shown, &overflow)
    };
    match std::io::stdout().write_all(rendered.as_bytes()) {
        Ok(()) => Ok(()),
        // A reader that closed the pipe early has what it wanted.
        Err(cause) if cause.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(cause) => Err(Failure::CouldNotRun(format!(
            "cannot write to stdout: {cause}"
        ))),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, Failure> {
    process::run("git", args, dir, process::DEADLINE)
        .map_err(|cause| Failure::CouldNotRun(cause.into_message()))
}

/// Line numbers come from the committed diff and the lines' text from the
/// working tree, so a changed file edited since the last commit would pair
/// the two wrongly: the command refuses it instead.
fn refuse_uncommitted(root: &Path, changed: &[String]) -> Result<(), Failure> {
    let edited = git(root, &["diff", "--name-only", "-z", "HEAD", "--"])?;
    match edited
        .split('\0')
        .find(|name| changed.iter().any(|path| path == name))
    {
        Some(path) => Err(Failure::CouldNotRun(format!(
            "{path} has uncommitted changes; commit or stash them, candidates reads the committed diff"
        ))),
        None => Ok(()),
    }
}

/// Per file, the line numbers a `git diff -U0` patch adds. A `+++ ` line is a
/// file header only between `diff --git` and the first hunk; inside a hunk it
/// is an added line that starts with `++ `.
fn added_lines(patch: &str) -> BTreeMap<String, BTreeSet<usize>> {
    let mut added: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_header = false;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            in_header = true;
            current = None;
        } else if let (true, Some(path)) = (in_header, line.strip_prefix("+++ ")) {
            current = path.strip_prefix("b/").map(|name| name.to_string());
        } else if let (Some(path), Some(hunk)) = (&current, line.strip_prefix("@@ ")) {
            in_header = false;
            if let Some(range) = hunk.split_whitespace().find(|part| part.starts_with('+')) {
                let (start, count) = match range[1..].split_once(',') {
                    Some((start, count)) => (start.parse::<usize>(), count.parse::<usize>()),
                    None => (range[1..].parse::<usize>(), Ok(1)),
                };
                if let (Ok(start), Ok(count)) = (start, count) {
                    added
                        .entry(path.clone())
                        .or_default()
                        .extend(start..start + count);
                }
            }
        }
    }
    added
}

fn is_added(added: &BTreeMap<String, BTreeSet<usize>>, path: &str, line: usize) -> bool {
    added.get(path).is_some_and(|lines| lines.contains(&line))
}

/// The ast-grep matches of the selected rules that start on an added line.
fn scan(
    options: &Options,
    root: &Path,
    changed: &[String],
    added: &BTreeMap<String, BTreeSet<usize>>,
) -> Result<Vec<Candidate>, Failure> {
    let files: Vec<String> = changed
        .iter()
        .filter(|name| check::is_source_file(Path::new(name)))
        .cloned()
        .collect();
    let rule_files: Vec<&str> = EMBEDDED
        .iter()
        .filter(|(rule, _)| options.rule.as_deref().is_none_or(|wanted| wanted == *rule))
        .map(|(_, text)| *text)
        .collect();
    if files.is_empty() || rule_files.is_empty() {
        return Ok(Vec::new());
    }
    let program = check::ast_grep_program();
    check::check_version(&program)?;
    let matches = check::ast_grep_matches(&program, &rule_files, &files, root)?;
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut candidates = Vec::new();
    for found in matches {
        let finding = found.finding;
        if !is_added(added, &finding.path, finding.line)
            || !path_fits_rule(&finding.rule, &finding.path)
        {
            continue;
        }
        if !sources.contains_key(&finding.path) {
            let text = std::fs::read_to_string(root.join(&finding.path)).map_err(|cause| {
                Failure::CouldNotRun(format!("cannot read {}: {cause}", finding.path))
            })?;
            sources.insert(
                finding.path.clone(),
                text.lines().map(str::to_string).collect(),
            );
        }
        let text = sources[&finding.path]
            .get(finding.line - 1)
            .map_or_else(String::new, |line| normalize(line));
        candidates.push(candidate(
            &finding.rule,
            &finding.detail,
            &finding.path,
            finding.line,
            finding.col,
            text,
        ));
    }
    Ok(candidates)
}

fn candidate(
    rule: &str,
    pattern: &str,
    path: &str,
    line: usize,
    col: usize,
    text: String,
) -> Candidate {
    Candidate {
        id: candidate_id(rule, path, &text),
        rule: rule.to_string(),
        pattern: pattern.to_string(),
        path: path.to_string(),
        line,
        col,
        text,
    }
}

/// Whitespace trimmed and every run of it collapsed to one space.
fn normalize(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The stable id: sha256 of rule, NUL, path, NUL, the normalized line.
fn candidate_id(rule: &str, path: &str, text: &str) -> String {
    sha256_hex(format!("{rule}\0{path}\0{text}").as_bytes())
}

/// Whether the file is one the rule is about: a wait only matters in a test
/// file, and a print only matters outside the file that prints the output.
fn path_fits_rule(rule: &str, path: &str) -> bool {
    match rule {
        WAIT_RULE => is_test_path(path),
        STDOUT_RULE => !is_output_path(path),
        _ => true,
    }
}

fn is_test_path(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or("").to_lowercase();
    parts.iter().any(|dir| TEST_DIRECTORIES.contains(dir))
        || file.contains("test")
        || file.contains(".spec.")
}

fn is_output_path(path: &str) -> bool {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| OUTPUT_STEMS.contains(&stem))
}

/// The first `max` candidates, and how many more each rule has.
fn split_overflow(
    mut candidates: Vec<Candidate>,
    max: usize,
) -> (Vec<Candidate>, BTreeMap<String, usize>) {
    let mut overflow = BTreeMap::new();
    if candidates.len() > max {
        for cut in candidates.split_off(max) {
            *overflow.entry(cut.rule).or_insert(0) += 1;
        }
    }
    (candidates, overflow)
}

fn render_json(shown: &[Candidate], overflow: &BTreeMap<String, usize>) -> String {
    let mut array: Vec<serde_json::Value> = shown
        .iter()
        .map(|found| {
            serde_json::json!({
                "id": found.id,
                "rule": found.rule,
                "pattern": found.pattern,
                "path": found.path,
                "line": found.line,
                "col": found.col,
                "text": found.text,
            })
        })
        .collect();
    if !overflow.is_empty() {
        array.push(serde_json::json!({ "overflow": overflow }));
    }
    format!("{}\n", serde_json::Value::Array(array))
}

fn render_text(shown: &[Candidate], overflow: &BTreeMap<String, usize>) -> String {
    let mut by_rule: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for found in shown {
        by_rule.entry(found.rule.as_str()).or_default().push(found);
    }
    let mut text = String::new();
    for (rule, group) in by_rule {
        text.push_str(&format!("rule {rule} ({})\n", group.len()));
        for found in group {
            text.push_str(&format!(
                "{} {}:{}:{}: {}: {}\n",
                &found.id[..12],
                found.path,
                found.line,
                found.col,
                found.rule,
                found.pattern
            ));
        }
        text.push_str(&format!("aval rule {rule}\n"));
    }
    for (rule, count) in overflow {
        text.push_str(&format!("… and {count} more in {rule}\n"));
    }
    text
}

/// A dependency list line in a manifest the diff adds to.
fn manifest_candidates(
    root: &Path,
    changed: &[String],
    added: &BTreeMap<String, BTreeSet<usize>>,
) -> Result<Vec<Candidate>, Failure> {
    let mut candidates = Vec::new();
    for path in changed {
        let name = path.rsplit('/').next().unwrap_or(path);
        let is_dependency_line: fn(&[String], usize) -> bool = match name {
            "Cargo.toml" => cargo_dependency_line,
            "pyproject.toml" => pyproject_dependency_line,
            "go.mod" => go_dependency_line,
            "package.json" => package_dependency_line,
            _ => continue,
        };
        let text = std::fs::read_to_string(root.join(path))
            .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))?;
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        for number in added.get(path).into_iter().flatten() {
            let Some(line) = lines.get(number - 1) else {
                continue;
            };
            if is_dependency_line(&lines, number - 1) {
                let column = line.len() - line.trim_start().len() + 1;
                candidates.push(candidate(
                    DEPENDENCY_RULE,
                    DEPENDENCY_MESSAGE,
                    path,
                    *number,
                    column,
                    normalize(line),
                ));
            }
        }
    }
    Ok(candidates)
}

fn is_blank_or_comment(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// The `[table]` header a line sits under, and whether an open array
/// `dependencies = [` is still unclosed at it.
struct TomlPosition {
    header: String,
    in_dependency_array: bool,
}

fn toml_position(lines: &[String], index: usize) -> TomlPosition {
    let mut position = TomlPosition {
        header: String::new(),
        in_dependency_array: false,
    };
    for line in &lines[..index] {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && !position.in_dependency_array {
            position.header = trimmed.trim_matches(|c| c == '[' || c == ']').to_string();
        } else if position.in_dependency_array {
            position.in_dependency_array = !trimmed.starts_with(']');
        } else if let Some(rest) = array_opening(trimmed) {
            position.in_dependency_array = !rest.contains(']');
        }
    }
    position
}

/// The text after `[` of a `dependencies = [` line.
fn array_opening(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("dependencies")?.trim_start();
    rest.strip_prefix('=')?.trim_start().strip_prefix('[')
}

fn cargo_dependency_line(lines: &[String], index: usize) -> bool {
    let trimmed = lines[index].trim();
    if trimmed.starts_with('[') {
        return trimmed.contains("dependencies.");
    }
    !is_blank_or_comment(trimmed)
        && trimmed.contains('=')
        && toml_position(lines, index).header.ends_with("dependencies")
}

fn pyproject_dependency_line(lines: &[String], index: usize) -> bool {
    let trimmed = lines[index].trim();
    if trimmed.starts_with('[') {
        return trimmed.contains("dependencies.");
    }
    if is_blank_or_comment(trimmed) || trimmed.starts_with(']') {
        return false;
    }
    let position = toml_position(lines, index);
    if position.in_dependency_array {
        return true;
    }
    if let Some(rest) = array_opening(trimmed) {
        return !rest.trim().is_empty() && rest.trim() != "]";
    }
    trimmed.contains('=') && position.header.ends_with("dependencies")
}

fn go_dependency_line(lines: &[String], index: usize) -> bool {
    let trimmed = lines[index].trim();
    if is_blank_or_comment(trimmed) || trimmed == ")" {
        return false;
    }
    if let Some(rest) = trimmed.strip_prefix("require") {
        return !rest.trim_start().starts_with('(') && !rest.trim().is_empty();
    }
    let mut in_block = false;
    for line in &lines[..index] {
        let earlier = line.trim();
        if earlier.starts_with("require") && earlier.ends_with('(') {
            in_block = true;
        } else if earlier == ")" {
            in_block = false;
        }
    }
    in_block
}

fn package_dependency_line(lines: &[String], index: usize) -> bool {
    let trimmed = lines[index].trim();
    if !trimmed.starts_with('"') {
        return false;
    }
    if is_dependency_block_opening(trimmed) {
        return trimmed.contains('}')
            && trimmed.matches('"').count() > 2
            && !trimmed.ends_with('{');
    }
    let mut in_block = false;
    for line in &lines[..index] {
        let earlier = line.trim();
        if is_dependency_block_opening(earlier) && earlier.ends_with('{') {
            in_block = true;
        } else if earlier.starts_with('}') {
            in_block = false;
        }
    }
    in_block
}

fn is_dependency_block_opening(trimmed: &str) -> bool {
    ["\"dependencies\"", "\"devDependencies\""]
        .iter()
        .any(|key| trimmed.starts_with(key))
        && trimmed.contains('{')
}

const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 as lowercase hex. Hand-written because the id must agree on every
/// machine and one function is cheaper to own than a dependency
/// (change.a-new-dependency-is-owned); pinned by the FIPS 180-4 vectors below.
fn sha256_hex(message: &[u8]) -> String {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&((message.len() as u64) * 8).to_be_bytes());
    for block in padded.chunks(64) {
        let mut schedule = [0u32; 64];
        for (slot, word) in schedule.iter_mut().zip(block.chunks(4)) {
            *slot = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for step in 16..64 {
            let early = schedule[step - 15];
            let late = schedule[step - 2];
            let sigma0 = early.rotate_right(7) ^ early.rotate_right(18) ^ (early >> 3);
            let sigma1 = late.rotate_right(17) ^ late.rotate_right(19) ^ (late >> 10);
            schedule[step] = schedule[step - 16]
                .wrapping_add(sigma0)
                .wrapping_add(schedule[step - 7])
                .wrapping_add(sigma1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for step in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let first = h
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(SHA256_ROUND_CONSTANTS[step])
                .wrapping_add(schedule[step]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (slot, word) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(word);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn an_id_pins_to_rule_path_and_collapsed_text() {
        // printf 'names.reveal-intent\0src/a.rs\0let d = 1;' | shasum -a 256
        let expected = "0d4ac074b2e5132f0cd08216919b96b51966709b7691396d85f399551737fcdb";
        let text = normalize("    let   d =\t1;  ");
        assert_eq!(
            candidate_id("names.reveal-intent", "src/a.rs", &text),
            expected
        );
        assert_ne!(
            candidate_id("names.reveal-intent", "src/b.rs", &text),
            expected
        );
    }

    #[test]
    fn added_lines_come_from_the_new_side_of_each_hunk() {
        let patch = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -3,0 +4,2 @@\n+a\n+b\n@@ -9 +11 @@\n-c\n+d\n@@ -20,2 +22,0 @@\n-e\n-f\n--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-g\n";
        let added = added_lines(patch);
        assert_eq!(added["x.rs"], BTreeSet::from([4, 5, 11]));
        assert_eq!(added.len(), 1);
    }

    #[test]
    fn an_added_line_that_looks_like_a_file_header_is_still_an_added_line() {
        let patch = "diff --git a/x.md b/x.md\n--- a/x.md\n+++ b/x.md\n@@ -2,0 +3 @@\n+++ not a header\n@@ -8,0 +10,2 @@\n+p\n+q\n";
        assert_eq!(added_lines(patch)["x.md"], BTreeSet::from([3, 10, 11]));
    }

    #[test]
    fn options_need_a_base_and_a_known_rule() {
        let parsed = parse_options(&args(&["--base=main", "--max", "3", "--json"])).expect("valid");
        assert_eq!(
            (parsed.base.as_str(), parsed.max, parsed.json),
            ("main", 3, true)
        );
        assert!(matches!(parse_options(&args(&[])), Err(Failure::Usage(_))));
        assert!(matches!(
            parse_options(&args(&["--base", "main", "--rule", "no.such"])),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_options(&args(&["--base", "main", "--max", "x"])),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_options(&args(&["--base", "-x"])),
            Err(Failure::Usage(_))
        ));
        assert!(parse_options(&args(&["--base", "m", "--rule", DEPENDENCY_RULE])).is_ok());
    }

    #[test]
    fn rule_paths_decide_where_a_pattern_counts() {
        assert!(is_test_path("tests/a.rs"));
        assert!(is_test_path("src/a_test.go"));
        assert!(is_test_path("web/a.spec.ts"));
        assert!(is_test_path("web/Thing.test.tsx"));
        assert!(!is_test_path("src/a.rs"));
        assert!(is_output_path("src/output.rs"));
        assert!(is_output_path("cmd/render.go"));
        assert!(!is_output_path("src/outcome.rs"));
        assert!(path_fits_rule(WAIT_RULE, "tests/a.rs") && !path_fits_rule(WAIT_RULE, "src/a.rs"));
        assert!(!path_fits_rule(STDOUT_RULE, "src/out.rs"));
    }

    #[test]
    fn a_cargo_dependency_line_is_one_under_a_dependencies_table() {
        let manifest = lines(
            "[package]\nname = \"a\"\n\n[dependencies]\n# why\nserde = \"1\"\n\n[dev-dependencies.tempfile]\nversion = \"3\"\n\n[lints.rust]\nunsafe_code = \"forbid\"\n",
        );
        let hits: Vec<usize> = (0..manifest.len())
            .filter(|index| cargo_dependency_line(&manifest, *index))
            .collect();
        assert_eq!(hits, vec![5, 7]);
    }

    #[test]
    fn a_pyproject_dependency_is_an_array_item_or_a_table_key() {
        let manifest = lines(
            "[project]\nname = \"a\"\ndependencies = [\n  \"requests\",\n]\nversion = \"1\"\n\n[tool.poetry.dependencies]\nflask = \"2\"\n",
        );
        let hits: Vec<usize> = (0..manifest.len())
            .filter(|index| pyproject_dependency_line(&manifest, *index))
            .collect();
        assert_eq!(hits, vec![3, 8]);
    }

    #[test]
    fn a_go_mod_dependency_is_inside_a_require_or_a_single_require() {
        let manifest = lines(
            "module a\n\ngo 1.22\n\nrequire (\n\tgithub.com/x/y v1.0.0\n\t// indirect note\n)\n\nrequire github.com/z/w v2.0.0\n",
        );
        let hits: Vec<usize> = (0..manifest.len())
            .filter(|index| go_dependency_line(&manifest, *index))
            .collect();
        assert_eq!(hits, vec![5, 9]);
    }

    #[test]
    fn a_package_json_dependency_is_inside_dependencies_or_dev_dependencies() {
        let manifest = lines(
            "{\n  \"name\": \"a\",\n  \"dependencies\": {\n    \"left-pad\": \"1\"\n  },\n  \"scripts\": {\n    \"x\": \"y\"\n  },\n  \"devDependencies\": {\n    \"vitest\": \"2\"\n  }\n}\n",
        );
        let hits: Vec<usize> = (0..manifest.len())
            .filter(|index| package_dependency_line(&manifest, *index))
            .collect();
        assert_eq!(hits, vec![3, 9]);
    }

    #[test]
    fn overflow_counts_what_the_cut_dropped_per_rule() {
        let make = |rule: &str, line| candidate(rule, "p", "a.rs", line, 1, "t".to_string());
        let all = vec![
            make("a.r", 1),
            make("a.r", 2),
            make("b.r", 1),
            make("b.r", 2),
        ];
        let (shown, overflow) = split_overflow(all, 3);
        assert_eq!(shown.len(), 3);
        assert_eq!(overflow, BTreeMap::from([("b.r".to_string(), 1)]));
        let json = render_json(&shown, &overflow);
        assert!(json.contains("{\"overflow\":{\"b.r\":1}}"), "{json}");
        let text = render_text(&shown, &overflow);
        assert!(text.ends_with("… and 1 more in b.r\n"), "{text}");
    }
}
