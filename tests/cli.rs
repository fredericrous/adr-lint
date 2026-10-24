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
    for args in [
        vec!["-V"],
        vec!["--version"],
        vec!["--bogus", "--version"],
        vec!["check", "-V"],
    ] {
        let out = adr_lint(&args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
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
    let flag = String::from_utf8_lossy(&adr_lint(&["--bogus"]).stderr).into_owned();
    assert!(flag.contains("unknown flag `--bogus`"), "{flag}");
    let command = String::from_utf8_lossy(&adr_lint(&["frobnicate"]).stderr).into_owned();
    assert!(
        command.contains("unknown command `frobnicate`"),
        "{command}"
    );
}

#[test]
fn candidates_has_its_own_help_and_is_listed_as_available() {
    let usage = String::from_utf8_lossy(&adr_lint(&["help", "candidates"]).stdout).into_owned();
    assert!(
        usage.starts_with("usage: adr-lint candidates --base <ref>"),
        "{usage}"
    );
    let top = String::from_utf8_lossy(&adr_lint(&["--help"]).stdout).into_owned();
    let available = top.split("commands (land in later releases)").next();
    assert!(
        available.is_some_and(|text| text.contains("  candidates  ")),
        "{top}"
    );
}
