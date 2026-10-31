//! `adr-lint cli-probe`: runs a program's own command line and checks the
//! help, version and unknown-flag exit statuses that cli.help.help-and-version
//! and cli.basics.exit-status decide. The program is started through
//! `process`, so stdin is /dev/null and each invocation has one deadline.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::coverage::Failure;
use crate::process;

pub(crate) const USAGE: &str = "\
usage: adr-lint cli-probe [<program> [args...]]

Runs the program with stdin from /dev/null and a 10 s limit per invocation,
and checks that `-h`, `--help`, `--bogus --help` and `--version` each exit 0
and that an unknown flag exits 2. Any `args` are put before each flag, so
`adr-lint cli-probe mytool sub` probes `mytool sub --help`.

Prints one PASS or FAIL line per check, with the invocation and the exit
code observed.

With no program, reads `adr-lint.toml` at the repository root (the git
top level, else the current directory): for each `[[cli.programs]]` entry it
runs `build` (an argv array, from the root, no shell, 10 minute limit), starts
`run` (the program path, then any leading args) once with `--version` as a
warm-up (60 s limit, result ignored: macOS scans a fresh executable on its
first exec), then runs the five checks on it. With no adr-lint.toml or no
`[cli]` programs it prints `cli-probe: no [cli] programs declared` to stderr
and exits 0 (docs/formats.md).

exit status:
  0  every check passed
  1  a check failed
  2  usage error
  3  the program could not be started, or an invocation timed out, or a
     build failed or timed out, or adr-lint.toml is unreadable
";

const DEADLINE: Duration = Duration::from_secs(10);
const BUILD_DEADLINE: Duration = Duration::from_secs(600);
const BUILD_STDERR_TAIL_LINES: usize = 20;
const WARM_UP_DEADLINE: Duration = Duration::from_secs(60);
const CONFIG_FILE: &str = "adr-lint.toml";
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
        return run_declared();
    };
    if program.starts_with('-') {
        return Err(Failure::Usage(format!(
            "`{program}` is a flag, not a program"
        )));
    }
    let leading: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    probe(program, &leading, Path::new("."))
}

/// A program declared in `adr-lint.toml`.
#[derive(Debug, PartialEq, Eq)]
struct Declared {
    name: String,
    build: Vec<String>,
    run: Vec<String>,
}

/// Builds and probes every program `adr-lint.toml` declares.
fn run_declared() -> Result<usize, Failure> {
    let root = repository_root();
    let path = root.join(CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(cause) => {
            return Err(Failure::CouldNotRun(format!(
                "cannot read {}: {cause}",
                path.display()
            )))
        }
    };
    let programs = parse_declared(&text)?;
    if programs.is_empty() {
        eprintln!("cli-probe: no [cli] programs declared");
        return Ok(0);
    }
    let mut failed = 0;
    for declared in &programs {
        build(declared, &root)?;
        let program = resolve_program(&declared.run[0], &root);
        let program = program.as_str();
        let leading: Vec<&str> = declared.run[1..].iter().map(String::as_str).collect();
        process::run_status(
            program,
            &leading_with(&leading, "--version"),
            &root,
            WARM_UP_DEADLINE,
        )
        .ok(); // warm-up only: the probe that follows is what is judged
        failed += probe(program, &leading, &root)?;
    }
    Ok(failed)
}

/// A program path with a separator is relative to the repository root, not
/// to whatever the platform resolves a relative path against after `chdir`.
fn resolve_program(program: &str, root: &Path) -> String {
    if program.contains('/') && Path::new(program).is_relative() {
        root.join(program).to_string_lossy().into_owned()
    } else {
        program.to_string()
    }
}

fn leading_with<'a>(leading: &[&'a str], flag: &'a str) -> Vec<&'a str> {
    leading.iter().copied().chain([flag]).collect()
}

/// The git top level, else the current directory.
fn repository_root() -> PathBuf {
    process::run(
        "git",
        &["rev-parse", "--show-toplevel"],
        Path::new("."),
        process::DEADLINE,
    )
    .ok()
    .map(|printed| PathBuf::from(printed.trim()))
    .filter(|root| !root.as_os_str().is_empty())
    .unwrap_or_else(|| PathBuf::from("."))
}

fn build(declared: &Declared, root: &Path) -> Result<(), Failure> {
    let arguments: Vec<&str> = declared.build[1..].iter().map(String::as_str).collect();
    match process::run_status_with_stderr(&declared.build[0], &arguments, root, BUILD_DEADLINE) {
        Ok(process::EndedWithStderr::Exited { status, .. }) if status.success() => Ok(()),
        Ok(process::EndedWithStderr::Exited { status, stderr }) => {
            Err(Failure::CouldNotRun(format!(
                "the build of `{}` failed ({status}){}",
                declared.name,
                stderr_tail(&stderr)
            )))
        }
        Ok(process::EndedWithStderr::TimedOut { stderr }) => Err(Failure::CouldNotRun(format!(
            "the build of `{}` did not finish within {} s{}",
            declared.name,
            BUILD_DEADLINE.as_secs(),
            stderr_tail(&stderr)
        ))),
        Err(failure) => Err(Failure::CouldNotRun(format!(
            "the build of `{}` could not start: {}",
            declared.name,
            failure.into_message()
        ))),
    }
}

/// The last lines of a build's standard error, ready to append to a message;
/// empty when the build printed nothing there.
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    let tail = lines[lines.len().saturating_sub(BUILD_STDERR_TAIL_LINES)..].join("\n");
    if tail.is_empty() {
        String::new()
    } else {
        format!(":\n{tail}")
    }
}

fn parse_declared(text: &str) -> Result<Vec<Declared>, Failure> {
    let bad = |cause: String| Failure::CouldNotRun(format!("{CONFIG_FILE}: {cause}"));
    let table: toml::Table = text.parse().map_err(|cause| bad(format!("{cause}")))?;
    let Some(cli) = table.get("cli") else {
        return Ok(Vec::new());
    };
    let Some(entries) = cli.get("programs") else {
        return Ok(Vec::new());
    };
    let entries = entries
        .as_array()
        .ok_or_else(|| bad("`cli.programs` must be an array of tables".to_string()))?;
    let mut programs = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let argv = |key: &str| -> Result<Vec<String>, Failure> {
            let strings: Option<Vec<String>> = entry
                .get(key)
                .and_then(toml::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().map(str::to_string))
                        .collect::<Option<Vec<String>>>()
                })
                .unwrap_or(None);
            strings.filter(|argv| !argv.is_empty()).ok_or_else(|| {
                bad(format!(
                    "cli.programs entry {} needs `{key}` as a non-empty array of strings",
                    index + 1
                ))
            })
        };
        let run = argv("run")?;
        let name = entry
            .get("name")
            .and_then(toml::Value::as_str)
            .map_or_else(|| run[0].clone(), str::to_string);
        programs.push(Declared {
            name,
            build: argv("build")?,
            run,
        });
    }
    Ok(programs)
}

/// Runs the five checks on `program` and returns how many failed.
fn probe(program: &str, leading: &[&str], dir: &Path) -> Result<usize, Failure> {
    let mut failed = 0;
    for (flags, expected) in CHECKS {
        let invocation: Vec<&str> = leading.iter().chain(flags.iter()).copied().collect();
        let shown = format!("{program} {}", invocation.join(" "));
        match observe(program, &invocation, dir) {
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
fn observe(program: &str, invocation: &[&str], dir: &Path) -> Result<Observed, String> {
    match process::run_status(program, invocation, dir, DEADLINE) {
        Ok(process::Ended::TimedOut) => Ok(Observed::TimedOut),
        Ok(process::Ended::Exited(status)) => Ok(match status.code() {
            Some(code) => Observed::Exit(code),
            None => Observed::Abnormal(status.to_string()),
        }),
        Err(failure) => Err(failure.into_message()),
    }
}
