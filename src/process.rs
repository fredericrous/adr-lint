//! Running another program: the one place adr-lint starts a subprocess.
//! Stdin is /dev/null, and the exit and both pipe reads are bounded by one
//! deadline (reliability.every-wait-is-bounded).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Why a program gave no answer.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The program could not be started because it does not exist.
    Missing(String),
    /// Started, but timed out, could not be waited for or exited non-zero.
    Failed(String),
}

impl Failure {
    pub(crate) fn into_message(self) -> String {
        match self {
            Failure::Missing(message) | Failure::Failed(message) => message,
        }
    }
}

/// Runs `program args…` in `dir` and returns its standard output.
pub(crate) fn run(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
) -> Result<String, Failure> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|cause| {
            let message = format!("cannot run `{program}`: {cause}");
            if cause.kind() == std::io::ErrorKind::NotFound {
                Failure::Missing(message)
            } else {
                Failure::Failed(message)
            }
        })?;
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
                return Err(timed_out(program, deadline));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(cause) => {
                return Err(Failure::Failed(format!(
                    "cannot wait for `{program}`: {cause}"
                )))
            }
        }
    };
    // A grandchild that inherited a pipe can hold it open after the program
    // exits: the reads are bounded by what is left of the same deadline.
    let printed = collect(&stdout, program, started, deadline)?;
    let complained = collect(&stderr, program, started, deadline)?;
    if !status.success() {
        let first = complained.lines().next().unwrap_or("").trim();
        return Err(Failure::Failed(format!(
            "`{program}` failed ({status}): {first}"
        )));
    }
    Ok(printed)
}

fn timed_out(program: &str, deadline: Duration) -> Failure {
    Failure::Failed(format!(
        "`{program}` did not answer within {}",
        describe(deadline)
    ))
}

/// Whole seconds from one second up, milliseconds below it, so a short
/// deadline is never reported as "0 s".
fn describe(deadline: Duration) -> String {
    if deadline < Duration::from_secs(1) {
        format!("{} ms", deadline.as_millis())
    } else {
        format!("{} s", deadline.as_secs())
    }
}

/// Reads a stream to its end on a thread and sends the text back.
fn drain<R: Read + Send + 'static>(stream: Option<R>) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stream) = stream {
            // A stream that stops mid-read leaves what was read; the exit
            // status and the caller's decode report the failure.
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
) -> Result<String, Failure> {
    let left = deadline.saturating_sub(started.elapsed());
    receiver
        .recv_timeout(left)
        .map_err(|_| timed_out(program, deadline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deadline_under_a_second_is_described_in_milliseconds() {
        assert_eq!(describe(Duration::from_millis(300)), "300 ms");
        assert_eq!(describe(Duration::from_secs(30)), "30 s");
        assert_eq!(describe(Duration::from_millis(1500)), "1 s");
    }

    #[test]
    fn a_program_that_does_not_exist_is_missing() {
        let answer = run(
            "adr-lint-no-such-program",
            &[],
            Path::new("."),
            Duration::from_secs(1),
        );
        assert!(matches!(answer, Err(Failure::Missing(_))));
    }
}
