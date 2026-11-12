//! The Rust module graph (ADR-0056) through the binary's public surface:
//! scratch crates on disk, the real ast-grep named in AST_GREP_VERSION, and
//! the findings `adr-lint check --json` prints for an `[architecture]`
//! contract. Skipped locally when that ast-grep is not on PATH.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");

const FAKE_AVAL: &str = r#"#!/bin/sh
echo '{"rules":[{"id":"architecture.dependency-rule","active":true,"statement":"Layers."},{"id":"boundaries.own-the-interface","active":true,"statement":"Own it."},{"id":"components.acyclic-dependencies","active":true,"statement":"No cycles."}]}'
"#;

const INNER_OUTER: &str = r#"
[[architecture.layers]]
name = "inner"
modules = ["crate::low", "crate::cli"]

[[architecture.layers]]
name = "outer"
modules = ["crate::high", "crate::mid", "crate::inline", "crate::sub"]
"#;

struct Crate(PathBuf);

impl Crate {
    fn new(name: &str, contract: &str) -> Crate {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-graph-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(dir.join("src")).expect("scratch dir");
        let scratch = Crate(dir);
        scratch.write(
            "Cargo.toml",
            "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        scratch.write("adr-lint.toml", contract);
        scratch.write("aval-fake", FAKE_AVAL);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                scratch.0.join("aval-fake"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("chmod");
        }
        scratch
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
    }

    /// The findings of one rule over `src`, as (path, line, detail).
    fn findings(&self, rule: &str) -> Vec<(String, u64, String)> {
        let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .args(["check", "--json", "--rule", rule, "src"])
            .current_dir(&self.0)
            .env_remove("GREENGATE_FILES")
            .env_remove("ADR_LINT_AST_GREP")
            .env("ADR_LINT_AVAL", self.0.join("aval-fake"))
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            matches!(out.status.code(), Some(0 | 1)),
            "adr-lint failed: {stderr}"
        );
        let document: Value =
            serde_json::from_slice(&out.stdout).expect("check --json prints an array");
        document
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
            .collect()
    }
}

impl Drop for Crate {
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

fn imports(importer: &str, written: &str, target: &str) -> String {
    let (from_layer, to_layer) = if importer == "crate::low" || importer == "crate::cli" {
        ("inner", "outer")
    } else {
        ("outer", "inner")
    };
    format!(
        "`{importer}` (layer `{from_layer}`) imports `{written}` from `{target}` (layer \
         `{to_layer}`); a layer may import only itself and the layers before it, and \
         `{to_layer}` is declared after `{from_layer}` at adr-lint.toml:6"
    )
}

fn dependency(scratch: &Crate) -> Vec<(String, u64, String)> {
    scratch.findings("architecture.dependency-rule")
}

fn lib_with(scratch: &Crate, modules: &str) {
    scratch.write("src/lib.rs", modules);
}

#[test]
fn a_brace_group_is_one_edge_per_leaf_and_mod_rs_and_x_rs_both_key_modules() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("brace", INNER_OUTER);
    lib_with(&scratch, "pub mod low;\npub mod high;\npub mod mid;\n");
    scratch.write(
        "src/low.rs",
        "use crate::high::{a::f, b};\nuse crate::mid::{leaf::{g}};\n",
    );
    scratch.write("src/high/mod.rs", "pub mod a;\npub mod b;\n");
    scratch.write("src/high/a.rs", "pub fn f() {}\n");
    scratch.write("src/high/b.rs", "pub fn h() {}\n");
    scratch.write("src/mid.rs", "pub mod leaf;\n");
    scratch.write("src/mid/leaf.rs", "pub fn g() {}\n");
    let found = dependency(&scratch);
    let details: Vec<(u64, String)> = found
        .iter()
        .map(|(path, line, detail)| {
            assert_eq!(path, "src/low.rs");
            (*line, detail.clone())
        })
        .collect();
    assert_eq!(
        details,
        [
            (
                1,
                imports("crate::low", "crate::high::a::f", "crate::high::a")
            ),
            (1, imports("crate::low", "crate::high::b", "crate::high::b")),
            (
                2,
                imports("crate::low", "crate::mid::leaf::g", "crate::mid::leaf")
            ),
        ]
    );
}

#[test]
fn super_and_self_paths_resolve_to_the_module_they_name() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("super", INNER_OUTER);
    lib_with(&scratch, "pub mod low;\npub mod high;\n");
    scratch.write("src/low.rs", "pub fn g() {}\n");
    scratch.write("src/high/mod.rs", "pub mod a;\npub mod b;\n");
    scratch.write("src/high/a.rs", "pub fn f() {}\n");
    scratch.write(
        "src/high/b.rs",
        "use super::a::f;\nuse self::deep::q;\npub mod deep {}\n",
    );
    let layers = r#"
[[architecture.layers]]
name = "inner"
modules = ["crate::high::b"]

[[architecture.layers]]
name = "outer"
modules = ["crate::high::a"]
"#;
    scratch.write("adr-lint.toml", layers);
    let found = dependency(&scratch);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "src/high/b.rs");
    assert_eq!(found[0].1, 1);
    assert!(
        found[0].2.contains(
            "`crate::high::b` (layer `inner`) imports `super::a::f` from `crate::high::a`"
        ),
        "{}",
        found[0].2
    );

    scratch.write(
        "adr-lint.toml",
        "[[architecture.only_in]]\nimports = [\"crate::high::b::deep\"]\nmodules = [\"crate::low\"]\n",
    );
    let found = scratch.findings("boundaries.own-the-interface");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, 2);
    assert!(
        found[0]
            .2
            .contains("`self::deep::q` is imported in `crate::high::b`"),
        "{}",
        found[0].2
    );
}

#[test]
fn an_inline_mod_is_a_module_and_test_code_is_not_part_of_the_graph() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("inline", INNER_OUTER);
    scratch.write(
        "src/lib.rs",
        "pub mod low;\npub mod inline {\n    pub fn q() {}\n}\n",
    );
    scratch.write(
        "src/low.rs",
        "use crate::inline::q;\n\
         #[cfg(test)]\nmod tests {\n    use crate::inline::q as in_tests;\n}\n\
         #[cfg(all(test, feature = \"x\"))]\nuse crate::inline::q as when_testing;\n\
         #[cfg(test)]\nfn helper() {\n    crate::inline::q();\n}\n\
         pub fn g() {\n    crate::inline::q();\n}\n",
    );
    scratch.write("src/tests.rs", "use crate::inline::q;\n");
    let found = dependency(&scratch);
    let lines: Vec<(&str, u64)> = found
        .iter()
        .map(|(path, line, _)| (path.as_str(), *line))
        .collect();
    assert_eq!(lines, [("src/low.rs", 1), ("src/low.rs", 13)], "{found:?}");
    assert_eq!(
        found[0].2,
        imports("crate::low", "crate::inline::q", "crate::inline")
    );
}

#[test]
fn a_fully_qualified_path_is_one_edge() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("path", INNER_OUTER);
    lib_with(&scratch, "pub mod low;\npub mod high;\n");
    scratch.write("src/low.rs", "pub fn g() {\n    crate::high::a::f();\n}\n");
    scratch.write("src/high/mod.rs", "pub mod a;\n");
    scratch.write("src/high/a.rs", "pub fn f() {}\n");
    let found = dependency(&scratch);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, 2);
    assert_eq!(
        found[0].2,
        imports("crate::low", "crate::high::a::f", "crate::high::a")
    );
}

#[test]
fn a_parent_glob_import_is_reported_as_a_fact() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("glob", INNER_OUTER);
    lib_with(&scratch, "pub mod high;\n");
    scratch.write("src/high/mod.rs", "pub mod a;\n");
    scratch.write("src/high/a.rs", "use super::*;\n");
    let found = dependency(&scratch);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0.as_str(), found[0].1), ("src/high/a.rs", 1));
    assert_eq!(
        found[0].2,
        "`use super::*;` in `crate::high::a` (layer `outer`, adr-lint.toml:6) imports everything \
         a parent or sibling module has in scope, which the layer order cannot check: name what \
         is imported"
    );
}

fn cycle_contract() -> &'static str {
    "[architecture.cycles]\ncheck = true\n"
}

#[test]
fn a_two_module_cycle_names_its_members_cycle_and_use_sites() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("two", cycle_contract());
    lib_with(&scratch, "pub mod two;\n");
    scratch.write("src/two/mod.rs", "pub mod a;\npub mod b;\n");
    scratch.write("src/two/a.rs", "use super::b::g;\npub fn f() {}\n");
    scratch.write("src/two/b.rs", "\nuse super::a::f;\npub fn g() {}\n");
    let found = scratch.findings("components.acyclic-dependencies");
    let detail = "these modules import each other, all inside `crate::two`:\n  crate::two::a\n  \
                  crate::two::b\na shortest cycle, one import per step:\n  \
                  crate::two::a -> crate::two::b (src/two/a.rs:1)\n  \
                  crate::two::b -> crate::two::a (src/two/b.rs:2)";
    let expected: Vec<(String, u64, String)> = vec![
        ("src/two/a.rs".into(), 1, detail.into()),
        ("src/two/b.rs".into(), 2, detail.into()),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_three_module_cycle_at_depth_names_one_import_per_step() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("three", cycle_contract());
    lib_with(&scratch, "pub mod outer;\n");
    scratch.write("src/outer/mod.rs", "pub mod ring;\n");
    scratch.write(
        "src/outer/ring/mod.rs",
        "pub mod x;\npub mod y;\npub mod z;\n",
    );
    scratch.write("src/outer/ring/x.rs", "use super::y::Y;\n");
    scratch.write("src/outer/ring/y.rs", "\n\nuse super::z::Z;\n");
    scratch.write("src/outer/ring/z.rs", "use crate::outer::ring::x::X;\n");
    let found = scratch.findings("components.acyclic-dependencies");
    let detail = "these modules import each other, all inside `crate::outer::ring`:\n  \
                  crate::outer::ring::x\n  crate::outer::ring::y\n  crate::outer::ring::z\n\
                  a shortest cycle, one import per step:\n  \
                  crate::outer::ring::x -> crate::outer::ring::y (src/outer/ring/x.rs:1)\n  \
                  crate::outer::ring::y -> crate::outer::ring::z (src/outer/ring/y.rs:3)\n  \
                  crate::outer::ring::z -> crate::outer::ring::x (src/outer/ring/z.rs:1)";
    let expected: Vec<(String, u64, String)> = vec![
        ("src/outer/ring/x.rs".into(), 1, detail.into()),
        ("src/outer/ring/y.rs".into(), 3, detail.into()),
        ("src/outer/ring/z.rs".into(), 1, detail.into()),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_cycle_only_visible_through_a_parent_re_export_is_found() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("reexport", cycle_contract());
    lib_with(&scratch, "pub mod parent;\n");
    scratch.write(
        "src/parent/mod.rs",
        "pub mod a;\npub mod b;\npub use self::a::Item as Alias;\n",
    );
    scratch.write("src/parent/a.rs", "use super::b::g;\npub struct Item;\n");
    scratch.write("src/parent/b.rs", "use super::Alias;\npub fn g() {}\n");
    let found = scratch.findings("components.acyclic-dependencies");
    let detail = "these modules import each other, all inside `crate::parent`:\n  \
                  crate::parent::a\n  crate::parent::b\n\
                  a shortest cycle, one import per step:\n  \
                  crate::parent::a -> crate::parent::b (src/parent/a.rs:1)\n  \
                  crate::parent::b -> crate::parent::a (src/parent/b.rs:1)";
    let expected: Vec<(String, u64, String)> = vec![
        ("src/parent/a.rs".into(), 1, detail.into()),
        ("src/parent/b.rs".into(), 1, detail.into()),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_bin_reaches_its_own_lib_through_the_crate_name() {
    if !real_ast_grep() {
        return;
    }
    let scratch = Crate::new("bin", INNER_OUTER);
    lib_with(&scratch, "pub mod high;\n");
    scratch.write("src/high/mod.rs", "pub mod a;\n");
    scratch.write("src/high/a.rs", "pub fn f() {}\n");
    scratch.write("src/main.rs", "mod cli;\nfn main() {}\n");
    scratch.write("src/cli.rs", "use scratch::high::a::f;\n");
    let found = dependency(&scratch);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0.as_str(), found[0].1), ("src/cli.rs", 1));
    assert_eq!(
        found[0].2,
        imports("crate::cli", "scratch::high::a::f", "crate::high::a")
    );
}
