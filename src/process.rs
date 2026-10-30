//! Running another program: the one place adr-lint starts a subprocess.
//! Stdin is /dev/null unless the caller passes input, and the exit and both pipe reads are bounded by one
//! deadline (reliability.every-wait-is-bounded).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
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

/// Runs `program args…` in `dir` and returns its standard output as text;
/// bytes that are not UTF-8 are replaced.
pub(crate) fn run(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
) -> Result<String, Failure> {
    run_bytes(program, args, dir, deadline)
        .map(|printed| String::from_utf8_lossy(&printed).into_owned())
}

/// Runs `program args…` in `dir` and returns its standard output unchanged.
/// The exit and both pipe reads share one deadline; a non-zero exit reports
/// the first line of standard error.
pub(crate) fn run_bytes(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
) -> Result<Vec<u8>, Failure> {
    run_fed(program, args, dir, deadline, None)
}

/// The same as `run_bytes`, with `input` written to the program's standard
/// input on a thread of its own, so a program that prints while it reads
/// cannot deadlock against the write.
pub(crate) fn run_with_input(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
    input: Vec<u8>,
) -> Result<Vec<u8>, Failure> {
    run_fed(program, args, dir, deadline, Some(input))
}

fn run_fed(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
    input: Option<Vec<u8>>,
) -> Result<Vec<u8>, Failure> {
    match launch(program, args, dir, deadline, input)? {
        Finished::TimedOut => Err(timed_out(program, deadline)),
        Finished::Exited {
            status,
            printed,
            complained,
        } => {
            if !status.success() {
                let complaint = String::from_utf8_lossy(&complained);
                let first = complaint.lines().next().unwrap_or("").trim();
                return Err(Failure::Failed(format!(
                    "`{program}` failed ({status}): {first}"
                )));
            }
            Ok(printed)
        }
    }
}

/// How a program ended, with its exit status as a value.
pub(crate) enum Ended {
    Exited(ExitStatus),
    TimedOut,
}

/// Runs `program args…` in `dir` with stdin from /dev/null and returns how it
/// ended, whatever its exit status; only a program that cannot be started or
/// waited for is a `Failure`.
pub(crate) fn run_status(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
) -> Result<Ended, Failure> {
    Ok(match launch(program, args, dir, deadline, None)? {
        Finished::TimedOut => Ended::TimedOut,
        Finished::Exited { status, .. } => Ended::Exited(status),
    })
}

enum Finished {
    TimedOut,
    Exited {
        status: ExitStatus,
        printed: Vec<u8>,
        complained: Vec<u8>,
    },
}

fn launch(
    program: &str,
    args: &[&str],
    dir: &Path,
    deadline: Duration,
    input: Option<Vec<u8>>,
) -> Result<Finished, Failure> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        thread::spawn(move || {
            // A program that exits before reading everything closes the pipe;
            // its exit status reports that, not this write.
            stdin.write_all(&input).ok();
        });
    }
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
                return Ok(Finished::TimedOut);
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
    let (Some(printed), Some(complained)) = (
        collect(&stdout, started, deadline),
        collect(&stderr, started, deadline),
    ) else {
        return Ok(Finished::TimedOut);
    };
    Ok(Finished::Exited {
        status,
        printed,
        complained,
    })
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

/// Reads a stream to its end on a thread and sends the bytes back.
fn drain<R: Read + Send + 'static>(stream: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut stream) = stream {
            // A stream that stops mid-read leaves what was read; the exit
            // status and the caller's decode report the failure.
            stream.read_to_end(&mut text).ok();
        }
        // The receiver is gone only after a timeout was reported.
        sender.send(text).ok();
    });
    receiver
}

/// The bytes a drain read, waiting no longer than the deadline allows;
/// `None` when the stream was still open at the deadline.
fn collect(
    receiver: &mpsc::Receiver<Vec<u8>>,
    started: Instant,
    deadline: Duration,
) -> Option<Vec<u8>> {
    let left = deadline.saturating_sub(started.elapsed());
    receiver.recv_timeout(left).ok()
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
