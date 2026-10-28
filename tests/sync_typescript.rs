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
        "import { react, effect, tests } from \"@duro-app/eslint-config\"\n\nexport default [\n  { ignores: [\"build/**\"] },\n  ...react,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, effect, tests, adrLint } from \"@duro-app/eslint-config\"\n\nexport default [\n  { ignores: [\"build/**\"] },\n  ...react,\n  ...tests,\n  ...adrLint({ stack: [\"react\", \"effect\"] }),\n]\n"
    );
}

#[test]
fn a_last_element_without_a_trailing_comma_gets_none() {
    let dir = scratch("a-no-comma");
    put(
        &dir,
        "eslint.config.js",
        "import { react, tests } from \"@duro-app/eslint-config\"\nexport default [\n    ...react,\n    ...tests // last\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, tests, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n    ...react,\n    ...tests, // last\n    ...adrLint({ stack: [\"react\"] })\n]\n"
    );
}

#[test]
fn a_single_line_array_gets_the_element_inline() {
    let dir = scratch("a-inline");
    put(
        &dir,
        "eslint.config.js",
        "import { react, effect, tests } from \"@duro-app/eslint-config\"\nexport default [ ...react, ...tests ]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { react, effect, tests, adrLint } from \"@duro-app/eslint-config\"\nexport default [ ...react, ...tests, ...adrLint({ stack: [\"react\", \"effect\"] }) ]\n"
    );
}

#[test]
fn a_multi_line_import_list_keeps_its_one_per_line_style() {
    let dir = scratch("a-multiline-import");
    put(
        &dir,
        "eslint.config.js",
        "import {\n  base,\n  effect,\n} from \"@duro-app/eslint-config\"\nexport default [...base]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {\n  base,\n  effect,\n  adrLint,\n} from \"@duro-app/eslint-config\"\nexport default [...base, ...adrLint({ stack: [\"effect\"] })]\n"
    );
}

#[test]
fn without_react_or_effect_the_stack_is_empty() {
    let dir = scratch("b");
    put(
        &dir,
        "eslint.config.js",
        "import {base, tests} from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {base, tests, adrLint} from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n  ...tests,\n  ...adrLint({ stack: [] }),\n]\n"
    );
}

#[test]
fn single_quotes_and_a_tight_import_are_kept() {
    let dir = scratch("c");
    put(
        &dir,
        "eslint.config.js",
        "import {react,tests} from '@duro-app/eslint-config'\nexport default [\n  ...react,\n  ...tests,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import {react,tests,adrLint} from '@duro-app/eslint-config'\nexport default [\n  ...react,\n  ...tests,\n  ...adrLint({ stack: ['react'] }),\n]\n"
    );
}

#[test]
fn tseslint_config_gets_the_spread_as_its_last_argument() {
    let dir = scratch("d");
    put(
        &dir,
        "eslint.config.mjs",
        "import tseslint from \"typescript-eslint\";\nimport { base, react } from \"@duro-app/eslint-config\";\n\nexport default tseslint.config(\n  { ignores: [\"build/**\"] },\n  ...base,\n  ...react.map((c) => ({ ...c, files: [\"app/**\"] })),\n);\n",
    );
    sync_ok(&dir, "eslint.config.mjs");
    assert_eq!(
        read(&dir, "eslint.config.mjs"),
        "import tseslint from \"typescript-eslint\";\nimport { base, react, adrLint } from \"@duro-app/eslint-config\";\n\nexport default tseslint.config(\n  { ignores: [\"build/**\"] },\n  ...base,\n  ...react.map((c) => ({ ...c, files: [\"app/**\"] })),\n  ...adrLint({ stack: [\"react\"] }),\n);\n"
    );
}

#[test]
fn define_config_is_edited_like_tseslint_config() {
    let dir = scratch("d-define");
    put(
        &dir,
        "eslint.config.ts",
        "import { defineConfig } from \"eslint/config\";\nimport { base } from \"@duro-app/eslint-config\";\nexport default defineConfig(...base);\n",
    );
    sync_ok(&dir, "eslint.config.ts");
    assert_eq!(
        read(&dir, "eslint.config.ts"),
        "import { defineConfig } from \"eslint/config\";\nimport { base, adrLint } from \"@duro-app/eslint-config\";\nexport default defineConfig(...base, ...adrLint({ stack: [] }));\n"
    );
}

#[test]
fn a_config_that_already_calls_adr_lint_is_in_sync_and_unchanged() {
    let dir = scratch("e");
    let text = "import { react, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n  ...react,\n  ...adrLint({ stack: [\"react\"] }),\n]\n";
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
fn a_config_without_the_package_import_is_refused_and_not_written() {
    let dir = scratch("f");
    let text = "import js from \"@eslint/js\"\nexport default [js.configs.recommended]\n";
    put(&dir, "eslint.config.js", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        stderr_lines(&out),
        vec![
            NO_CARGO.to_string(),
            format!(
                "adr-lint sync: {}: no import from \"@duro-app/eslint-config\"; adr-lint sync only edits a config that imports it",
                dir.join("eslint.config.js").display()
            )
        ]
    );
    assert_eq!(read(&dir, "eslint.config.js"), text);
}

#[test]
fn a_default_export_that_is_an_identifier_is_refused_and_not_written() {
    let dir = scratch("g-identifier");
    let text = "import { base } from \"@duro-app/eslint-config\"\nconst configs = [...base]\nexport default configs\n";
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
        "import { base } from \"@duro-app/eslint-config\"\nexport default compose(...base)\n";
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
fn mjs_ts_cjs_mts_and_cts_configs_are_found() {
    for name in [
        "eslint.config.mjs",
        "eslint.config.ts",
        "eslint.config.cjs",
        "eslint.config.mts",
        "eslint.config.cts",
    ] {
        let dir = scratch(&format!("h-{name}"));
        put(
            &dir,
            name,
            "import { base } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n]\n",
        );
        sync_ok(&dir, name);
        assert_eq!(
            read(&dir, name),
            "import { base, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n  ...adrLint({ stack: [] }),\n]\n",
            "{name}"
        );
    }
}

#[test]
fn the_first_config_in_eslints_order_wins() {
    let dir = scratch("h-first");
    let text =
        "import { base } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n]\n";
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
    let text = "import { base } from \"@duro-app/eslint-config\"\nexport default [...base]\n";
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
        "import { base } from \"@duro-app/eslint-config\"\nexport default [...base]\n",
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
        "import { base } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n]\n",
    );
    let path = dir.join("eslint.config.js").display().to_string();
    let before = adr_lint(&dir, &["--check"]);
    assert_eq!(before.status.code(), Some(1));
    assert_eq!(
        stderr_lines(&before),
        vec![
            NO_CARGO.to_string(),
            format!("{path}: adrLint is not imported from @duro-app/eslint-config ({RULE})"),
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

const SYNCED_CONFIG: &str = "import { base, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n  ...adrLint({ stack: [] }),\n]\n";

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
    assert_eq!(stderr_lines(&out), vec![NO_CARGO.to_string()]);
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
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert_eq!(
        lines[4],
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
    assert_eq!(stderr_lines(&out), vec![NO_CARGO.to_string()]);
}

#[test]
fn an_adr_lint_imported_from_elsewhere_does_not_count_as_the_package_import() {
    let dir = scratch("n-elsewhere");
    put(
        &dir,
        "eslint.config.js",
        "import { adrLint } from \"elsewhere\"\nimport { base } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n]\n",
    );
    sync_ok(&dir, "eslint.config.js");
    assert_eq!(
        read(&dir, "eslint.config.js"),
        "import { adrLint } from \"elsewhere\"\nimport { base, adrLint } from \"@duro-app/eslint-config\"\nexport default [\n  ...base,\n  ...adrLint({ stack: [] }),\n]\n"
    );
}

#[test]
fn a_subpath_of_the_package_is_not_the_package() {
    let dir = scratch("n-subpath");
    let text =
        "import { base } from \"@duro-app/eslint-config/legacy\"\nexport default [...base]\n";
    put(&dir, "eslint.config.js", text);
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(read(&dir, "eslint.config.js"), text);
}

// The version gate. `adrLint` exists from @duro-app/eslint-config 5.33.0 on;
// measured in duro-app with 5.9.0 installed, the import fails with
// "SyntaxError: ... does not provide an export named 'adrLint'".

const GATED_CONFIG: &str =
    "import { react, tests } from \"@duro-app/eslint-config\"\n\nexport default [\n  ...react,\n  ...tests,\n]\n";

fn gated(name: &str) -> PathBuf {
    let dir = scratch(name);
    put(&dir, "eslint.config.js", GATED_CONFIG);
    dir
}

fn install(dir: &Path, version: &str) {
    let package = dir.join("node_modules/@duro-app/eslint-config");
    fs::create_dir_all(&package).expect("node_modules");
    fs::write(
        package.join("package.json"),
        format!("{{\"name\": \"@duro-app/eslint-config\", \"version\": \"{version}\"}}\n"),
    )
    .expect("installed package.json");
}

fn declare(dir: &Path, range: &str) {
    put(
        dir,
        "package.json",
        &format!("{{\"devDependencies\": {{\"@duro-app/eslint-config\": \"{range}\"}}}}\n"),
    );
}

fn too_old(dir: &Path, version: &str, origin: &str) -> String {
    format!(
        "{}: @duro-app/eslint-config {version} ({origin}) is older than 5.33.0; upgrade @duro-app/eslint-config to 5.33.0 or later first ({RULE})",
        dir.join("eslint.config.js").display()
    )
}

#[test]
fn an_installed_preset_older_than_5_33_is_refused_and_nothing_is_written() {
    let dir = gated("gate-a");
    install(&dir, "5.9.0");
    for extra in [&[][..], &["--check"][..]] {
        let out = adr_lint(&dir, extra);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert!(
            stderr_lines(&out).contains(&too_old(&dir, "5.9.0", "installed")),
            "{extra:?} {:?}",
            stderr_lines(&out)
        );
        assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG, "{extra:?}");
    }
}

#[test]
fn an_installed_5_33_passes_the_gate_and_the_edit_is_written() {
    let dir = gated("gate-b");
    install(&dir, "5.33.0");
    sync_ok(&dir, "eslint.config.js");
    assert!(read(&dir, "eslint.config.js").contains("...adrLint({ stack: [\"react\"] }),"));
}

#[test]
fn a_declared_range_below_5_33_is_refused_when_nothing_is_installed() {
    let dir = gated("gate-c");
    declare(&dir, "^5.3.0");
    let out = adr_lint(&dir, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(
        stderr_lines(&out).contains(&too_old(&dir, "5.3.0", "declared as ^5.3.0")),
        "{:?}",
        stderr_lines(&out)
    );
    assert_eq!(read(&dir, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn a_declared_range_at_or_above_5_33_passes_the_gate() {
    for (index, range) in ["^5.33.0", ">=5.40.0", "5.33.2"].iter().enumerate() {
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
    install(&passes, "5.33.1");
    declare(&passes, "^5.3.0");
    sync_ok(&passes, "eslint.config.js");

    let refused = gated("gate-g2");
    install(&refused, "5.9.0");
    declare(&refused, "^5.40.0");
    let out = adr_lint(&refused, &[]);
    assert_eq!(out.status.code(), Some(1), "{:?}", stderr_lines(&out));
    assert!(stderr_lines(&out).contains(&too_old(&refused, "5.9.0", "installed")));
    assert_eq!(read(&refused, "eslint.config.js"), GATED_CONFIG);
}

#[test]
fn an_aliased_adr_lint_import_is_refused_and_the_file_is_unchanged() {
    // Writing `...adrLint(...)` here would leave the call unbound.
    let dir = scratch("alias");
    let original = "import { base, adrLint as lint } from \"@duro-app/eslint-config\"\n\nexport default [\n  ...base,\n]\n";
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
