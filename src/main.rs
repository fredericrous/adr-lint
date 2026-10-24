//! adr-lint: which decided code rules a tree violates mechanically
//! (decisions ADR-0044). `coverage` is in place; the other commands land
//! in the pull requests that follow, each with its own tests.

mod aval;
mod checked_by;
mod coverage;

use std::process::ExitCode;

use coverage::Failure;

const USAGE: &str = "\
usage: adr-lint <command> [flags]

Checks a tree against the decided code rules a linter matches exactly, and
marks candidate lines for the reviewers that judge the rest (ADR-0044).

commands:
  coverage    every active code rule has a Checked-by line and a mechanism

commands (land in later releases):
  check       the exact rules, over staged or named files
  sync        write the native lint presets between generated-block markers
  candidates  the candidate lines in a diff, for a reviewer to rule on
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Help wins over anything else on the line, valid or not
    // (cli.help.help-and-version: `adr-lint --bogus --help` prints help).
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        if args.first().map(String::as_str) == Some("coverage") {
            print!("{}", coverage::USAGE);
        } else {
            print!("{USAGE}");
        }
        return ExitCode::SUCCESS;
    }
    // Version is honoured the same way, wherever it sits on the line.
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("adr-lint {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match args.first().map(String::as_str) {
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
