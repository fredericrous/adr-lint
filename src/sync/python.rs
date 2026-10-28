//! The Python half of `adr-lint sync`: ruff `PGH003`, `PGH004` and `RUF100`,
//! and pyright `enableTypeIgnoreComments` and
//! `reportUnnecessaryTypeIgnoreComment`, in the repository's own config files.

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use toml_edit::{Array, Document, DocumentMut, Item, RawString, Table, TableLike, Value};

use super::{line_of, lookup, write_atomically, Options, RULE};
use crate::coverage::Failure;

const CODES: [&str; 3] = ["PGH003", "PGH004", "RUF100"];
const SELECT_KEYS: [&str; 2] = ["select", "extend-select"];
const IGNORE_KEYS: [&str; 2] = ["ignore", "extend-ignore"];
const PER_FILE_KEYS: [&str; 2] = ["per-file-ignores", "extend-per-file-ignores"];

/// The two pyright settings that police suppressions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PyrightKey {
    IgnoreComments,
    UnnecessaryComment,
}

const PYRIGHT_KEYS: [PyrightKey; 2] = [PyrightKey::IgnoreComments, PyrightKey::UnnecessaryComment];

impl PyrightKey {
    fn name(self) -> &'static str {
        match self {
            PyrightKey::IgnoreComments => "enableTypeIgnoreComments",
            PyrightKey::UnnecessaryComment => "reportUnnecessaryTypeIgnoreComment",
        }
    }

    fn wanted(self) -> &'static str {
        match self {
            PyrightKey::IgnoreComments => "false",
            PyrightKey::UnnecessaryComment => "\"error\" or true",
        }
    }

    /// Whether `found`, a setting rendered as JSON text, keeps the key strong.
    fn accepts(self, found: &str) -> bool {
        match self {
            PyrightKey::IgnoreComments => found == "false",
            PyrightKey::UnnecessaryComment => found == "\"error\"" || found == "true",
        }
    }

    /// What `sync` writes when the key is missing, as JSON text.
    fn json_text(self) -> &'static str {
        match self {
            PyrightKey::IgnoreComments => "false",
            PyrightKey::UnnecessaryComment => "\"error\"",
        }
    }

    fn toml_value(self) -> Item {
        match self {
            PyrightKey::IgnoreComments => toml_edit::value(false),
            PyrightKey::UnnecessaryComment => toml_edit::value("error"),
        }
    }
}

/// What an audit found in one config file.
enum Finding {
    RuffMissing {
        path: PathBuf,
        code: &'static str,
    },
    RuffIgnored {
        path: PathBuf,
        line: usize,
        entry: String,
        code: &'static str,
    },
    RuffPerFileIgnored {
        path: PathBuf,
        line: usize,
        glob: String,
        entry: String,
        code: &'static str,
    },
    PyrightMissing {
        path: PathBuf,
        key: PyrightKey,
    },
    PyrightConflict {
        path: PathBuf,
        line: usize,
        key: PyrightKey,
        found: String,
    },
}

impl Finding {
    /// A conflict with what the person wrote: sync reports it and writes nothing.
    fn blocks_write(&self) -> bool {
        match self {
            Finding::RuffIgnored { .. }
            | Finding::RuffPerFileIgnored { .. }
            | Finding::PyrightConflict { .. } => true,
            Finding::RuffMissing { .. } | Finding::PyrightMissing { .. } => false,
        }
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::RuffMissing { path, code } => {
                write!(out, "{}: ruff {code} is not selected", path.display())
            }
            Finding::RuffIgnored {
                path,
                line,
                entry,
                code,
            } => write!(
                out,
                "{}:{line}: ruff ignores \"{entry}\", which disables {code} ({RULE})",
                path.display()
            ),
            Finding::RuffPerFileIgnored {
                path,
                line,
                glob,
                entry,
                code,
            } => write!(
                out,
                "{}:{line}: ruff per-file-ignores \"{glob}\" lists \"{entry}\", which disables {code} ({RULE})",
                path.display()
            ),
            Finding::PyrightMissing { path, key } => {
                write!(out, "{}: pyright {} is missing", path.display(), key.name())
            }
            Finding::PyrightConflict {
                path,
                line,
                key,
                found,
            } => write!(
                out,
                "{}:{line}: pyright {} is {found}, wanted {} ({RULE})",
                path.display(),
                key.name(),
                key.wanted()
            ),
        }
    }
}

/// Findings of one tool in one file, and the codes or keys it lacks.
struct Audit<Missing> {
    findings: Vec<Finding>,
    missing: Vec<Missing>,
}

/// A TOML config, read once: `parsed` keeps the spans for line numbers,
/// `document` is the copy that is edited.
struct TomlFile {
    path: PathBuf,
    raw: String,
    parsed: Document<String>,
    document: DocumentMut,
    changed: bool,
}

/// The pyrightconfig.json, as text.
struct JsonFile {
    path: PathBuf,
    raw: String,
}

/// One tool's list entry, with the line it is written on.
struct Entry {
    owner: String,
    text: String,
    line: usize,
}

/// Where a tool's settings are and what to add to them.
enum Edit {
    Ruff {
        file: usize,
        table: Vec<&'static str>,
        codes: Vec<&'static str>,
    },
    PyrightToml {
        file: usize,
        keys: Vec<PyrightKey>,
    },
    PyrightJson {
        keys: Vec<PyrightKey>,
    },
}

fn line_at(raw: &str, span: Option<Range<usize>>) -> usize {
    let start = span.map_or(0, |span| span.start);
    raw[..start.min(raw.len())].matches('\n').count() + 1
}

/// How specifically `entry` selects or ignores `code`: its length, with
/// `ALL` the least specific; `None` when it does not cover the code.
fn specificity(entry: &str, code: &str) -> Option<usize> {
    if entry == "ALL" {
        Some(0)
    } else if !entry.is_empty() && code.starts_with(entry) {
        Some(entry.len())
    } else {
        None
    }
}

fn entries(file: &TomlFile, table: Option<&Item>, keys: &[&str]) -> Vec<Entry> {
    let Some(table) = table.and_then(Item::as_table_like) else {
        return Vec::new();
    };
    keys.iter()
        .filter_map(|key| table.get(key))
        .filter_map(Item::as_array)
        .flat_map(Array::iter)
        .filter_map(|value| {
            Some(Entry {
                owner: String::new(),
                text: value.as_str()?.to_string(),
                line: line_at(&file.raw, value.span()),
            })
        })
        .collect()
}

fn per_file_entries(file: &TomlFile, table: Option<&Item>) -> Vec<Entry> {
    let Some(table) = table.and_then(Item::as_table_like) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for key in PER_FILE_KEYS {
        let Some(globs) = table.get(key).and_then(Item::as_table_like) else {
            continue;
        };
        for (glob, listed) in globs.iter() {
            for value in listed.as_array().into_iter().flat_map(Array::iter) {
                if let Some(text) = value.as_str() {
                    found.push(Entry {
                        owner: glob.to_string(),
                        text: text.to_string(),
                        line: line_at(&file.raw, value.span()),
                    });
                }
            }
        }
    }
    found
}

/// The table that holds ruff's lint settings: `lint`, or the base itself
/// when it still uses the deprecated top-level `select`, `ignore` or `per-file-ignores`.
fn lint_table_path(file: &TomlFile, base: &[&'static str]) -> Vec<&'static str> {
    let root = file.parsed.as_item();
    let at = |segment: &'static str| {
        let mut path = base.to_vec();
        path.push(segment);
        path
    };
    let lint = at("lint");
    if lookup(root, &lint).is_some() {
        return lint;
    }
    let top_level_settings = SELECT_KEYS
        .iter()
        .chain(&IGNORE_KEYS)
        .chain(&PER_FILE_KEYS)
        .any(|key| lookup(root, &at(key)).is_some());
    if top_level_settings {
        base.to_vec()
    } else {
        lint
    }
}

fn audit_ruff(file: &TomlFile, table_path: &[&str]) -> Audit<&'static str> {
    let table = lookup(file.parsed.as_item(), table_path);
    let selects = entries(file, table, &SELECT_KEYS);
    let ignores = entries(file, table, &IGNORE_KEYS);
    let per_file = per_file_entries(file, table);
    let mut audit = Audit {
        findings: Vec::new(),
        missing: Vec::new(),
    };
    for code in CODES {
        let enabled_at = selects
            .iter()
            .filter_map(|select| specificity(&select.text, code))
            .max();
        let mut conflicted = false;
        for ignore in &ignores {
            let Some(ignored_at) = specificity(&ignore.text, code) else {
                continue;
            };
            if enabled_at.is_none_or(|enabled| ignored_at >= enabled) {
                conflicted = true;
                audit.findings.push(Finding::RuffIgnored {
                    path: file.path.clone(),
                    line: ignore.line,
                    entry: ignore.text.clone(),
                    code,
                });
            }
        }
        for listed in &per_file {
            if specificity(&listed.text, code).is_some() {
                conflicted = true;
                audit.findings.push(Finding::RuffPerFileIgnored {
                    path: file.path.clone(),
                    line: listed.line,
                    glob: listed.owner.clone(),
                    entry: listed.text.clone(),
                    code,
                });
            }
        }
        if enabled_at.is_none() && !conflicted {
            audit.findings.push(Finding::RuffMissing {
                path: file.path.clone(),
                code,
            });
            audit.missing.push(code);
        }
    }
    audit
}

fn toml_setting_text(setting: &Item) -> String {
    match (setting.as_bool(), setting.as_str()) {
        (Some(flag), _) => flag.to_string(),
        (None, Some(text)) => format!("\"{text}\""),
        (None, None) => "not a plain value".to_string(),
    }
}

fn audit_pyright_toml(file: &TomlFile, table_path: &[&str]) -> Audit<PyrightKey> {
    let root = file.parsed.as_item();
    let mut audit = Audit {
        findings: Vec::new(),
        missing: Vec::new(),
    };
    for key in PYRIGHT_KEYS {
        let mut path = table_path.to_vec();
        path.push(key.name());
        match lookup(root, &path) {
            None => {
                audit.findings.push(Finding::PyrightMissing {
                    path: file.path.clone(),
                    key,
                });
                audit.missing.push(key);
            }
            Some(setting) => {
                let found = toml_setting_text(setting);
                if !key.accepts(&found) {
                    audit.findings.push(Finding::PyrightConflict {
                        path: file.path.clone(),
                        line: line_of(&file.raw, Some(setting)),
                        key,
                        found,
                    });
                }
            }
        }
    }
    audit
}

fn audit_pyright_json(json: &JsonFile) -> Result<Audit<PyrightKey>, Failure> {
    let refused = |cause: String| {
        Failure::CouldNotRun(format!(
            "{}: {cause}; refusing to rewrite it",
            json.path.display()
        ))
    };
    let parsed: serde_json::Value = serde_json::from_str(&json.raw)
        .map_err(|cause| refused(format!("not strict JSON ({cause})")))?;
    let object = parsed
        .as_object()
        .ok_or_else(|| refused("not a JSON object".to_string()))?;
    let mut audit = Audit {
        findings: Vec::new(),
        missing: Vec::new(),
    };
    for key in PYRIGHT_KEYS {
        match object.get(key.name()) {
            None => {
                audit.findings.push(Finding::PyrightMissing {
                    path: json.path.clone(),
                    key,
                });
                audit.missing.push(key);
            }
            Some(setting) => {
                let found = setting.to_string();
                if !key.accepts(&found) {
                    let quoted = format!("\"{}\"", key.name());
                    let start = json.raw.find(&quoted);
                    audit.findings.push(Finding::PyrightConflict {
                        path: json.path.clone(),
                        line: line_at(&json.raw, start.map(|at| at..at)),
                        key,
                        found,
                    });
                }
            }
        }
    }
    Ok(audit)
}

/// Inserts the missing keys before the closing brace, copying the file's own
/// indentation and closing whitespace, and checks the result parses.
fn insert_json_keys(json: &JsonFile, keys: &[PyrightKey]) -> Result<String, Failure> {
    let raw = &json.raw;
    let failed = |cause: &str| {
        Failure::CouldNotRun(format!(
            "{}: {cause}; refusing to rewrite it",
            json.path.display()
        ))
    };
    let open = raw.find('{').ok_or_else(|| failed("no object"))?;
    let close = raw.rfind('}').ok_or_else(|| failed("no closing brace"))?;
    let before = raw[..close].trim_end();
    let closing_whitespace = &raw[before.len()..close];
    let closing_whitespace = if closing_whitespace.contains('\n') {
        closing_whitespace
    } else {
        "\n"
    };
    let body = &raw[open + 1..close];
    let indent = if body.contains('\n') {
        body.lines()
            .find(|line| !line.trim().is_empty())
            .map_or("  ", |line| &line[..line.len() - line.trim_start().len()])
    } else {
        "  "
    };
    let lines: Vec<String> = keys
        .iter()
        .map(|key| format!("\n{indent}\"{}\": {}", key.name(), key.json_text()))
        .collect();
    let mut text = before.to_string();
    if !before.ends_with('{') {
        text.push(',');
    }
    text.push_str(&lines.join(","));
    text.push_str(closing_whitespace);
    text.push_str(&raw[close..]);
    let reparsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| failed("the edited text is not valid JSON"))?;
    for key in keys {
        let written = reparsed.get(key.name()).map(serde_json::Value::to_string);
        if written.as_deref() != Some(key.json_text()) {
            return Err(failed("the edited text lost a key"));
        }
    }
    Ok(text)
}

/// Walks to the table at `path`, creating what is absent.
fn table_at<'a>(
    document: &'a mut DocumentMut,
    path: &[&str],
) -> Result<&'a mut dyn TableLike, String> {
    let mut current: &mut dyn TableLike = document.as_table_mut();
    for (index, segment) in path.iter().enumerate() {
        if current.get(segment).is_none() {
            let mut table = Table::new();
            table.set_implicit(index + 1 < path.len());
            current.insert(segment, Item::Table(table));
        }
        current = current
            .get_mut(segment)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| format!("`{segment}` is not a table"))?;
    }
    Ok(current)
}

/// Appends `code` to `array`, indented like the array's last entry. Without
/// a trailing comma, the whitespace before `]` moves to the new last entry.
fn push_like_last(array: &mut Array, code: &str) {
    let mut value = Value::from(code);
    let Some(last_index) = array.len().checked_sub(1) else {
        array.push(code);
        return;
    };
    let has_trailing_comma = array.trailing_comma();
    let trailing = array.trailing().as_str().unwrap_or("").to_string();
    let Some(last) = array.get_mut(last_index) else {
        array.push(code);
        return;
    };
    let prefix = last
        .decor()
        .prefix()
        .and_then(RawString::as_str)
        .map(|prefix| match prefix.rsplit_once('\n') {
            Some((_, indent)) => format!("\n{indent}"),
            None if prefix.is_empty() => " ".to_string(),
            None => prefix.to_string(),
        })
        .unwrap_or_else(|| " ".to_string());
    let suffix = last
        .decor()
        .suffix()
        .and_then(RawString::as_str)
        .unwrap_or("")
        .to_string();
    // The text after the last entry's line: with a trailing comma it is the
    // array's own `trailing`, without one it is the entry's suffix.
    let tail = if has_trailing_comma {
        &trailing
    } else {
        &suffix
    };
    let (comment, rest) = split_line_comment(tail).unwrap_or(("", tail));
    value.decor_mut().set_prefix(format!("{comment}{prefix}"));
    if has_trailing_comma {
        value.decor_mut().set_suffix(suffix.as_str());
        last.decor_mut().set_suffix("");
        array.set_trailing(rest);
    } else {
        value.decor_mut().set_suffix(rest);
        last.decor_mut().set_suffix("");
    }
    array.push_formatted(value);
}

/// Splits the text after an array's last entry into a comment that shares that
/// entry's line (it stays with the entry, ahead of the new ones) and the rest,
/// which starts at the line's newline. `None` when there is no such comment.
fn split_line_comment(tail: &str) -> Option<(&str, &str)> {
    let end = tail.find('\n')?;
    tail[..end].contains('#').then(|| tail.split_at(end))
}

fn apply_ruff(
    document: &mut DocumentMut,
    table_path: &[&str],
    codes: &[&'static str],
) -> Result<(), String> {
    let table = table_at(document, table_path)?;
    let key = if table.contains_key("select") {
        "select"
    } else {
        "extend-select"
    };
    if table.contains_key(key) {
        let array = table
            .get_mut(key)
            .and_then(Item::as_array_mut)
            .ok_or_else(|| format!("`{key}` is not an array"))?;
        for code in codes {
            push_like_last(array, code);
        }
    } else {
        table.insert(
            key,
            toml_edit::value(codes.iter().copied().collect::<Array>()),
        );
    }
    Ok(())
}

fn apply_pyright(
    document: &mut DocumentMut,
    table_path: &[&str],
    keys: &[PyrightKey],
) -> Result<(), String> {
    let table = table_at(document, table_path)?;
    for key in keys {
        table.insert(key.name(), key.toml_value());
    }
    Ok(())
}

/// Reads a config file; `None` when it is absent, an error when it is a symlink.
fn read_config(path: &Path) -> Result<Option<String>, Failure> {
    let failed =
        |cause: std::io::Error| Failure::CouldNotRun(format!("{}: {cause}", path.display()));
    match fs::symlink_metadata(path) {
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(failed(cause)),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Failure::CouldNotRun(format!(
            "{} is a symlink; refusing to rewrite it",
            path.display()
        ))),
        Ok(_) => fs::read_to_string(path).map(Some).map_err(failed),
    }
}

fn toml_file(path: PathBuf, raw: String) -> Result<TomlFile, Failure> {
    let failed = |cause: String| Failure::CouldNotRun(format!("{}: {cause}", path.display()));
    let parsed = Document::parse(raw.clone()).map_err(|cause| failed(cause.to_string()))?;
    let document = raw
        .parse::<DocumentMut>()
        .map_err(|cause| failed(cause.to_string()))?;
    Ok(TomlFile {
        path,
        raw,
        parsed,
        document,
        changed: false,
    })
}

fn apply(
    edit: &Edit,
    files: &mut [TomlFile],
    json: &Option<JsonFile>,
) -> Result<Option<String>, Failure> {
    let (file_index, outcome) = match edit {
        Edit::Ruff { file, table, codes } => {
            (*file, apply_ruff(&mut files[*file].document, table, codes))
        }
        Edit::PyrightToml { file, keys } => (
            *file,
            apply_pyright(&mut files[*file].document, &["tool", "pyright"], keys),
        ),
        Edit::PyrightJson { keys } => {
            let json = json.as_ref().ok_or_else(|| {
                Failure::CouldNotRun("pyrightconfig.json vanished during sync".to_string())
            })?;
            return insert_json_keys(json, keys).map(Some);
        }
    };
    outcome.map_err(|cause| {
        Failure::CouldNotRun(format!("{}: {cause}", files[file_index].path.display()))
    })?;
    files[file_index].changed = true;
    Ok(None)
}

/// Runs the command; the exit status is the `Ok` value (0 or 1).
///
/// Known limits: a nested ruff config that does not extend the root's is not
/// read; a pyright `executionEnvironments` entry can override the two keys;
/// `ruff.toml` `extend` chains are not followed. Files read: `.ruff.toml`,
/// `ruff.toml`, `pyproject.toml` and `pyrightconfig.json` in the repository
/// root. A conflict is an ignore of a covered code (or a prefix of it) at
/// least as specific as the select that enables it, a per-file-ignores entry
/// for one, or a pyright key set to a weaker value.
pub(super) fn run(options: &Options) -> Result<u8, Failure> {
    let dir = &options.dir;
    let mut files: Vec<TomlFile> = Vec::new();
    let mut ruff_toml = None;
    for name in [".ruff.toml", "ruff.toml"] {
        let path = dir.join(name);
        if let Some(raw) = read_config(&path)? {
            files.push(toml_file(path, raw)?);
            ruff_toml = Some(files.len() - 1);
            break;
        }
    }
    let pyproject_path = dir.join("pyproject.toml");
    let mut pyproject = None;
    if let Some(raw) = read_config(&pyproject_path)? {
        files.push(toml_file(pyproject_path, raw)?);
        pyproject = Some(files.len() - 1);
    }
    let json_path = dir.join("pyrightconfig.json");
    let json = read_config(&json_path)?.map(|raw| JsonFile {
        path: json_path,
        raw,
    });
    if files.is_empty() && json.is_none() {
        return Ok(0);
    }

    let has_table = |index: Option<usize>, path: &[&str]| {
        index.filter(|index| lookup(files[*index].parsed.as_item(), path).is_some())
    };
    let ruff = ruff_toml.map(|index| (index, Vec::new())).or_else(|| {
        has_table(pyproject, &["tool", "ruff"]).map(|index| (index, vec!["tool", "ruff"]))
    });
    let pyright_pyproject = if json.is_none() {
        has_table(pyproject, &["tool", "pyright"])
    } else {
        None
    };

    let mut findings: Vec<Finding> = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    match &ruff {
        None => eprintln!("sync: no ruff config, ruff skipped"),
        Some((index, base)) => {
            let table = lint_table_path(&files[*index], base);
            let audit = audit_ruff(&files[*index], &table);
            findings.extend(audit.findings);
            if !audit.missing.is_empty() {
                edits.push(Edit::Ruff {
                    file: *index,
                    table,
                    codes: audit.missing,
                });
            }
        }
    }
    let configured_pyright = match (&json, pyright_pyproject) {
        (Some(json), _) => {
            let audit = audit_pyright_json(json)?;
            findings.extend(audit.findings);
            if !audit.missing.is_empty() {
                edits.push(Edit::PyrightJson {
                    keys: audit.missing,
                });
            }
            true
        }
        (None, Some(index)) => {
            let audit = audit_pyright_toml(&files[index], &["tool", "pyright"]);
            findings.extend(audit.findings);
            if !audit.missing.is_empty() {
                edits.push(Edit::PyrightToml {
                    file: index,
                    keys: audit.missing,
                });
            }
            true
        }
        (None, None) => {
            eprintln!("sync: no pyright config, pyright skipped");
            false
        }
    };

    if options.check {
        for finding in &findings {
            eprintln!("{finding}");
        }
        return Ok(u8::from(!findings.is_empty()));
    }
    let conflicts: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.blocks_write())
        .collect();
    if !conflicts.is_empty() {
        for conflict in conflicts {
            eprintln!("{conflict}");
        }
        return Ok(1);
    }
    if findings.is_empty() {
        if ruff.is_some() || configured_pyright {
            eprintln!("sync: already in sync");
        }
        return Ok(0);
    }
    let mut new_json = None;
    for edit in &edits {
        if let Some(text) = apply(edit, &mut files, &json)? {
            new_json = Some(text);
        }
    }
    for file in files.iter().filter(|file| file.changed) {
        write_atomically(&file.path, &file.document.to_string())?;
        eprintln!("sync: wrote {}", file.path.display());
    }
    if let (Some(text), Some(json)) = (new_json, &json) {
        write_atomically(&json.path, &text)?;
        eprintln!("sync: wrote {}", json.path.display());
    }
    Ok(0)
}
