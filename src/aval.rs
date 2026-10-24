//! Running `aval`: the one place adr-lint starts the program that says
//! what is decided.

use std::path::Path;
use std::time::Duration;

use crate::process;

const AVAL_ARGS: [&str; 7] = [
    "--json",
    "rules",
    "--level",
    "constraint",
    "--governs",
    "code",
    "--full",
];

/// Runs `aval --json rules --level constraint --governs code --full` in
/// `dir` with stdin from /dev/null and returns its standard output.
pub(crate) fn code_constraints(program: &str, dir: &Path) -> Result<String, String> {
    run(program, dir, process::DEADLINE)
}

fn run(program: &str, dir: &Path, deadline: Duration) -> Result<String, String> {
    process::run(program, &AVAL_ARGS, dir, deadline).map_err(process::Failure::into_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// The scratch directory of one fake aval, removed when the test ends,
    /// whether it passes or fails.
    #[cfg(unix)]
    struct Scratch(std::path::PathBuf);

    #[cfg(unix)]
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
        }
    }

    /// A script runs only through a shell, so these tests are unix-only.
    #[cfg(unix)]
    fn fake(name: &str, script: &str) -> (Scratch, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("adr-lint-aval-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let scratch = Scratch(dir.clone());
        let path = dir.join("aval");
        std::fs::write(&path, script).expect("fake aval");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        (scratch, path)
    }

    #[cfg(unix)]
    #[test]
    fn a_grandchild_holding_stdout_open_cannot_outlast_the_deadline() {
        let (_scratch, program) = fake("grandchild", "#!/bin/sh\nsleep 5 &\nexit 0\n");
        let started = Instant::now();
        let answer = run(
            program.to_str().expect("utf-8 path"),
            Path::new("."),
            Duration::from_millis(300),
        );
        assert!(answer.is_err_and(|why| why.contains("did not answer within 300 ms")));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }
}
