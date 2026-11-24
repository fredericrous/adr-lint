//! The one fallback read of the old config file name (ADR-0073): until the
//! fleet sweep has renamed it, a repository with only the old file keeps its
//! contract, and the tool says so once on stderr.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// The old file name, in two pieces so the rename sed leaves it alone.
const OLD: &str = concat!("adr", "-lint.toml");

const CONTRACT: &str = r#"[[architecture.layers]]
name = "domain"
modules = ["crate::domain"]

[[architecture.layers]]
name = "adapters"
modules = ["crate::adapters"]
"#;

const BROKEN: &str = r#"[[architecture.layers]]
name = "domain"
modules = ["crate::domain"]

[[architecture.layers]]
name = "domain"
modules = ["crate::other"]
"#;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("greenspotter-legacy-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_greenspotter"))
        .args(args)
        .current_dir(dir)
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .output()
        .expect("greenspotter runs")
}

fn stderr_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn only_the_old_file_is_read_with_one_warning_and_the_same_stdout() {
    let old = scratch("old");
    std::fs::write(old.join(OLD), CONTRACT).expect("fixture");
    let new = scratch("new");
    std::fs::write(new.join("greenspotter.toml"), CONTRACT).expect("fixture");
    let from_old = run(&old, &["layers", "summary"]);
    let from_new = run(&new, &["layers", "summary"]);
    assert_eq!(from_old.status.code(), Some(0), "{from_old:?}");
    assert_eq!(from_old.stdout, from_new.stdout);
    assert!(!from_old.stdout.is_empty());
    let lines = stderr_lines(&from_old);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains(&format!(
            "warning: {OLD} is the old name; run `git mv {OLD} greenspotter.toml`"
        )),
        "{lines:?}"
    );
    assert!(stderr_lines(&from_new).is_empty());
}

#[test]
fn both_files_read_the_new_one_and_warn_once() {
    let dir = scratch("both");
    std::fs::write(dir.join("greenspotter.toml"), CONTRACT).expect("fixture");
    std::fs::write(dir.join(OLD), CONTRACT).expect("fixture");
    let same = run(&dir, &["layers", "summary"]);
    assert_eq!(same.status.code(), Some(0));
    let lines = stderr_lines(&same);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains(&format!("{OLD} is ignored; run `git rm {OLD}`")),
        "{lines:?}"
    );
    assert!(!lines[0].contains("differ"), "{lines:?}");
    std::fs::write(dir.join(OLD), BROKEN).expect("fixture");
    let differ = run(&dir, &["layers", "summary"]);
    let lines = stderr_lines(&differ);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(
        "its contents differ from greenspotter.toml; merge them first, then run `git rm"
    ));
}

#[test]
fn a_contract_error_names_the_file_it_read() {
    let dir = scratch("broken");
    std::fs::write(dir.join(OLD), BROKEN).expect("fixture");
    let out = run(&dir, &["layers", "summary"]);
    assert_ne!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains(&format!("{OLD}:")), "{text}");
    assert!(!text.contains("greenspotter.toml:"), "{text}");
}

#[test]
fn cli_probe_reads_the_old_file_in_a_repository() {
    let dir = scratch("probe");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(
        dir.join(OLD),
        "[[cli.programs]]\nbuild = [\"true\"]\nrun = [\"/usr/bin/false\"]\n",
    )
    .expect("fixture");
    let out = run(&dir, &["cli-probe"]);
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("is the old name"), "{text}");
    assert!(!text.contains("no [cli] programs declared"), "{text}");
}
