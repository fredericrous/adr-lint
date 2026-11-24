//! The one reader of `greenspotter.toml`, at the repository root. Each command
//! takes the text from here and parses the section it owns.

use std::fmt;
use std::path::Path;

use crate::contract::{self, Contract, ContractError};

/// The file's name at the repository root.
pub const FILE: &str = "greenspotter.toml";

/// The file's old name, still read until the fleet sweep has renamed it
/// (ADR-0073's one fallback read). The sweep (programme topo-spotter-rename,
/// t10) removes this constant and its callers.
pub const LEGACY_FILE: &str = "adr-lint.toml";

/// The text of `greenspotter.toml`; empty when the repository has none.
#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    text: String,
    file: &'static str,
    note: Option<String>,
}

/// Why the configuration could not be used.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    Unreadable { path: String, cause: String },
    Contract(ContractError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Unreadable { path, cause } => write!(out, "cannot read {path}: {cause}"),
            ConfigError::Contract(error) => write!(out, "{error}"),
        }
    }
}

/// The warning for a repository that has only the old file name.
pub fn old_name_note() -> String {
    format!("warning: {LEGACY_FILE} is the old name; run `git mv {LEGACY_FILE} {FILE}`")
}

/// The warning for a repository that has both files and cannot compare them
/// (a commit read): the old one is ignored.
pub fn ignored_note() -> String {
    format!("warning: {LEGACY_FILE} is ignored; run `git rm {LEGACY_FILE}`")
}

/// Reads `greenspotter.toml` under `root`. A missing file is an empty
/// configuration, not an error.
/// When only the old name exists it is read, and `note` carries the one
/// warning line for the binary to print; the library never prints.
pub fn load(root: &Path) -> Result<Config, ConfigError> {
    let read = |name: &str| -> Result<Option<String>, ConfigError> {
        let path = root.join(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(cause) => Err(ConfigError::Unreadable {
                path: path.display().to_string(),
                cause: cause.to_string(),
            }),
        }
    };
    let current = read(FILE)?;
    let legacy = read(LEGACY_FILE)?;
    Ok(match (current, legacy) {
        (Some(text), None) => Config::named(text, FILE, None),
        (Some(text), Some(old)) => {
            let note = if old == text {
                ignored_note()
            } else {
                format!(
                    "warning: {LEGACY_FILE} is ignored and its contents differ from {FILE}; merge them first, then run `git rm {LEGACY_FILE}`"
                )
            };
            Config::named(text, FILE, Some(note))
        }
        (None, Some(old)) => Config::named(old, LEGACY_FILE, Some(old_name_note())),
        (None, None) => Config::named(String::new(), FILE, None),
    })
}

impl Config {
    fn named(text: String, file: &'static str, note: Option<String>) -> Config {
        Config { text, file, note }
    }

    pub fn from_text(text: &str) -> Config {
        Config::named(text.to_string(), FILE, None)
    }

    /// Like `from_text`, for text read from the file named `file`.
    pub fn from_text_named(text: &str, file: &'static str) -> Config {
        Config::named(text.to_string(), file, None)
    }

    /// The file the text came from.
    pub fn file(&self) -> &'static str {
        self.file
    }

    /// The warning for the binary to print once on stderr, if any.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The validated `[architecture]` contract; `None` when the file has no
    /// such section.
    pub fn contract(&self) -> Result<Option<Contract>, ContractError> {
        contract::parse(&self.text, self.file)
    }
}

/// The validation entry point: loads the configuration under `root` and
/// checks its `[architecture]` section.
pub fn validate(root: &Path) -> Result<Option<Contract>, ConfigError> {
    load(root)?.contract().map_err(ConfigError::Contract)
}

/// `validate`, plus the legacy-name warning the caller prints once.
pub fn validate_noted(root: &Path) -> Result<(Option<Contract>, Option<String>), ConfigError> {
    let config = load(root)?;
    let contract = config.contract().map_err(ConfigError::Contract)?;
    Ok((contract, config.note().map(str::to_string)))
}
