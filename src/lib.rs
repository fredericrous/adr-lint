//! The parts of greenspotter that are read by more than one command: the loader
//! of `greenspotter.toml` and the architecture contract it declares (ADR-0056).

pub mod config;
pub mod contract;
