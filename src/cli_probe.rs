//! `adr-lint cli-probe`: runs a program's own command line and checks the
//! help, version and unknown-flag exit statuses that cli.help.help-and-version
//! and cli.basics.exit-status decide. The program is started through
//! `process`, so stdin is /dev/null and each invocation has one deadline.

use std::path::Path;
use std::time::Duration;

use crate::coverage::Failure;
use crate::process;

pub(crate) const USAGE: &str = "\
usage: adr-lint cli-probe <program> [args...]

Runs the program with stdin from /dev/null and a 10 s limit per invocation,
and checks that `-h`, `--help`, `--bogus --help` and `--version` each exit 0
and that an unknown flag exits 2. Any `args` are put before each flag, so
`adr-lint cli-probe mytool sub` probes `mytool sub --help`.

Prints one PASS or FAIL line per check, with the invocation and the exit
code observed.

exit status:
  0  every check passed
  1  a check failed
  2  usage error
  3  the program could not be started, or an invocation timed out
";

const DEADLINE: Duration = Duration::from_secs(10);
const UNKNOWN_FLAG: &str = "--bogus-flag-for-probe";

/// The flags put after the program's own arguments, and the exit status
/// each must give.
const CHECKS: [(&[&str], i32); 5] = [
    (&["-h"], 0),
    (&["--help"], 0),
    (&["--bogus", "--help"], 0),
    (&["--version"], 0),
    (&[UNKNOWN_FLAG], 2),
];

/// What one invocation did.
#[derive(Debug, PartialEq, Eq)]
enum Observed {
    Exit(i32),
    TimedOut,
    /// Killed by a signal, so there is no exit code.
    Abnormal(String),
}

/// Runs the checks and returns how many failed.
pub(crate) fn run(args: &[String]) -> Result<usize, Failure> {
    let Some(program) = args.first() else {
        return Err(Failure::Usage("no program to probe".to_string()));
    };
    if program.starts_with('-') {
        return Err(Failure::Usage(format!(
            "`{program}` is a flag, not a program"
        )));
    }
    let leading: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let mut failed = 0;
    for (flags, expected) in CHECKS {
        let invocation: Vec<&str> = leading.iter().chain(flags.iter()).copied().collect();
        let shown = format!("{program} {}", invocation.join(" "));
        match observe(program, &invocation) {
            Err(cause) => return Err(Failure::CouldNotRun(cause)),
            Ok(Observed::TimedOut) => {
                return Err(Failure::CouldNotRun(format!(
                    "`{shown}` did not exit within {} s",
                    DEADLINE.as_secs()
                )))
            }
            Ok(Observed::Exit(code)) if code == expected => {
                println!("PASS {shown} (exit {code})");
            }
            Ok(Observed::Exit(code)) => {
                println!("FAIL {shown} (exit {code}, expected {expected})");
                failed += 1;
            }
            Ok(Observed::Abnormal(how)) => {
                println!("FAIL {shown} ({how}, expected exit {expected})");
                failed += 1;
            }
        }
    }
    Ok(failed)
}

/// Runs one invocation; `Err` means the program could not be started.
fn observe(program: &str, invocation: &[&str]) -> Result<Observed, String> {
    match process::run_status(program, invocation, Path::new("."), DEADLINE) {
        Ok(process::Ended::TimedOut) => Ok(Observed::TimedOut),
        Ok(process::Ended::Exited(status)) => Ok(match status.code() {
            Some(code) => Observed::Exit(code),
            None => Observed::Abnormal(status.to_string()),
        }),
        Err(failure) => Err(failure.into_message()),
    }
}
