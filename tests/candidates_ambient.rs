//! `adr-lint candidates` for effects.no-ambient-access through the real
//! binary, over scratch git repositories, with the real ast-grep named in
//! AST_GREP_VERSION when it is on PATH.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const EXPECTED_AST_GREP: &str = include_str!("../AST_GREP_VERSION");
const RULE: &str = "effects.no-ambient-access";
const AMBIENT: &str =
    "[architecture.ambient]\nmodules = [\"crate::clock\", \"pkg.clock\"]\npaths = [\"clock\"]\n";
const NO_AMBIENT: &str = "[architecture]\n";

type Fixture = (&'static str, &'static str);

struct Repository(PathBuf);

impl Repository {
    fn new(name: &str, before: &[Fixture], after: &[Fixture]) -> Repository {
        let dir =
            std::env::temp_dir().join(format!("adr-lint-ambient-{name}-{}", std::process::id()));
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
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(&self, files: &[Fixture], message: &str) {
        for (name, content) in files {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, content).expect("fixture");
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_adr-lint"))
            .arg("candidates")
            .args(["--base", "base", "--json", "--rule", RULE])
            .args(args)
            .current_dir(&self.0)
            .env_remove("ADR_LINT_AST_GREP")
            .stdin(Stdio::null())
            .output()
            .expect("adr-lint runs")
    }

    fn count(&self, args: &[&str]) -> usize {
        let out = self.run(args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let document: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
        document["candidates"].as_array().expect("array").len()
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
    }
}

fn real_ast_grep() -> bool {
    let wanted = format!("ast-grep {}", EXPECTED_AST_GREP.trim());
    let found = Command::new("ast-grep")
        .arg("--version")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
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

struct Language {
    name: &'static str,
    outside: Fixture,
    inside: Fixture,
    not_ambient: Fixture,
    scaffold: &'static [Fixture],
}

const LANGUAGES: [Language; 3] = [
    Language {
        name: "rust",
        outside: (
            "src/ledger.rs",
            "pub fn f() {\n    let t = std::time::SystemTime::now();\n}\n",
        ),
        inside: (
            "src/clock.rs",
            "pub fn f() {\n    let t = std::time::SystemTime::now();\n}\n",
        ),
        not_ambient: (
            "src/ledger.rs",
            "pub fn f() {\n    let t = std::time::Instant::now();\n}\n",
        ),
        scaffold: &[("src/lib.rs", "")],
    },
    Language {
        name: "python",
        outside: (
            "src/pkg/ledger.py",
            "import time\n\n\ndef f():\n    return time.time()\n",
        ),
        inside: (
            "src/pkg/clock.py",
            "import time\n\n\ndef f():\n    return time.time()\n",
        ),
        not_ambient: (
            "src/pkg/ledger.py",
            "import time\n\n\ndef f():\n    return time.perf_counter()\n",
        ),
        scaffold: &[
            ("pyproject.toml", "[project]\nname = \"pkg\"\n"),
            ("src/pkg/__init__.py", ""),
        ],
    },
    Language {
        name: "go",
        outside: (
            "ledger/ledger.go",
            "package ledger\n\nimport \"time\"\n\nfunc F() time.Time {\n\treturn time.Now()\n}\n",
        ),
        inside: (
            "clock/clock.go",
            "package clock\n\nimport \"time\"\n\nfunc F() time.Time {\n\treturn time.Now()\n}\n",
        ),
        not_ambient: (
            "ledger/ledger.go",
            "package ledger\n\nimport \"math/rand\"\n\nfunc F() *rand.Rand {\n\treturn rand.New(rand.NewSource(1))\n}\n",
        ),
        scaffold: &[("go.mod", "module example.invalid/m\n")],
    },
];

fn repository(
    language: &Language,
    label: &str,
    contract: Option<&'static str>,
    change: Fixture,
) -> Repository {
    let mut base: Vec<Fixture> = language.scaffold.to_vec();
    if let Some(text) = contract {
        base.push(("adr-lint.toml", text));
    }
    Repository::new(&format!("{}-{label}", language.name), &base, &[change])
}

#[test]
fn an_ambient_read_outside_the_table_is_one_candidate() {
    if !real_ast_grep() {
        return;
    }
    for language in &LANGUAGES {
        let repo = repository(language, "out", Some(AMBIENT), language.outside);
        assert_eq!(repo.count(&[]), 1, "{}", language.name);
        assert_eq!(repo.count(&["--head", "HEAD"]), 1, "{} head", language.name);
    }
}

#[test]
fn the_same_read_inside_the_table_is_none() {
    if !real_ast_grep() {
        return;
    }
    for language in &LANGUAGES {
        let repo = repository(language, "in", Some(AMBIENT), language.inside);
        assert_eq!(repo.count(&[]), 0, "{}", language.name);
        assert_eq!(repo.count(&["--head", "HEAD"]), 0, "{} head", language.name);
    }
}

#[test]
fn a_read_that_is_not_ambient_is_none() {
    if !real_ast_grep() {
        return;
    }
    for language in &LANGUAGES {
        let repo = repository(language, "mono", Some(AMBIENT), language.not_ambient);
        assert_eq!(repo.count(&[]), 0, "{}", language.name);
    }
}

#[test]
fn a_repository_without_the_table_is_none() {
    if !real_ast_grep() {
        return;
    }
    for language in &LANGUAGES {
        let repo = repository(language, "bare", Some(NO_AMBIENT), language.outside);
        assert_eq!(
            repo.count(&["--head", "HEAD"]),
            0,
            "{} table",
            language.name
        );
        let repo = repository(language, "none", None, language.outside);
        assert_eq!(repo.count(&["--head", "HEAD"]), 0, "{} file", language.name);
    }
}

#[test]
fn head_reads_the_committed_contract_not_the_working_tree() {
    if !real_ast_grep() {
        return;
    }
    let language = &LANGUAGES[0];
    let repo = repository(language, "dirty", Some(AMBIENT), language.outside);
    std::fs::write(repo.0.join("adr-lint.toml"), NO_AMBIENT).expect("dirty contract");
    assert_eq!(repo.count(&["--head", "HEAD"]), 1);
}

#[test]
fn a_commit_without_adr_lint_toml_has_no_contract_even_when_the_tree_has_one() {
    if !real_ast_grep() {
        return;
    }
    let language = &LANGUAGES[0];
    let repo = repository(language, "absent", None, language.outside);
    std::fs::write(repo.0.join("adr-lint.toml"), AMBIENT).expect("untracked contract");
    assert_eq!(repo.count(&["--head", "HEAD"]), 0);
}
