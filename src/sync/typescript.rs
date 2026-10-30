//! The TypeScript half of `adr-lint sync`: the `adrLint()` call from
//! `@duro-app/eslint-config`, which sets `reportUnusedDisableDirectives`,
//! `ban-ts-comment` and the eslint-comments rules. The edit points are found
//! with the ast-grep adr-lint already requires; the edit itself is text,
//! inserted at the byte ranges ast-grep reports.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{write_atomically, Options, RULE};
use crate::check::{ast_grep_failure, ast_grep_program, check_version};
use crate::coverage::Failure;
use crate::process;

/// ESLint's own order of flat config names. A `.cjs` config is found so that
/// it is refused, never skipped: ESLint would load it before a later name.
const CONFIG_NAMES: [&str; 6] = [
    "eslint.config.js",
    "eslint.config.mjs",
    COMMONJS_CONFIG,
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
];
const COMMONJS_CONFIG: &str = "eslint.config.cjs";
const PACKAGE: &str = "@duro-app/eslint-config";
const MINIMUM_VERSION: (u64, u64, u64) = (5, 33, 0);
/// The first release whose `stack` takes `{ name, files }` entries.
const SCOPED_MINIMUM_VERSION: (u64, u64, u64) = (5, 38, 0);
const PRESETS: [&str; 2] = ["react", "effect"];
const CALL: &str = "adrLint";
const ESLINT_PROGRAM: &str = "node_modules/.bin/eslint";
const SAMPLE_PATHSPECS: [&str; 2] = ["*.ts", "*.tsx"];
const SAMPLE_LIMIT: usize = 20;

/// The configs a default export may call: their arguments are config objects.
const CONFIG_CALLEES: [&str; 2] = ["tseslint.config", "defineConfig"];

/// The kinds an element of an array or an argument list can have; `last`
/// matches one of them.
const ELEMENT_KINDS: [&str; 20] = [
    "identifier",
    "call_expression",
    "member_expression",
    "spread_element",
    "object",
    "array",
    "string",
    "template_string",
    "arrow_function",
    "function_expression",
    "parenthesized_expression",
    "ternary_expression",
    "binary_expression",
    "await_expression",
    "new_expression",
    "subscript_expression",
    "number",
    "'true'",
    "'false'",
    "'null'",
];

const COMMENTS_RULES: [&str; 3] = [
    "@eslint-community/eslint-comments/no-unlimited-disable",
    "@eslint-community/eslint-comments/require-description",
    "@eslint-community/eslint-comments/disable-enable-pair",
];
const BAN_TS_COMMENT: &str = "@typescript-eslint/ban-ts-comment";

/// A range ast-grep reported, in bytes of the text it was given.
#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

/// What ast-grep found in a config file.
#[derive(Default)]
struct Facts {
    named_imports: Option<Span>,
    specifiers: Vec<Span>,
    package_source: Option<Span>,
    package_imports: usize,
    default_array: Option<Span>,
    default_call: Option<Span>,
    call_arguments: Option<Span>,
    last_elements: Vec<Span>,
    adr_lint_calls: usize,
    adr_lint_imports: Vec<Span>,
    scoped_spreads: Vec<ScopedSpread>,
    plain_spreads: Vec<Span>,
}

/// A `...preset.map((c) => ({ ...c, files: X }))` element of the default export.
struct ScopedSpread {
    preset: Span,
    files: Span,
}

/// One entry of the `stack` the appended call carries.
enum StackEntry<'a> {
    Plain(&'static str),
    Scoped { name: &'static str, files: &'a str },
}

/// A language ast-grep parses the config as.
#[derive(Clone, Copy)]
enum Language {
    JavaScript,
    TypeScript,
}

impl Language {
    fn of(path: &Path) -> Language {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("ts" | "mts" | "cts") => Language::TypeScript,
            _ => Language::JavaScript,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
        }
    }
}

/// What a config lacks.
struct Gaps {
    import: bool,
    call: bool,
}

/// The ast-grep rules, one YAML document per fact. `last` takes the final
/// element of the default export's array or arguments, whatever its kind.
fn rules(language: Language) -> String {
    let language = language.name();
    let kinds = ELEMENT_KINDS
        .iter()
        .map(|kind| format!("{{kind: {kind}}}"))
        .collect::<Vec<_>>()
        .join(", ");
    let package = format!(
        "kind: import_statement, has: {{field: source, kind: string, regex: '^.{PACKAGE}.$'}}"
    );
    let default_value = "inside: {field: value, kind: export_statement}";
    format!(
        "id: named
language: {language}
rule:
  kind: named_imports
  inside: {{stopBy: end, {package}}}
---
id: spec
language: {language}
rule:
  kind: import_specifier
  inside:
    kind: named_imports
    inside: {{stopBy: end, {package}}}
---
id: source
language: {language}
rule:
  kind: string
  regex: '^.{PACKAGE}.$'
  inside: {{field: source, kind: import_statement}}
---
id: imported
language: {language}
rule:
  kind: import_specifier
  regex: '(^|\\s){CALL}(\\s|$)'
  inside:
    kind: named_imports
    inside: {{stopBy: end, {package}}}
---
id: array
language: {language}
rule:
  kind: array
  {default_value}
---
id: call
language: {language}
rule:
  kind: call_expression
  {default_value}
---
id: arguments
language: {language}
rule:
  kind: arguments
  inside:
    kind: call_expression
    {default_value}
---
id: last
language: {language}
rule:
  any: [{kinds}]
  nthChild: {{position: 1, reverse: true, ofRule: {{not: {{kind: comment}}}}}}
  inside:
    any:
      - kind: array
        {default_value}
      - kind: arguments
        inside:
          kind: call_expression
          {default_value}
---
id: called
language: {language}
rule:
  kind: call_expression
  has: {{field: function, kind: identifier, regex: '^{CALL}$'}}
---
id: scoped
language: {language}
rule:
  any:
    - pattern: \"$PRESET.map(($PARAM) => ({{ ...$PARAM, files: $FILES }}))\"
    - pattern: \"$PRESET.map($PARAM => ({{ ...$PARAM, files: $FILES }}))\"
  inside: {{kind: spread_element, inside: {{any: [{{kind: array}}, {{kind: arguments}}]}}}}
constraints:
  PRESET: {{kind: identifier}}
  FILES: {{any: [{{kind: identifier}}, {{kind: array}}]}}
---
id: plainspread
language: {language}
rule:
  kind: identifier
  inside: {{kind: spread_element, inside: {{any: [{{kind: array}}, {{kind: arguments}}]}}}}
"
    )
}

fn parse_facts(printed: &[u8]) -> Result<Facts, Failure> {
    let unreadable = |what: &str| ast_grep_failure(&format!("ast-grep printed {what}"));
    let mut facts = Facts::default();
    for line in String::from_utf8_lossy(printed)
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let hit: Value = serde_json::from_str(line)
            .map_err(|cause| unreadable(&format!("output that is not JSON: {cause}")))?;
        let offset = |side: &str| {
            hit.pointer(&format!("/range/byteOffset/{side}"))
                .and_then(Value::as_u64)
                .and_then(|found| usize::try_from(found).ok())
        };
        let (Some(rule), Some(start), Some(end)) = (
            hit.pointer("/ruleId").and_then(Value::as_str),
            offset("start"),
            offset("end"),
        ) else {
            return Err(unreadable("JSON without a rule id or a byte range"));
        };
        let span = Span { start, end };
        let variable = |name: &str| {
            let at = |side: &str| {
                hit.pointer(&format!(
                    "/metaVariables/single/{name}/range/byteOffset/{side}"
                ))
                .and_then(Value::as_u64)
                .and_then(|found| usize::try_from(found).ok())
            };
            Some(Span {
                start: at("start")?,
                end: at("end")?,
            })
        };
        match rule {
            "scoped" => {
                let (Some(preset), Some(files)) = (variable("PRESET"), variable("FILES")) else {
                    return Err(unreadable("a scoped spread without its captures"));
                };
                facts.scoped_spreads.push(ScopedSpread { preset, files });
            }
            "plainspread" => facts.plain_spreads.push(span),
            "named" => facts.named_imports = Some(span),
            "spec" => facts.specifiers.push(span),
            "source" => {
                facts.package_source = Some(span);
                facts.package_imports += 1;
            }
            "imported" => facts.adr_lint_imports.push(span),
            "array" => facts.default_array = Some(span),
            "call" => facts.default_call = Some(span),
            "arguments" => facts.call_arguments = Some(span),
            "last" => facts.last_elements.push(span),
            "called" => facts.adr_lint_calls += 1,
            other => return Err(unreadable(&format!("a rule id `{other}` it was not given"))),
        }
    }
    Ok(facts)
}

fn scan(program: &str, dir: &Path, language: Language, source: &str) -> Result<Facts, Failure> {
    let rules = rules(language);
    let args = [
        "scan",
        "--inline-rules",
        rules.as_str(),
        "--stdin",
        "--json=stream",
        "--color",
        "never",
    ];
    let printed = process::run_with_input(
        program,
        &args,
        dir,
        process::DEADLINE,
        source.as_bytes().to_vec(),
    )
    .map_err(|cause| ast_grep_failure(&cause.into_message()))?;
    parse_facts(&printed)
}

/// The first config in ESLint's order, or `None`; a symlink is refused.
fn find_config(dir: &Path) -> Result<Option<(PathBuf, String)>, Failure> {
    for name in CONFIG_NAMES {
        let path = dir.join(name);
        let failed =
            |cause: std::io::Error| Failure::CouldNotRun(format!("{}: {cause}", path.display()));
        match fs::symlink_metadata(&path) {
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(failed(cause)),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Failure::CouldNotRun(format!(
                    "{} is a symlink; refusing to rewrite it",
                    path.display()
                )))
            }
            Ok(_) if name == COMMONJS_CONFIG => {
                return Err(refusal(
                    &path,
                    "a CommonJS config (module.exports) is not edited; add `adrLint` by hand or use an ESM config",
                ))
            }
            Ok(_) => {
                let text = fs::read_to_string(&path).map_err(failed)?;
                return Ok(Some((path, text)));
            }
        }
    }
    Ok(None)
}

fn refusal(path: &Path, reason: &str) -> Failure {
    Failure::CouldNotRun(format!("{}: {reason}", path.display()))
}

fn slice(source: &str, span: Span) -> &str {
    source.get(span.start..span.end).unwrap_or("")
}

/// The `stack` the existing import implies: `react` and `effect`, in that
/// order, when the import names them. A preset only ever spread through
/// `.map` with a `files` scope gets one scoped entry per such spread (the
/// preset dedupes a ban repeated by name); one spread plainly anywhere stays a
/// single plain name.
fn stack_entries<'a>(source: &'a str, facts: &Facts) -> Vec<StackEntry<'a>> {
    let imported: Vec<&str> = facts
        .specifiers
        .iter()
        .filter_map(|span| slice(source, *span).split_whitespace().next())
        .collect();
    PRESETS
        .into_iter()
        .filter(|name| imported.contains(name))
        .flat_map(|name| {
            let spread_plainly = facts
                .plain_spreads
                .iter()
                .any(|span| slice(source, *span) == name);
            let scoped: Vec<StackEntry<'a>> = facts
                .scoped_spreads
                .iter()
                .filter(|spread| slice(source, spread.preset) == name)
                .map(|spread| StackEntry::Scoped {
                    name,
                    files: slice(source, spread.files),
                })
                .collect();
            if spread_plainly || scoped.is_empty() {
                vec![StackEntry::Plain(name)]
            } else {
                scoped
            }
        })
        .collect()
}

/// The lowest `@duro-app/eslint-config` the edit this sync writes needs; a config
/// that already calls `adrLint` gets no new call.
fn required_version(source: &str, facts: &Facts) -> Version {
    let writes_scoped_call = facts.adr_lint_calls == 0
        && stack_entries(source, facts)
            .iter()
            .any(|entry| matches!(entry, StackEntry::Scoped { .. }));
    if writes_scoped_call {
        SCOPED_MINIMUM_VERSION
    } else {
        MINIMUM_VERSION
    }
}

fn call_text(source: &str, facts: &Facts) -> String {
    let quote = facts
        .package_source
        .and_then(|span| slice(source, span).chars().next())
        .filter(|quote| *quote == '\'')
        .unwrap_or('"');
    let stack = stack_entries(source, facts)
        .iter()
        .map(|entry| match entry {
            StackEntry::Plain(name) => format!("{quote}{name}{quote}"),
            StackEntry::Scoped { name, files } => {
                format!("{{ name: {quote}{name}{quote}, files: {files} }}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("...{CALL}({{ stack: [{stack}] }})")
}

/// The insertion that adds `adrLint` to the import specifiers, in the
/// separator style the list already uses.
fn import_edit(path: &Path, source: &str, facts: &Facts) -> Result<(usize, String), Failure> {
    let Some(last) = facts.specifiers.last().copied() else {
        return Err(refusal(
            path,
            &format!("{PACKAGE} is imported without a named import list; add `{CALL}` by hand"),
        ));
    };
    let separator = facts
        .specifiers
        .len()
        .checked_sub(2)
        .map(|index| &source[facts.specifiers[index].end..last.start])
        .filter(|separator| !separator.contains("//") && !separator.contains("/*"))
        .unwrap_or(", ");
    Ok((last.end, format!("{separator}{CALL}")))
}

/// The insertions that append `text` as the last element of the container
/// spanning `container` (an array or an argument list).
fn append_edits(
    path: &Path,
    source: &str,
    container: Span,
    last: Option<Span>,
    text: &str,
) -> Result<Vec<(usize, String)>, Failure> {
    let Some(last) = last else {
        let inner = slice(
            source,
            Span {
                start: container.start + 1,
                end: container.end - 1,
            },
        );
        if !inner.trim().is_empty() {
            return Err(refusal(
                path,
                "cannot find the last element of the default export; add the call by hand",
            ));
        }
        return Ok(vec![(container.start + 1, text.to_string())]);
    };
    let line_start = source[..last.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let indent = &source[line_start..last.start];
    if !indent.trim().is_empty() {
        return Ok(vec![(last.end, format!(", {text}"))]);
    }
    let after = &source[last.end..];
    let gap = after.len() - after.trim_start_matches([' ', '\t']).len();
    let trailing_comma = after[gap..].starts_with(',');
    let comma_end = if trailing_comma {
        last.end + gap + 1
    } else {
        last.end
    };
    let line_end = comma_end
        + source[comma_end..]
            .find(['\r', '\n'])
            .unwrap_or(source.len() - comma_end);
    let rest_of_line = &source[comma_end..line_end];
    let point = if rest_of_line.trim().is_empty() || rest_of_line.trim_start().starts_with("//") {
        line_end
    } else {
        comma_end
    };
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let closing = if trailing_comma { "," } else { "" };
    let element = format!("{newline}{indent}{text}{closing}");
    if trailing_comma {
        Ok(vec![(point, element)])
    } else if point == last.end {
        Ok(vec![(point, format!(",{element}"))])
    } else {
        Ok(vec![(point, element), (last.end, ",".to_string())])
    }
}

fn call_edits(path: &Path, source: &str, facts: &Facts) -> Result<Vec<(usize, String)>, Failure> {
    let text = call_text(source, facts);
    let last = facts.last_elements.last().copied();
    if let Some(array) = facts.default_array {
        return append_edits(path, source, array, last, &text);
    }
    match (facts.default_call, facts.call_arguments) {
        (Some(call), Some(arguments)) => {
            let callee = source[call.start..arguments.start].trim();
            if CONFIG_CALLEES.contains(&callee) {
                append_edits(path, source, arguments, last, &text)
            } else {
                Err(refusal(
                    path,
                    &format!(
                        "the default export calls `{callee}`; only an array or {} are edited",
                        CONFIG_CALLEES.join(" / ")
                    ),
                ))
            }
        }
        _ => Err(refusal(
            path,
            &format!(
                "the default export is neither an array literal nor a {} call; add `{CALL}` by hand",
                CONFIG_CALLEES.join(" / ")
            ),
        )),
    }
}

fn apply(source: &str, mut edits: Vec<(usize, String)>) -> String {
    edits.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    let mut edited = source.to_string();
    for (offset, text) in edits {
        edited.insert_str(offset, &text);
    }
    edited
}

fn print_gaps(path: &Path, gaps: &Gaps) {
    if gaps.import {
        eprintln!(
            "{}: {CALL} is not imported from {PACKAGE} ({RULE})",
            path.display()
        );
    }
    if gaps.call {
        eprintln!(
            "{}: {CALL}() is not called in the default export ({RULE})",
            path.display()
        );
    }
}

/// How ESLint resolves a rule's level, from a `--print-config` entry.
fn level(entry: Option<&Value>) -> &'static str {
    let first = match entry {
        Some(Value::Array(items)) => items.first(),
        other => other,
    };
    match first {
        Some(Value::Number(number)) => match number.as_u64() {
            Some(2) => "error",
            Some(1) => "warn",
            _ => "off",
        },
        Some(Value::String(text)) => match text.as_str() {
            "error" => "error",
            "warn" => "warn",
            _ => "off",
        },
        _ => "off",
    }
}

fn resolved_problems(config: &Value, sample: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let unused = level(config.pointer("/linterOptions/reportUnusedDisableDirectives"));
    if unused != "error" {
        problems.push(format!(
            "eslint resolves linterOptions.reportUnusedDisableDirectives to {unused} for {sample}, wanted error ({RULE})"
        ));
    }
    for name in COMMENTS_RULES.iter().copied().chain([BAN_TS_COMMENT]) {
        let resolved = level(config.pointer("/rules").and_then(|rules| rules.get(name)));
        if resolved != "error" {
            problems.push(format!(
                "eslint resolves {name} to {resolved} for {sample}, wanted error ({RULE})"
            ));
        }
    }
    let ban_entry = config
        .pointer("/rules")
        .and_then(|rules| rules.get(BAN_TS_COMMENT));
    if level(ban_entry) == "error" {
        let option = ban_entry
            .and_then(Value::as_array)
            .and_then(|items| items.get(1))
            .and_then(|options| options.get("ts-expect-error"));
        let described = option == Some(&Value::String("allow-with-description".to_string()))
            || option == Some(&Value::Bool(true));
        if !described {
            problems.push(format!(
                "eslint resolves {BAN_TS_COMMENT} with ts-expect-error not limited to a described directive for {sample} ({RULE})"
            ));
        }
    }
    problems
}

/// Asks the repository's own ESLint how it resolves the required settings for
/// one sample file; returns whether they all hold.
fn resolves(dir: &Path) -> Result<bool, Failure> {
    let relative_program = dir.join(ESLINT_PROGRAM);
    if !relative_program.exists() {
        eprintln!("sync: eslint not installed; rule resolution not checked");
        return Ok(true);
    }
    let program = fs::canonicalize(&relative_program)
        .map_err(|cause| refusal(&relative_program, &cause.to_string()))?;
    let mut args = vec![
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
    ];
    args.extend(SAMPLE_PATHSPECS);
    let listed = process::run("git", &args, dir, process::DEADLINE)
        .map_err(|cause| Failure::CouldNotRun(format!("git ls-files: {}", cause.into_message())))?;
    let candidates: Vec<&str> = listed
        .lines()
        .filter(|line| !line.ends_with(".d.ts"))
        .take(SAMPLE_LIMIT)
        .collect();
    if candidates.is_empty() {
        eprintln!("sync: no .ts or .tsx file listed by git; rule resolution not checked");
        return Ok(true);
    }
    let program = program.to_string_lossy();
    // Each sample's failure is kept: the last one is reported if none works,
    // so a broken ESLint is told apart from samples that are all ignored.
    let mut last_failure = String::new();
    let mut resolved = None;
    for sample in &candidates {
        let printed = match process::run(
            &program,
            &["--print-config", sample],
            dir,
            process::DEADLINE,
        ) {
            Ok(printed) => printed,
            Err(failure) => {
                last_failure = failure.into_message();
                continue;
            }
        };
        match serde_json::from_str::<Value>(&printed) {
            Ok(config) => {
                resolved = Some((*sample, config));
                break;
            }
            Err(cause) => last_failure = format!("{sample}: not JSON ({cause})"),
        }
    }
    let Some((sample, config)) = resolved else {
        return Err(Failure::CouldNotRun(format!(
            "eslint --print-config printed no JSON config for any of the {} .ts/.tsx files tried; the last failure: {last_failure}",
            candidates.len()
        )));
    };
    eprintln!("sync: rule resolution checked on {sample}");
    let sample = sample.to_string();
    let problems = resolved_problems(&config, &sample);
    for problem in &problems {
        eprintln!("{problem}");
    }
    Ok(problems.is_empty())
}

/// A version as `major.minor.patch`; missing parts read as zero.
type Version = (u64, u64, u64);

/// A version and whether it is a prerelease, which sorts below its release.
#[derive(Clone, PartialEq, Eq)]
struct Parsed {
    core: Version,
    prerelease: Option<String>,
}

impl Parsed {
    fn admits(&self, minimum: Version) -> bool {
        self.core > minimum || (self.core == minimum && self.prerelease.is_none())
    }

    fn show(&self) -> String {
        match &self.prerelease {
            Some(label) => format!("{}-{label}", show(self.core)),
            None => show(self.core),
        }
    }
}

fn parse_version(text: &str) -> Option<Parsed> {
    let without_build = text.split('+').next()?;
    let (core, prerelease) = match without_build.split_once('-') {
        Some((core, label)) => (core, Some(label.to_string())),
        None => (without_build, None),
    };
    let mut parts = core.split('.');
    let mut next = || -> Option<u64> {
        match parts.next() {
            Some(part) => part.parse().ok(),
            None => Some(0),
        }
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(Parsed {
        core: version,
        prerelease,
    })
}

/// The lowest version a dependency range admits, for the ranges a
/// repository writes: `^`, `~`, `>=`, `>`, `=` or no operator before one
/// version. `>` admits the next patch release of its bound (a prerelease
/// bound included). A union, a pair of
/// bounds, a hyphen range, `x` or `*` is not read.
fn lowest_admitted(range: &str) -> Option<Parsed> {
    let range = range.trim();
    if range.contains("||") {
        return None;
    }
    let (operator, bare) = ["^", "~", ">=", ">", "="]
        .iter()
        .find_map(|operator| range.strip_prefix(operator).map(|bare| (*operator, bare)))
        .unwrap_or(("", range));
    let bare = bare.trim_start_matches('v');
    if bare.contains(char::is_whitespace) {
        return None;
    }
    let mut lowest = parse_version(bare)?;
    if operator == ">" {
        lowest.core.2 += 1;
        lowest.prerelease = None;
    }
    Some(lowest)
}

fn read_json(path: &Path) -> Result<Option<Value>, Failure> {
    match fs::read_to_string(path) {
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(refusal(path, &cause.to_string())),
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|cause| refusal(path, &format!("not JSON: {cause}"))),
    }
}

fn show(version: Version) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
}

/// `adrLint` exists from 5.33.0 on, and a scoped `stack` entry from 5.38.0; against an older `@duro-app/eslint-config`
/// the edit makes ESLint fail to load. The installed version decides, else
/// the lowest version `package.json` admits. Returns the refusal line, or
/// `None` when the version passes; no version at all is a failure.
fn version_gate(
    config: &Path,
    dir: &Path,
    first_with_the_call: Version,
) -> Result<Option<String>, Failure> {
    let installed_path = dir.join("node_modules").join(PACKAGE).join("package.json");
    let (found, origin) = if let Some(installed) = read_json(&installed_path)? {
        let text = installed.get("version").and_then(Value::as_str);
        let Some(version) = text.and_then(parse_version) else {
            return Err(refusal(
                &installed_path,
                "has no version this tool can read; nothing written",
            ));
        };
        (version, "installed".to_string())
    } else {
        let manifest_path = dir.join("package.json");
        let Some(manifest) = read_json(&manifest_path)? else {
            return Err(no_version(config));
        };
        let declared = [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ]
        .iter()
        .find_map(|section| manifest.pointer(&format!("/{section}/{}", PACKAGE.replace('/', "~1"))))
        .and_then(Value::as_str);
        let Some(range) = declared else {
            return Err(no_version(config));
        };
        if range.starts_with("workspace:") {
            return Ok(None);
        }
        let Some(version) = lowest_admitted(range) else {
            return Err(refusal(
                &manifest_path,
                &format!("cannot read the range `{range}` of {PACKAGE}; nothing written"),
            ));
        };
        (version, format!("declared as {range}"))
    };
    if found.admits(first_with_the_call) {
        return Ok(None);
    }
    Ok(Some(format!(
        "{}: {PACKAGE} {} ({origin}) is older than {}; upgrade {PACKAGE} to {} or later first ({RULE})",
        config.display(),
        found.show(),
        show(first_with_the_call),
        show(first_with_the_call),
    )))
}

fn no_version(config: &Path) -> Failure {
    refusal(
        config,
        &format!("no installed or declared {PACKAGE} version found; nothing written"),
    )
}

/// An aliased `adrLint` specifier would leave the appended call unbound.
fn reject_aliased_imports(path: &Path, source: &str, facts: &Facts) -> Result<(), Failure> {
    for span in &facts.adr_lint_imports {
        let text = slice(source, *span);
        let words: Vec<&str> = text.split_whitespace().collect();
        if words != [CALL] && words != [CALL, "as", CALL] {
            return Err(refusal(
                path,
                &format!(
                    "{PACKAGE} is imported as `{text}`, not as the plain binding {CALL}; rename it by hand"
                ),
            ));
        }
    }
    Ok(())
}

/// Adds `adrLint` to the repository's ESLint config; with `--check`, only
/// reports. Returns the exit status, 0 or 1.
pub(super) fn run(options: &Options) -> Result<u8, Failure> {
    let Some((path, source)) = find_config(&options.dir)? else {
        return Ok(0);
    };
    let program = ast_grep_program();
    check_version(&program)?;
    let language = Language::of(&path);
    let facts = scan(&program, &options.dir, language, &source)?;
    if facts.package_source.is_none() {
        return Err(refusal(
            &path,
            &format!(
                "no import from \"{PACKAGE}\"; adr-lint sync only edits a config that imports it"
            ),
        ));
    }
    if facts.package_imports > 1 {
        return Err(refusal(
            &path,
            &format!(
                "{PACKAGE} is imported by {} import declarations; merge them into one by hand",
                facts.package_imports
            ),
        ));
    }
    if let Some(refused) = version_gate(&path, &options.dir, required_version(&source, &facts))? {
        eprintln!("{refused}");
        return Ok(1);
    }
    reject_aliased_imports(&path, &source, &facts)?;
    let gaps = Gaps {
        import: facts.adr_lint_imports.is_empty(),
        call: facts.adr_lint_calls == 0,
    };
    let mut edits = Vec::new();
    if gaps.import {
        edits.push(import_edit(&path, &source, &facts)?);
    }
    if gaps.call {
        edits.extend(call_edits(&path, &source, &facts)?);
    }
    if options.check {
        print_gaps(&path, &gaps);
        if gaps.import || gaps.call {
            return Ok(1);
        }
        return Ok(u8::from(!resolves(&options.dir)?));
    }
    if edits.is_empty() {
        eprintln!("sync: already in sync");
        return Ok(0);
    }
    let edited = apply(&source, edits);
    let after = scan(&program, &options.dir, language, &edited)?;
    if after.adr_lint_calls != 1 || after.adr_lint_imports.is_empty() {
        return Err(refusal(
            &path,
            &format!("the edited config does not hold exactly one {CALL}() call and its import; nothing written"),
        ));
    }
    write_atomically(&path, &edited)?;
    eprintln!("sync: wrote {}", path.display());
    Ok(0)
}
