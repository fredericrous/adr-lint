//! The file listing the TypeScript, Python and Go graphs share.

use std::path::Path;

use crate::coverage::Failure;
use crate::generated::is_generated;
use crate::process;

use super::scan::clean;

/// Tracked and untracked-not-ignored files under `root`, relative and
/// `/`-separated; every file outside dot directories, `node_modules` and
/// `target` when `root` is not a git work tree.
pub(super) fn listed(root: &Path) -> Result<Vec<String>, Failure> {
    match process::run(
        "git",
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        root,
        process::DEADLINE,
    ) {
        Ok(printed) => Ok(printed
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(clean)
            .collect()),
        Err(_) => walked(root), // not a git work tree: the walk lists the same files
    }
}

pub(super) fn generated(root: &Path, path: &str) -> bool {
    std::fs::read_to_string(root.join(path)).is_ok_and(|text| is_generated(&text))
}

fn walked(root: &Path) -> Result<Vec<String>, Failure> {
    walk(root, "")
}

fn walk(root: &Path, relative: &str) -> Result<Vec<String>, Failure> {
    let mut files = Vec::new();
    let directory = root.join(relative);
    let unreadable = |cause: std::io::Error| {
        Failure::CouldNotRun(format!("cannot read {}: {cause}", directory.display()))
    };
    for entry in std::fs::read_dir(&directory).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = format!("{relative}{name}");
        if entry.file_type().map_err(unreadable)?.is_dir() {
            files.extend(walk(root, &format!("{path}/"))?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}
