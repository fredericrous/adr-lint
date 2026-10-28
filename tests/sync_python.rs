//! `adr-lint sync`, the Python half, through the real binary over scratch directories.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn adr_lint(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adr-lint"))
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
    let dir = std::env::temp_dir().join(format!("adr-lint-sync-py-{}-{name}", std::process::id()));
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

fn wrote(dir: &Path, name: &str) -> String {
    format!("sync: wrote {}", dir.join(name).display())
}

fn shown(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

fn pyproject_with_ruff(lint: &str) -> String {
    format!("# my notes\n[project]\nname = \"x\"  # keep\n\n[tool.ruff.lint]\n{lint}")
}

const PYRIGHT_KEYS_JSON: &str =
    "\"enableTypeIgnoreComments\": false,\n  \"reportUnnecessaryTypeIgnoreComment\": \"error\"";

#[test]
fn select_with_ruf_gets_pgh_appended_and_not_ruf100() {
    let dir = scratch("a");
    let original = pyproject_with_ruff("select = [\"E\", \"RUF\"]  # lint\nline-length = 100\n");
    put(&dir, "pyproject.toml", &original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "pyproject.toml"),
        pyproject_with_ruff(
            "select = [\"E\", \"RUF\", \"PGH003\", \"PGH004\"]  # lint\nline-length = 100\n"
        )
    );
    assert!(stderr_lines(&out).contains(&wrote(&dir, "pyproject.toml")));
}

#[test]
fn a_multi_line_select_stays_multi_line_with_its_trailing_comma() {
    let dir = scratch("b");
    put(
        &dir,
        ".ruff.toml",
        "[lint]\nselect = [\n    \"E\",\n    \"F\",\n]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, ".ruff.toml"),
        "[lint]\nselect = [\n    \"E\",\n    \"F\",\n    \"PGH003\",\n    \"PGH004\",\n    \"RUF100\",\n]\n"
    );
}

#[test]
fn a_multi_line_select_without_a_trailing_comma_keeps_that_style() {
    let dir = scratch("b2");
    put(&dir, "ruff.toml", "[lint]\nselect = [\n  \"E\"\n]\n");
    adr_lint(&dir, &[]);
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\n  \"E\",\n  \"PGH003\",\n  \"PGH004\",\n  \"RUF100\"\n]\n"
    );
}

#[test]
fn a_comment_on_the_last_entry_stays_on_that_entry() {
    // sre-agent's real shape: the last entry has a comment after its trailing comma.
    let dir = scratch("b3");
    put(
        &dir,
        "ruff.toml",
        "[lint]\nselect = [\n  \"E\",\n  \"T20\",    # print\n]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\n  \"E\",\n  \"T20\",    # print\n  \"PGH003\",\n  \"PGH004\",\n  \"RUF100\",\n]\n"
    );
}

#[test]
fn a_comment_on_a_last_entry_without_a_comma_stays_on_that_entry() {
    let dir = scratch("b4");
    put(
        &dir,
        "ruff.toml",
        "[lint]\nselect = [\n  \"E\",\n  \"T20\"    # print\n]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\n  \"E\",\n  \"T20\",    # print\n  \"PGH003\",\n  \"PGH004\",\n  \"RUF100\"\n]\n"
    );
}

#[test]
fn no_select_creates_extend_select_with_the_three_codes() {
    let dir = scratch("c");
    put(&dir, "ruff.toml", "[lint]\nignore = [\"E501\"]\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nignore = [\"E501\"]\nextend-select = [\"PGH003\", \"PGH004\", \"RUF100\"]\n"
    );
}

#[test]
fn select_all_is_already_in_sync() {
    let dir = scratch("d");
    put(&dir, "ruff.toml", "[lint]\nselect = [\"ALL\"]\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stderr_lines(&out).contains(&"sync: already in sync".to_string()));
    assert_eq!(read(&dir, "ruff.toml"), "[lint]\nselect = [\"ALL\"]\n");
}

#[test]
fn a_pgh_prefix_covers_both_pgh_codes() {
    let dir = scratch("e");
    put(&dir, "ruff.toml", "[lint]\nselect = [\"PGH\"]\n");
    adr_lint(&dir, &[]);
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\"PGH\", \"RUF100\"]\n"
    );
}

#[test]
fn an_ignore_without_a_more_specific_select_is_a_conflict() {
    for ignored in ["PGH", "PGH003", "RUF100"] {
        let dir = scratch(&format!("f-{ignored}"));
        let original = format!("[lint]\nselect = [\"E\"]\nignore = [\"E501\", \"{ignored}\"]\n");
        put(&dir, "ruff.toml", &original);
        let out = adr_lint(&dir, &[]);
        assert_eq!(out.status.code(), Some(1), "{ignored}");
        let expected = format!(
            "{}:3: ruff ignores \"{ignored}\", which disables {} (general.linters-police-suppressions)",
            shown(&dir, "ruff.toml"),
            if ignored == "RUF100" { "RUF100" } else { "PGH003" }
        );
        assert!(
            stderr_lines(&out).contains(&expected),
            "{ignored}: {:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "ruff.toml"), original, "nothing written");
    }
}

#[test]
fn an_ignore_as_specific_as_the_select_wins_the_tie() {
    // Measured with ruff 0.16.0: `select = ["PGH003"]` with `ignore = ["PGH003"]`
    // reports nothing on a blanket `# type: ignore`, so the code is off.
    let dir = scratch("f-tie");
    let original = "[lint]\nselect = [\"PGH003\", \"PGH004\", \"RUF100\"]\nignore = [\"PGH003\"]\n";
    put(&dir, "ruff.toml", original);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    let expected = format!(
        "{}:3: ruff ignores \"PGH003\", which disables PGH003 (general.linters-police-suppressions)",
        shown(&dir, "ruff.toml")
    );
    assert!(
        stderr_lines(&out).contains(&expected),
        "{:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(&dir, "ruff.toml"), original, "nothing written");
}

#[test]
fn a_specific_select_beats_a_broader_ignore() {
    let dir = scratch("f-specific");
    put(
        &dir,
        "ruff.toml",
        "[lint]\nselect = [\"PGH003\", \"PGH004\", \"RUF100\"]\nignore = [\"PGH\"]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&"sync: already in sync".to_string()));
}

#[test]
fn a_specific_select_beats_a_broader_ignore_for_only_the_code_it_names() {
    let dir = scratch("f-partial");
    put(
        &dir,
        "ruff.toml",
        "[lint]\nselect = [\"PGH003\"]\nignore = [\"PGH\"]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1));
    let lines = stderr_lines(&out);
    assert!(
        !lines.iter().any(|line| line.contains("disables PGH003")),
        "{lines:?}"
    );
    assert!(lines
        .iter()
        .any(|line| line.ends_with("which disables PGH004 (general.linters-police-suppressions)")));
}

#[test]
fn a_per_file_ignore_of_a_covered_code_or_prefix_is_a_conflict() {
    for listed in ["PGH003", "PGH"] {
        let dir = scratch(&format!("g-{listed}"));
        let original = format!(
            "[lint]\nselect = [\"ALL\"]\n\n[lint.per-file-ignores]\n\"tests/*\" = [\"{listed}\"]\n"
        );
        put(&dir, "ruff.toml", &original);
        let out = adr_lint(&dir, &[]);
        assert_eq!(out.status.code(), Some(1), "{listed}");
        let expected = format!(
            "{}:5: ruff per-file-ignores \"tests/*\" lists \"{listed}\", which disables PGH003 (general.linters-police-suppressions)",
            shown(&dir, "ruff.toml")
        );
        assert!(
            stderr_lines(&out).contains(&expected),
            "{:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "ruff.toml"), original);
    }
}

#[test]
fn ruff_configs_are_chosen_in_ruffs_own_order() {
    let dir = scratch("h");
    put(&dir, ".ruff.toml", "[lint]\nselect = [\"ALL\"]\n");
    put(&dir, "ruff.toml", "[lint]\nselect = [\"E\"]\n");
    put(
        &dir,
        "pyproject.toml",
        "[tool.ruff.lint]\nselect = [\"F\"]\n",
    );
    adr_lint(&dir, &[]);
    assert_eq!(read(&dir, "ruff.toml"), "[lint]\nselect = [\"E\"]\n");
    assert_eq!(
        read(&dir, "pyproject.toml"),
        "[tool.ruff.lint]\nselect = [\"F\"]\n"
    );

    fs::remove_file(dir.join(".ruff.toml")).unwrap();
    adr_lint(&dir, &[]);
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\"E\", \"PGH003\", \"PGH004\", \"RUF100\"]\n"
    );
    assert_eq!(
        read(&dir, "pyproject.toml"),
        "[tool.ruff.lint]\nselect = [\"F\"]\n"
    );
}

#[test]
fn a_pyproject_without_tool_ruff_is_not_a_ruff_config() {
    let dir = scratch("h2");
    put(&dir, "pyproject.toml", "[project]\nname = \"x\"\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(read(&dir, "pyproject.toml"), "[project]\nname = \"x\"\n");
}

#[test]
fn a_top_level_ignore_without_a_select_is_a_conflict() {
    let dir = scratch("i-ignore");
    put(&dir, "ruff.toml", "ignore = [\"PGH003\"]\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&format!(
        "{}:1: ruff ignores \"PGH003\", which disables PGH003 (general.linters-police-suppressions)",
        shown(&dir, "ruff.toml")
    )));
    assert_eq!(read(&dir, "ruff.toml"), "ignore = [\"PGH003\"]\n");
}

#[test]
fn a_deprecated_top_level_select_is_the_lint_table() {
    let dir = scratch("i");
    put(&dir, "ruff.toml", "select = [\"E\"]\nline-length = 90\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "ruff.toml"),
        "select = [\"E\", \"PGH003\", \"PGH004\", \"RUF100\"]\nline-length = 90\n"
    );
}

fn pyright_json(indent: &str, trailing: &str) -> String {
    format!("{{\n{indent}\"include\": [\"src\"]\n}}{trailing}")
}

#[test]
fn pyrightconfig_gets_both_keys_with_the_files_own_indentation() {
    for (indent, trailing) in [("  ", "\n"), ("  ", ""), ("    ", "\n"), ("    ", "")] {
        let dir = scratch(&format!("j-{}-{}", indent.len(), trailing.len()));
        put(&dir, "pyrightconfig.json", &pyright_json(indent, trailing));
        let out = adr_lint(&dir, &[]);
        assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
        let expected = format!(
            "{{\n{indent}\"include\": [\"src\"],\n{indent}\"enableTypeIgnoreComments\": false,\n{indent}\"reportUnnecessaryTypeIgnoreComment\": \"error\"\n}}{}",
            trailing
        );
        let text = read(&dir, "pyrightconfig.json");
        assert_eq!(text, expected);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["enableTypeIgnoreComments"], serde_json::json!(false));
        assert_eq!(
            parsed["reportUnnecessaryTypeIgnoreComment"],
            serde_json::json!("error")
        );
        assert!(stderr_lines(&out).contains(&wrote(&dir, "pyrightconfig.json")));
    }
}

#[test]
fn an_empty_pyrightconfig_object_gets_both_keys() {
    let dir = scratch("j-empty");
    put(&dir, "pyrightconfig.json", "{}\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "pyrightconfig.json"),
        format!("{{\n  {PYRIGHT_KEYS_JSON}\n}}\n")
    );
}

#[test]
fn weak_pyright_values_are_conflicts_and_strong_ones_are_accepted() {
    for (ignore, unnecessary, conflict_key) in [
        ("true", "\"error\"", Some("enableTypeIgnoreComments")),
        (
            "false",
            "\"warning\"",
            Some("reportUnnecessaryTypeIgnoreComment"),
        ),
        ("false", "false", Some("reportUnnecessaryTypeIgnoreComment")),
        ("false", "true", None),
        ("false", "\"error\"", None),
    ] {
        let dir = scratch(&format!("k-{ignore}-{}", unnecessary.len()));
        let original = format!(
            "{{\n  \"enableTypeIgnoreComments\": {ignore},\n  \"reportUnnecessaryTypeIgnoreComment\": {unnecessary}\n}}\n"
        );
        put(&dir, "pyrightconfig.json", &original);
        let out = adr_lint(&dir, &[]);
        match conflict_key {
            Some(key) => {
                assert_eq!(out.status.code(), Some(1), "{ignore} {unnecessary}");
                let line = if key == "enableTypeIgnoreComments" {
                    2
                } else {
                    3
                };
                let found = if key == "enableTypeIgnoreComments" {
                    ignore
                } else {
                    unnecessary
                };
                let wanted = if key == "enableTypeIgnoreComments" {
                    "false"
                } else {
                    "\"error\" or true"
                };
                let expected = format!(
                    "{}:{line}: pyright {key} is {found}, wanted {wanted} (general.linters-police-suppressions)",
                    shown(&dir, "pyrightconfig.json")
                );
                assert!(
                    stderr_lines(&out).contains(&expected),
                    "{:?}",
                    stderr_lines(&out)
                );
                assert_eq!(read(&dir, "pyrightconfig.json"), original);
            }
            None => {
                assert_eq!(out.status.code(), Some(0), "{ignore} {unnecessary}");
                assert!(stderr_lines(&out).contains(&"sync: already in sync".to_string()));
                assert_eq!(read(&dir, "pyrightconfig.json"), original);
            }
        }
    }
}

#[test]
fn a_pyrightconfig_that_is_not_strict_json_is_refused() {
    for (name, text) in [
        ("comment", "{\n  // mine\n  \"include\": []\n}\n"),
        ("comma", "{\n  \"include\": [],\n}\n"),
    ] {
        let dir = scratch(&format!("l-{name}"));
        put(&dir, "pyrightconfig.json", text);
        let out = adr_lint(&dir, &[]);
        assert_eq!(out.status.code(), Some(3), "{name}");
        assert!(
            stderr_lines(&out)
                .iter()
                .any(|line| line.contains(&shown(&dir, "pyrightconfig.json"))
                    && line.contains("not strict JSON")),
            "{name}: {:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "pyrightconfig.json"), text);
    }
}

#[test]
fn tool_pyright_is_used_unless_a_pyrightconfig_exists() {
    let dir = scratch("m");
    put(
        &dir,
        "pyproject.toml",
        "[tool.pyright]\ninclude = [\"src\"]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        read(&dir, "pyproject.toml"),
        "[tool.pyright]\ninclude = [\"src\"]\nenableTypeIgnoreComments = false\nreportUnnecessaryTypeIgnoreComment = \"error\"\n"
    );

    let both = scratch("m-both");
    put(
        &both,
        "pyproject.toml",
        "[tool.pyright]\ninclude = [\"src\"]\n",
    );
    put(&both, "pyrightconfig.json", "{}\n");
    adr_lint(&both, &[]);
    assert_eq!(
        read(&both, "pyproject.toml"),
        "[tool.pyright]\ninclude = [\"src\"]\n"
    );
    assert!(read(&both, "pyrightconfig.json").contains("enableTypeIgnoreComments"));
}

#[test]
fn ruff_and_pyright_in_one_pyproject_are_one_write() {
    let dir = scratch("n");
    put(
        &dir,
        "pyproject.toml",
        "[tool.ruff.lint]\nselect = [\"E\"]\n\n[tool.pyright]\ninclude = [\"src\"]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    let wrote_lines: Vec<String> = stderr_lines(&out)
        .into_iter()
        .filter(|line| line.starts_with("sync: wrote "))
        .collect();
    assert_eq!(wrote_lines, vec![wrote(&dir, "pyproject.toml")]);
    assert_eq!(
        read(&dir, "pyproject.toml"),
        "[tool.ruff.lint]\nselect = [\"E\", \"PGH003\", \"PGH004\", \"RUF100\"]\n\n[tool.pyright]\ninclude = [\"src\"]\nenableTypeIgnoreComments = false\nreportUnnecessaryTypeIgnoreComment = \"error\"\n"
    );
}

#[test]
fn a_second_sync_is_in_sync_and_check_tells_before_from_after() {
    let dir = scratch("o");
    put(&dir, "ruff.toml", "[lint]\nselect = [\"E\"]\n");
    put(&dir, "pyrightconfig.json", "{}\n");
    let before = adr_lint(&dir, &["--check"]);
    assert_eq!(before.status.code(), Some(1));
    assert!(stderr_lines(&before).contains(&format!(
        "{}: ruff PGH003 is not selected",
        shown(&dir, "ruff.toml")
    )));
    assert_eq!(read(&dir, "ruff.toml"), "[lint]\nselect = [\"E\"]\n");
    assert_eq!(adr_lint(&dir, &[]).status.code(), Some(0));
    let again = adr_lint(&dir, &[]);
    assert_eq!(again.status.code(), Some(0));
    assert!(stderr_lines(&again).contains(&"sync: already in sync".to_string()));
    assert_eq!(adr_lint(&dir, &["--check"]).status.code(), Some(0));
}

#[test]
fn a_pyproject_with_no_tool_configs_skips_both_and_writes_nothing() {
    let dir = scratch("p");
    put(&dir, "pyproject.toml", "[project]\nname = \"x\"\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    let lines = stderr_lines(&out);
    assert!(lines.contains(&"sync: no ruff config, ruff skipped".to_string()));
    assert!(lines.contains(&"sync: no pyright config, pyright skipped".to_string()));
    assert!(!lines.iter().any(|line| line.starts_with("sync: wrote")));
    assert_eq!(read(&dir, "pyproject.toml"), "[project]\nname = \"x\"\n");
}

#[test]
fn a_repository_without_python_configs_prints_no_python_line() {
    let dir = scratch("p2");
    put(&dir, "README.md", "hello\n");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        !stderr_lines(&out)
            .iter()
            .any(|line| line.contains("ruff") || line.contains("pyright")),
        "{:?}",
        stderr_lines(&out)
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_config_is_refused_and_modes_are_kept() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = scratch("q");
    put(&dir, "real.toml", "[lint]\nselect = [\"E\"]\n");
    symlink(dir.join("real.toml"), dir.join("ruff.toml")).unwrap();
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr_lines(&out).contains(&format!(
            "adr-lint sync: {} is a symlink; refusing to rewrite it",
            shown(&dir, "ruff.toml")
        )),
        "{:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(&dir, "real.toml"), "[lint]\nselect = [\"E\"]\n");

    let kept = scratch("r");
    put(&kept, "ruff.toml", "[lint]\nselect = [\"E\"]\n");
    fs::set_permissions(kept.join("ruff.toml"), fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(adr_lint(&kept, &[]).status.code(), Some(0));
    let mode = fs::metadata(kept.join("ruff.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
    assert!(read(&kept, "ruff.toml").contains("PGH003"));
}

#[test]
fn both_halves_run_and_the_exit_status_is_the_larger() {
    let dir = scratch("s");
    fs::create_dir_all(dir.join("src")).unwrap();
    put(&dir, "src/lib.rs", "");
    put(
        &dir,
        "Cargo.toml",
        "[package]\nname = \"both\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    put(
        &dir,
        "ruff.toml",
        "[lint]\nselect = [\"E\"]\nignore = [\"PGH\"]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(read(&dir, "Cargo.toml").contains("allow_attributes = \"deny\""));
    assert!(stderr_lines(&out).contains(&wrote(&dir, "Cargo.toml")));
    assert!(stderr_lines(&out)
        .iter()
        .any(|line| line.contains("ruff ignores \"PGH\"")));
    assert_eq!(
        read(&dir, "ruff.toml"),
        "[lint]\nselect = [\"E\"]\nignore = [\"PGH\"]\n"
    );
}
