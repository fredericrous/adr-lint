//! The TypeScript import graph (ADR-0056) through the binary's public
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
            std::env::temp_dir().join(format!("adr-lint-graph-ts-{name}-{}", std::process::id()));
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

#[test]
fn a_two_file_cycle_through_an_import_type_names_its_members_and_steps() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("two");
    project.write(
        "a.ts",
        "import type { B } from \"./b\";\nexport type A = B;\n",
    );
    project.write(
        "b.ts",
        "import { x } from \"./a.js\";\nexport const b = x;\n",
    );
    let run = project.check();
    let detail = "these files import each other:\n  a.ts\n  b.ts\n\
                  a shortest cycle, one import per step:\n  a.ts -> b.ts (a.ts:1)\n  b.ts -> a.ts (b.ts:1)";
    assert_eq!(
        run.findings,
        [
            ("a.ts".to_string(), 1, detail.to_string()),
            ("b.ts".to_string(), 1, detail.to_string()),
        ]
    );
}

#[test]
fn alias_index_extension_and_reexport_edges_close_cycles() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("resolve");
    project.write("tsconfig.json", "{ \"extends\": \"./config/base.json\" }");
    project.write(
        "config/base.json",
        "{\n // comments and trailing commas are fine\n \"compilerOptions\": { \"paths\": { \"~/*\": [\"../app/*\"], }, },\n}",
    );
    project.write("app/entry.ts", "import \"~/lib/auth.server\";\n");
    project.write(
        "app/lib/auth.server.ts",
        "import { util } from \"../util\";\n",
    );
    project.write("app/util/index.ts", "import \"../entry.js\";\n");
    project.write("app/barrel.ts", "export * from \"./leaf\";\n");
    project.write("app/leaf.ts", "import \"./barrel\";\n");
    let run = project.check();
    let paths: Vec<&str> = run.findings.iter().map(|found| found.0.as_str()).collect();
    assert_eq!(
        paths,
        [
            "app/barrel.ts",
            "app/entry.ts",
            "app/leaf.ts",
            "app/lib/auth.server.ts",
            "app/util/index.ts"
        ]
    );
    assert!(run.findings[1]
        .2
        .contains("  app/entry.ts\n  app/lib/auth.server.ts\n  app/util/index.ts\n"));
    assert!(!run.stderr.contains("warning"), "{}", run.stderr);
}

#[test]
fn dynamic_imports_are_not_cycle_edges_and_computed_ones_are_ignored() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("dynamic");
    project.write("d.ts", "export const load = () => import(\"./e\");\nconst name = \"./e\";\nexport const other = () => import(name);\n");
    project.write("e.ts", "import \"./d\";\n");
    let run = project.check();
    assert_eq!(run.findings, []);
    assert!(!run.stderr.contains("warning"), "{}", run.stderr);
}

#[test]
fn test_files_and_ignored_files_are_left_out_and_untracked_ones_are_in() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("nodes");
    project.write(".gitignore", "skipped.ts\n");
    project.write(
        "f.ts",
        "import \"./f.test\";\nimport \"./skipped\";\nimport \"./g\";\n",
    );
    project.write("f.test.ts", "import \"./f\";\n");
    project.write("skipped.ts", "import \"./f\";\n");
    project.write("g.ts", "import \"./f\";\n");
    let run = project.check_files(&["f.ts", "g.ts"]);
    let paths: Vec<&str> = run.findings.iter().map(|found| found.0.as_str()).collect();
    assert_eq!(paths, ["f.ts", "g.ts"]);
    for found in &run.findings {
        assert!(found.2.contains("  f.ts\n  g.ts\n"), "{}", found.2);
        assert!(
            !found.2.contains("f.test.ts"),
            "a test file is a node:\n{}",
            found.2
        );
        assert!(
            !found.2.contains("skipped.ts"),
            "an ignored file is a node:\n{}",
            found.2
        );
    }
}

#[test]
fn assets_and_generated_route_types_are_silent_and_an_unresolved_alias_warns() {
    if !real_ast_grep() {
        return;
    }
    let project = Project::new("quiet");
    project.write(
        "tsconfig.json",
        "{ \"compilerOptions\": { \"rootDirs\": [\".\", \".react-router/types\"], \"paths\": { \"~/*\": [\"./app/*\"] } } }",
    );
    project.write(".gitignore", ".react-router\n");
    project.write(
        ".react-router/types/app/routes/+types/home.ts",
        "export type Info = {};\n",
    );
    project.write(
        "app/routes/home.tsx",
        "import \"./styles.css\";\nimport logo from \"~/logo.svg\";\nimport type { Info } from \"./+types/home\";\nimport \"~/missing\";\nimport { Effect } from \"effect\";\n",
    );
    let run = project.check();
    assert_eq!(run.findings, []);
    assert!(
        run.stderr
            .starts_with("adr-lint: warning: app/routes/home.tsx:4: cannot resolve `~/missing`\n"),
        "{}",
        run.stderr
    );
    assert_eq!(run.stderr.matches("warning").count(), 1, "{}", run.stderr);
}
