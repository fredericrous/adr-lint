//! `adr-lint cli-probe` through the real binary, against small shell scripts
//! written to a temp dir and probed as `/bin/sh <script>`. The script is read
//! by `sh`, never executed itself: macOS scans a fresh executable on its first
//! exec, one at a time, which took 18–52 s for 24 scripts started together
//! and timed the probe out.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const SHELL: &str = "/bin/sh";

fn adr_lint(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(args)
        .output()
        .expect("adr-lint runs")
}

/// A script that removes its temp dir when dropped.
struct Fixture {
    path: PathBuf,
}

impl std::ops::Deref for Fixture {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(dir) = self.path.parent() {
            // Best-effort cleanup of a temp dir; a leftover is harmless.
            std::fs::remove_dir_all(dir).ok();
        }
    }
}

/// Writes a shell script into a fresh temp dir and returns its path.
fn fixture(name: &str, body: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("adr-lint-probe-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("prog");
    std::fs::write(&path, format!("{body}\n")).expect("script");
    Fixture { path }
}

/// Probes `/bin/sh <script>`; the script is the leading argument.
fn probe(script: &Path) -> (Option<i32>, String, String) {
    let out = adr_lint(&["cli-probe", SHELL, script.to_str().expect("utf-8 path")]);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// How the probe shows an invocation of the script.
fn shown(script: &Path) -> String {
    format!("{SHELL} {}", script.display())
}

#[test]
fn a_conforming_program_passes_every_check() {
    let path = fixture(
        "ok",
        r#"for arg; do case "$arg" in -h|--help|--version) exit 0;; esac; done; exit 2"#,
    );
    let program = shown(&path);
    let (code, out, err) = probe(&path);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out,
        format!(
            "PASS {program} -h (exit 0)\n\
             PASS {program} --help (exit 0)\n\
             PASS {program} --bogus --help (exit 0)\n\
             PASS {program} --version (exit 0)\n\
             PASS {program} --bogus-flag-for-probe (exit 2)\n"
        )
    );
}

#[test]
fn a_program_that_fails_on_bogus_with_help_fails_that_line_and_the_run() {
    let path = fixture(
        "bogus-help",
        r#"[ "$*" = "--bogus --help" ] && exit 2
for arg; do case "$arg" in -h|--help|--version) exit 0;; esac; done; exit 2"#,
    );
    let program = shown(&path);
    let (code, out, err) = probe(&path);
    assert_eq!(code, Some(1), "{err}");
    assert!(out.contains(&format!(
        "FAIL {program} --bogus --help (exit 2, expected 0)\n"
    )));
    assert!(out.contains(&format!("PASS {program} -h (exit 0)\n")));
    assert!(out.contains(&format!("PASS {program} --bogus-flag-for-probe (exit 2)\n")));
}

#[test]
fn an_unknown_flag_that_exits_0_fails() {
    let path = fixture("lenient", "exit 0");
    let program = shown(&path);
    let (code, out, err) = probe(&path);
    assert_eq!(code, Some(1), "{err}");
    assert!(out.contains(&format!(
        "FAIL {program} --bogus-flag-for-probe (exit 0, expected 2)\n"
    )));
}

#[test]
fn a_program_that_never_returns_exits_3_naming_the_invocation() {
    let path = fixture("hang", "exec sleep 60");
    let program = shown(&path);
    let started = Instant::now();
    let (code, _, err) = probe(&path);
    assert_eq!(code, Some(3));
    assert!(
        err.contains(&format!("`{program} -h` did not exit within 10 s")),
        "{err}"
    );
    assert!(started.elapsed().as_secs() < 20);
}

#[test]
fn a_missing_program_exits_3() {
    let out = adr_lint(&["cli-probe", "/nonexistent/adr-lint-probe-prog"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot run"), "{err}");
}

#[test]
fn no_program_exits_2_with_usage() {
    let out = adr_lint(&["cli-probe"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no program to probe"), "{err}");
    assert!(err.contains("adr-lint cli-probe --help"), "{err}");
}

#[test]
fn a_program_that_reads_stdin_to_the_end_sees_dev_null_and_answers() {
    let path = fixture(
        "stdin",
        r#"cat >/dev/null
for arg; do case "$arg" in -h|--help|--version) exit 0;; esac; done; exit 2"#,
    );
    // Had stdin not been /dev/null, `cat` would block until the 10 s deadline
    // and the probe would exit 3, so the exit status is the whole check.
    let (code, _, err) = probe(&path);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn an_error_line_that_looks_like_a_timeout_is_still_an_exit_code() {
    let path = fixture(
        "lookalike",
        r#"echo "did not answer within 10 s" >&2
for arg; do case "$arg" in -h|--help|--version) exit 0;; esac; done; exit 2"#,
    );
    let (code, _, err) = probe(&path);
    assert_eq!(code, Some(0), "{err}");
}
