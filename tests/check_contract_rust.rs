//! `adr-lint check` with an `[architecture]` contract (ADR-0056) through the
//! binary: scratch git repositories, the real ast-grep named in
//! AST_GREP_VERSION, and a fake aval. Skipped locally when that ast-grep is
//! not on PATH.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");

const FAKE_AVAL: &str = r#"#!/bin/sh
cat <<'JSON'
{"rules":[
{"id":"architecture.dependency-rule","active":true,"statement":"Layers."},
{"id":"boundaries.own-the-interface","active":true,"statement":"Own it."},
{"id":"components.acyclic-dependencies","active":true,"statement":"No cycles."},
{"id":"tests.first-properties","active":true,"statement":"A skipped test says why."}
]}
JSON
"#;

const LAYERS: &str = r#"[[architecture.layers]]
name = "application"
modules = ["crate::low"]

[[architecture.layers]]
name = "adapters"
modules = ["crate::high"]
"#;

const ONLY_IN: &str = r#"
[[architecture.only_in]]
imports = ["serde"]
modules = ["crate::high"]
"#;

const LATER_LAYER_IMPORT: &str = "use crate::high::f;\npub fn g() {\n    f();\n}\n";

struct Repo(PathBuf);

impl Repo {
    fn new(name: &str, toml: &str) -> Repo {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-contract-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(dir.join("src")).expect("scratch dir");
        let repo = Repo(dir);
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        repo.write("src/lib.rs", "pub mod high;\npub mod low;\n");
        repo.write("src/high.rs", "pub fn f() {}\n");
        repo.write("src/low.rs", "pub fn g() {}\n");
        if !toml.is_empty() {
            repo.write("adr-lint.toml", toml);
        }
        repo.write("aval-fake", FAKE_AVAL);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                repo.0.join("aval-fake"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("chmod");
        }
        repo.git(&["init", "-q"]);
        repo
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit_all(&self) {
        self.git(&["add", "src", "Cargo.toml"]);
        if self.0.join("adr-lint.toml").exists() {
            self.git(&["add", "adr-lint.toml"]);
        }
        self.git(&["commit", "-q", "-m", "base"]);
    }

    fn check(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .arg("check")
            .args(args)
            .current_dir(&self.0)
            .env_remove("AMONT_FILES")
            .env_remove("ADR_LINT_AST_GREP")
            .env_remove("NO_COLOR")
            .env("ADR_LINT_AVAL", self.0.join("aval-fake"))
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs")
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

fn real_ast_grep() -> bool {
    if !cfg!(unix) {
        eprintln!("SKIPPED: the fake aval is a shell script, which needs unix");
        return false;
    }
    let wanted = format!("ast-grep {}", EXPECTED_AST_GREP.trim());
    let found = Command::new("ast-grep")
        .arg("--version")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default(); // a missing binary reads as no version
    if found == wanted {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI needs `{wanted}` on PATH (found `{found}`)"
    );
    eprintln!("SKIPPED: `{wanted}` is not on PATH (found `{found}`)");
    false
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_later_layer_import_is_reported_with_its_detail_lines() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("later", LAYERS);
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    let out = repo.check(&["src/low.rs"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/low.rs:1:5: architecture.dependency-rule: Layers.\n"),
        "{stdout}"
    );
    let detail: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect();
    let joined = detail.join(" ");
    for expected in [
        "application",
        "adapters",
        "crate::high::f",
        "adr-lint.toml:5",
    ] {
        assert!(joined.contains(expected), "{expected}\n{stdout}");
    }
    let importer = joined.find("(layer `application`) imports");
    let imported = joined.find("(layer `adapters`)");
    assert!(
        matches!((importer, imported), (Some(a), Some(b)) if a < b),
        "the importing layer is named before the imported one\n{stdout}"
    );
    assert!(detail.len() >= 2, "{stdout}");
    for line in detail {
        assert!(line.chars().count() <= 80, "{line}");
    }
}

#[test]
fn an_only_in_crate_used_in_an_unlayered_file_is_reported() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("only-in", &format!("{LAYERS}{ONLY_IN}"));
    repo.write("src/lib.rs", "pub mod high;\npub mod low;\npub mod util;\n");
    repo.write("src/util.rs", "use serde::Serialize;\n");
    let out = repo.check(&["src/util.rs"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/util.rs:1:5: boundaries.own-the-interface: Own it.\n"),
        "{stdout}"
    );
}

#[test]
fn a_waiver_on_the_import_line_gives_exit_0() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("waiver", LAYERS);
    repo.write(
        "src/low.rs",
        "use crate::high::f; // adr-lint: allow architecture.dependency-rule -- a bridge\npub fn g() {\n    f();\n}\n",
    );
    let out = repo.check(&["src/low.rs"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn a_contract_rule_selects_only_contract_findings() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("select", LAYERS);
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    repo.write("src/skip.rs", "#[test]\n    #[ignore]\nfn slow() {}\n");
    let all = repo.check(&["src/low.rs", "src/skip.rs"]);
    assert!(text(&all.stdout).contains("rule tests.first-properties"));
    let out = repo.check(&[
        "--rule",
        "architecture.dependency-rule",
        "src/low.rs",
        "src/skip.rs",
    ]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("rule architecture.dependency-rule (1)"));
    assert!(!stdout.contains("tests.first-properties"), "{stdout}");
}

#[test]
fn an_embedded_rule_selected_builds_no_graph() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("embedded", LAYERS);
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    let out = repo.check(&["--rule", "tests.first-properties", "src/low.rs"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert_eq!(text(&out.stdout), "");

    let broken = Repo::new("embedded-broken", "[architecture]\nlayers = 3\n");
    broken.write("src/low.rs", LATER_LAYER_IMPORT);
    let out = broken.check(&["--rule", "tests.first-properties", "src/low.rs"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
}

#[test]
fn only_the_contract_staged_reports_findings_in_an_unchanged_file() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("config-staged", "");
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    repo.commit_all();
    repo.write("adr-lint.toml", LAYERS);
    let before = repo.check(&[]);
    assert_eq!(before.status.code(), Some(0), "{}", text(&before.stdout));
    repo.git(&["add", "adr-lint.toml"]);
    let out = repo.check(&[]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/low.rs:1:5: architecture.dependency-rule: Layers.\n"),
        "{stdout}"
    );
}

#[test]
fn a_staged_rename_widens_to_every_file() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("rename", LAYERS);
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    repo.write("notes.txt", "first\n");
    repo.write("src/clean.rs", "pub fn clean() {}\n");
    repo.commit_all();
    repo.git(&["add", "notes.txt"]);
    repo.git(&["commit", "-q", "-m", "notes"]);
    repo.git(&["mv", "notes.txt", "notes-renamed.txt"]);
    repo.write("src/clean.rs", "pub fn clean() {}\npub fn more() {}\n");
    repo.git(&["add", "src/clean.rs"]);
    let out = repo.check(&[]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/low.rs:1:5: architecture.dependency-rule"),
        "{stdout}"
    );

    repo.git(&["reset", "-q", "--hard"]);
    repo.write("src/clean.rs", "pub fn clean() {}\npub fn more() {}\n");
    repo.git(&["add", "src/clean.rs"]);
    let narrow = repo.check(&[]);
    assert_eq!(narrow.status.code(), Some(0), "{}", text(&narrow.stdout));
}

#[test]
fn an_invalid_contract_exits_3_naming_both_entries() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new(
        "invalid",
        "[[architecture.layers]]\nname = \"application\"\nmodules = [\"crate::low\"]\n\n\
         [[architecture.layers]]\nname = \"adapters\"\nmodules = [\"crate::low\"]\n",
    );
    repo.write("src/low.rs", LATER_LAYER_IMPORT);
    let out = repo.check(&["src/low.rs"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.contains("application") && stderr.contains("adapters"),
        "{stderr}"
    );
}

// Captured from the binary built at a6b16db, before the contract was
// integrated, run on `src/skip.rs` below with `check` and `check --json`.
const SKIPPED_TEST: &str = "#[test]\n    #[ignore]\nfn slow() {}\n";
const EMBEDDED_TEXT: &str = "rule tests.first-properties (1)\n\
src/skip.rs:2:5: tests.first-properties: A skipped test says why.\n\
aval rule tests.first-properties\n";
const EMBEDDED_JSON: &str = "[{\"col\":5,\"detail\":\"an `#[ignore]` with no reason: write `#[ignore = \\\"reason and owner\\\"]` or delete the test\",\"line\":2,\"message\":\"A skipped test says why.\",\"path\":\"src/skip.rs\",\"rule\":\"tests.first-properties\",\"text\":\"#[ignore]\"}]\n";

#[test]
fn a_repository_without_a_contract_prints_what_it_printed_before() {
    if !real_ast_grep() {
        return;
    }
    for (name, toml) in [("none", ""), ("other", "[other]\nkey = 1\n")] {
        let repo = Repo::new(&format!("plain-{name}"), toml);
        repo.write("src/skip.rs", SKIPPED_TEST);
        let text_out = repo.check(&["src/skip.rs"]);
        assert_eq!(text_out.status.code(), Some(1));
        assert_eq!(text(&text_out.stdout), EMBEDDED_TEXT);
        let json_out = repo.check(&["--json", "src/skip.rs"]);
        assert_eq!(text(&json_out.stdout), EMBEDDED_JSON);
    }
}
