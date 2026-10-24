//! Running `aval`: the one place adr-lint starts the program that says
//! what is decided.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Runs `aval --json rules --level constraint --governs code --full` in
/// `dir` with stdin from /dev/null and returns its standard output.
pub(crate) fn code_constraints(program: &str, dir: &Path) -> Result<String, String> {
    run(program, dir, DEADLINE)
}

fn run(program: &str, dir: &Path, deadline: Duration) -> Result<String, String> {
    let mut child = Command::new(program)
        .args([
            "--json",
            "rules",
            "--level",
            "constraint",
            "--governs",
            "code",
            "--full",
        ])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|cause| format!("cannot run `{program}`: {cause}"))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= deadline => {
                // The process is being abandoned: a failed kill or wait
                // changes nothing about the timeout reported below.
                child.kill().ok();
                child.wait().ok();
                return Err(format!(
                    "`{program}` did not answer within {} s",
                    deadline.as_secs()
                ));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(cause) => return Err(format!("cannot wait for `{program}`: {cause}")),
        }
    };
    let printed = stdout.join().unwrap_or_default();
    let complained = stderr.join().unwrap_or_default();
    if !status.success() {
        let first = complained.lines().next().unwrap_or("").trim();
        return Err(format!("`{program}` failed ({status}): {first}"));
    }
    Ok(printed)
}

fn drain<R: Read + Send + 'static>(stream: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stream) = stream {
            // A stream that stops mid-read leaves what was read; the exit
            // status and the JSON decode report the failure.
            stream.read_to_string(&mut text).ok();
        }
        text
    })
}
