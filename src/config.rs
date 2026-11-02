//! The one reader of `adr-lint.toml`, at the repository root. Each command
//! takes the text from here and parses the section it owns.

use std::fmt;
use std::path::Path;

use crate::contract::{self, Contract, ContractError};

/// The file's name at the repository root.
pub const FILE: &str = "adr-lint.toml";

/// The text of `adr-lint.toml`; empty when the repository has none.
#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    text: String,
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

/// Reads `adr-lint.toml` under `root`. A missing file is an empty
/// configuration, not an error.
pub fn load(root: &Path) -> Result<Config, ConfigError> {
    let path = root.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Config { text }),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(Config {
            text: String::new(),
        }),
        Err(cause) => Err(ConfigError::Unreadable {
            path: path.display().to_string(),
            cause: cause.to_string(),
        }),
    }
}

impl Config {
    pub fn from_text(text: &str) -> Config {
        Config {
            text: text.to_string(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The validated `[architecture]` contract; `None` when the file has no
    /// such section.
    pub fn contract(&self) -> Result<Option<Contract>, ContractError> {
        contract::parse(&self.text)
    }
}

/// The validation entry point: loads the configuration under `root` and
/// checks its `[architecture]` section.
pub fn validate(root: &Path) -> Result<Option<Contract>, ConfigError> {
    load(root)?.contract().map_err(ConfigError::Contract)
}
