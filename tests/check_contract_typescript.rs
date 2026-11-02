//! `adr-lint check` with an `[architecture]` contract over TypeScript files
//! (ADR-0056) through the binary: scratch git repositories, the real ast-grep
//! named in AST_GREP_VERSION, and a fake aval. Skipped locally when that
//! ast-grep is not on PATH.

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

const LAYERS: &str = r#"[[architecture.layers]]
name = "domain"
paths = ["src/domain/**/*.ts"]

[[architecture.layers]]
name = "adapters"
paths = ["src/adapters/**/*.ts"]
"#;

const ONLY_IN: &str = r#"
[[architecture.only_in]]
imports = ["@effect/sql"]
paths = ["src/adapters/**/*.ts"]
"#;

const DOMAIN_IMPORTS_ADAPTER: &str =
    "import { db } from \"../adapters/db\";\nexport const x = db;\n";

struct Repo(PathBuf);

impl Repo {
    fn new(name: &str, toml: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("adr-lint-ts-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let repo = Repo(dir);
        repo.write("adr-lint.toml", toml);
        repo.write("tsconfig.json", "{\"compilerOptions\":{}}\n");
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
        self.git(&["add", "src", "adr-lint.toml", "tsconfig.json"]);
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

fn layered(name: &str) -> Repo {
    let repo = Repo::new(name, &format!("{LAYERS}{ONLY_IN}"));
    repo.write("src/adapters/db.ts", "export const db = 1;\n");
    repo.write(
        "src/adapters/types.ts",
        "export type Row = { id: number };\n",
    );
    repo
}

#[test]
fn a_later_layer_import_is_reported_with_its_detail_lines() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("later");
    repo.write("src/domain/core.ts", DOMAIN_IMPORTS_ADAPTER);
    let out = repo.check(&["src/domain/core.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/domain/core.ts:1:1: architecture.dependency-rule: Layers.\n"),
        "{stdout}"
    );
    let detail: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("    "))
        .collect();
    let joined = detail.join(" ");
    for expected in ["src/adapters/db.ts", "../adapters/db", "adr-lint.toml:5"] {
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
fn a_type_import_and_a_dynamic_import_across_layers_are_reported() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("forms");
    repo.write(
        "src/domain/typed.ts",
        "import type { Row } from \"../adapters/types\";\nexport type R = Row;\n",
    );
    repo.write(
        "src/domain/lazy.ts",
        "export const load = () => import(\"../adapters/db\");\n",
    );
    let out = repo.check(&["src/domain/typed.ts", "src/domain/lazy.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/domain/typed.ts:1:1: architecture.dependency-rule"),
        "{stdout}"
    );
    assert!(
        stdout.contains("src/domain/lazy.ts:1:"),
        "a dynamic import counts for layers\n{stdout}"
    );
}

#[test]
fn a_suffix_exclude_leaves_a_file_in_no_layer() {
    if !real_ast_grep() {
        return;
    }
    let repo = Repo::new(
        "excludes",
        r#"[[architecture.layers]]
name = "domain"
paths = ["app/domain/**/*.ts"]

[[architecture.layers]]
name = "lib"
paths = ["app/lib/**/*.ts", "!app/lib/**/*.server.ts"]
"#,
    );
    repo.write("app/lib/util.ts", "export const util = 1;\n");
    repo.write("app/lib/db.server.ts", "export const db = 1;\n");
    repo.write(
        "app/domain/a.ts",
        "import { util } from \"../lib/util\";\nexport const a = util;\n",
    );
    repo.write(
        "app/domain/b.ts",
        "import { db } from \"../lib/db.server\";\nexport const b = db;\n",
    );
    let out = repo.check(&["app/domain/a.ts", "app/domain/b.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("app/domain/a.ts:1:1: architecture.dependency-rule"),
        "{stdout}"
    );
    assert!(!stdout.contains("app/domain/b.ts"), "{stdout}");
}

#[test]
fn an_only_in_package_is_reported_in_an_unlayered_file_and_allowed_inside() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("only-in");
    repo.write(
        "src/routes/home.ts",
        "import { sql } from \"@effect/sql\";\nimport { x } from \"@effect/sql/x\";\nexport const h = [sql, x];\n",
    );
    repo.write(
        "src/adapters/pg.ts",
        "import { sql } from \"@effect/sql\";\nexport const s = sql;\n",
    );
    let out = repo.check(&["src/routes/home.ts", "src/adapters/pg.ts"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/routes/home.ts:1:1: boundaries.own-the-interface: Own it.\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("src/routes/home.ts:2:1: boundaries.own-the-interface"),
        "{stdout}"
    );
    assert!(!stdout.contains("src/adapters/pg.ts"), "{stdout}");
}

#[test]
fn a_waiver_on_the_import_line_gives_exit_0() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("waiver");
    repo.write(
        "src/domain/core.ts",
        "import { db } from \"../adapters/db\"; // adr-lint: allow architecture.dependency-rule -- a bridge\nexport const x = db;\n",
    );
    let out = repo.check(&["src/domain/core.ts"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn only_tsconfig_staged_widens_to_every_file() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("tsconfig");
    repo.write("src/domain/core.ts", DOMAIN_IMPORTS_ADAPTER);
    repo.commit_all();
    let quiet = repo.check(&[]);
    assert_eq!(quiet.status.code(), Some(0), "{}", text(&quiet.stdout));
    repo.write("tsconfig.json", "{\"compilerOptions\":{\"strict\":true}}\n");
    repo.git(&["add", "tsconfig.json"]);
    let out = repo.check(&[]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("src/domain/core.ts:1:1: architecture.dependency-rule"),
        "{stdout}"
    );
}

#[test]
fn unresolved_warnings_stop_at_five_then_count_the_rest() {
    if !real_ast_grep() {
        return;
    }
    let repo = layered("warnings");
    let body: String = (0..8)
        .map(|index| format!("import \"./missing{index}\";\n"))
        .collect();
    repo.write("src/domain/broken.ts", &body);
    let out = repo.check(&["src/domain/broken.ts"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{}{stderr}", text(&out.stdout));
    assert_eq!(stderr.matches("cannot resolve").count(), 5, "{stderr}");
    assert!(stderr.contains("+3 more"), "{stderr}");
}
