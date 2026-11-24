//! greenspotter: which decided code rules a tree violates mechanically
//! (decisions ADR-0044). `coverage` is in place; the other commands land
//! in the pull requests that follow, each with its own tests.

mod candidates;
mod check;
mod checked_by;
mod cli_probe;
mod coverage;
mod generated;
mod graph;
mod greentopo;
mod layers;
mod learn;
mod process;
mod reviewed_by;
mod ruling;
mod sync;

use std::process::ExitCode;

use coverage::Failure;

const USAGE: &str = "\
usage: greenspotter <command> [flags]

Checks a tree against the decided code rules a linter matches exactly, and
marks candidate lines for the reviewers that judge the rest (ADR-0044).

commands:
  coverage    every active code rule has a Checked-by line and a mechanism
  check       the exact rules, over staged or named files
  candidates  the candidate lines in a diff, for a reviewer to rule on
  ruling-record  `--check <file>` validates a file of ruling records
  learn       what reviewers' `fine:` rulings say about each pattern
  sync        write the settings that make clippy, ruff, pyright (Python),
              golangci-lint (Go) and eslint (TypeScript) police suppressions
  layers      show the [architecture] contract: `summary`, `explain [PATH]`
  cli-probe   black-box help, version and exit-code checks of a CLI
  help        `greenspotter help <command>` prints that command's usage

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
        Some("learn") => learn::USAGE,
        Some("sync") => sync::USAGE,
        Some("layers") => layers::USAGE,
        Some("cli-probe") => cli_probe::USAGE,
        _ => USAGE,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Help wins over anything else on the line, valid or not
    // (cli.help.help-and-version: `greenspotter --bogus --help` prints help).
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{}", usage_of(args.first().map(String::as_str)));
        return ExitCode::SUCCESS;
    }
    // Version is honoured the same way, wherever it sits on the line.
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("greenspotter {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match args.first().map(String::as_str) {
        Some("help") => match args.get(1).map(String::as_str) {
            None => {
                print!("{USAGE}");
                ExitCode::SUCCESS
            }
            Some(
                command @ ("coverage" | "check" | "candidates" | "ruling-record" | "learn" | "sync"
                | "layers" | "cli-probe"),
            ) => {
                print!("{}", usage_of(Some(command)));
                ExitCode::SUCCESS
            }
            Some(other) => {
                eprintln!("greenspotter: no help for `{other}`; try `greenspotter --help`");
                ExitCode::from(EXIT_USAGE)
            }
        },
        Some("check") => match check::run(&args[1..]) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter check: {cause}; try `greenspotter check --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter check: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("layers") if args.len() == 1 => {
            eprint!("{}", layers::USAGE);
            ExitCode::from(EXIT_USAGE)
        }
        Some("layers") => match layers::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter layers: {cause}; try `greenspotter layers --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter layers: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("candidates") => match candidates::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter candidates: {cause}; try `greenspotter candidates --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter candidates: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("cli-probe") => match cli_probe::run(&args[1..]) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter cli-probe: {cause}; try `greenspotter cli-probe --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter cli-probe: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("sync") => match sync::run(&args[1..]) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter sync: {cause}; try `greenspotter sync --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter sync: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("learn") => match learn::run(&args[1..]) {
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter learn: {cause}; try `greenspotter learn --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter learn: {cause}");
                ExitCode::from(EXIT_COULD_NOT_RUN)
            }
        },
        Some("ruling-record") => {
            match ruling::run(&args[1..]) {
                Ok(None) => ExitCode::SUCCESS,
                Ok(Some(problem)) => {
                    eprintln!("greenspotter ruling-record: {problem}");
                    ExitCode::from(EXIT_FINDINGS)
                }
                Err(Failure::Usage(cause)) => {
                    eprintln!("greenspotter ruling-record: {cause}; try `greenspotter ruling-record --help`");
                    ExitCode::from(EXIT_USAGE)
                }
                Err(Failure::CouldNotRun(cause)) => {
                    eprintln!("greenspotter ruling-record: {cause}");
                    ExitCode::from(EXIT_COULD_NOT_RUN)
                }
            }
        }
        Some("coverage") => match coverage::run(&args[1..]) {
            Ok(problems) if problems.is_empty() => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(EXIT_FINDINGS),
            Err(Failure::Usage(cause)) => {
                eprintln!("greenspotter coverage: {cause}; try `greenspotter coverage --help`");
                ExitCode::from(EXIT_USAGE)
            }
            Err(Failure::CouldNotRun(cause)) => {
                eprintln!("greenspotter coverage: {cause}");
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
            eprintln!("greenspotter: unknown {kind} `{unknown}`; try `greenspotter --help`");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
