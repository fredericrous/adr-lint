//! `adr-lint check` with an `[architecture]` contract over Go packages
//! (ADR-0056) through the binary: scratch git repositories, the real
//! ast-grep named in AST_GREP_VERSION, and a fake aval. Skipped locally when
//! that ast-grep is not on PATH.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");

const FAKE_AVAL: &str = r#"#!/bin/sh
cat <<'JSON'
{"rules":[
{"id":"architecture.dependency-rule","active":true,"statement":"Layers."},
{"id":"boundaries.own-the-interface","active":true,"statement":"Own it."},
{"id":"components.acyclic-dependencies","active":true,"statement":"No cycles."}
]}
JSON
"#;

const CONTRACT: &str = r#"[[architecture.layers]]
name = "domain"
paths = ["internal/domain/**"]

[[architecture.layers]]
name = "adapters"
paths = ["internal/adapters/**"]

[[architecture.only_in]]
imports = ["github.com/jackc/pgx", "github.com/lib/pq", "example.com/sibling"]
paths = ["internal/adapters/**"]

[architecture.cycles]
check = true
"#;

const DB: &str = "example.com/app/internal/adapters/db";

struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("adr-lint-go-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let repo = Repo(dir);
        repo.write("adr-lint.toml", CONTRACT);
        repo.write(
            "go.mod",
            "module example.com/app\n\ngo 1.22\n\nreplace example.com/sibling => ../sibling\n",
        );
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
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo.0)
            .status()
            .expect("git runs");
        assert!(init.success());
        repo.write("internal/adapters/db/db.go", "package db\n");
        repo
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
    }

    fn add(&self, names: &[&str]) {
        let added = Command::new("git")
            .arg("add")
            .args(names)
            .current_dir(&self.0)
            .status()
            .expect("git runs");
        assert!(added.success());
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

fn importing(path: &str) -> String {
    format!("package domain\n\nimport \"{path}\"\n")
}

#[test]
fn a_later_layer_import_is_reported_with_its_detail_lines() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("later");
    repo.write("internal/domain/core.go", &importing(DB));
    let out = repo.check(&["internal/domain/core.go"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("internal/domain/core.go:3:8: architecture.dependency-rule: Layers.\n"),
        "{stdout}"
    );
    let detail: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect();
    let joined = detail.join(" ");
    for expected in [
        "internal/adapters/db",
        "example.com/app/internal/adapters/db",
        "adr-lint.toml:5",
    ] {
        assert!(joined.contains(expected), "{expected}\n{stdout}");
    }
    let importer = joined.find("(layer `domain`) imports");
    let imported = joined.rfind("`adapters`)");
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
fn nested_modules_tests_and_ignored_files_are_left_out_and_generated_and_tagged_files_count() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("files");
    repo.write(
        "internal/domain/tool/go.mod",
        "module example.com/tool\n\ngo 1.22\n",
    );
    repo.write(
        "internal/domain/tool/main.go",
        &format!("package main\n\nimport \"{DB}\"\n"),
    );
    repo.write(
        "internal/domain/uses_tools.go",
        &importing("example.com/app/internal/domain/tool"),
    );
    repo.write("internal/domain/core_test.go", &importing(DB));
    repo.write(
        "internal/domain/script.go",
        &format!("//go:build ignore\n\n{}", importing(DB)),
    );
    repo.write(
        "internal/domain/linux.go",
        &format!("//go:build linux\n\n{}", importing(DB)),
    );
    repo.write(
        "internal/domain/zz_generated.deepcopy.go",
        &format!(
            "// Code generated by deepcopy-gen. DO NOT EDIT.\n\n{}",
            importing(DB)
        ),
    );
    let out = repo.check(&["."]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    let reported: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains(": architecture.dependency-rule:"))
        .collect();
    assert_eq!(reported.len(), 2, "{stdout}");
    assert!(stdout.contains("internal/domain/linux.go:5:"), "{stdout}");
    assert!(
        stdout.contains("internal/domain/zz_generated.deepcopy.go:5:"),
        "{stdout}"
    );
}

#[test]
fn every_import_spec_form_is_an_edge() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("forms");
    for name in ["a", "b", "c", "d", "e"] {
        repo.write(
            &format!("internal/adapters/{name}/{name}.go"),
            &format!("package {name}\n"),
        );
    }
    repo.write(
        "internal/domain/forms.go",
        "package domain\n\nimport (\n\t\"example.com/app/internal/adapters/a\"\n\
         \tb \"example.com/app/internal/adapters/b\"\n\
         \t_ \"example.com/app/internal/adapters/c\"\n\
         \t. \"example.com/app/internal/adapters/d\"\n\
         \t`example.com/app/internal/adapters/e`\n)\n",
    );
    let out = repo.check(&["internal/domain/forms.go"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    for line in 4..=8 {
        let expected = format!("internal/domain/forms.go:{line}:");
        assert!(stdout.contains(&expected), "{expected}\n{stdout}");
    }
}

#[test]
fn an_only_in_import_is_matched_on_whole_segments_and_a_blank_driver_import_is_reported() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("only-in");
    repo.write(
        "internal/domain/store.go",
        "package domain\n\nimport (\n\t\"github.com/jackc/pgx/v5\"\n\
         \t\"github.com/jackc/pgxlisten\"\n\
         \t_ \"github.com/lib/pq\"\n\
         \t\"example.com/sibling/util\"\n)\n",
    );
    repo.write(
        "internal/adapters/db/pg.go",
        "package db\n\nimport (\n\t\"github.com/jackc/pgx/v5\"\n\t_ \"github.com/lib/pq\"\n)\n",
    );
    let out = repo.check(&["internal/domain/store.go", "internal/adapters/db/pg.go"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    for expected in [
        "internal/domain/store.go:4:2: boundaries.own-the-interface: Own it.\n",
        "internal/domain/store.go:6:2: boundaries.own-the-interface",
        "internal/domain/store.go:7:2: boundaries.own-the-interface",
    ] {
        assert!(stdout.contains(expected), "{expected}\n{stdout}");
    }
    assert!(!stdout.contains("internal/domain/store.go:5:"), "{stdout}");
    assert!(!stdout.contains("internal/adapters/db/pg.go"), "{stdout}");
    assert!(!stdout.contains("architecture.dependency-rule"), "{stdout}");
}

#[test]
fn an_untracked_package_counts_as_the_target_of_an_import() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("untracked");
    repo.write("internal/domain/core.go", &importing(DB));
    repo.add(&["adr-lint.toml", "go.mod", "internal/domain/core.go"]);
    let out = repo.check(&["internal/domain/core.go"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(stdout.contains("internal/domain/core.go:3:"), "{stdout}");
}

#[test]
fn a_go_mod_change_widens_the_report_to_every_file() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("widen");
    repo.write("internal/domain/core.go", &importing(DB));
    let narrow = repo.check(&["internal/adapters/db/db.go"]);
    assert_eq!(
        narrow.status.code(),
        Some(0),
        "{}{}",
        text(&narrow.stdout),
        text(&narrow.stderr)
    );
    let out = repo.check(&["go.mod"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(stdout.contains("internal/domain/core.go:3:"), "{stdout}");
}

#[test]
fn a_waiver_on_the_import_line_gives_exit_0() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("waiver");
    repo.write(
        "internal/domain/core.go",
        &format!(
            "package domain\n\nimport \"{DB}\" // adr-lint: allow architecture.dependency-rule -- a bridge\n"
        ),
    );
    let out = repo.check(&["internal/domain/core.go"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn two_packages_that_import_each_other_are_no_cycle_finding() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("cycle");
    repo.write(
        "internal/adapters/x/x.go",
        "package x\n\nimport \"example.com/app/internal/adapters/y\"\n",
    );
    repo.write(
        "internal/adapters/y/y.go",
        "package y\n\nimport \"example.com/app/internal/adapters/x\"\n",
    );
    let out = repo.check(&["."]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}{}", text(&out.stderr));
    assert!(
        !stdout.contains("components.acyclic-dependencies"),
        "{stdout}"
    );
}
