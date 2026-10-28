//! The Go half of `adr-lint sync`: golangci-lint's `nolintlint` with
//! `require-explanation`, `require-specific` and `allow-unused: false`, in the
//! repository's own `.golangci.yml` or `.golangci.yaml`. The edit goes through
//! `yaml-edit`, which keeps comments and layout.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use yaml_edit::{Document, Mapping, YamlNode};

use super::{write_atomically, Options, RULE};
use crate::coverage::Failure;
use crate::process;

const CONFIG_NAMES: [&str; 4] = [
    ".golangci.yml",
    ".golangci.yaml",
    ".golangci.toml",
    ".golangci.json",
];
const LINTER: &str = "nolintlint";
// holds-until: a file indented by other than two spaces gets new blocks indented
// by two, because yaml-edit places the first level of an inserted block two
// columns in whatever the file uses. It ends when yaml-edit takes the file's
// indentation, or sync re-indents what it inserts.
const INDENT: &str = "  ";

/// The three `nolintlint` settings that police suppressions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    RequireExplanation,
    RequireSpecific,
    AllowUnused,
}

const SETTINGS: [Setting; 3] = [
    Setting::RequireExplanation,
    Setting::RequireSpecific,
    Setting::AllowUnused,
];

impl Setting {
    fn name(self) -> &'static str {
        match self {
            Setting::RequireExplanation => "require-explanation",
            Setting::RequireSpecific => "require-specific",
            Setting::AllowUnused => "allow-unused",
        }
    }

    fn wanted(self) -> bool {
        match self {
            Setting::RequireExplanation | Setting::RequireSpecific => true,
            Setting::AllowUnused => false,
        }
    }
}

enum Finding {
    NotEnabled {
        path: PathBuf,
    },
    Disabled {
        path: PathBuf,
        line: usize,
    },
    Excluded {
        path: PathBuf,
        line: usize,
    },
    SettingMissing {
        path: PathBuf,
        setting: Setting,
    },
    SettingWeak {
        path: PathBuf,
        line: usize,
        setting: Setting,
        found: String,
    },
    NotVerified {
        path: PathBuf,
        reason: String,
    },
}

impl Finding {
    /// A conflict with what the person wrote: sync reports it and writes nothing.
    fn blocks_write(&self) -> bool {
        match self {
            Finding::Disabled { .. } | Finding::Excluded { .. } | Finding::SettingWeak { .. } => {
                true
            }
            Finding::NotEnabled { .. }
            | Finding::SettingMissing { .. }
            | Finding::NotVerified { .. } => false,
        }
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::NotEnabled { path } => {
                write!(out, "{}: golangci-lint {LINTER} is not enabled", path.display())
            }
            Finding::Disabled { path, line } => write!(
                out,
                "{}:{line}: golangci-lint disables {LINTER} in linters.disable ({RULE})",
                path.display()
            ),
            Finding::Excluded { path, line } => write!(
                out,
                "{}:{line}: golangci-lint exclusions.rules lists {LINTER}, which silences it ({RULE})",
                path.display()
            ),
            Finding::SettingMissing { path, setting } => write!(
                out,
                "{}: golangci-lint {LINTER} setting {} is missing",
                path.display(),
                setting.name()
            ),
            Finding::SettingWeak {
                path,
                line,
                setting,
                found,
            } => write!(
                out,
                "{}:{line}: golangci-lint {LINTER} setting {} is {found}, wanted {} ({RULE})",
                path.display(),
                setting.name(),
                setting.wanted()
            ),
            Finding::NotVerified { path, reason } => write!(
                out,
                "{}: golangci-lint config verify failed: {reason}",
                path.display()
            ),
        }
    }
}

/// The first golangci-lint config of the root, as text; `None` when there is none.
fn read_config(dir: &Path) -> Result<Option<(PathBuf, String)>, Failure> {
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
            Ok(_) => {}
        }
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            return Err(Failure::CouldNotRun(format!(
                "{} is not YAML; sync edits only .golangci.yml and .golangci.yaml",
                path.display()
            )));
        }
        let raw = fs::read_to_string(&path).map_err(failed)?;
        return Ok(Some((path, raw)));
    }
    Ok(None)
}

fn parse(path: &Path, raw: &str) -> Result<Document, Failure> {
    let failed = |cause: String| Failure::CouldNotRun(format!("{}: {cause}", path.display()));
    let document = Document::from_str(raw).map_err(|cause| failed(cause.to_string()))?;
    if document.to_string() != raw {
        return Err(failed(
            "the YAML parser does not reproduce this file".to_string(),
        ));
    }
    let root = document
        .as_mapping()
        .ok_or_else(|| failed("the top level is not a mapping".to_string()))?;
    let version = root
        .get("version")
        .and_then(|node| node.as_scalar().map(|scalar| scalar.as_string()));
    if version.as_deref() != Some("2") {
        return Err(failed(
            "no `version: \"2\"`; sync edits only golangci-lint v2 configs".to_string(),
        ));
    }
    Ok(document)
}

fn line_of(raw: &str, node: &YamlNode) -> usize {
    let start = node.byte_range().start as usize;
    raw[..start.min(raw.len())].matches('\n').count() + 1
}

fn scalar_text(node: &YamlNode) -> Option<String> {
    node.as_scalar().map(|scalar| scalar.as_string())
}

fn item_named(sequence_owner: &Mapping, key: &str, name: &str) -> Option<YamlNode> {
    sequence_owner
        .get_sequence(key)?
        .values()
        .find(|item| scalar_text(item).as_deref() == Some(name))
}

fn audit(path: &Path, raw: &str, document: &Document) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(linters) = document.get_mapping("linters") else {
        findings.push(Finding::NotEnabled {
            path: path.to_path_buf(),
        });
        for setting in SETTINGS {
            findings.push(Finding::SettingMissing {
                path: path.to_path_buf(),
                setting,
            });
        }
        return findings;
    };
    let covers_all = linters
        .get("default")
        .and_then(|node| scalar_text(&node))
        .as_deref()
        == Some("all");
    if let Some(item) = item_named(&linters, "disable", LINTER) {
        findings.push(Finding::Disabled {
            path: path.to_path_buf(),
            line: line_of(raw, &item),
        });
    }
    if !covers_all && item_named(&linters, "enable", LINTER).is_none() {
        findings.push(Finding::NotEnabled {
            path: path.to_path_buf(),
        });
    }
    if let Some(rules) = linters
        .get_mapping("exclusions")
        .and_then(|exclusions| exclusions.get_sequence("rules"))
    {
        for rule in rules.values().filter_map(|rule| rule.as_mapping().cloned()) {
            if let Some(item) = item_named(&rule, "linters", LINTER) {
                findings.push(Finding::Excluded {
                    path: path.to_path_buf(),
                    line: line_of(raw, &item),
                });
            }
        }
    }
    let settings = linters
        .get_mapping("settings")
        .and_then(|settings| settings.get_mapping(LINTER));
    for setting in SETTINGS {
        let found = settings
            .as_ref()
            .and_then(|table| table.get(setting.name()));
        match found {
            None => findings.push(Finding::SettingMissing {
                path: path.to_path_buf(),
                setting,
            }),
            Some(node) => {
                let text = node
                    .as_scalar()
                    .map(|scalar| scalar.value())
                    .unwrap_or_default();
                // A quoted `"true"` is a string, which golangci-lint rejects
                // ("got string, want boolean"), so it is named as such.
                let text = if text.starts_with('"') || text.starts_with('\'') {
                    format!("the string {text}, not a boolean")
                } else {
                    text
                };
                if text.to_ascii_lowercase() != setting.wanted().to_string() {
                    findings.push(Finding::SettingWeak {
                        path: path.to_path_buf(),
                        line: line_of(raw, &node),
                        setting,
                        found: if text.is_empty() {
                            node.to_string().trim().to_string()
                        } else {
                            text
                        },
                    });
                }
            }
        }
    }
    findings
}

/// The missing settings as `key: value` lines, unindented.
fn setting_lines(missing: &[Setting]) -> String {
    missing
        .iter()
        .map(|setting| format!("{}: {}\n", setting.name(), setting.wanted()))
        .collect()
}

/// The `nolintlint:` mapping holding the missing settings.
fn nolintlint_block(missing: &[Setting]) -> String {
    let mut text = format!("{LINTER}:\n");
    for line in setting_lines(missing).lines() {
        text.push_str(&format!("{INDENT}{line}\n"));
    }
    text
}

/// The text of `raw` with `findings`' gaps filled.
fn apply(path: &Path, raw: &str, findings: &[Finding]) -> Result<String, Failure> {
    let failed = |cause: String| Failure::CouldNotRun(format!("{}: {cause}", path.display()));
    let document = parse(path, raw)?;
    let root = document
        .as_mapping()
        .ok_or_else(|| failed("the top level is not a mapping".to_string()))?;
    let enable = findings
        .iter()
        .any(|finding| matches!(finding, Finding::NotEnabled { .. }));
    let missing: Vec<Setting> = findings
        .iter()
        .filter_map(|finding| match finding {
            Finding::SettingMissing { setting, .. } => Some(*setting),
            Finding::NotEnabled { .. }
            | Finding::Disabled { .. }
            | Finding::Excluded { .. }
            | Finding::SettingWeak { .. }
            | Finding::NotVerified { .. } => None,
        })
        .collect();

    let Some(linters) = root.get_mapping("linters") else {
        if root
            .get("linters")
            .is_some_and(|node| !node.as_scalar().is_some_and(|scalar| scalar.is_null()))
        {
            return Err(failed("`linters` is not a mapping".to_string()));
        }
        let mut snippet = format!("enable:\n{INDENT}- {LINTER}\nsettings:\n");
        for line in nolintlint_block(&missing).lines() {
            snippet.push_str(&format!("{INDENT}{line}\n"));
        }
        let block = Document::from_str(&snippet).map_err(|cause| failed(cause.to_string()))?;
        let mapping = block
            .as_mapping()
            .ok_or_else(|| failed("internal: snippet is not a mapping".to_string()))?;
        root.set("linters", mapping);
        return Ok(document.to_string());
    };
    if enable {
        match linters.get_sequence("enable") {
            Some(list) => list.push(LINTER),
            None => {
                let block = Document::from_str(&format!("- {LINTER}\n"))
                    .map_err(|cause| failed(cause.to_string()))?;
                let list = block
                    .as_sequence()
                    .ok_or_else(|| failed("internal: snippet is not a list".to_string()))?;
                linters.set("enable", list);
            }
        }
    }
    if !missing.is_empty() {
        match linters.get_mapping("settings") {
            None => {
                let block = Document::from_str(&nolintlint_block(&missing))
                    .map_err(|cause| failed(cause.to_string()))?;
                let mapping = block
                    .as_mapping()
                    .ok_or_else(|| failed("internal: snippet is not a mapping".to_string()))?;
                linters.set("settings", mapping);
            }
            Some(settings) => match settings.get_mapping(LINTER) {
                Some(table) => {
                    for setting in &missing {
                        table.set(setting.name(), setting.wanted());
                    }
                }
                None => {
                    let block = Document::from_str(&setting_lines(&missing))
                        .map_err(|cause| failed(cause.to_string()))?;
                    let mapping = block
                        .as_mapping()
                        .ok_or_else(|| failed("internal: snippet is not a mapping".to_string()))?;
                    settings.set(LINTER, mapping);
                }
            },
        }
    }
    Ok(document.to_string())
}

/// What `golangci-lint config verify` said about the file.
enum Verified {
    Passed,
    NotInstalled,
    Failed(Finding),
}

/// Runs `golangci-lint config verify` on the file, in the repository root.
fn verify(dir: &Path, path: &Path) -> Verified {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match process::run(
        "golangci-lint",
        &["config", "verify", "--config", &name],
        dir,
        process::DEADLINE,
    ) {
        Ok(_) => Verified::Passed,
        Err(process::Failure::Missing(_)) => Verified::NotInstalled,
        Err(process::Failure::Failed(reason)) => Verified::Failed(Finding::NotVerified {
            path: path.to_path_buf(),
            reason,
        }),
    }
}

/// Runs the command; the exit status is the `Ok` value (0 or 1).
///
/// File read: the first of `.golangci.yml`, `.golangci.yaml`, `.golangci.toml`
/// and `.golangci.json` in the repository root; only YAML with `version: "2"`
/// is edited.
// holds-until: sync reads the config at the repository root only; a `--config`
// flag that CI passes to golangci-lint is invisible to it. It ends when sync
// reads the config the project's CI names.
pub(super) fn run(options: &Options) -> Result<u8, Failure> {
    let Some((path, raw)) = read_config(&options.dir)? else {
        return Ok(0);
    };
    let document = parse(&path, &raw)?;
    let mut findings = audit(&path, &raw, &document);

    if options.check {
        let not_installed = match verify(&options.dir, &path) {
            Verified::Passed => false,
            Verified::NotInstalled => true,
            Verified::Failed(finding) => {
                findings.push(finding);
                false
            }
        };
        for finding in &findings {
            eprintln!("{finding}");
        }
        if not_installed {
            eprintln!("sync: golangci-lint not installed; config not verified");
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
        eprintln!("sync: already in sync");
        return Ok(0);
    }
    let edited = apply(&path, &raw, &findings)?;
    let reparsed = parse(&path, &edited)?;
    if let Some(left) = audit(&path, &edited, &reparsed).first() {
        return Err(Failure::CouldNotRun(format!(
            "{}: the edit did not take ({left}); nothing written",
            path.display()
        )));
    }
    write_atomically(&path, &edited)?;
    eprintln!("sync: wrote {}", path.display());
    Ok(0)
}
