//! `adr-lint sync`, the Go half, through the real binary over scratch directories.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NO_RUST: &str = "sync: no Cargo.toml, nothing to do for Rust";
const NOT_VERIFIED: &str = "sync: golangci-lint not installed; config not verified";
const SETTINGS: &str = "    nolintlint:\n      require-explanation: true\n      require-specific: true\n      allow-unused: false\n";

/// Runs `sync` with a PATH that holds no golangci-lint, so the result does not
/// depend on the machine.
fn adr_lint(dir: &Path, extra: &[&str]) -> Output {
    let empty = dir.join("empty-path");
    fs::create_dir_all(&empty).expect("empty PATH dir");
    Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .env("PATH", &empty)
        .arg("sync")
        .args(extra)
        .arg("-C")
        .arg(dir)
        .output()
        .expect("adr-lint runs")
}

fn stderr_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_string)
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adr-lint-sync-go-{}-{name}", std::process::id()));
    fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may be absent
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn put(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).expect("write fixture");
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).expect("read file")
}

fn shown(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

fn wrote(dir: &Path, name: &str) -> String {
    format!("sync: wrote {}", shown(dir, name))
}

const DURO: &str = "version: \"2\"\n\n# Why this file exists.\n# Defaults already on: errcheck, govet.\nlinters:\n  enable:\n    - gosec # security patterns\n    - misspell\n\n  exclusions:\n    rules:\n      # Test fixtures trip these.\n      - path: _test\\.go\n        linters:\n          - errcheck\n          - gosec\n";

const IN_SYNC: &str = "version: \"2\"\nlinters:\n  default: all\n  settings:\n    nolintlint:\n      require-explanation: true\n      require-specific: true\n      allow-unused: false\n";

#[test]
fn duro_operators_shape_gets_the_linter_in_its_list_style_and_the_settings_block() {
    let dir = scratch("a");
    put(&dir, ".golangci.yml", DURO);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        format!(
            "version: \"2\"\n\n# Why this file exists.\n# Defaults already on: errcheck, govet.\nlinters:\n  enable:\n    - gosec # security patterns\n    - misspell\n    - nolintlint\n\n  exclusions:\n    rules:\n      # Test fixtures trip these.\n      - path: _test\\.go\n        linters:\n          - errcheck\n          - gosec\n  settings:\n{SETTINGS}"
        )
    );
    assert_eq!(
        stderr_lines(&out),
        vec![NO_RUST.to_string(), wrote(&dir, ".golangci.yml")]
    );
}

#[test]
fn garage_operators_default_none_shape_gets_the_linter_appended() {
    let dir = scratch("b");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nrun:\n  allow-parallel-runners: true\nlinters:\n  default: none\n  enable:\n    - errcheck\n    - unused\n  exclusions:\n    generated: lax\nformatters:\n  enable:\n    - gofmt\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        format!(
            "version: \"2\"\nrun:\n  allow-parallel-runners: true\nlinters:\n  default: none\n  enable:\n    - errcheck\n    - unused\n    - nolintlint\n  exclusions:\n    generated: lax\n  settings:\n{SETTINGS}formatters:\n  enable:\n    - gofmt\n"
        )
    );
}

#[test]
fn a_file_without_linters_gets_a_block_linters_mapping() {
    let dir = scratch("c");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\n\noutput:\n  show-stats: true\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        format!(
            "version: \"2\"\n\noutput:\n  show-stats: true\nlinters:\n  enable:\n    - nolintlint\n  settings:\n{SETTINGS}"
        )
    );
}

#[test]
fn default_all_gets_the_settings_but_no_enable_entry() {
    let dir = scratch("d");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  default: all\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(read(&dir, ".golangci.yml"), IN_SYNC);
}

#[test]
fn a_four_space_file_keeps_its_list_and_gets_two_space_blocks() {
    let dir = scratch("d2");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n    enable: [gosec, misspell]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        "version: \"2\"\nlinters:\n    enable: [gosec, misspell, nolintlint]\n    settings:\n      nolintlint:\n        require-explanation: true\n        require-specific: true\n        allow-unused: false\n"
    );
}

#[test]
fn nolintlint_in_disable_is_a_conflict_with_its_line_and_nothing_is_written() {
    let dir = scratch("e");
    let original = "version: \"2\"\nlinters:\n  enable:\n    - gosec\n  disable:\n    - unused\n    - nolintlint\n";
    put(&dir, ".golangci.yml", original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_RUST.to_string(),
            format!(
                "{}:7: golangci-lint disables nolintlint in linters.disable (general.linters-police-suppressions)",
                shown(&dir, ".golangci.yml")
            )
        ]
    );
    assert_eq!(read(&dir, ".golangci.yml"), original);
}

#[test]
fn a_weaker_setting_is_a_conflict_with_its_line_and_nothing_is_written() {
    for (key, value, wanted) in [
        ("require-explanation", "false", "true"),
        ("require-specific", "false", "true"),
        ("allow-unused", "true", "false"),
    ] {
        let dir = scratch(&format!("f-{key}"));
        let original = format!(
            "version: \"2\"\nlinters:\n  default: all\n  settings:\n    nolintlint:\n      {key}: {value}\n"
        );
        put(&dir, ".golangci.yml", &original);
        let out = adr_lint(&dir, &[]);
        assert_eq!(out.status.code(), Some(1), "{key}");
        assert_eq!(
            stderr_lines(&out),
            vec![
                NO_RUST.to_string(),
                format!(
                    "{}:6: golangci-lint nolintlint setting {key} is {value}, wanted {wanted} (general.linters-police-suppressions)",
                    shown(&dir, ".golangci.yml")
                )
            ],
            "{key}"
        );
        assert_eq!(read(&dir, ".golangci.yml"), original, "{key}");
    }
}

#[test]
fn an_exclusion_rule_that_lists_nolintlint_is_a_conflict() {
    let dir = scratch("g");
    let original = "version: \"2\"\nlinters:\n  enable:\n    - nolintlint\n  exclusions:\n    rules:\n      - path: _test\\.go\n        linters:\n          - errcheck\n          - nolintlint\n";
    put(&dir, ".golangci.yml", original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_RUST.to_string(),
            format!(
                "{}:10: golangci-lint exclusions.rules lists nolintlint, which silences it (general.linters-police-suppressions)",
                shown(&dir, ".golangci.yml")
            )
        ]
    );
    assert_eq!(read(&dir, ".golangci.yml"), original);
}

#[test]
fn only_missing_settings_are_added_and_other_keys_stay() {
    let dir = scratch("h");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  default: all\n  settings:\n    govet:\n      enable-all: true\n    nolintlint:\n      allow-unused: false # keep\n      allow-no-explanation: [gosec]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        "version: \"2\"\nlinters:\n  default: all\n  settings:\n    govet:\n      enable-all: true\n    nolintlint:\n      allow-unused: false # keep\n      allow-no-explanation: [gosec]\n      require-explanation: true\n      require-specific: true\n"
    );
}

#[test]
fn a_settings_block_without_nolintlint_gets_one_beside_its_siblings() {
    let dir = scratch("h2");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  default: all\n  settings:\n    govet:\n      enable-all: true\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        format!(
            "version: \"2\"\nlinters:\n  default: all\n  settings:\n    govet:\n      enable-all: true\n{SETTINGS}"
        )
    );
}

#[test]
fn toml_json_and_unversioned_configs_are_refused_with_exit_3() {
    for (name, text, mentions) in [
        (".golangci.toml", "version = \"2\"\n", "is not YAML"),
        (".golangci.json", "{\"version\": \"2\"}\n", "is not YAML"),
        (
            ".golangci.yml",
            "linters:\n  enable:\n    - gosec\n",
            "no `version: \"2\"`",
        ),
        (
            ".golangci.yml",
            "version: \"1\"\nlinters:\n  enable: [gosec]\n",
            "no `version: \"2\"`",
        ),
    ] {
        let dir = scratch(&format!("i-{}", name.trim_start_matches('.')));
        put(&dir, name, text);
        let out = adr_lint(&dir, &[]);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{name}: {:?}",
            stderr_lines(&out)
        );
        let stderr = stderr_lines(&out).join("\n");
        assert!(stderr.contains(&shown(&dir, name)), "{name}: {stderr}");
        assert!(stderr.contains(mentions), "{name}: {stderr}");
        assert_eq!(read(&dir, name), text, "{name}");
    }
}

#[test]
fn golangci_yml_wins_over_golangci_yaml() {
    let dir = scratch("j");
    let other = "version: \"2\"\nlinters:\n  default: all\n";
    put(&dir, ".golangci.yml", other);
    put(&dir, ".golangci.yaml", other);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(read(&dir, ".golangci.yml"), IN_SYNC);
    assert_eq!(read(&dir, ".golangci.yaml"), other);
}

#[test]
fn a_yaml_extension_is_edited_when_it_is_the_only_config() {
    let dir = scratch("j2");
    put(
        &dir,
        ".golangci.yaml",
        "version: \"2\"\nlinters:\n  default: all\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&wrote(&dir, ".golangci.yaml")));
    assert_eq!(read(&dir, ".golangci.yaml"), IN_SYNC);
}

#[test]
fn a_symlinked_config_is_refused() {
    let dir = scratch("k");
    let target = "version: \"2\"\nlinters:\n  default: all\n";
    put(&dir, "real.yml", target);
    symlink(dir.join("real.yml"), dir.join(".golangci.yml")).expect("symlink");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr_lines(&out)
        .join("\n")
        .contains(&format!("{} is a symlink", shown(&dir, ".golangci.yml"))));
    assert_eq!(read(&dir, "real.yml"), target);
}

#[test]
fn the_file_mode_is_kept() {
    let dir = scratch("k2");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  default: all\n",
    );
    fs::set_permissions(dir.join(".golangci.yml"), fs::Permissions::from_mode(0o640))
        .expect("chmod");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(read(&dir, ".golangci.yml"), IN_SYNC);
    let mode = fs::metadata(dir.join(".golangci.yml"))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn a_second_sync_is_already_in_sync_and_check_goes_from_1_to_0() {
    let dir = scratch("l");
    put(&dir, ".golangci.yml", DURO);
    let before = adr_lint(&dir, &["--check"]);
    assert_eq!(before.status.code(), Some(1));
    let file = shown(&dir, ".golangci.yml");
    assert_eq!(
        stderr_lines(&before),
        vec![
            NO_RUST.to_string(),
            format!("{file}: golangci-lint nolintlint is not enabled"),
            format!("{file}: golangci-lint nolintlint setting require-explanation is missing"),
            format!("{file}: golangci-lint nolintlint setting require-specific is missing"),
            format!("{file}: golangci-lint nolintlint setting allow-unused is missing"),
            NOT_VERIFIED.to_string(),
        ]
    );
    assert_eq!(read(&dir, ".golangci.yml"), DURO);

    assert_eq!(adr_lint(&dir, &[]).status.code(), Some(0));
    let synced = read(&dir, ".golangci.yml");
    assert_ne!(synced, DURO);
    let again = adr_lint(&dir, &[]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        stderr_lines(&again),
        vec![NO_RUST.to_string(), "sync: already in sync".to_string()]
    );
    assert_eq!(read(&dir, ".golangci.yml"), synced);

    let after = adr_lint(&dir, &["--check"]);
    assert_eq!(after.status.code(), Some(0));
    assert_eq!(
        stderr_lines(&after),
        vec![NO_RUST.to_string(), NOT_VERIFIED.to_string()]
    );
}

#[test]
fn check_says_when_golangci_lint_is_missing() {
    let dir = scratch("m");
    put(&dir, ".golangci.yml", IN_SYNC);
    let out = adr_lint(&dir, &["--check"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr_lines(&out),
        vec![NO_RUST.to_string(), NOT_VERIFIED.to_string()]
    );
}

#[test]
fn check_reports_a_failing_verify_of_the_installed_tool() {
    let dir = scratch("m2");
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let fake = bin.join("golangci-lint");
    fs::write(&fake, "#!/bin/sh\necho 'bad config' >&2\nexit 3\n").expect("fake tool");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");
    put(&dir, ".golangci.yml", IN_SYNC);
    let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .env("PATH", format!("{}:/bin:/usr/bin", bin.display()))
        .args(["sync", "--check", "-C"])
        .arg(&dir)
        .output()
        .expect("adr-lint runs");
    assert_eq!(out.status.code(), Some(1));
    let lines = stderr_lines(&out);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], NO_RUST);
    assert!(
        lines[1].starts_with(&format!(
            "{}: golangci-lint config verify failed: ",
            shown(&dir, ".golangci.yml")
        )) && lines[1].ends_with("bad config"),
        "{}",
        lines[1]
    );
}

#[test]
fn a_repository_without_a_golangci_config_prints_no_go_line() {
    let dir = scratch("n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr_lines(&out), vec![NO_RUST.to_string()]);
    let check = adr_lint(&dir, &["--check"]);
    assert_eq!(check.status.code(), Some(0));
    assert_eq!(stderr_lines(&check), vec![NO_RUST.to_string()]);
}

#[test]
fn a_quoted_boolean_setting_is_named_a_string() {
    // golangci-lint 2.4.0 `config verify` rejects it: "got string, want boolean".
    let dir = scratch("quoted-bool");
    let original = "version: \"2\"\nlinters:\n  enable:\n    - nolintlint\n  settings:\n    nolintlint:\n      require-explanation: \"true\"\n      require-specific: true\n      allow-unused: false\n";
    put(&dir, ".golangci.yml", original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    let expected = format!(
        "{}:7: golangci-lint nolintlint setting require-explanation is the string \"true\", not a boolean, wanted true (general.linters-police-suppressions)",
        shown(&dir, ".golangci.yml")
    );
    assert!(
        stderr_lines(&out).contains(&expected),
        "{:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(&dir, ".golangci.yml"), original, "nothing written");
}

#[test]
fn a_tab_indented_file_is_refused_with_exit_3_and_left_alone() {
    let dir = scratch("tab");
    let text = "version: \"2\"\nlinters:\n\tenable:\n\t\t- errcheck\n";
    put(&dir, ".golangci.yml", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    let stderr = stderr_lines(&out).join("\n");
    assert!(stderr.contains(&shown(&dir, ".golangci.yml")), "{stderr}");
    assert!(stderr.contains("tabs"), "{stderr}");
    assert_eq!(read(&dir, ".golangci.yml"), text);
}

#[test]
fn a_crlf_file_gets_crlf_on_every_inserted_line() {
    let dir = scratch("crlf");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\r\nlinters:\r\n  enable:\r\n    - errcheck\r\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        "version: \"2\"\r\nlinters:\r\n  enable:\r\n    - errcheck\r\n    - nolintlint\r\n  settings:\r\n    nolintlint:\r\n      require-explanation: true\r\n      require-specific: true\r\n      allow-unused: false\r\n"
    );
}

#[test]
fn a_linters_key_holding_only_a_comment_keeps_the_comment() {
    let dir = scratch("comment-only");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  # nothing yet\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        "version: \"2\"\nlinters:\n  enable:\n    - nolintlint\n  settings:\n    nolintlint:\n      require-explanation: true\n      require-specific: true\n      allow-unused: false\n  # nothing yet\n"
    );
}

#[test]
fn a_trailing_comment_on_the_last_enable_item_stays_on_it() {
    let dir = scratch("trailing");
    put(
        &dir,
        ".golangci.yml",
        "version: \"2\"\nlinters:\n  enable:\n    - gosec\n    - errcheck # keep\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".golangci.yml"),
        "version: \"2\"\nlinters:\n  enable:\n    - gosec\n    - errcheck # keep\n    - nolintlint\n  settings:\n    nolintlint:\n      require-explanation: true\n      require-specific: true\n      allow-unused: false\n"
    );
}

#[test]
fn a_tab_after_spaces_before_a_key_is_refused() {
    let dir = scratch("tab-after-spaces");
    let original = "version: \"2\"\nlinters:\n  \tenable:\n    - errcheck\n";
    put(&dir, ".golangci.yml", original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    let says = stderr_lines(&out).iter().any(|line| {
        line.contains(".golangci.yml") && line.contains("line 3 is indented with tabs")
    });
    assert!(says, "{:?}", stderr_lines(&out));
    assert_eq!(read(&dir, ".golangci.yml"), original);
}
