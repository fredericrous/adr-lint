//! `adr-lint learn` through the real binary, over fixture files.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Row<'a> {
    review: &'a str,
    candidate: &'a str,
    pattern_id: &'a str,
    rule: &'a str,
    ruling: &'a str,
    clause: &'a str,
    at: &'a str,
}

impl<'a> Row<'a> {
    fn new(review: &'a str, ruling: &'a str) -> Self {
        Row {
            review,
            candidate: ID,
            pattern_id: "r/p",
            rule: "r",
            ruling,
            clause: "c",
            at: "2026-10-26T12:00:00Z",
        }
    }

    fn line(&self) -> String {
        serde_json::json!({
            "format": "adr-lint.ruling/1", "review": self.review, "tool": "relais",
            "repo": "adr-lint", "tree": ID, "candidate": self.candidate,
            "rule": self.rule, "pattern_id": self.pattern_id, "pattern": "p",
            "ruling": self.ruling, "clause": self.clause, "at": self.at,
        })
        .to_string()
    }
}

fn candidate(index: usize) -> String {
    format!("{index:064x}")
}

fn fixture(name: &str, lines: &[String]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adr-lint-learn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join(name);
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");
    path
}

fn learn(args: &[&str], files: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_adr-lint"));
    command.arg("learn").args(args);
    if !files.is_empty() {
        command.arg("--from").args(files);
    }
    command.output().expect("adr-lint runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_last_record_of_a_key_wins_in_a_file() {
    let file = fixture(
        "a.jsonl",
        &[
            Row::new("rv", "violation").line(),
            Row::new("rv", "fine").line(),
        ],
    );
    let out = learn(&[], &[&file]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out), "r/p: violation 0, fine 1\n");
}

#[test]
fn the_second_file_wins_across_files() {
    let first = fixture("b1.jsonl", &[Row::new("rv", "fine").line()]);
    let second = fixture("b2.jsonl", &[Row::new("rv", "violation").line()]);
    let out = learn(&[], &[&first, &second]);
    assert_eq!(stdout(&out), "r/p: violation 1, fine 0\n");
    let reversed = learn(&[], &[&second, &first]);
    assert_eq!(stdout(&reversed), "r/p: violation 0, fine 1\n");
}

#[test]
fn since_drops_earlier_records() {
    let mut early = Row::new("early", "violation");
    early.at = "2026-10-24T23:59:59Z";
    let mut late = Row::new("late", "fine");
    late.at = "2026-10-25T00:00:00Z";
    let file = fixture("c.jsonl", &[early.line(), late.line()]);
    let out = learn(&["--since", "2026-10-25"], &[&file]);
    assert_eq!(stdout(&out), "r/p: violation 0, fine 1\n");
    let all = learn(&[], &[&file]);
    assert_eq!(stdout(&all), "r/p: violation 1, fine 1\n");
}

fn fines_with_clauses(name: &str, clauses: &[&str]) -> PathBuf {
    let lines: Vec<String> = clauses
        .iter()
        .enumerate()
        .map(|(n, clause)| {
            let id = candidate(n);
            let mut row = Row::new("rv", "fine");
            row.candidate = &id;
            row.clause = clause;
            row.line()
        })
        .collect();
    fixture(name, &lines)
}

/// The plan's behaviour 7 case: six `fine` rulings sharing one clause make
/// one proposal, followed by the six ruled lines to draw a fixture from.
#[test]
fn six_fines_sharing_a_clause_make_one_proposal_naming_the_ruled_lines() {
    let file = fines_with_clauses("d6.jsonl", &["same"; 6]);
    let out = learn(&[], &[&file]);
    let ruled: String = (0..6)
        .map(|index| format!("  ruled: tree {ID} candidate {}\n", candidate(index)))
        .collect();
    assert_eq!(
        stdout(&out),
        format!(
            "r/p: violation 0, fine 6\n  fine x6: same\nproposal: narrow r/p or clarify r body\n{ruled}"
        )
    );
    assert_eq!(stdout(&out).matches("proposal:").count(), 1);
}

#[test]
fn five_fines_sharing_a_clause_are_enough_and_four_are_not() {
    let file = fines_with_clauses("d5.jsonl", &["same"; 5]);
    assert!(stdout(&learn(&[], &[&file])).contains("proposal: narrow r/p or clarify r body\n"));
    let four = fines_with_clauses("d4.jsonl", &["same"; 4]);
    assert_eq!(stdout(&learn(&[], &[&four])), "r/p: violation 0, fine 4\n");
}

#[test]
fn five_different_clauses_make_no_proposal() {
    let file = fines_with_clauses("e.jsonl", &["a", "b", "c", "d", "e"]);
    assert_eq!(stdout(&learn(&[], &[&file])), "r/p: violation 0, fine 5\n");
}

fn rulings_of_a_rule(name: &str, fine: usize, total: usize) -> PathBuf {
    let lines: Vec<String> = (0..total)
        .map(|n| {
            let id = candidate(n);
            let mut row = Row::new("rv", if n < fine { "fine" } else { "violation" });
            row.candidate = &id;
            row.clause = &id;
            row.line()
        })
        .collect();
    fixture(name, &lines)
}

#[test]
fn a_rule_over_ninety_percent_fine_is_a_retire_candidate() {
    let file = rulings_of_a_rule("f46.jsonl", 46, 50);
    let out = stdout(&learn(&[], &[&file]));
    assert!(
        out.lines()
            .any(|line| line == "retire candidate: r (46 of 50 rulings fine, 92%)"),
        "{out}"
    );
    let edge = rulings_of_a_rule("f45.jsonl", 45, 50);
    assert!(!stdout(&learn(&[], &[&edge])).contains("retire candidate"));
}

#[test]
fn a_malformed_line_exits_3_naming_file_and_line_with_no_report() {
    let bad_format = Row::new("rv", "fine")
        .line()
        .replace("ruling/1", "ruling/9");
    for (name, bad) in [
        ("g-blank.jsonl", String::new()),
        ("g-format.jsonl", bad_format),
    ] {
        let file = fixture(name, &[Row::new("rv", "fine").line(), bad]);
        let out = learn(&[], &[&file]);
        assert_eq!(out.status.code(), Some(3), "{name}");
        assert_eq!(stdout(&out), "", "{name}");
        let message = String::from_utf8_lossy(&out.stderr);
        assert!(
            message.contains(name) && message.contains("line 2:"),
            "{message}"
        );
    }
}

#[test]
fn missing_from_is_usage_and_a_missing_file_is_3() {
    let out = learn(&[], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stdout(&out), "");
    assert_eq!(learn(&["--bogus"], &[]).status.code(), Some(2));
    let file = fixture("h.jsonl", &[Row::new("rv", "fine").line()]);
    assert_eq!(
        learn(&["--since", "yesterday"], &[&file]).status.code(),
        Some(2)
    );
    let absent = file.with_file_name("does-not-exist.jsonl");
    assert_eq!(learn(&[], &[&absent]).status.code(), Some(3));
}

#[test]
fn learn_writes_nothing() {
    let file = fixture("i.jsonl", &[Row::new("rv", "fine").line()]);
    let before = std::fs::read(&file).expect("read");
    learn(&[], &[&file]);
    assert_eq!(std::fs::read(&file).expect("read"), before);
}
