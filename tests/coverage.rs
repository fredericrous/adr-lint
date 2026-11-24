//! `greenspotter coverage` through the real binary, with a fake greentopo.

use std::process::{Command, Output};

fn greenspotter(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_greenspotter"))
        .args(args)
        .output()
        .expect("greenspotter runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_missing_greentopo_exits_3() {
    let out = Command::new(env!("CARGO_BIN_EXE_greenspotter"))
        .args(["coverage"])
        .env("GREENSPOTTER_GREENTOPO", "/nonexistent/greentopo")
        .output()
        .expect("greenspotter runs");
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn usage_errors_exit_2_and_help_wins() {
    assert_eq!(
        greenspotter(&["coverage", "--bogus"]).status.code(),
        Some(2)
    );
    assert_eq!(greenspotter(&["coverage", "stray"]).status.code(), Some(2));
    let help = greenspotter(&["coverage", "--bogus", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(stdout(&help).starts_with("usage: greenspotter coverage"));
}

// The fake greentopo is a unix sh script; these tests need it.
#[cfg(unix)]
mod with_fake_greentopo {
    use super::{stdout, Command, Output};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    const GOOD_RULES: &str = r#"{"rules":[
 {"id":"a.exact","active":true,"body":"x\n\n**Checked by:** exact (fmt), judge."},
 {"id":"b.judged","active":true,"body":"**Checked by:** judge.\n**Reviewed by:** unix."},
 {"id":"c.off","active":false,"body":"**Checked by:** exact (nothing)."}]}"#;

    const GOOD_MAP: &str = "[rules.\"a.exact\"]\nexact = \"fmt\"\nmisses = \"all else\"\n";

    /// A scratch directory with a fake greentopo printing a file and exiting with
    /// a code, and a map file.
    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str, printed: &str, exit_code: i32, map: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "greenspotter-coverage-{name}-{}",
                std::process::id()
            ));
            fs::create_dir_all(dir.join("map")).expect("mkdir");
            fs::write(dir.join("out.json"), printed).expect("write json");
            fs::write(dir.join("map/rules.toml"), map).expect("write map");
            let script = dir.join("greentopo");
            fs::write(
                &script,
                format!("#!/bin/sh\ncat out.json\nexit {exit_code}\n"),
            )
            .expect("write script");
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
            Fixture { dir }
        }

        fn run(&self, extra: &[&str]) -> Output {
            let mut args = vec!["coverage", "-C", self.dir.to_str().expect("utf8")];
            args.extend_from_slice(extra);
            Command::new(env!("CARGO_BIN_EXE_greenspotter"))
                .args(&args)
                .env("GREENSPOTTER_GREENTOPO", self.dir.join("greentopo"))
                .output()
                .expect("greenspotter runs")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Best-effort cleanup of a temp directory; a leftover is harmless.
            fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[test]
    fn all_good_exits_0() {
        let fixture = Fixture::new("good", GOOD_RULES, 0, GOOD_MAP);
        let out = fixture.run(&[]);
        assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn a_constraint_with_no_line_exits_1_naming_it() {
        let rules = r#"{"rules":[{"id":"d.silent","active":true,"body":"prose"}]}"#;
        let fixture = Fixture::new("noline", rules, 0, "");
        let out = fixture.run(&[]);
        assert_eq!(out.status.code(), Some(1));
        assert!(stdout(&out).starts_with("d.silent: "), "{}", stdout(&out));
    }

    #[test]
    fn an_exact_class_with_no_mechanism_exits_1() {
        let fixture = Fixture::new("nomech", GOOD_RULES, 0, "");
        let out = fixture.run(&[]);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            stdout(&out),
            "a.exact: exact class has no mechanism in the map\n"
        );
    }

    #[test]
    fn a_map_id_that_is_not_an_active_rule_exits_1_and_json_lists_it() {
        let map = format!("{GOOD_MAP}[rules.\"c.off\"]\nexact = \"x\"\n");
        let fixture = Fixture::new("stale", GOOD_RULES, 0, &map);
        let out = fixture.run(&["--json"]);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            stdout(&out).trim(),
            r#"[{"problem":"in the map but not an active rule","rule":"c.off"}]"#
        );
    }

    #[test]
    fn greentopo_failing_exits_3_with_empty_stdout() {
        let fixture = Fixture::new("failing", "", 1, GOOD_MAP);
        let out = fixture.run(&[]);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert_eq!(String::from_utf8_lossy(&out.stderr).lines().count(), 1);
    }

    #[test]
    fn greentopo_printing_no_json_exits_3() {
        let fixture = Fixture::new("notjson", "not json", 0, GOOD_MAP);
        let out = fixture.run(&[]);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn an_unreadable_map_exits_3() {
        let fixture = Fixture::new("nomap", GOOD_RULES, 0, GOOD_MAP);
        let out = fixture.run(&["--map", "/nonexistent/rules.toml"]);
        assert_eq!(out.status.code(), Some(3));
    }
}
