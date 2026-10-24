//! Running `aval`: the one place adr-lint starts the program that says
//! what is decided.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
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
    // A grandchild that inherited a pipe can hold it open after aval exits:
    // the reads are bounded by what is left of the same deadline.
    let printed = collect(&stdout, program, started, deadline)?;
    let complained = collect(&stderr, program, started, deadline)?;
    if !status.success() {
        let first = complained.lines().next().unwrap_or("").trim();
        return Err(format!("`{program}` failed ({status}): {first}"));
    }
    Ok(printed)
}

/// Reads a stream to its end on a thread and sends the text back.
fn drain<R: Read + Send + 'static>(stream: Option<R>) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stream) = stream {
            // A stream that stops mid-read leaves what was read; the exit
            // status and the JSON decode report the failure.
            stream.read_to_string(&mut text).ok();
        }
        // The receiver is gone only after a timeout was reported.
        sender.send(text).ok();
    });
    receiver
}

/// The text a drain read, waiting no longer than the deadline allows.
fn collect(
    receiver: &mpsc::Receiver<String>,
    program: &str,
    started: Instant,
    deadline: Duration,
) -> Result<String, String> {
    let left = deadline.saturating_sub(started.elapsed());
    receiver
        .recv_timeout(left)
        .map_err(|_| format!("`{program}` did not answer within {} s", deadline.as_secs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A script runs only through a shell, so these tests are unix-only.
    #[cfg(unix)]
    fn fake(name: &str, script: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("adr-lint-aval-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("aval");
        std::fs::write(&path, script).expect("fake aval");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[test]
    fn a_grandchild_holding_stdout_open_cannot_outlast_the_deadline() {
        let program = fake("grandchild", "#!/bin/sh\nsleep 5 &\nexit 0\n");
        let started = Instant::now();
        let answer = run(
            program.to_str().expect("utf-8 path"),
            Path::new("."),
            Duration::from_millis(300),
        );
        assert!(answer.is_err_and(|why| why.contains("did not answer")));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }
}
