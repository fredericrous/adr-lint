//! The Python modules of a repository: the uv workspace members (and the
//! root project when it holds a package), each member's `src/` or project
//! directory as a root, and the `.py` files under its top-level packages.

use std::collections::BTreeSet;
use std::path::Path;

use adr_lint::contract::glob_matches;

use crate::coverage::Failure;

use super::super::files::{generated, listed};
use super::super::scan::clean;

/// A Python module: its dotted name and the file that holds it. A package's
/// `__init__.py` stands for the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Module {
    pub(super) name: String,
    pub(super) file: String,
    pub(super) package: bool,
}

/// The modules under the roots of the repository under `root`, plus the
/// `named` files that exist; sorted by name.
pub(super) fn modules(root: &Path, named: &[String]) -> Result<Vec<Module>, Failure> {
    let mut all: BTreeSet<String> = listed(root)?.into_iter().collect();
    all.extend(named.iter().map(|path| clean(path)));
    let existing: Vec<String> = all
        .into_iter()
        .filter(|path| root.join(path).is_file())
        .collect();
    modules_among(
        existing,
        &|path| {
            std::fs::read_to_string(root.join(path))
                .map_err(|cause| Failure::CouldNotRun(format!("cannot read {path}: {cause}")))
        },
        &|path| !generated(root, path),
    )
}

/// The modules among `existing` files, `read` giving a `pyproject.toml`'s
/// text and `keep` dropping the files that are no modules (generated ones).
pub(super) fn modules_among(
    existing: Vec<String>,
    read: &dyn Fn(&str) -> Result<String, Failure>,
    keep: &dyn Fn(&str) -> bool,
) -> Result<Vec<Module>, Failure> {
    let mut found = Vec::new();
    for directory in project_directories(read, &existing)? {
        let under_src = format!("{directory}src/");
        let source_directory = if top_level_packages(&existing, &under_src).is_empty() {
            directory.clone()
        } else {
            under_src
        };
        for package in top_level_packages(&existing, &source_directory) {
            for path in &existing {
                if let Some(module) = module_of(path, &source_directory, &package) {
                    if keep(path) {
                        found.push(module);
                    }
                }
            }
        }
    }
    found.sort_by(|first, second| first.name.cmp(&second.name));
    found.dedup_by(|second, first| first.name == second.name);
    Ok(found)
}

/// The project directories (`""` or ending in `/`): the root project when it
/// has a `pyproject.toml`, and the workspace members it names.
fn project_directories(
    read: &dyn Fn(&str) -> Result<String, Failure>,
    existing: &[String],
) -> Result<Vec<String>, Failure> {
    let manifests: Vec<&String> = existing
        .iter()
        .filter(|path| path.as_str() == "pyproject.toml" || path.ends_with("/pyproject.toml"))
        .collect();
    let mut directories = Vec::new();
    if manifests
        .iter()
        .any(|path| path.as_str() == "pyproject.toml")
    {
        directories.push(String::new());
        let text = read("pyproject.toml")?;
        let (members, excluded) = workspace_globs(&text);
        for manifest in manifests {
            let Some(directory) = manifest.strip_suffix("pyproject.toml") else {
                continue;
            };
            let trimmed = directory.trim_end_matches('/');
            if !trimmed.is_empty()
                && members.iter().any(|glob| glob_matches(glob, trimmed))
                && !excluded.iter().any(|glob| glob_matches(glob, trimmed))
            {
                directories.push(directory.to_string());
            }
        }
    }
    Ok(directories)
}

/// `[tool.uv.workspace] members` and `exclude`, as written; empty for a
/// pyproject.toml that is not valid TOML or declares no workspace.
fn workspace_globs(text: &str) -> (Vec<String>, Vec<String>) {
    let Ok(table) = text.parse::<toml::Table>() else {
        return (Vec::new(), Vec::new());
    };
    let workspace = table
        .get("tool")
        .and_then(|tool| tool.get("uv"))
        .and_then(|uv| uv.get("workspace"));
    let strings = |key: &str| -> Vec<String> {
        workspace
            .and_then(|found| found.get(key))
            .and_then(toml::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(toml::Value::as_str)
                    .map(|glob| {
                        glob.trim_start_matches("./")
                            .trim_end_matches('/')
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    (strings("members"), strings("exclude"))
}

/// The directories directly under `directory` that hold an `__init__.py`.
fn top_level_packages(existing: &[String], directory: &str) -> BTreeSet<String> {
    existing
        .iter()
        .filter_map(|path| {
            let rest = path.strip_prefix(directory)?;
            let (package, tail) = rest.split_once('/')?;
            (tail == "__init__.py").then(|| package.to_string())
        })
        .collect()
}

fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/').any(|part| part == "tests")
        || name == "conftest.py"
        || (name.starts_with("test_") && name.ends_with(".py"))
        || name.ends_with("_test.py")
}

fn module_of(path: &str, source_directory: &str, package: &str) -> Option<Module> {
    let relative = path.strip_prefix(source_directory)?;
    let inside = relative.strip_prefix(package)?.strip_prefix('/')?;
    let stem = inside.strip_suffix(".py")?;
    if is_test_file(relative) || relative.split('/').any(|part| part == "node_modules") {
        return None;
    }
    let mut parts: Vec<&str> = stem.split('/').collect();
    let package_init = parts.last() == Some(&"__init__");
    if package_init {
        parts.pop();
    }
    let mut name = package.to_string();
    for part in parts {
        name.push('.');
        name.push_str(part);
    }
    Some(Module {
        name,
        file: path.to_string(),
        package: package_init,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_init_file_stands_for_its_package() {
        let module = module_of("src/pkg/sub/__init__.py", "src/", "pkg").expect("module");
        assert_eq!((module.name.as_str(), module.package), ("pkg.sub", true));
        let module = module_of("src/pkg/__init__.py", "src/", "pkg").expect("module");
        assert_eq!(module.name, "pkg");
        assert_eq!(
            module_of("src/pkg/a.py", "src/", "pkg")
                .expect("module")
                .name,
            "pkg.a"
        );
    }

    #[test]
    fn tests_are_left_out() {
        for path in [
            "src/pkg/tests/a.py",
            "src/pkg/test_a.py",
            "src/pkg/a_test.py",
            "src/pkg/conftest.py",
        ] {
            assert_eq!(module_of(path, "src/", "pkg"), None, "{path}");
        }
    }
}
