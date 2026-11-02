//! `adr-lint check` with an `[architecture]` contract over Python modules
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
modules = ["app.domain"]

[[architecture.layers]]
name = "adapters"
modules = ["app.adapters"]

[[architecture.only_in]]
imports = ["psycopg"]
modules = ["app.adapters"]
"#;

struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("adr-lint-py-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let repo = Repo(dir);
        repo.write("adr-lint.toml", CONTRACT);
        repo.write("pyproject.toml", "[project]\nname = \"app\"\n");
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
        for package in ["app", "app/domain", "app/adapters", "app/routes"] {
            repo.write(&format!("{package}/__init__.py"), "");
        }
        repo.write("app/adapters/db.py", "X = 1\n");
        repo
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
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
    let repo = Repo::new("later");
    repo.write(
        "app/domain/core.py",
        "from app.adapters import db\nY = db.X\n",
    );
    let out = repo.check(&["app/domain/core.py"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("app/domain/core.py:1:1: architecture.dependency-rule: Layers.\n"),
        "{stdout}"
    );
    let detail: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect();
    let joined = detail.join(" ");
    for expected in [
        "app.adapters.db",
        "from app.adapters import db",
        "adr-lint.toml:5",
    ] {
        assert!(joined.contains(expected), "{expected}\n{stdout}");
    }
    let importer = joined.find("(layer `domain`) imports");
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
fn a_type_checking_import_and_a_literal_importlib_import_are_reported() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("forms");
    repo.write(
        "app/domain/typed.py",
        "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from app.adapters.db import X\n",
    );
    repo.write(
        "app/domain/lazy.py",
        "import importlib\n\n\ndef load():\n    return importlib.import_module(\"app.adapters.db\")\n",
    );
    let out = repo.check(&["app/domain/typed.py", "app/domain/lazy.py"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("app/domain/typed.py:4:"),
        "a TYPE_CHECKING import counts\n{stdout}"
    );
    assert!(
        stdout.contains("app/domain/lazy.py:5:"),
        "a literal importlib import counts\n{stdout}"
    );
}

#[test]
fn an_only_in_package_is_reported_outside_its_modules_and_psycopg2_is_not_matched() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("only-in");
    repo.write(
        "app/domain/core.py",
        "import psycopg\nfrom psycopg.rows import dict_row\nimport psycopg2\n",
    );
    repo.write(
        "app/adapters/pg.py",
        "import psycopg\nfrom psycopg import rows\n",
    );
    repo.write("app/routes/home.py", "import psycopg\n");
    let out = repo.check(&[
        "app/domain/core.py",
        "app/adapters/pg.py",
        "app/routes/home.py",
    ]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    for expected in [
        "app/domain/core.py:1:1: boundaries.own-the-interface: Own it.\n",
        "app/domain/core.py:2:1: boundaries.own-the-interface",
        "app/routes/home.py:1:1: boundaries.own-the-interface",
    ] {
        assert!(stdout.contains(expected), "{expected}\n{stdout}");
    }
    assert!(!stdout.contains("app/domain/core.py:3:"), "{stdout}");
    assert!(!stdout.contains("app/adapters/pg.py"), "{stdout}");
}

#[test]
fn a_waiver_on_the_import_line_gives_exit_0() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new("waiver");
    repo.write(
        "app/domain/core.py",
        "from app.adapters import db  # adr-lint: allow architecture.dependency-rule -- a bridge\n",
    );
    let out = repo.check(&["app/domain/core.py"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}
