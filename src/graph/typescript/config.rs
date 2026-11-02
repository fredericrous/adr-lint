//! The resolution settings of the root `tsconfig.json`: `baseUrl`, `paths`
//! and `rootDirs`, following relative `extends` chains. Project references
//! are not followed.

use std::path::Path;

use super::jsonc;

const MAX_EXTENDS_DEPTH: usize = 10;

/// One `paths` entry: a pattern with at most one `*` and its targets as
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PathMap {
    pub(super) pattern: String,
    pub(super) targets: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Config {
    /// Repository-relative.
    pub(super) base_url: Option<String>,
    pub(super) paths: Vec<PathMap>,
    /// The directory `paths` targets are relative to: `baseUrl` when one is
    /// set, otherwise the directory of the tsconfig that defines `paths`.
    pub(super) paths_base: String,
    /// Repository-relative.
    pub(super) root_dirs: Vec<String>,
}

#[derive(Default)]
struct Options {
    base_url: Option<String>,
    paths: Option<(Vec<PathMap>, String)>,
    root_dirs: Option<Vec<String>>,
}

/// Reads `tsconfig.json` under `root`; a missing one is an empty config.
/// Returns the config and the problems met while reading it.
pub(super) fn load(root: &Path) -> (Config, Vec<String>) {
    if !root.join("tsconfig.json").is_file() {
        return (Config::default(), Vec::new());
    }
    let (options, warnings) = load_file(root, "tsconfig.json", 0);
    let (paths, defined_in) = options.paths.unwrap_or_default();
    let config = Config {
        paths_base: options.base_url.clone().unwrap_or(defined_in),
        base_url: options.base_url,
        paths,
        root_dirs: options.root_dirs.unwrap_or_default(),
    };
    (config, warnings)
}

fn load_file(root: &Path, file: &str, depth: usize) -> (Options, Vec<String>) {
    let text = match std::fs::read_to_string(root.join(file)) {
        Ok(text) => text,
        Err(cause) => {
            return (
                Options::default(),
                vec![format!("cannot read {file}: {cause}")],
            );
        }
    };
    let document: serde_json::Value = match serde_json::from_str(&jsonc::to_json(&text)) {
        Ok(document) => document,
        Err(cause) => {
            return (
                Options::default(),
                vec![format!("cannot parse {file}: {cause}")],
            );
        }
    };
    let mut warnings = Vec::new();
    let directory = directory_of(file);
    let mut options = Options::default();
    let extended: Vec<&str> = match document.get("extends") {
        Some(serde_json::Value::String(one)) => vec![one.as_str()],
        Some(serde_json::Value::Array(many)) => {
            many.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };
    for target in extended {
        let parent = match extended_file(root, &directory, target) {
            Ok(parent) => parent,
            Err(warning) => {
                warnings.push(warning);
                continue;
            }
        };
        if depth >= MAX_EXTENDS_DEPTH {
            warnings.push(format!(
                "{file}: extends chain deeper than {MAX_EXTENDS_DEPTH}"
            ));
            continue;
        }
        let (inherited, inherited_warnings) = load_file(root, &parent, depth + 1);
        warnings.extend(inherited_warnings);
        options.base_url = inherited.base_url.or(options.base_url);
        options.paths = inherited.paths.or(options.paths);
        options.root_dirs = inherited.root_dirs.or(options.root_dirs);
    }
    let Some(compiler) = document.get("compilerOptions") else {
        return (options, warnings);
    };
    if let Some(base_url) = compiler.get("baseUrl").and_then(serde_json::Value::as_str) {
        options.base_url = Some(join(&directory, base_url));
    }
    if let Some(paths) = compiler.get("paths").and_then(serde_json::Value::as_object) {
        let maps = paths
            .iter()
            .map(|(pattern, targets)| PathMap {
                pattern: pattern.clone(),
                targets: targets
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        options.paths = Some((maps, directory.clone()));
    }
    if let Some(dirs) = compiler
        .get("rootDirs")
        .and_then(serde_json::Value::as_array)
    {
        options.root_dirs = Some(
            dirs.iter()
                .filter_map(serde_json::Value::as_str)
                .map(|dir| join(&directory, dir))
                .collect(),
        );
    }
    (options, warnings)
}

/// The file a relative `extends` names; a package or absolute one is not
/// followed, and the error is the warning to show.
fn extended_file(root: &Path, directory: &str, target: &str) -> Result<String, String> {
    if !(target.starts_with("./") || target.starts_with("../")) {
        return Err(format!(
            "tsconfig extends `{target}`, which is not a relative path and is not followed"
        ));
    }
    let exact = join(directory, target);
    if root.join(&exact).is_file() {
        return Ok(exact);
    }
    Ok(format!("{exact}.json"))
}

/// `relative` below `directory`, with `.` and `..` folded; `directory` is
/// empty for the repository root.
pub(super) fn join(directory: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in directory.split('/').chain(relative.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub(super) fn directory_of(file: &str) -> String {
    file.rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir()
                .join(format!("adr-lint-tsconfig-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok(); // a leftover from an earlier run may not exist
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn write(&self, name: &str, text: &str) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, text).expect("fixture");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok(); // a leftover temp dir fails nothing
        }
    }

    #[test]
    fn join_folds_dots() {
        assert_eq!(join("", "./app/*"), "app/*");
        assert_eq!(join("a/b", "../c"), "a/c");
        assert_eq!(join("", "../x"), "../x");
        assert_eq!(join("a", "../../x"), "../x");
    }

    #[test]
    fn paths_resolve_against_the_config_that_defines_them() {
        let scratch = Scratch::new("extends");
        scratch.write(
            "tsconfig.json",
            "{ \"extends\": \"./config/base\", // comment\n \"compilerOptions\": { \"rootDirs\": [\".\", \".types\",], }, }",
        );
        scratch.write(
            "config/base.json",
            r#"{ "compilerOptions": { "paths": { "~/*": ["./app/*"] } } }"#,
        );
        let (config, warnings) = load(&scratch.0);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.paths_base, "config");
        assert_eq!(config.paths[0].pattern, "~/*");
        assert_eq!(config.root_dirs, ["", ".types"]);
    }

    #[test]
    fn a_base_url_wins_as_the_paths_base() {
        let scratch = Scratch::new("baseurl");
        scratch.write(
            "tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": "./src", "paths": { "@/*": ["*"] } } }"#,
        );
        let (config, warnings) = load(&scratch.0);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.base_url.as_deref(), Some("src"));
        assert_eq!(config.paths_base, "src");
    }

    #[test]
    fn a_package_extends_is_a_warning() {
        let scratch = Scratch::new("package");
        scratch.write(
            "tsconfig.json",
            r#"{ "extends": "@tsconfig/node20/tsconfig.json" }"#,
        );
        let (config, warnings) = load(&scratch.0);
        assert_eq!(config, Config::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("@tsconfig/node20"));
    }
}
