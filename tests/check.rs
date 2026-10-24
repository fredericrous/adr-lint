//! `adr-lint check` through the real binary, over fixture files in a scratch
//! directory, with the real ast-grep 0.51.0 when it is on PATH.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-check-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).expect("fixture");
    }

    fn check(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .arg("check")
            .args(args)
            .current_dir(&self.0)
            .env_remove("AMONT_FILES")
            .env_remove("ADR_LINT_AST_GREP")
            .env_remove("NO_COLOR")
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

/// True when the real ast-grep 0.51.0 is available; says why not otherwise.
fn real_ast_grep() -> bool {
    let printed = Command::new("ast-grep").arg("--version").output();
    let found = printed
        .as_ref()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    if found == "ast-grep 0.51.0" {
        true
    } else {
        eprintln!("SKIPPED: ast-grep 0.51.0 is not on PATH (found `{found}`)");
        false
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_clean_file_exits_0() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("clean");
    scratch.write("clean.rs", "fn main() {}\n");
    let out = scratch.check(&["clean.rs"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty());
}

#[test]
fn one_finding_per_rule_exits_1_with_grouped_output() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("findings");
    scratch.write("ignored.rs", "#[test]\n    #[ignore]\nfn slow() {}\n");
    scratch.write("skipped.ts", "it.skip(\"later\", () => {});\n");
    scratch.write(
        "swallow.ts",
        "const p = effect.catchAll(() => Effect.void);\n",
    );
    scratch.write("loose.py", "# ruff: noqa\nx = 1\n");
    scratch.write("named.ts", "interface IThing {}\n");
    let out = scratch.check(&[
        "ignored.rs",
        "skipped.ts",
        "swallow.ts",
        "loose.py",
        "named.ts",
    ]);
    let stdout = text(&out.stdout);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stdout}{stderr}");
    for expected in [
        "ignored.rs:2:5: tests.first-properties: ",
        "skipped.ts:1:1: tests.first-properties: ",
        "swallow.ts:1:11: errors.never-swallowed: ",
        "loose.py:1:1: general.no-disabled-safety: ",
        "named.ts:1:1: names.reveal-intent: ",
    ] {
        assert!(stdout.contains(expected), "{expected}\n{stdout}");
    }
    for rule in [
        "tests.first-properties",
        "errors.never-swallowed",
        "general.no-disabled-safety",
        "names.reveal-intent",
    ] {
        assert_eq!(
            stdout.matches(&format!("aval rule {rule}\n")).count(),
            1,
            "{stdout}"
        );
        assert_eq!(stdout.matches(&format!("rule {rule} (")).count(), 1);
    }
    assert!(
        stderr.contains("adr-lint: allow <rule-id> -- <reason>"),
        "{stderr}"
    );
    assert_eq!(stderr.lines().last(), Some("5 findings in 4 rules"));
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn a_waiver_with_a_reason_removes_the_finding() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("waived");
    scratch.write(
        "waived.ts",
        "// adr-lint: allow tests.first-properties -- tracked in the backlog, owner: ci\nit.skip(\"later\", () => {});\n",
    );
    let out = scratch.check(&["waived.ts"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
}

#[test]
fn a_waiver_with_no_reason_is_a_finding_and_waives_nothing() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("reasonless");
    scratch.write(
        "waived.ts",
        "// adr-lint: allow names.reveal-intent --\ninterface IThing {}\n",
    );
    let out = scratch.check(&["waived.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("waived.ts:1:4: adr-lint.waiver-without-reason: "),
        "{stdout}"
    );
    assert!(
        stdout.contains("waived.ts:2:1: names.reveal-intent: "),
        "{stdout}"
    );
}

#[test]
fn a_comment_on_the_line_or_above_excuses_a_skip_and_a_catch_all() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("commented");
    scratch.write(
        "commented.ts",
        "// flaky upstream, owner: ci\nit.skip(\"a\", () => {});\nit.skip(\"b\", () => {}); // owner: ci\n\
         const p = effect.catchAll(() => Effect.void); // cleanup failure is irrelevant\n",
    );
    let out = scratch.check(&["commented.ts"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
}

#[test]
fn json_prints_only_the_array_on_stdout() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("json");
    scratch.write("skipped.ts", "it.skip(\"later\", () => {});\n");
    let out = scratch.check(&["--json", "skipped.ts"]);
    assert_eq!(out.status.code(), Some(1));
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("only JSON");
    let first = &parsed[0];
    assert_eq!(first["path"], "skipped.ts");
    assert_eq!(first["line"], 1);
    assert_eq!(first["col"], 1);
    assert_eq!(first["rule"], "tests.first-properties");
    assert!(first["message"].is_string());
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
}

#[test]
fn rule_filters_the_findings_and_an_unknown_rule_is_a_usage_error() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("filter");
    scratch.write(
        "both.ts",
        "interface IThing {}\nit.skip(\"later\", () => {});\n",
    );
    let out = scratch.check(&["--rule=names.reveal-intent", "both.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout.contains("names.reveal-intent") && !stdout.contains("tests.first-properties"));
    let unknown = scratch.check(&["--rule", "no.such-rule", "both.ts"]);
    assert_eq!(unknown.status.code(), Some(2));
    let bogus = scratch.check(&["--bogus"]);
    assert_eq!(bogus.status.code(), Some(2));
}

#[test]
fn paths_come_from_amont_files_when_none_are_named() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("amont-files");
    scratch.write("skipped.ts", "it.skip(\"later\", () => {});\n");
    scratch.write("clean.ts", "export const one = 1;\n");
    let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .arg("check")
        .current_dir(&scratch.0)
        .env("AMONT_FILES", "clean.ts\nskipped.ts\n")
        .stdin(Stdio::null())
        .output()
        .expect("adr-lint runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("skipped.ts:1:1: tests.first-properties"));
}

#[test]
fn a_closed_stdout_exits_0_quietly() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Scratch::new("epipe");
    scratch.write("skipped.ts", "it.skip(\"later\", () => {});\n");
    let mut child = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["check", "skipped.ts"])
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("adr-lint runs");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("exits");
    // The report is small enough to fit the pipe buffer before the reader
    // closes on some systems; either way the run must not fail noisily.
    assert!(matches!(out.status.code(), Some(0) | Some(1)));
    if out.status.code() == Some(0) {
        assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    }
}

/// A shell script is the only way to fake a program, so the fake-ast-grep
/// tests are unix-only.
#[cfg(unix)]
#[test]
fn another_ast_grep_version_exits_3_naming_the_install() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("fake");
    scratch.write("clean.rs", "fn main() {}\n");
    scratch.write("ast-grep", "#!/bin/sh\necho 'ast-grep 0.50.0'\n");
    let fake: &Path = &scratch.0.join("ast-grep");
    std::fs::set_permissions(fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["check", "clean.rs"])
        .current_dir(&scratch.0)
        .env("ADR_LINT_AST_GREP", fake)
        .stdin(Stdio::null())
        .output()
        .expect("adr-lint runs");
    assert_eq!(out.status.code(), Some(3));
    let stderr = text(&out.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("brew install ast-grep"), "{stderr}");
    assert!(stderr.contains("0.50.0"), "{stderr}");
}

#[test]
fn a_missing_ast_grep_exits_3() {
    let scratch = Scratch::new("missing");
    scratch.write("clean.rs", "fn main() {}\n");
    let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["check", "clean.rs"])
        .current_dir(&scratch.0)
        .env("ADR_LINT_AST_GREP", "adr-lint-no-such-ast-grep")
        .stdin(Stdio::null())
        .output()
        .expect("adr-lint runs");
    assert_eq!(out.status.code(), Some(3));
    assert!(text(&out.stderr).contains("brew install ast-grep"));
}

#[test]
fn help_prints_a_commands_usage() {
    for command in ["coverage", "check"] {
        let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .args(["help", command])
            .output()
            .expect("adr-lint runs");
        assert_eq!(out.status.code(), Some(0), "{command}");
        assert!(text(&out.stdout).starts_with(&format!("usage: adr-lint {command}")));
    }
}
