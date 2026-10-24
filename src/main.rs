//! adr-lint: which decided code rules a tree violates mechanically
//! (decisions ADR-0044). This is the scaffold: the commands land in the
//! pull requests that follow, each with its own tests.

use std::process::ExitCode;

const USAGE: &str = "\
usage: adr-lint <command> [flags]

Checks a tree against the decided code rules a linter matches exactly, and
marks candidate lines for the reviewers that judge the rest (ADR-0044).

commands (land in later releases):
  check       the exact rules, over staged or named files
  sync        write the native lint presets between generated-block markers
  coverage    every active code rule has a Checked-by line and a mechanism
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

/// Exit status for a usage error (cli.basics.exit-status).
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Help wins over anything else on the line, valid or not
    // (cli.help.help-and-version: `adr-lint --bogus --help` prints help).
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match args.first().map(String::as_str) {
        Some("-V" | "--version") => {
            println!("adr-lint {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        None => {
            eprint!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
        Some(unknown) => {
            eprintln!("adr-lint: unknown command `{unknown}`; try `adr-lint --help`");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
