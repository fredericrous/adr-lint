//! `adr-lint layers summary` and `layers explain` through the binary
//! (ADR-0056): scratch directories, and for the one test that lists a file's
//! findings the real ast-grep named in AST_GREP_VERSION and a fake aval.

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

const THREE_LAYERS: &str = r#"[[architecture.layers]]
name = "domain"
modules = ["crate::domain"]

[[architecture.layers]]
name = "application"
modules = ["crate::application"]

[[architecture.layers]]
name = "adapters"
modules = ["crate::adapters"]

[[architecture.only_in]]
imports = ["rusqlite"]
modules = ["crate::adapters"]
"#;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str, toml: &str) -> Dir {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-layers-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let scratch = Dir(dir);
        if !toml.is_empty() {
            scratch.write("adr-lint.toml", toml);
        }
        scratch
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("fixture");
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_adr-lint"));
        command
            .arg("layers")
            .args(args)
            .current_dir(&self.0)
            .env_remove("ADR_LINT_AST_GREP")
            .env_remove("NO_COLOR")
            .env("ADR_LINT_AVAL", self.0.join("aval-fake"))
            .stdin(Stdio::null());
        command
    }

    fn layers(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("adr-lint runs")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
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

#[test]
fn bare_layers_prints_usage_on_stderr_and_exits_2() {
    let dir = Dir::new("bare", THREE_LAYERS);
    let out = dir.layers(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).starts_with("usage: adr-lint layers"));
}

#[test]
fn help_layers_exits_0_and_the_top_level_usage_lists_the_command() {
    let dir = Dir::new("help", "");
    let out = dir.command(&[]).arg("--help").output().expect("runs");
    assert_eq!(out.status.code(), Some(0));
    let help = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["help", "layers"])
        .output()
        .expect("runs");
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help.stdout).starts_with("usage: adr-lint layers"));
    let top = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .arg("--help")
        .output()
        .expect("runs");
    assert!(text(&top.stdout).contains("  layers "));
}

#[test]
fn summary_of_three_layers_and_one_only_in_is_the_documented_four_lines() {
    let dir = Dir::new("three", THREE_LAYERS);
    let out = dir.layers(&["summary"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "architecture (adr-lint.toml, ADR-0056):\n\
         \x20 layers   domain -> application -> adapters\n\
         \x20 only in  rusqlite: crate::adapters\n\
         \x20 detail   adr-lint layers explain [PATH]\n"
    );
}

#[test]
fn summary_accepts_no_color_as_check_does() {
    let dir = Dir::new("summary-no-color", THREE_LAYERS);
    let out = dir.layers(&["summary", "--no-color"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout).lines().count(), 4);
}

#[test]
fn summary_keeps_every_line_within_80_columns_and_marks_a_cut_list() {
    let mut toml = String::new();
    for index in 0..8 {
        toml.push_str(&format!(
            "[[architecture.layers]]\nname = \"a_rather_long_layer_name_{index}\"\nmodules = [\"crate::m{index}\"]\n\n"
        ));
    }
    toml.push_str("[[architecture.only_in]]\nimports = [\"first_package_with_a_long_name\", \"second_package_with_a_long_name\"]\npaths = [");
    for index in 0..12 {
        toml.push_str(&format!("\"src/some/deep/directory/number_{index}/**\", "));
    }
    toml.push_str("]\n");
    let dir = Dir::new("long", &toml);
    let out = dir.layers(&["summary"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert_eq!(stdout.lines().count(), 4, "{stdout}");
    for line in stdout.lines() {
        assert!(line.chars().count() <= 80, "{line}");
    }
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines[1].contains(" more"), "{stdout}");
    assert!(lines[1].starts_with("  layers   a_rather_long_layer_name_0 -> "));
    assert!(lines[1].contains(", +"), "{stdout}");
    assert!(lines[2].starts_with("  only in  first_package_with_a_long_name"));
    assert!(
        lines[2].contains(", +") && lines[2].ends_with(" more"),
        "{stdout}"
    );
}

#[test]
fn summary_without_only_in_omits_line_three() {
    let dir = Dir::new(
        "noonly",
        "[[architecture.layers]]\nname = \"core\"\nmodules = [\"crate::core\"]\n",
    );
    let out = dir.layers(&["summary"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        text(&out.stdout),
        "architecture (adr-lint.toml, ADR-0056):\n  layers   core\n  detail   adr-lint layers explain [PATH]\n"
    );
}

#[test]
fn no_architecture_prints_nothing_and_exits_0() {
    for toml in ["", "[cli]\n"] {
        let dir = Dir::new("none", toml);
        for sub in ["summary", "explain"] {
            let out = dir.layers(&[sub]);
            assert_eq!(out.status.code(), Some(0), "{sub}");
            assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{sub}");
        }
    }
}

#[test]
fn an_invalid_contract_prints_nothing_on_stdout_and_exits_3() {
    let dir = Dir::new("invalid", "[[architecture.layers]]\nname = \"core\"\n");
    for sub in ["summary", "explain"] {
        let out = dir.layers(&[sub]);
        assert_eq!(out.status.code(), Some(3), "{sub}");
        assert!(out.stdout.is_empty(), "{sub}");
        assert!(text(&out.stderr).contains("adr-lint.toml:"), "{sub}");
    }
}

#[test]
fn summary_reads_the_directory_given_with_c() {
    let dir = Dir::new("dashc", THREE_LAYERS);
    let elsewhere = Dir::new("dashc-elsewhere", "");
    let out = elsewhere
        .command(&["summary", "-C", dir.0.to_str().expect("utf-8")])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("domain -> application -> adapters"));
}

#[test]
fn explain_prints_every_layer_and_only_in_entry() {
    let toml = format!(
        "{THREE_LAYERS}\n[[architecture.only_in]]\nimports = [\"serde\"]\npaths = [\"src/wire/**\"]\n\n[architecture.cycles]\ncheck = false\n"
    );
    let dir = Dir::new("explain", &toml);
    let out = dir.layers(&["explain"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    for expected in [
        "1. domain",
        "2. application",
        "3. adapters",
        "crate::domain",
        "rusqlite",
        "crate::adapters",
        "serde",
        "src/wire/**",
        "cycles   off",
    ] {
        assert!(stdout.contains(expected), "{expected}\n{stdout}");
    }
}

#[test]
fn explain_of_a_path_outside_the_tree_exits_2() {
    let dir = Dir::new("outside", THREE_LAYERS);
    for path in ["../elsewhere.rs", "/etc/hosts"] {
        let out = dir.layers(&["explain", path]);
        assert_eq!(out.status.code(), Some(2), "{path}");
        assert!(out.stdout.is_empty(), "{path}");
    }
}

#[test]
fn explain_of_a_file_names_its_layer_and_the_finding_check_prints() {
    if !real_ast_grep() {
        return;
    }
    let dir = Dir::new("file", THREE_LAYERS);
    dir.write(
        "Cargo.toml",
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    dir.write(
        "src/lib.rs",
        "pub mod adapters;\npub mod application;\npub mod domain;\n",
    );
    dir.write("src/adapters.rs", "pub fn f() {}\n");
    dir.write("src/application.rs", "pub fn a() {}\n");
    dir.write(
        "src/domain.rs",
        "use crate::adapters::f;\npub fn g() {\n    f();\n}\n",
    );
    dir.write("aval-fake", FAKE_AVAL);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.0.join("aval-fake"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir.0)
        .status()
        .expect("git runs");
    assert!(git.success());
    let out = dir.layers(&["explain", "src/domain.rs"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("layer    domain (1 of 3)\n"), "{stdout}");
    assert!(stdout.contains("may import  domain\n"), "{stdout}");
    let checked = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["check", "src/domain.rs"])
        .current_dir(&dir.0)
        .env_remove("AMONT_FILES")
        .env_remove("ADR_LINT_AST_GREP")
        .env_remove("NO_COLOR")
        .env("ADR_LINT_AVAL", dir.0.join("aval-fake"))
        .output()
        .expect("runs");
    let finding = text(&checked.stdout)
        .lines()
        .find(|line| line.starts_with("src/domain.rs:"))
        .expect("check reports a finding")
        .to_string();
    assert!(
        stdout.lines().any(|line| line == finding),
        "{finding}\n{stdout}"
    );
}

#[test]
fn explain_of_a_file_lists_only_contract_findings() {
    if !real_ast_grep() {
        return;
    }
    let dir = Dir::new("contract-only", THREE_LAYERS);
    dir.write(
        "Cargo.toml",
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    dir.write(
        "src/lib.rs",
        "pub mod adapters;\npub mod application;\npub mod domain;\n",
    );
    dir.write("src/adapters.rs", "pub fn f() {}\n");
    dir.write("src/application.rs", "pub fn a() {}\n");
    // A waiver without a reason is a finding of the waiver rule, not of the
    // contract: `check` reports it, `layers explain` must not. The marker is
    // split so this test file does not carry the waiver itself.
    dir.write(
        "src/domain.rs",
        concat!(
            "pub fn g() -> u8 {\n    // adr-lint",
            ": allow errors.never-swallowed\n    1\n}\n"
        ),
    );
    dir.write("aval-fake", FAKE_AVAL);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.0.join("aval-fake"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir.0)
        .status()
        .expect("git runs");
    assert!(git.success());
    let checked = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["check", "src/domain.rs"])
        .current_dir(&dir.0)
        .env_remove("AMONT_FILES")
        .env_remove("ADR_LINT_AST_GREP")
        .env_remove("NO_COLOR")
        .env("ADR_LINT_AVAL", dir.0.join("aval-fake"))
        .output()
        .expect("runs");
    assert_eq!(
        checked.status.code(),
        Some(1),
        "check reports the reasonless waiver\n{}",
        text(&checked.stdout)
    );
    let out = dir.layers(&["explain", "src/domain.rs"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("findings none\n"), "{stdout}");
}

#[test]
fn explain_into_a_closed_pipe_exits_0_quietly() {
    let dir = Dir::new("pipe", THREE_LAYERS);
    let mut child = dir
        .command(&["explain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("waits");
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
}
