//! adr-lint: which decided code rules a tree violates mechanically
//! (decisions ADR-0044). `coverage` is in place; the other commands land
//! in the pull requests that follow, each with its own tests.

mod aval;
mod candidates;
mod check;
mod checked_by;
mod coverage;
mod generated;
mod process;
mod ruling;
mod sync;

use std::process::ExitCode;

use coverage::Failure;

const USAGE: &str = "\
usage: adr-lint <command> [flags]

Checks a tree against the decided code rules a linter matches exactly, and
marks candidate lines for the reviewers that judge the rest (ADR-0044).

commands:
  coverage    every active code rule has a Checked-by line and a mechanism
  check       the exact rules, over staged or named files
  candidates  the candidate lines in a diff, for a reviewer to rule on
  ruling-record  `--check <file>` validates a file of ruling records
  sync        write the settings that make clippy, ruff, pyright and
              golangci-lint police suppressions
  help        `adr-lint help <command>` prints that command's usage

commands (land in later releases):
  cli-probe   black-box help, version and exit-code checks of a CLI
  learn       what reviewers' `fine:` rulings say about each pattern

flags:
  -h, --help     this text
  -V, --version  the version

exit status:
  0  success
  1  findings, a stale block or a failed probe
  2  usage error
  3  could not run (a tool missing or failing, the map unreadable)
";

/// Exit statuses (cli.basics.exit-status); the table is in USAGE.
const EXIT_FINDINGS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_COULD_NOT_RUN: u8 = 3;

/// The usage of the command named first on the line, else the top level.
fn usage_of(command: Option<&str>) -> &'static str {
    match command {
        Some("coverage") => coverage::USAGE,
        Some("check") => check::USAGE,
        Some("candidates") => candidates::USAGE,
        Some("ruling-record") => ruling::USAGE,
        Some("sync") => sync::USAGE,
        _ => USAGE,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Help wins over anything else on the line, valid or not
    // (cli.help.help-and-version: `adr-lint --bogus --help` prints help).
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{}", usage_of(args.first().map(String::as_str)));
        return ExitCode::SUCCESS;
    }
    // Version is honoured the same way, wherever it sits on the line.
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("adr-lint {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match args.first().map(String::as_str) {
        Some("help") => match args.get(1).map(String::as_str) {
            None => {
                print!("{USAGE}");
                ExitCode::SUCCESS
            }
            Some(command @ ("coverage" | "check" | "candidates" | "ruling-record" | "sync")) => {
                print!("{}", usage_of(Some(command)));
                ExitCode::SUCCESS
            }
            Some(other) => {
                eprintln!("adr-lint: no help for `{other}`; try `adr-lint --help`");
                ExitCode::from(EXIT_USAGE)
            }
        },
        Some("check") => match check::run(&args[1..]) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("adr-lint check: {cause}; try `adr-lint check --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("adr-lint check: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("candidates") => match candidates::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(Failure::Usage(cause)) => {
                eprintln!("adr-lint candidates: {cause}; try `adr-lint candidates --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("adr-lint candidates: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("sync") => match sync::run(&args[1..]) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("adr-lint sync: {cause}; try `adr-lint sync --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("adr-lint sync: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("ruling-record") => match ruling::run(&args[1..]) {
            Ok(None) => ExitCode::SUCCESS,
            Ok(Some(problem)) => {
                eprintln!("adr-lint ruling-record: {problem}");
                ExitCode::from(EXIT_FINDINGS)
            }
            Err(Failure::Usage(cause)) => {
                eprintln!("adr-lint ruling-record: {cause}; try `adr-lint ruling-record --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("adr-lint ruling-record: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("coverage") => match coverage::run(&args[1..]) {
            Ok(problems) if problems.is_empty() => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("adr-lint coverage: {cause}; try `adr-lint coverage --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("adr-lint coverage: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        None => {
            eprint!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
        Some(unknown) => {
            let kind = if unknown.starts_with('-') {
                "flag"
            } else {
                "command"
            };
            eprintln!("adr-lint: unknown {kind} `{unknown}`; try `adr-lint --help`");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
