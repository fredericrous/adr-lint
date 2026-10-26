//! `adr-lint sync` through the real binary, over scratch directories.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const COMMENT: &str = "# adr-lint sync: general.linters-police-suppressions";

fn adr_lint(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .arg("sync")
        .args(extra)
        .arg("-C")
        .arg(dir)
        .output()
        .expect("adr-lint runs")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adr-lint-sync-{}-{name}", std::process::id()));
    fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may be absent
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn package(dir: &Path, name: &str, extra: &str) {
    fs::create_dir_all(dir.join("src")).expect("src dir");
    fs::write(dir.join("src/lib.rs"), "").expect("lib");
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra}"),
    )
    .expect("manifest");
}

const HAND_WRITTEN: &str =
    "# my notes\n[dependencies]   # none\n\n[features]\nzeta = []\nalpha = []\n";

#[test]
fn a_package_without_lints_gains_both_keys_and_keeps_the_rest() {
    let dir = scratch("single");
    package(&dir, "single", HAND_WRITTEN);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(text.contains(&format!(
        "[lints.clippy]\n{COMMENT}\nallow_attributes = \"deny\"\nallow_attributes_without_reason = \"deny\"\n"
    )), "{text}");
    assert!(text.contains(HAND_WRITTEN), "{text}");
}

#[test]
fn a_workspace_gets_the_workspace_table_and_every_member_opts_in() {
    let dir = scratch("workspace");
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"a\", \"b\"]\n\n[workspace.lints.clippy]\ndbg_macro = \"deny\"\n",
    )
    .unwrap();
    package(&dir.join("a"), "a", "\n[lints]\nworkspace = true\n");
    package(&dir.join("b"), "b", "");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let root = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(root.contains("[workspace.lints.clippy]"), "{root}");
    assert!(root.contains("allow_attributes = \"deny\""), "{root}");
    assert!(
        root.contains("allow_attributes_without_reason = \"deny\""),
        "{root}"
    );
    let member = fs::read_to_string(dir.join("b/Cargo.toml")).unwrap();
    assert!(member.contains("[lints]\nworkspace = true"), "{member}");
    let second = adr_lint(&dir, &["--check"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
}

#[test]
fn a_weaker_level_is_a_conflict_and_nothing_is_written() {
    let dir = scratch("conflict");
    package(
        &dir,
        "conflict",
        "\n[lints.clippy]\nallow_attributes = \"warn\"\n",
    );
    let before = fs::read(dir.join("Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("Cargo.toml:7: clippy.allow_attributes is warn, weaker than deny (general.linters-police-suppressions)"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), before);
}

#[test]
fn forbid_is_kept() {
    let dir = scratch("forbid");
    package(
        &dir,
        "forbid",
        "\n[lints.clippy]\nallow_attributes = \"forbid\"\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(text.contains("allow_attributes = \"forbid\""), "{text}");
    assert!(
        text.contains("allow_attributes_without_reason = \"deny\""),
        "{text}"
    );
}

#[test]
fn an_old_rust_version_skips_rust() {
    let dir = scratch("old");
    package(&dir, "old", "rust-version = \"1.80\"\n");
    let before = fs::read(dir.join("Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("sync: rust-version 1.80 is below 1.81"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), before);
}

#[test]
fn a_second_sync_changes_nothing() {
    let dir = scratch("twice");
    package(&dir, "twice", "");
    assert_eq!(adr_lint(&dir, &[]).status.code(), Some(0));
    let first = fs::read(dir.join("Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("sync: already in sync"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), first);
}

#[test]
fn check_fails_before_sync_and_passes_after_without_writing() {
    let dir = scratch("check");
    package(&dir, "check", "");
    let before = fs::read(dir.join("Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &["--check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("clippy.allow_attributes is missing"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), before);
    assert_eq!(adr_lint(&dir, &[]).status.code(), Some(0));
    let synced = fs::read(dir.join("Cargo.toml")).unwrap();
    assert_eq!(adr_lint(&dir, &["--check"]).status.code(), Some(0));
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), synced);
}

#[test]
fn no_cargo_toml_is_nothing_to_do() {
    let dir = scratch("none");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("sync: no Cargo.toml, nothing to do for Rust"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_manifest_is_refused() {
    let dir = scratch("symlink");
    package(&dir.join("real"), "real", "");
    std::os::unix::fs::symlink(dir.join("real/Cargo.toml"), dir.join("Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).contains("Cargo.toml is a symlink"),
        "{}",
        stderr(&out)
    );
}

#[cfg(unix)]
#[test]
fn the_file_mode_is_kept() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("mode");
    package(&dir, "mode", "");
    fs::set_permissions(dir.join("Cargo.toml"), fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(adr_lint(&dir, &[]).status.code(), Some(0));
    let mode = fs::metadata(dir.join("Cargo.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn a_member_that_overrides_lints_blocks_a_workspace_write() {
    let dir = scratch("member-override");
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"a\"]\n",
    )
    .unwrap();
    package(&dir.join("a"), "a", "\n[lints.rust]\nunused = \"warn\"\n");
    let root_before = fs::read(dir.join("Cargo.toml")).unwrap();
    let member_before = fs::read(dir.join("a/Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("a/Cargo.toml:6: lints are set here without `workspace = true`"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), root_before);
    assert_eq!(fs::read(dir.join("a/Cargo.toml")).unwrap(), member_before);
}

#[test]
fn a_directory_named_is_missing_does_not_hide_a_conflict() {
    // The space matters: the old check searched the printed message for
    // " is missing", which a path segment `/is missing/` does not contain.
    let dir = scratch("dir-is-missing");
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"member is missing\"]\n",
    )
    .unwrap();
    package(
        &dir.join("member is missing"),
        "gap",
        "\n[lints.rust]\nunused = \"warn\"\n",
    );
    let root_before = fs::read(dir.join("Cargo.toml")).unwrap();
    let member_before = fs::read(dir.join("member is missing/Cargo.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(fs::read(dir.join("Cargo.toml")).unwrap(), root_before);
    assert_eq!(
        fs::read(dir.join("member is missing/Cargo.toml")).unwrap(),
        member_before
    );
}
