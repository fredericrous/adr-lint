//! The command line through the real binary: help, version and usage errors.

use std::process::{Command, Output};

fn adr_lint(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(args)
        .output()
        .expect("adr-lint runs")
}

#[test]
fn help_and_version_exit_0_on_stdout() {
    for args in [vec!["-h"], vec!["--help"], vec!["--bogus", "--help"]] {
        let out = adr_lint(&args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with("usage: adr-lint"),
            "{args:?}"
        );
    }
    for flag in ["-V", "--version"] {
        let out = adr_lint(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        let printed = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            printed.trim(),
            format!("adr-lint {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn no_command_and_an_unknown_one_exit_2_on_stderr() {
    for args in [vec![], vec!["--bogus"], vec!["frobnicate"]] {
        let out = adr_lint(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(!out.stderr.is_empty(), "{args:?}");
    }
}
