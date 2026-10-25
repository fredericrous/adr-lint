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

#[test]
fn ruling_record_check_exits_0_1_2_and_3_and_has_its_help() {
    let directory = std::env::temp_dir().join(format!("adr-lint-ruling-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let tree = "a".repeat(64);
    let good = format!(
        r#"{{"format":"adr-lint.ruling/1","review":"r1","tool":"relais","repo":"adr-lint","tree":"{tree}","candidate":"{tree}","rule":"names.reveal-intent","pattern_id":"names.reveal-intent/rust-weak-name","pattern":"p","ruling":"fine","clause":"c","at":"2026-10-24T12:00:00Z"}}"#
    );
    let valid = directory.join("valid.jsonl");
    let invalid = directory.join("invalid.jsonl");
    std::fs::write(&valid, format!("{good}\n")).expect("write");
    std::fs::write(&invalid, format!("{good}\n{{}}\n")).expect("write");
    let check = |path: &std::path::Path| {
        adr_lint(&["ruling-record", "--check", path.to_str().expect("utf-8")])
    };
    assert_eq!(check(&valid).status.code(), Some(0));
    let bad = check(&invalid);
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("line 2: "));
    // A readable file that is not UTF-8 is a bad line (1), not an unreadable file (3).
    let not_utf8 = directory.join("not-utf8.jsonl");
    let mut bytes = format!("{good}\n").into_bytes();
    bytes.extend_from_slice(&[0xff, b'\n']);
    std::fs::write(&not_utf8, bytes).expect("write");
    let latin = check(&not_utf8);
    assert_eq!(latin.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&latin.stderr).contains("line 2: not UTF-8"));
    assert_eq!(check(&directory.join("absent")).status.code(), Some(3));
    assert_eq!(adr_lint(&["ruling-record"]).status.code(), Some(2));
    let usage = String::from_utf8_lossy(&adr_lint(&["help", "ruling-record"]).stdout).into_owned();
    assert!(
        usage.starts_with("usage: adr-lint ruling-record --check <file>"),
        "{usage}"
    );
    std::fs::remove_dir_all(&directory).ok(); // a leftover temp dir fails nothing
}
