//! The old config file name (`adr` + `-lint.toml`) is no longer read: a
//! repository with only that file has no contract and no warning.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Removes the scratch directory when the test ends, pass or panic.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0); // cleanup only: a leftover temp dir fails no test
    }
}

#[test]
fn only_the_old_file_is_ignored_with_no_warning() {
    // The old name, in two pieces so the rename sed leaves it alone.
    let old = concat!("adr", "-lint.toml");
    let dir = std::env::temp_dir().join(format!("greenspotter-old-config-{}", std::process::id()));
    let _guard = Scratch(dir.clone());
    std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(
        dir.join(old),
        "[[architecture.layers]]\nname = \"domain\"\nmodules = [\"crate::domain\"]\n\n[[architecture.layers]]\nname = \"domain\"\nmodules = [\"crate::other\"]\n",
    )
    .expect("write old config");
    let out = Command::new(env!("CARGO_BIN_EXE_greenspotter"))
        .args(["layers", "summary"])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("run greenspotter");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("warning"), "{stderr}");
    assert!(!stderr.contains(old), "{stderr}");
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(out.status.code(), Some(0), "{stderr}");
}
