//! The parts of adr-lint that are read by more than one command: the loader
//! of `adr-lint.toml` and the architecture contract it declares (ADR-0056).

pub mod config;
pub mod contract;
