//! `adr-lint candidates` through the real binary, over a scratch git
//! repository with a base commit and a change commit, with the real ast-grep
//! named in AST_GREP_VERSION when it is on PATH.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The version of ast-grep the command demands, written once in the repository.
const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");

struct Repository(PathBuf);

impl Repository {
    /// A repository whose `base` branch holds the first commit and whose
    /// HEAD holds the change on top of it.
    fn with_change(name: &str, before: &[(&str, &str)], after: &[(&str, &str)]) -> Repository {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-candidates-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let repository = Repository(dir);
        repository.git(&["init", "-q", "-b", "main"]);
        repository.commit(before, "base");
        repository.git(&["branch", "base"]);
        repository.commit(after, "change");
        repository
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
    }

    fn output(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(&self.0)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        text(&out.stdout).trim().to_string()
    }

    fn commit(&self, files: &[(&str, &str)], message: &str) {
        for (name, content) in files {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, content).expect("fixture");
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn candidates(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .arg("candidates")
            .args(args)
            .current_dir(&self.0)
            .env_remove("ADR_LINT_AST_GREP")
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs")
    }

    fn document(&self, args: &[&str]) -> serde_json::Value {
        let out = self.candidates(args);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        serde_json::from_slice(&out.stdout).expect("stdout is JSON")
    }

    fn json(&self, args: &[&str]) -> Vec<serde_json::Value> {
        match self.document(args)["candidates"].clone() {
            serde_json::Value::Array(items) => items,
            other => panic!("not an array: {other}"),
        }
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// True when the real ast-grep is available. Locally it says why not and the
/// test returns; under CI a missing ast-grep fails.
fn real_ast_grep() -> bool {
    let wanted = format!("ast-grep {}", EXPECTED_AST_GREP.trim());
    let printed = Command::new("ast-grep").arg("--version").output();
    let found = printed
        .as_ref()
        .map(|out| text(&out.stdout).trim().to_string())
        .unwrap_or_default();
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

const BEFORE: &[(&str, &str)] = &[
    ("src/lib.rs", "fn old() {\n    let d = 1;\n}\n"),
    ("Cargo.toml", "[package]\nname = \"a\"\n\n[dependencies]\n"),
];
const AFTER: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "fn old() {\n    let d = 1;\n}\n\nfn fresh() {\n    let x = 2;\n    panic!(\"no\");\n}\n",
    ),
    (
        "Cargo.toml",
        "[package]\nname = \"a\"\n\n[dependencies]\nserde_json = \"1\"\n",
    ),
];

fn rules_of(items: &[serde_json::Value]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| item["rule"].as_str())
        .collect()
}

#[test]
fn a_candidate_on_an_added_line_is_listed_with_a_hex_id_and_an_old_one_is_not() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("added", BEFORE, AFTER);
    let items = repository.json(&["--base", "base", "--json"]);
    let names: Vec<&serde_json::Value> = items
        .iter()
        .filter(|item| item["rule"] == "names.reveal-intent")
        .collect();
    assert_eq!(names.len(), 1, "{items:?}");
    assert_eq!(names[0]["line"], 6);
    assert_eq!(names[0]["text"], "let x = 2;");
    let id = names[0]["id"].as_str().expect("id");
    assert_eq!(id.len(), 64);
    assert!(
        id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "{id}"
    );
    assert!(items.iter().all(|item| item["line"] != 2), "{items:?}");
    let panics = rules_of(&items)
        .iter()
        .filter(|rule| **rule == "failures.expected-vs-defect")
        .count();
    assert_eq!(panics, 1);
}

#[test]
fn the_json_document_is_versioned_with_base_head_and_an_overflow_object() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("document", BEFORE, AFTER);
    let document = repository.document(&["--base", "base", "--json"]);
    assert_eq!(document["format"], "adr-lint.candidates/1");
    assert_eq!(document["base"].as_str().map(str::len), Some(40));
    assert_eq!(document["head"].as_str().map(str::len), Some(40));
    assert_ne!(document["base"], document["head"]);
    assert_eq!(document["overflow"], serde_json::json!({}));
}

/// The candidates `errors.never-swallowed` marks in a changed file, by pattern id.
fn swallow_patterns(name: &str, file: &str, body: &str) -> Vec<String> {
    let repository = Repository::with_change(
        name,
        &[("README.md", "base\n")],
        &[("README.md", "changed\n"), (file, body)],
    );
    repository
        .json(&["--base", "base", "--json"])
        .iter()
        .filter(|item| item["rule"] == "errors.never-swallowed")
        .map(|item| item["pattern_id"].as_str().expect("pattern id").to_string())
        .collect()
}

#[test]
fn a_discard_is_a_candidate_only_without_a_same_line_comment() {
    if !real_ast_grep() {
        return;
    }
    // (file, uncommented body, commented body, the pattern it marks)
    let cases = [
        (
            "a.rs",
            "fn f(tx: Tx) {\n    let _ = tx.send(1);\n}\n",
            "fn f(tx: Tx) {\n    let _ = tx.send(1); // receiver closed at shutdown\n}\n",
            "errors.never-swallowed/rust-let-discard",
        ),
        (
            "b.rs",
            "fn f(tx: Tx) {\n    tx.send(1).ok();\n}\n",
            "fn f(tx: Tx) {\n    tx.send(1).ok(); // receiver closed at shutdown\n}\n",
            "errors.never-swallowed/rust-ok-discard",
        ),
        (
            "c.go",
            "package c\n\nfunc f() {\n\t_ = g()\n}\n",
            "package c\n\nfunc f() {\n\t_ = g() // best effort cleanup\n}\n",
            "errors.never-swallowed/go-blank-discard",
        ),
        // Python: tree-sitter takes the trailing comment into the clause, so
        // the rule's own pass-only match is what drops the commented form,
        // not the shared exemption (that is covered by a unit test).
        (
            "d.py",
            "def f():\n    try:\n        g()\n    except ValueError:\n        pass\n",
            "def f():\n    try:\n        g()\n    except ValueError:\n        pass  # the cache is optional\n",
            "errors.never-swallowed/python-except-pass",
        ),
    ];
    for (file, bare, excused, pattern) in cases {
        let marked = swallow_patterns(&format!("swallow-{file}"), file, bare);
        assert_eq!(marked, [pattern], "{file} without a comment");
        let quiet = swallow_patterns(&format!("excused-{file}"), file, excused);
        assert!(
            quiet.is_empty(),
            "{file} with a same-line comment: {quiet:?}"
        );
    }
}

#[test]
fn a_one_line_except_pass_is_a_candidate_until_a_comment_follows_it() {
    if !real_ast_grep() {
        return;
    }
    // The one-line form. tree-sitter-python takes the trailing comment into
    // the clause, so the rule's own `pass`-only match stops matching: the
    // result is the same whether the rule or the shared exemption drops it,
    // and the `#` handling itself is covered by a unit test in src/check.rs.
    let bare = "def f():\n    try:\n        g()\n    except ValueError: pass\n";
    let excused =
        "def f():\n    try:\n        g()\n    except ValueError: pass  # the cache is optional\n";
    assert_eq!(
        swallow_patterns("py-inline-bare", "e.py", bare),
        ["errors.never-swallowed/python-except-pass"]
    );
    assert!(swallow_patterns("py-inline-excused", "e.py", excused).is_empty());
}

#[test]
fn a_rule_without_comment_exempt_still_lists_a_match_that_has_a_comment() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change(
        "no-exempt",
        &[("README.md", "base\n")],
        &[("a.rs", "fn f() {\n    let data = 1; // x\n}\n")],
    );
    let items = repository.json(&["--base", "base", "--json"]);
    assert!(
        items
            .iter()
            .any(|item| item["rule"] == "names.reveal-intent"),
        "{items:?}"
    );
}

#[test]
fn a_changed_file_that_is_not_utf8_does_not_fail_the_command() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change(
        "latin1",
        &[("README.md", "base\n")],
        &[("README.md", "changed\n")],
    );
    // Latin-1 e-acute (0xE9) is not UTF-8; the file holds nothing to flag.
    std::fs::write(
        repository.0.join("legacy.go"),
        b"package legacy\n\n// caf\xE9\nfunc Serve() {}\n",
    )
    .expect("fixture");
    repository.git(&["add", "-A"]);
    repository.git(&["commit", "-q", "-m", "legacy"]);
    assert!(repository.json(&["--base", "base", "--json"]).is_empty());
}

#[test]
fn json_items_carry_the_documented_fields_and_the_text_form_groups_by_rule() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("shape", BEFORE, AFTER);
    let items = repository.json(&["--base=base", "--json", "--rule", "names.reveal-intent"]);
    assert_eq!(items.len(), 1);
    let mut keys: Vec<&str> = items[0]
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "col",
            "id",
            "line",
            "path",
            "pattern",
            "pattern_id",
            "rule",
            "text"
        ]
    );
    assert_eq!(items[0]["path"], "src/lib.rs");
    let id = items[0]["id"].as_str().expect("id").to_string();
    let out = repository.candidates(&["--base", "base", "--rule", "names.reveal-intent"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout.starts_with("rule names.reveal-intent (1)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "{} src/lib.rs:6:9: names.reveal-intent: ",
            &id[..12]
        )),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("aval rule names.reveal-intent\n"),
        "{stdout}"
    );
}

#[test]
fn max_keeps_the_first_and_counts_the_rest_per_rule() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("max", BEFORE, AFTER);
    let all = repository.json(&["--base", "base", "--json"]);
    assert!(all.len() >= 3, "{all:?}");
    let document = repository.document(&["--base", "base", "--json", "--max", "1"]);
    let cut = document["candidates"].as_array().expect("candidates");
    assert_eq!(cut.len(), 1, "{cut:?}");
    assert_eq!(cut[0], all[0]);
    let overflow = document["overflow"].as_object().expect("overflow object");
    let dropped: u64 = overflow
        .values()
        .filter_map(serde_json::Value::as_u64)
        .sum();
    assert_eq!(dropped as usize, all.len() - 1);
    let out = repository.candidates(&["--base", "base", "--max", "1"]);
    assert!(
        text(&out.stdout).contains(" more in "),
        "{}",
        text(&out.stdout)
    );
}

#[test]
fn rule_selects_one_rule_and_an_unknown_rule_is_a_usage_error() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("rule", BEFORE, AFTER);
    let items = repository.json(&[
        "--base",
        "base",
        "--json",
        "--rule",
        "failures.expected-vs-defect",
    ]);
    assert_eq!(rules_of(&items), vec!["failures.expected-vs-defect"]);
    let out = repository.candidates(&["--base", "base", "--rule", "no.such"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn a_new_cargo_dependency_line_is_a_dependency_candidate() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change("dependency", BEFORE, AFTER);
    let items = repository.json(&[
        "--base",
        "base",
        "--json",
        "--rule",
        "change.a-new-dependency-is-owned",
    ]);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["path"], "Cargo.toml");
    assert_eq!(items[0]["line"], 5);
    assert_eq!(items[0]["text"], "serde_json = \"1\"");
}

#[test]
fn no_base_is_a_usage_error_and_an_unknown_base_could_not_run() {
    let repository = Repository::with_change("errors", BEFORE, AFTER);
    let missing = repository.candidates(&["--json"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    assert!(
        text(&missing.stderr).contains("--base"),
        "{}",
        text(&missing.stderr)
    );
    let unknown = repository.candidates(&["--base", "no-such-ref"]);
    assert_eq!(unknown.status.code(), Some(3));
    assert!(unknown.stdout.is_empty());
    assert_eq!(text(&unknown.stderr).lines().count(), 1);
}

#[test]
fn a_directory_outside_a_repository_could_not_run() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_adr-lint"))
        .args(["candidates", "--base", "main"])
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap_or(dir))
        .output()
        .expect("adr-lint runs");
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
}

#[test]
fn a_changed_file_with_uncommitted_edits_is_refused() {
    let repository = Repository::with_change("dirty", BEFORE, AFTER);
    let path = repository.0.join("src/lib.rs");
    let committed = std::fs::read_to_string(&path).expect("fixture");
    std::fs::write(&path, format!("// moved\n{committed}")).expect("edit");
    let out = repository.candidates(&["--base", "base", "--json"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("src/lib.rs has uncommitted changes"));
}

#[test]
fn an_assignment_added_inside_an_existing_function_is_a_candidate_on_its_own_line() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change(
        "inside",
        &[("a.go", "package a\n\nfunc f(xs []int) {\n}\n")],
        &[("a.go", "package a\n\nfunc f(xs []int) {\n\txs[0] = 1\n}\n")],
    );
    let items = repository.json(&[
        "--base",
        "base",
        "--json",
        "--rule",
        "values.immutable-by-default",
    ]);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["line"], 4);
    assert_eq!(items[0]["text"], "xs[0] = 1");
}

#[test]
fn a_version_change_is_not_a_new_dependency_but_a_new_name_is() {
    if !real_ast_grep() {
        return;
    }
    let before = [(
        "Cargo.toml",
        "[package]\nname = \"a\"\n\n[dependencies]\nserde = \"1\"\n",
    )];
    let after = [(
        "Cargo.toml",
        "[package]\nname = \"a\"\n\n[dependencies]\nserde = \"2\"\nserde_json = \"1\"\n",
    )];
    let repository = Repository::with_change("bump", &before, &after);
    let items = repository.json(&[
        "--base",
        "base",
        "--json",
        "--rule",
        "change.a-new-dependency-is-owned",
    ]);
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["text"], "serde_json = \"1\"");
}

#[test]
fn head_lists_the_candidates_of_a_commit_that_is_not_checked_out() {
    if !real_ast_grep() {
        return;
    }
    let before = &[("src/lib.rs", "fn old() {\n    let d = 1;\n}\n")];
    let after = &[(
        "src/lib.rs",
        "fn old() {\n    let d = 1;\n}\n\nfn fresh() {\n    let x = 2;\n}\n",
    )];
    let repository = Repository::with_change("head", before, after);
    let checked_out = repository.json(&["--base", "base", "--json"]);
    let tree = repository.output(&["rev-parse", "HEAD^{tree}"]);
    let parent = repository.output(&["rev-parse", "base"]);
    let commit = repository.output(&["commit-tree", &tree, "-p", &parent, "-m", "detached"]);
    std::fs::write(
        repository.0.join("src/lib.rs"),
        "// unrelated\nfn old() {\n    let d = 1;\n}\n",
    )
    .expect("edit");
    let document = repository.document(&["--base", "base", "--head", &commit, "--json"]);
    assert_eq!(document["head"], commit.as_str());
    let items = document["candidates"].as_array().expect("array");
    let names: Vec<&serde_json::Value> = items
        .iter()
        .filter(|item| item["rule"] == "names.reveal-intent")
        .collect();
    let expected: Vec<&serde_json::Value> = checked_out
        .iter()
        .filter(|item| item["rule"] == "names.reveal-intent")
        .collect();
    assert_eq!(names.len(), 1, "{items:?}");
    assert_eq!(names[0]["id"], expected[0]["id"]);
    assert_eq!(names[0]["line"], 6);
}

#[test]
fn head_with_an_unknown_rev_could_not_run_and_without_a_value_is_a_usage_error() {
    let repository = Repository::with_change("head-errors", BEFORE, AFTER);
    let unknown = repository.candidates(&["--base", "base", "--head", "no-such-rev"]);
    assert_eq!(unknown.status.code(), Some(3), "{}", text(&unknown.stderr));
    assert!(unknown.stdout.is_empty());
    assert!(text(&unknown.stderr).contains("no-such-rev"));
    let valueless = repository.candidates(&["--base", "base", "--head"]);
    assert_eq!(valueless.status.code(), Some(2));
    let dashed = repository.candidates(&["--base", "base", "--head", "-x"]);
    assert_eq!(dashed.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn a_changed_symlink_to_a_source_file_yields_no_candidate_under_the_link() {
    if !real_ast_grep() {
        return;
    }
    let repository = Repository::with_change(
        "symlink",
        &[("README.md", "base\n")],
        // One line, so a followed link would put the candidate at
        // `src/link.rs:1`, the one line the diff adds to the link.
        &[("src/lib.rs", "fn fresh() { let d = 1; }\n")],
    );
    std::os::unix::fs::symlink("lib.rs", repository.0.join("src/link.rs")).expect("symlink");
    repository.git(&["add", "-A"]);
    repository.git(&["commit", "-q", "--amend", "--no-edit"]);
    let head = repository.output(&["rev-parse", "HEAD"]);
    let checked_out = repository.json(&["--base", "base", "--json"]);
    let by_rev = repository.json(&["--base", "base", "--head", &head, "--json"]);
    for items in [checked_out, by_rev] {
        let paths: Vec<&str> = items
            .iter()
            .filter_map(|item| item["path"].as_str())
            .collect();
        assert!(paths.contains(&"src/lib.rs"), "{items:?}");
        assert!(!paths.contains(&"src/link.rs"), "{items:?}");
    }
}

#[test]
fn a_generated_file_yields_no_candidate() {
    if !real_ast_grep() {
        return;
    }
    let receiver = "package a\n\nfunc (m *Mock) X() {}\n";
    let header = "// Code generated by MockGen. DO NOT EDIT.\n";
    let repository = Repository::with_change(
        "generated",
        &[("old.go", "package old\n")],
        &[
            ("old.go", "package old\n"),
            ("plain.go", receiver),
            ("mock.go", &format!("{header}{receiver}")),
        ],
    );
    let items = repository.json(&["--base", "base", "--json", "--rule", "names.reveal-intent"]);
    let paths: Vec<&str> = items
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"plain.go"), "{items:?}");
    assert!(!paths.contains(&"mock.go"), "{items:?}");
}
