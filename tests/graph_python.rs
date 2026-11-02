//! The Python import graph (ADR-0056) through the binary's public
//! surface: scratch projects on disk, the real ast-grep named in
//! AST_GREP_VERSION, and the cycle findings and warnings `adr-lint check`
//! prints for an `[architecture.cycles]` contract. Skipped locally when that
//! ast-grep is not on PATH.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");

const FAKE_AVAL: &str = r#"#!/bin/sh
echo '{"rules":[{"id":"components.acyclic-dependencies","active":true,"statement":"No cycles."}]}'
"#;

const RULE: &str = "components.acyclic-dependencies";

struct Project(PathBuf);

struct Run {
    findings: Vec<(String, u64, String)>,
    stderr: String,
}

impl Project {
    fn new(name: &str) -> Project {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-graph-py-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let project = Project(dir);
        project.write("adr-lint.toml", "[architecture.cycles]\ncheck = true\n");
        project.write("aval-fake", FAKE_AVAL);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                project.0.join("aval-fake"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("chmod");
        }
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&project.0)
            .status()
            .expect("git runs");
        assert!(init.success());
        project
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
    }

    fn check(&self) -> Run {
        self.check_files(&["."])
    }

    fn check_files(&self, files: &[&str]) -> Run {
        let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .args(["check", "--json", "--rule", RULE])
            .args(files)
            .current_dir(&self.0)
            .env_remove("AMONT_FILES")
            .env_remove("ADR_LINT_AST_GREP")
            .env("ADR_LINT_AVAL", self.0.join("aval-fake"))
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            matches!(out.status.code(), Some(0 | 1)),
            "adr-lint failed: {stderr}"
        );
        let document: Value =
            serde_json::from_slice(&out.stdout).expect("check --json prints an array");
        let mut findings: Vec<(String, u64, String)> = document
            .as_array()
            .expect("an array of findings")
            .iter()
            .map(|found| {
                (
                    found["path"].as_str().expect("path").to_string(),
                    found["line"].as_u64().expect("line"),
                    found["detail"].as_str().expect("detail").to_string(),
                )
            })
            .collect();
        findings.sort();
        Run { findings, stderr }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

fn real_ast_grep() -> bool {
    let wanted = format!("ast-grep {}", EXPECTED_AST_GREP.trim());
    let printed = Command::new("ast-grep").arg("--version").output();
    let found = printed
        .as_ref()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    if found == wanted {
        return cfg!(unix);
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI needs `{wanted}` on PATH (found `{found}`)"
    );
    eprintln!("SKIPPED: `{wanted}` is not on PATH (found `{found}`)");
    false
}

const FLAT: &str = "[project]\nname = \"app\"\n";

fn paths(run: &Run) -> Vec<&str> {
    run.findings.iter().map(|found| found.0.as_str()).collect()
}

fn members(detail: &str) -> Vec<&str> {
    detail
        .lines()
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .map(str::trim)
        .collect()
}

#[test]
fn a_src_layout_member_maps_its_import_name_and_virtual_roots_and_excluded_members_are_skipped() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("workspace");
    project.write(
        "pyproject.toml",
        "[tool.uv.workspace]\nmembers = [\"packages/*\"]\nexclude = [\"packages/skipped\"]\n",
    );
    project.write(
        "packages/domain-trade/pyproject.toml",
        "[project]\nname = \"domain-trade\"\n",
    );
    project.write("packages/domain-trade/src/domain_trade/__init__.py", "");
    project.write(
        "packages/domain-trade/src/domain_trade/a.py",
        "import domain_trade.b\n",
    );
    project.write(
        "packages/domain-trade/src/domain_trade/b.py",
        "\nfrom domain_trade import a\n",
    );
    project.write(
        "packages/skipped/pyproject.toml",
        "[project]\nname = \"skipped\"\n",
    );
    project.write("packages/skipped/src/skipped/__init__.py", "");
    project.write("packages/skipped/src/skipped/a.py", "import skipped.b\n");
    project.write("packages/skipped/src/skipped/b.py", "import skipped.a\n");
    let run = project.check();
    let detail = "these modules import each other, all inside `domain_trade`:\n  domain_trade.a\n  domain_trade.b\n\
                  a shortest cycle, one import per step:\n  domain_trade.a -> domain_trade.b (packages/domain-trade/src/domain_trade/a.py:1)\n  \
                  domain_trade.b -> domain_trade.a (packages/domain-trade/src/domain_trade/b.py:2)";
    assert_eq!(
        run.findings,
        [
            (
                "packages/domain-trade/src/domain_trade/a.py".to_string(),
                1,
                detail.to_string()
            ),
            (
                "packages/domain-trade/src/domain_trade/b.py".to_string(),
                2,
                detail.to_string()
            ),
        ]
    );
}

#[test]
fn a_flat_project_is_a_root_and_a_root_with_a_package_counts() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("flat");
    project.write("pyproject.toml", FLAT);
    project.write("app/__init__.py", "");
    project.write("app/a.py", "import app.b\n");
    project.write("app/b.py", "import app.a\n");
    let run = project.check();
    assert!(!run.stderr.contains("warning"), "{}", run.stderr);
    assert_eq!(paths(&run), ["app/a.py", "app/b.py"]);
    assert_eq!(members(&run.findings[0].2), ["app.a", "app.b"]);
}

#[test]
fn relative_imports_resolve_at_two_depths_and_a_package_is_folded() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("relative");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write("pkg/sub/__init__.py", "");
    project.write("pkg/sub/x.py", "from .. import y\n");
    project.write("pkg/y.py", "from .sub import x\n");
    project.write("pkg/sub/deep/__init__.py", "");
    project.write(
        "pkg/sub/deep/z.py",
        "from ..x import value\nfrom ...y import other\n",
    );
    let run = project.check();
    assert_eq!(
        paths(&run),
        ["pkg/sub/deep/z.py", "pkg/sub/x.py", "pkg/y.py"]
    );
    assert_eq!(members(&run.findings[0].2), ["pkg.sub", "pkg.y"]);
}

#[test]
fn a_submodule_import_is_an_edge_to_it_and_a_name_import_is_an_edge_to_the_package() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("names");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "from pkg.c import thing\n");
    project.write("pkg/a.py", "from pkg import b\n");
    project.write("pkg/b.py", "import pkg.a\n");
    project.write("pkg/c.py", "from pkg import thing\nthing = 1\n");
    let run = project.check();
    assert_eq!(paths(&run), ["pkg/a.py", "pkg/b.py"]);
    assert_eq!(members(&run.findings[0].2), ["pkg.a", "pkg.b"]);
}

#[test]
fn aliases_parenthesized_lists_and_star_imports_are_edges() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("forms");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write("pkg/a.py", "import pkg.b as bb\n");
    project.write("pkg/b.py", "from pkg import (\n    c,\n    d as dd,\n)\n");
    project.write("pkg/c.py", "from pkg.a import *\n");
    project.write("pkg/d.py", "");
    let run = project.check();
    assert_eq!(paths(&run), ["pkg/a.py", "pkg/b.py", "pkg/c.py"]);
    assert_eq!(members(&run.findings[0].2), ["pkg.a", "pkg.b", "pkg.c"]);
}

#[test]
fn guarded_function_local_and_try_imports_count_for_cycles() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("guarded");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write("pkg/hub.py", "import pkg.t\nimport pkg.f\nimport pkg.e\n");
    project.write(
        "pkg/t.py",
        "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from pkg import hub\n",
    );
    project.write("pkg/f.py", "def go():\n    import pkg.hub\n");
    project.write(
        "pkg/e.py",
        "try:\n    from pkg import hub\nexcept ImportError:\n    hub = None\n",
    );
    let run = project.check();
    assert_eq!(
        paths(&run),
        [
            "pkg/e.py",
            "pkg/f.py",
            "pkg/hub.py",
            "pkg/hub.py",
            "pkg/hub.py",
            "pkg/t.py"
        ]
    );
    assert_eq!(
        members(&run.findings[0].2),
        ["pkg.e", "pkg.f", "pkg.hub", "pkg.t"]
    );
}

#[test]
fn a_literal_importlib_import_is_not_a_cycle_edge() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("importlib");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write(
        "pkg/x.py",
        "import importlib\nload = lambda: importlib.import_module(\"pkg.y\")\n",
    );
    project.write("pkg/y.py", "import pkg.x\n");
    let run = project.check();
    assert_eq!(run.findings, []);
}

#[test]
fn tests_and_conftest_are_left_out() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("tests");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write(
        "pkg/a.py",
        "import pkg.test_a\nimport pkg.a_test\nimport pkg.conftest\nimport pkg.tests.t\n",
    );
    project.write("pkg/test_a.py", "import pkg.a\n");
    project.write("pkg/a_test.py", "import pkg.a\n");
    project.write("pkg/conftest.py", "import pkg.a\n");
    project.write("pkg/tests/__init__.py", "");
    project.write("pkg/tests/t.py", "import pkg.a\n");
    let run = project.check();
    assert_eq!(run.findings, []);
}

#[test]
fn a_cycle_between_sibling_packages_shows_only_when_the_package_is_folded() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("folded");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write(
        "pkg/adaptive.py",
        "from pkg.sources.errors import Failure\n",
    );
    project.write("pkg/sources/__init__.py", "");
    project.write("pkg/sources/errors.py", "class Failure: ...\n");
    project.write(
        "pkg/sources/boursorama.py",
        "from pkg.adaptive import run\n",
    );
    let run = project.check();
    let detail = "these modules import each other, all inside `pkg`:\n  pkg.adaptive\n  pkg.sources\n\
                  a shortest cycle, one import per step:\n  pkg.adaptive -> pkg.sources (pkg/adaptive.py:1)\n  \
                  pkg.sources -> pkg.adaptive (pkg/sources/boursorama.py:1)";
    assert_eq!(
        run.findings,
        [
            ("pkg/adaptive.py".to_string(), 1, detail.to_string()),
            (
                "pkg/sources/boursorama.py".to_string(),
                1,
                detail.to_string()
            ),
        ]
    );
}

#[test]
fn findings_are_for_checked_files_until_a_pyproject_or_init_change_widens_them() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("widen");
    project.write("pyproject.toml", FLAT);
    project.write("pkg/__init__.py", "");
    project.write("pkg/a.py", "import pkg.b\n");
    project.write("pkg/b.py", "import pkg.a\n");
    assert_eq!(paths(&project.check_files(&["pkg/a.py"])), ["pkg/a.py"]);
    assert_eq!(
        paths(&project.check_files(&["pyproject.toml"])),
        ["pkg/a.py", "pkg/b.py"]
    );
    assert_eq!(
        paths(&project.check_files(&["pkg/__init__.py"])),
        ["pkg/a.py", "pkg/b.py"]
    );
}
