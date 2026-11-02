//! The files that are nodes of the TypeScript graph.

use std::collections::BTreeSet;
use std::path::Path;

use crate::coverage::Failure;
use crate::generated::is_generated;
use crate::process;

use super::super::scan::clean;

const EXTENSIONS: [&str; 4] = [".ts", ".tsx", ".mts", ".cts"];

/// Tracked and untracked-not-ignored TypeScript files under `root` (every
/// file outside dot directories and `node_modules` when `root` is not a git
/// work tree), plus `named` files that exist; without generated files and
/// test files. Sorted.
pub(super) fn nodes(root: &Path, named: &[String]) -> Result<Vec<String>, Failure> {
    let listed = match process::run(
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
        Ok(printed) => printed
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(clean)
            .collect(),
        Err(_) => walked(root)?, // not a git work tree: the walk lists the same files
    };
    let mut found = BTreeSet::new();
    for path in listed
        .into_iter()
        .chain(named.iter().map(|path| clean(path)))
    {
        if is_candidate(&path) && root.join(&path).is_file() && !generated(root, &path) {
            found.insert(path);
        }
    }
    Ok(found.into_iter().collect())
}

fn is_candidate(path: &str) -> bool {
    EXTENSIONS.iter().any(|extension| path.ends_with(extension))
        && !path.split('/').any(|part| part == "node_modules")
        && !is_test_file(path)
}

/// `x.test.ts` and `x.spec.tsx`: a `test` or `spec` segment between the name
/// and the extension.
fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() > 2
        && parts[1..parts.len() - 1]
            .iter()
            .any(|part| *part == "test" || *part == "spec")
}

fn generated(root: &Path, path: &str) -> bool {
    std::fs::read_to_string(root.join(path)).is_ok_and(|text| is_generated(&text))
}

fn walked(root: &Path) -> Result<Vec<String>, Failure> {
    let mut files = Vec::new();
    walk(root, "", &mut files)?;
    Ok(files)
}

fn walk(root: &Path, relative: &str, files: &mut Vec<String>) -> Result<(), Failure> {
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
            walk(root, &format!("{path}/"), files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_files_are_recognised_by_a_segment() {
        assert!(is_test_file("a/b.test.ts"));
        assert!(is_test_file("b.spec.tsx"));
        assert!(is_test_file("b.unit.test.mts"));
        assert!(!is_test_file("test.ts"));
        assert!(!is_test_file("a/latest.ts"));
        assert!(!is_candidate("node_modules/x/index.ts"));
        assert!(!is_candidate("a.js"));
        assert!(is_candidate("a/b.cts"));
    }
}
