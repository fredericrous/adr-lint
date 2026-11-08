//! `adr-lint sync`, the TypeScript half, through the real binary over scratch directories.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NO_CARGO: &str = "sync: no Cargo.toml, nothing to do for Rust";
const RULE: &str = "general.linters-police-suppressions";

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
    let dir = std::env::temp_dir().join(format!("adr-lint-sync-ts-{}-{name}", std::process::id()));
    fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may be absent
    fs::create_dir_all(&dir).expect("scratch dir");
    declare(&dir, "^0.16.0");
    dir
}

fn put(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture dir");
    }
    fs::write(path, text).expect("write fixture");
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).expect("read file")
}

fn wrote(dir: &Path, name: &str) -> String {
    format!("sync: wrote {}", dir.join(name).display())
}

fn sync_ok(dir: &Path, name: &str) {
    let out = adr_lint(dir, &[]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        stderr_lines(&out),
        vec![NO_CARGO.to_string(), wrote(dir, name)]
    );
}

#[test]
fn array_config_gets_the_import_and_a_last_element_indented_like_the_one_before() {
    let dir = scratch("a");
    put(
        &dir,
        "eslint.config.js",
        "import { react, effect, tests } from \"@aval-adr/eslint-config\"\n\nexport default [\n  { ignores: [\"build/**\"] },\n  ...react,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, effect, tests, adrLint } from \"@aval-adr/eslint-config\"\n\nexport default [\n  { ignores: [\"build/**\"] },\n  ...react,\n  ...tests,\n  ...adrLint(),\n]\n"
    );
}

#[test]
fn a_last_element_without_a_trailing_comma_gets_none() {
    let dir = scratch("a-no-comma");
    put(
        &dir,
        "eslint.config.js",
        "import { react, tests } from \"@aval-adr/eslint-config\"\nexport default [\n    ...react,\n    ...tests // last\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, tests, adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n    ...react,\n    ...tests, // last\n    ...adrLint()\n]\n"
    );
}

#[test]
fn a_single_line_array_gets_the_element_inline() {
    let dir = scratch("a-inline");
    put(
        &dir,
        "eslint.config.js",
        "import { react, effect, tests } from \"@aval-adr/eslint-config\"\nexport default [ ...react, ...tests ]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, effect, tests, adrLint } from \"@aval-adr/eslint-config\"\nexport default [ ...react, ...tests, ...adrLint() ]\n"
    );
}

#[test]
fn a_multi_line_import_list_keeps_its_one_per_line_style() {
    let dir = scratch("a-multiline-import");
    put(
        &dir,
        "eslint.config.js",
        "import {\n  base,\n  effect,\n} from \"@aval-adr/eslint-config\"\nexport default [...base]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {\n  base,\n  effect,\n  adrLint,\n} from \"@aval-adr/eslint-config\"\nexport default [...base, ...adrLint()]\n"
    );
}

#[test]
fn a_config_with_other_presets_gets_a_bare_call() {
    let dir = scratch("b");
    put(
        &dir,
        "eslint.config.js",
        "import {base, tests} from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {base, tests, adrLint} from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n  ...tests,\n  ...adrLint(),\n]\n"
    );
}

#[test]
fn single_quotes_and_a_tight_import_are_kept() {
    let dir = scratch("c");
    put(
        &dir,
        "eslint.config.js",
        "import {react,tests} from '@aval-adr/eslint-config'\nexport default [\n  ...react,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {react,tests,adrLint} from '@aval-adr/eslint-config'\nexport default [\n  ...react,\n  ...tests,\n  ...adrLint(),\n]\n"
    );
}

#[test]
fn define_config_is_edited_like_tseslint_config() {
    let dir = scratch("d-define");
    put(
        &dir,
        "eslint.config.ts",
        "import { defineConfig } from \"eslint/config\";\nimport { base } from \"@aval-adr/eslint-config\";\nexport default defineConfig(...base);\n",
    );
    sync_ok(&dir, "eslint.config.ts");
    assert_eq!(
        read(&dir, "eslint.config.ts"),
        "import { defineConfig } from \"eslint/config\";\nimport { base, adrLint } from \"@aval-adr/eslint-config\";\nexport default defineConfig(...base, ...adrLint());\n"
    );
}

#[test]
fn a_config_that_already_calls_adr_lint_is_in_sync_and_unchanged() {
    let dir = scratch("e");
    let text = "import { react, adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n  ...react,\n  ...adrLint(),\n]\n";
    put(&dir, "eslint.config.js", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr_lines(&out),
        vec![NO_CARGO.to_string(), "sync: already in sync".to_string()]
    );
    assert_eq!(read(&dir, "eslint.config.js"), text);
}

#[test]
fn a_default_export_that_is_an_identifier_is_refused_and_not_written() {
    let dir = scratch("g-identifier");
    let text = "import { base } from \"@aval-adr/eslint-config\"\nconst configs = [...base]\nexport default configs\n";
    put(&dir, "eslint.config.js", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            format!(
                "adr-lint sync: {}: the default export is neither an array literal nor a tseslint.config / defineConfig call; add `adrLint` by hand",
                dir.join("eslint.config.js").display()
            )
        ]
    );
    assert_eq!(read(&dir, "eslint.config.js"), text);
}

#[test]
fn another_callee_is_named_in_the_refusal_and_nothing_is_written() {
    let dir = scratch("g-callee");
    let text =
        "import { base } from \"@aval-adr/eslint-config\"\nexport default compose(...base)\n";
    put(&dir, "eslint.config.js", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        stderr_lines(&out)[1],
        format!(
            "adr-lint sync: {}: the default export calls `compose`; only an array or tseslint.config / defineConfig are edited",
            dir.join("eslint.config.js").display()
        )
    );
    assert_eq!(read(&dir, "eslint.config.js"), text);
}

#[test]
fn mjs_ts_mts_and_cts_configs_are_found() {
    for name in [
        "eslint.config.mjs",
        "eslint.config.ts",
        "eslint.config.mts",
        "eslint.config.cts",
    ] {
        let dir = scratch(&format!("h-{name}"));
        put(
            &dir,
            name,
            "import { base } from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n]\n",
        );
        sync_ok(&dir, name);
        assert_eq!(
            read(&dir, name),
            "import { base, adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n  ...adrLint(),\n]\n",
            "{name}"
        );
    }
}

#[test]
fn the_first_config_in_eslints_order_wins() {
    let dir = scratch("h-first");
    let text =
        "import { base } from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n]\n";
    put(&dir, "eslint.config.mjs", text);
    put(&dir, "eslint.config.ts", text);
    put(&dir, "eslint.config.js", text);
    sync_ok(&dir, "eslint.config.js");
    assert!(read(&dir, "eslint.config.js").contains("adrLint"));
    assert_eq!(read(&dir, "eslint.config.mjs"), text);
    assert_eq!(read(&dir, "eslint.config.ts"), text);
}

#[test]
fn a_symlinked_config_is_refused() {
    let dir = scratch("i");
    let text = "import { base } from \"@aval-adr/eslint-config\"\nexport default [...base]\n";
    put(&dir, "real.js", text);
    symlink(dir.join("real.js"), dir.join("eslint.config.js")).expect("symlink");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        stderr_lines(&out)[1],
        format!(
            "adr-lint sync: {} is a symlink; refusing to rewrite it",
            dir.join("eslint.config.js").display()
        )
    );
    assert_eq!(read(&dir, "real.js"), text);
}

#[test]
fn the_file_mode_is_kept() {
    let dir = scratch("j");
    put(
        &dir,
        "eslint.config.js",
        "import { base } from \"@aval-adr/eslint-config\"\nexport default [...base]\n",
    );
    fs::set_permissions(
        dir.join("eslint.config.js"),
        fs::Permissions::from_mode(0o640),
    )
    .expect("chmod");
    sync_ok(&dir, "eslint.config.js");
    let mode = fs::metadata(dir.join("eslint.config.js"))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn check_fails_before_and_passes_after_a_sync_and_says_eslint_is_missing() {
    let dir = scratch("k");
    put(
        &dir,
        "eslint.config.js",
        "import { base } from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n]\n",
    );
    let path = dir.join("eslint.config.js").display().to_string();
    let before = adr_lint(&dir, &["--check"]);
    assert_eq!(before.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&before),
        vec![
            NO_CARGO.to_string(),
            format!("{path}: adrLint is not imported from @aval-adr/eslint-config ({RULE})"),
            format!("{path}: adrLint() is not called in the default export ({RULE})"),
        ]
    );
    assert!(!read(&dir, "eslint.config.js").contains("adrLint"));
    sync_ok(&dir, "eslint.config.js");
    let after = adr_lint(&dir, &["--check"]);
    assert_eq!(after.status.code(), Some(0));
    assert_eq!(
        stderr_lines(&after),
        vec![
            NO_CARGO.to_string(),
            "sync: eslint not installed; rule resolution not checked".to_string()
        ]
    );
}

const SYNCED_CONFIG: &str = "import { base, adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n  ...base,\n  ...adrLint(),\n]\n";

fn repository_with_fake_eslint(name: &str, printed: &str) -> PathBuf {
    let dir = scratch(name);
    put(&dir, "eslint.config.js", SYNCED_CONFIG);
    fs::create_dir_all(dir.join("src")).expect("src dir");
    put(&dir, "src/a.ts", "export const a = 1\n");
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()
        .expect("git runs");
    assert!(git.success());
    let bin = dir.join("node_modules/.bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let script = bin.join("eslint");
    fs::write(
        &script,
        format!("#!/bin/sh\n[ \"$1\" = \"--print-config\" ] && [ \"$2\" = \"src/a.ts\" ] || exit 9\ncat <<'JSON'\n{printed}\nJSON\n"),
    )
    .expect("fake eslint");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    dir
}

fn print_config(unused: &str, comments_level: &str, ban_level: &str, expect_error: &str) -> String {
    format!(
        r#"{{"linterOptions":{{"reportUnusedDisableDirectives":{unused}}},"rules":{{
"@eslint-community/eslint-comments/no-unlimited-disable":[{comments_level}],
"@eslint-community/eslint-comments/require-description":[2],
"@eslint-community/eslint-comments/disable-enable-pair":[2,{{"allowWholeFile":true}}],
"@typescript-eslint/ban-ts-comment":[{ban_level},{{"ts-expect-error":{expect_error},"ts-ignore":true}}]}}}}"#
    )
}

/// Repeats `run` while a just-written script is refused with "text file busy" (ETXTBSY).
fn retry_while_busy(run: impl Fn() -> Output) -> Output {
    for _ in 0..20 {
        let out = run();
        if !String::from_utf8_lossy(&out.stderr).contains("usy") {
            return out;
        }
    }
    run()
}

fn check_with_fake(dir: &Path) -> Output {
    retry_while_busy(|| adr_lint(dir, &["--check"]))
}

#[test]
fn check_with_a_resolved_config_at_error_passes() {
    let dir = repository_with_fake_eslint(
        "l-ok",
        &print_config("2", "2", "2", "\"allow-with-description\""),
    );
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            "sync: rule resolution checked on src/a.ts".to_string()
        ]
    );
}

#[test]
fn check_names_a_rule_a_later_config_switched_off() {
    let dir = repository_with_fake_eslint(
        "l-off",
        &print_config("2", "0", "2", "\"allow-with-description\""),
    );
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            "sync: rule resolution checked on src/a.ts".to_string(),
            format!("eslint resolves @eslint-community/eslint-comments/no-unlimited-disable to off for src/a.ts, wanted error ({RULE})"),
        ]
    );
}

#[test]
fn check_names_a_downgraded_setting_and_a_weak_ts_expect_error_option() {
    let dir = repository_with_fake_eslint("l-weak", &print_config("1", "2", "2", "false"));
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            "sync: rule resolution checked on src/a.ts".to_string(),
            format!("eslint resolves linterOptions.reportUnusedDisableDirectives to warn for src/a.ts, wanted error ({RULE})"),
            format!("eslint resolves @typescript-eslint/ban-ts-comment with ts-expect-error not limited to a described directive for src/a.ts ({RULE})"),
        ]
    );
}

#[test]
fn check_reports_rules_missing_from_the_resolved_config_as_off() {
    let dir = repository_with_fake_eslint(
        "l-missing",
        r#"{"linterOptions":{"reportUnusedDisableDirectives":2},"rules":{}}"#,
    );
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(1));
    let lines = stderr_lines(&out);
    assert_eq!(lines.len(), 6, "{lines:?}");
    assert_eq!(lines[1], "sync: rule resolution checked on src/a.ts");
    assert_eq!(
        lines[5],
        format!("eslint resolves @typescript-eslint/ban-ts-comment to off for src/a.ts, wanted error ({RULE})")
    );
}

#[test]
fn a_repository_with_no_eslint_config_prints_no_typescript_line() {
    let dir = scratch("m");
    let out = adr_lint(&dir, &["--check"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr_lines(&out), vec![NO_CARGO.to_string()]);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr_lines(&out), vec![NO_CARGO.to_string()]);
}

#[test]
fn check_resolves_eslint_for_a_relative_directory() {
    let dir = repository_with_fake_eslint(
        "l-relative",
        &print_config("2", "2", "2", "\"allow-with-description\""),
    );
    let parent = dir.parent().expect("parent");
    let name = dir.file_name().expect("name");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .args(["sync", "--check", "-C"])
            .arg(name)
            .current_dir(parent)
            .output()
            .expect("adr-lint runs")
    };
    let out = retry_while_busy(run);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            "sync: rule resolution checked on src/a.ts".to_string()
        ]
    );
}

// The version gate. `adrLint` exists from @aval-adr/eslint-config 0.16.0 on;
// measured in duro-app with 0.9.0 installed, the import fails with
// "SyntaxError: ... does not provide an export named 'adrLint'".

const GATED_CONFIG: &str =
    "import { react, tests } from \"@aval-adr/eslint-config\"\n\nexport default [\n  ...react,\n  ...tests,\n]\n";

fn gated(name: &str) -> PathBuf {
    let dir = scratch(name);
    put(&dir, "eslint.config.js", GATED_CONFIG);
    dir
}

fn install(dir: &Path, version: &str) {
    let package = dir.join("node_modules/@aval-adr/eslint-config");
    fs::create_dir_all(&package).expect("node_modules");
    fs::write(
        package.join("package.json"),
        format!("{{\"name\": \"@aval-adr/eslint-config\", \"version\": \"{version}\"}}\n"),
    )
    .expect("installed package.json");
}

fn declare(dir: &Path, range: &str) {
    put(
        dir,
        "package.json",
        &format!("{{\"devDependencies\": {{\"@aval-adr/eslint-config\": \"{range}\"}}}}\n"),
    );
}

fn too_old(dir: &Path, version: &str, origin: &str) -> String {
    format!(
        "{}: @aval-adr/eslint-config {version} ({origin}) is older than 0.16.0; upgrade @aval-adr/eslint-config to 0.16.0 or later first ({RULE})",
        dir.join("eslint.config.js").display()
    )
}

#[test]
fn an_installed_preset_older_than_0_16_is_refused_and_nothing_is_written() {
    let dir = gated("gate-a");
    install(&dir, "0.9.0");
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert!(
            stderr_lines(&out).contains(&too_old(&dir, "0.9.0", "installed")),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG, "{extra:?}");
    }
}

#[test]
fn an_installed_0_16_passes_the_gate_and_the_edit_is_written() {
    let dir = gated("gate-b");
    install(&dir, "0.16.0");
    sync_ok(&dir, "eslint.config.js");
    assert!(read(&dir, "eslint.config.js").contains("...adrLint(),"));
}

#[test]
fn a_declared_range_below_0_16_is_refused_when_nothing_is_installed() {
    let dir = gated("gate-c");
    declare(&dir, "^0.3.0");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(
        stderr_lines(&out).contains(&too_old(&dir, "0.3.0", "declared as ^0.3.0")),
        "{:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn a_declared_range_at_or_above_0_16_passes_the_gate() {
    for (index, range) in ["^0.16.0", ">=0.40.0", "0.16.2"].iter().enumerate() {
        let dir = gated(&format!("gate-d{index}"));
        declare(&dir, range);
        sync_ok(&dir, "eslint.config.js");
        assert!(
            read(&dir, "eslint.config.js").contains("...adrLint("),
            "{range}"
        );
    }
}

#[test]
fn a_workspace_dependency_passes_the_gate() {
    for (index, range) in ["workspace:^", "workspace:*"].iter().enumerate() {
        let dir = gated(&format!("gate-e{index}"));
        declare(&dir, range);
        sync_ok(&dir, "eslint.config.js");
        assert!(
            read(&dir, "eslint.config.js").contains("...adrLint("),
            "{range}"
        );
    }
}

#[test]
fn an_unreadable_range_is_could_not_run_and_nothing_is_written() {
    for (index, range) in ["latest", "github:fredericrous/duro#main"]
        .iter()
        .enumerate()
    {
        let dir = gated(&format!("gate-f{index}"));
        declare(&dir, range);
        let out = adr_lint(&dir, &[]);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{range} {:?}",
            stderr_lines(&out)
        );
        let says = stderr_lines(&out).iter().any(|line| {
            line.contains(&dir.join("package.json").display().to_string())
                && line.contains(&format!("cannot read the range `{range}`"))
        });
        assert!(says, "{range} {:?}", stderr_lines(&out));
        assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG, "{range}");
    }
}

#[test]
fn the_installed_version_wins_over_the_declared_range() {
    let passes = gated("gate-g1");
    install(&passes, "0.16.1");
    declare(&passes, "^0.3.0");
    sync_ok(&passes, "eslint.config.js");

    let refused = gated("gate-g2");
    install(&refused, "0.9.0");
    declare(&refused, "^0.40.0");
    let out = adr_lint(&refused, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&too_old(&refused, "0.9.0", "installed")));
    assert_eq!(read(&refused, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn an_aliased_adr_lint_import_is_refused_and_the_file_is_unchanged() {
    // Writing `...adrLint(...)` here would leave the call unbound.
    let dir = scratch("alias");
    let original = "import { base, adrLint as lint } from \"@aval-adr/eslint-config\"\n\nexport default [\n  ...base,\n]\n";
    put(&dir, "eslint.config.js", original);
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        let says = stderr_lines(&out).iter().any(|line| {
            line.contains(&dir.join("eslint.config.js").display().to_string())
                && line
                    .contains("is imported as `adrLint as lint`, not as the plain binding adrLint")
        });
        assert!(says, "{extra:?} {:?}", stderr_lines(&out));
        assert_eq!(read(&dir, "eslint.config.js"), original, "{extra:?}");
    }
}

#[test]
fn a_cjs_config_is_refused_and_unchanged() {
    let dir = scratch("cjs");
    let text =
        "const { base } = require(\"@aval-adr/eslint-config\")\nmodule.exports = [...base]\n";
    put(&dir, "eslint.config.cjs", text);
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(out.status.code(), Some(3), "{extra:?}");
        assert!(
            stderr_lines(&out).iter().any(|line| line
                .contains(&dir.join("eslint.config.cjs").display().to_string())
                && line.contains("a CommonJS config (module.exports) is not edited")),
            "{:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.cjs"), text);
    }
}

fn refused_with(dir: &Path, extra: &[&str], path: &str, says: &str) {
    let out = adr_lint(dir, extra);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{extra:?} {:?}",
        stderr_lines(&out)
    );
    let wanted = format!("{}: {says}", dir.join(path).display());
    assert!(
        stderr_lines(&out)
            .iter()
            .any(|line| line.ends_with(&wanted)),
        "{wanted} in {:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(dir, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn check_outside_a_git_repository_reports_gits_message() {
    let dir = scratch("nogit");
    put(&dir, "eslint.config.js", SYNCED_CONFIG);
    let bin = dir.join("node_modules/.bin");
    fs::create_dir_all(&bin).expect("bin dir");
    put(&dir, "node_modules/.bin/eslint", "#!/bin/sh\nexit 0\n");
    fs::set_permissions(bin.join("eslint"), fs::Permissions::from_mode(0o755)).expect("chmod");
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    assert!(
        stderr_lines(&out)
            .iter()
            .any(|line| line.contains("git ls-files:") && line.contains("not a git repository")),
        "{:?}",
        stderr_lines(&out)
    );
}

fn fake_eslint_script(name: &str, body: &str) -> PathBuf {
    let dir = scratch(name);
    put(&dir, "eslint.config.js", SYNCED_CONFIG);
    fs::create_dir_all(dir.join("src")).expect("src dir");
    for file in ["src/a.ts", "src/b.ts"] {
        put(&dir, file, "export const a = 1\n");
    }
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()
        .expect("git runs");
    assert!(git.success());
    let bin = dir.join("node_modules/.bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let script = bin.join("eslint");
    fs::write(&script, format!("#!/bin/sh\n{body}\n")).expect("fake eslint");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    dir
}

#[test]
fn check_tries_the_next_file_until_eslint_prints_json() {
    let good = print_config("2", "2", "2", "\"allow-with-description\"");
    let dir = fake_eslint_script(
        "l-second",
        &format!("[ \"$2\" = \"src/b.ts\" ] || exit 0\ncat <<'JSON'\n{good}\nJSON"),
    );
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            "sync: rule resolution checked on src/b.ts".to_string()
        ]
    );
}

#[test]
fn check_exits_3_naming_the_count_when_no_file_resolves() {
    let dir = fake_eslint_script("l-never", "exit 0");
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    assert!(
        stderr_lines(&out)
            .iter()
            .any(|line| line
                .contains("printed no JSON config for any of the 2 .ts/.tsx files tried")),
        "{:?}",
        stderr_lines(&out)
    );
}

#[test]
fn a_prerelease_of_0_16_0_is_below_0_16_0() {
    let dir = gated("pre");
    install(&dir, "0.16.0-beta.1");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&too_old(&dir, "0.16.0-beta.1", "installed")));
    assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn a_range_open_above_0_15_0_admits_0_15_1_and_is_refused() {
    let dir = gated("gt");
    declare(&dir, ">0.15.0");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&too_old(&dir, "0.15.1", "declared as >0.15.0")));
    assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
    declare(&dir, ">0.15");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&too_old(&dir, "0.15.1", "declared as >0.15")));
    declare(&dir, ">=0.16.0");
    sync_ok(&dir, "eslint.config.js");
}

#[test]
fn ranges_this_tool_cannot_reduce_to_one_lowest_version_exit_3() {
    for (index, range) in [
        "5.x",
        "*",
        "^0.3.0 || ^1.0.0",
        ">=0.3.0 <1.0.0",
        "0.16.0 - 1.0.0",
    ]
    .iter()
    .enumerate()
    {
        let dir = gated(&format!("range-{index}"));
        declare(&dir, range);
        refused_with(
            &dir,
            &[],
            "package.json",
            &format!("cannot read the range `{range}` of @aval-adr/eslint-config; nothing written"),
        );
    }
}

#[test]
fn two_import_declarations_from_the_package_are_refused() {
    let dir = scratch("two-imports");
    let text = "import { base } from \"@aval-adr/eslint-config\"\nimport type { Options } from \"@aval-adr/eslint-config\"\nexport default [...base]\n";
    put(&dir, "eslint.config.ts", text);
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert!(
            stderr_lines(&out).iter().any(|line| line.ends_with(&format!(
                "{}: @aval-adr/eslint-config is imported by 2 import declarations; merge them into one by hand",
                dir.join("eslint.config.ts").display()
            ))),
            "{:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.ts"), text);
    }
}

#[test]
fn a_broken_eslint_is_reported_with_its_own_error_not_as_ignored_files() {
    // Before the fix the loop dropped ESLint's failure, so a config that throws
    // read as "are they ignored?".
    let dir = repository_with_fake_eslint("l-broken", "unused");
    let script = dir.join("node_modules/.bin/eslint");
    fs::write(
        &script,
        "#!/bin/sh\necho 'Error: Cannot find module typescript-eslint' >&2\nexit 2\n",
    )
    .expect("broken eslint");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    let out = check_with_fake(&dir);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    let reported = stderr_lines(&out).iter().any(|line| {
        line.contains("the last failure:") && line.contains("Cannot find module typescript-eslint")
    });
    assert!(reported, "{:?}", stderr_lines(&out));
}

const UNDECLARED: &str =
    "declare @aval-adr/eslint-config >= 0.16.0 first: pnpm add -D @aval-adr/eslint-config@^0.16.0";

#[test]
fn a_config_with_no_aval_import_gets_one_after_the_last_import_in_its_quote_style() {
    let dir = scratch("new-import");
    put(
        &dir,
        "eslint.config.js",
        "import js from \"@eslint/js\"\nimport tseslint from 'typescript-eslint'\n\nexport default [\n  js.configs.recommended,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import js from \"@eslint/js\"\nimport tseslint from 'typescript-eslint'\nimport {adrLint} from '@aval-adr/eslint-config'\n\nexport default [\n  js.configs.recommended,\n  ...adrLint(),\n]\n"
    );
}

#[test]
fn double_quotes_are_the_default_for_the_inserted_import() {
    let dir = scratch("new-import-double");
    put(
        &dir,
        "eslint.config.js",
        "import js from \"@eslint/js\"\nexport default [js.configs.recommended]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import js from \"@eslint/js\"\nimport {adrLint} from \"@aval-adr/eslint-config\"\nexport default [js.configs.recommended, ...adrLint()]\n"
    );
}

#[test]
fn a_config_with_no_imports_gets_the_import_at_the_top() {
    let dir = scratch("no-imports");
    put(
        &dir,
        "eslint.config.js",
        "export default [\n  { ignores: [\"build/**\"] },\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {adrLint} from \"@aval-adr/eslint-config\"\n\nexport default [\n  { ignores: [\"build/**\"] },\n  ...adrLint(),\n]\n"
    );
}

#[test]
fn undeclared_and_uninstalled_exits_3_writes_nothing_and_names_the_command() {
    let dir = scratch("undeclared");
    fs::remove_file(dir.join("package.json")).expect("remove package.json");
    put(&dir, "eslint.config.js", GATED_CONFIG);
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert!(
            stderr_lines(&out).contains(&format!("adr-lint sync: {UNDECLARED}")),
            "{:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
    }
}

#[test]
fn a_package_json_without_the_dependency_exits_3() {
    let dir = scratch("undeclared-manifest");
    put(&dir, "package.json", "{\"dependencies\": {}}\n");
    put(&dir, "eslint.config.js", GATED_CONFIG);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&format!("adr-lint sync: {UNDECLARED}")));
    assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
}

fn refused_as_duro_adr_lint(text: &str, name: &str) {
    let dir = scratch(name);
    put(&dir, "eslint.config.js", text);
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert!(
            stderr_lines(&out).iter().any(|line| line
                .contains("import {adrLint} from \"@aval-adr/eslint-config\" and drop `stack`")),
            "{:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.js"), text, "{extra:?}");
    }
}

#[test]
fn an_adr_lint_import_from_duro_is_refused_and_nothing_is_written() {
    refused_as_duro_adr_lint(
        "import { base, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n]\n",
        "duro-import",
    );
}

#[test]
fn a_call_with_a_stack_is_refused_and_nothing_is_written() {
    refused_as_duro_adr_lint(
        "import { adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n  ...adrLint({ stack: [\"react\"] }),\n]\n",
        "stack-call",
    );
}

#[test]
fn a_call_whose_text_only_contains_stack_is_not_refused() {
    let dir = gated("stack-string");
    put(
        &dir,
        "eslint.config.js",
        "import { adrLint } from \"@aval-adr/eslint-config\"\nexport default [\n  ...adrLint({ restrictedImports: [{ paths: [\"stackoverflow\"] }] }),\n]\n",
    );
    let out = adr_lint(&dir, &[]);
    assert!(
        !stderr_lines(&out)
            .iter()
            .any(|line| line.contains("drop `stack`")),
        "{:?}",
        stderr_lines(&out)
    );
}

#[test]
fn a_duro_import_without_adr_lint_is_left_alone() {
    let dir = scratch("duro-untouched");
    put(
        &dir,
        "eslint.config.js",
        "import { react } from \"@duro-app/eslint-config\"\nexport default [\n  ...react,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react } from \"@duro-app/eslint-config\"\nimport {adrLint} from \"@aval-adr/eslint-config\"\nexport default [\n  ...react,\n  ...adrLint(),\n]\n"
    );
}
