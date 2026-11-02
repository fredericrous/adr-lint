//! Where an import specifier leads: a file of the graph, a package, a file
//! the graph leaves out, or nowhere.

use std::collections::BTreeSet;

use super::config::{directory_of, join, Config, PathMap};

/// Extensions of files that are not TypeScript and are not looked for.
const ASSET_EXTENSIONS: [&str; 30] = [
    "css", "scss", "sass", "less", "svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "ico",
    "json", "md", "mdx", "txt", "html", "woff", "woff2", "ttf", "eot", "mp3", "mp4", "webm",
    "wasm", "yaml", "yml", "graphql", "gql", "csv",
];

/// The `.js`-family extensions and the TypeScript sources they stand for.
const SOURCE_FOR: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx"]),
    (".jsx", &[".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolution {
    /// A node of the graph.
    File(String),
    /// A bare package specifier.
    External(String),
    /// A file that exists but is not a node (generated types, say), or a
    /// non-TypeScript asset: no edge, no warning.
    Left,
    /// A relative or alias specifier that leads nowhere.
    Unresolved,
}

pub(super) struct Resolver<'a> {
    pub(super) config: &'a Config,
    pub(super) nodes: &'a BTreeSet<String>,
}

impl Resolver<'_> {
    /// `exists_on_disk` answers for files that are not nodes.
    pub(super) fn resolve(
        &self,
        from: &str,
        specifier: &str,
        exists_on_disk: &dyn Fn(&str) -> bool,
    ) -> Resolution {
        let bare = specifier.split(['?', '#']).next().unwrap_or(specifier);
        let relative =
            bare == "." || bare == ".." || bare.starts_with("./") || bare.starts_with("../");
        let bases = if relative {
            self.relative_bases(from, bare)
        } else if let Some(targets) = self.alias_bases(bare) {
            targets
        } else {
            return self.base_url_or_external(bare);
        };
        if is_asset(bare) {
            return Resolution::Left;
        }
        for base in &bases {
            if let Some(found) = probe(base, &|path| self.nodes.contains(path)) {
                return Resolution::File(found);
            }
        }
        if bases
            .iter()
            .any(|base| probe(base, exists_on_disk).is_some())
        {
            return Resolution::Left;
        }
        if relative && self.reaches_a_generated_root(from) {
            return Resolution::Left;
        }
        Resolution::Unresolved
    }

    /// Whether `from` sits in a `rootDirs` entry whose sibling entry is a
    /// generated tree not produced yet: it holds no node and is not a
    /// directory on disk (React Router's `.react-router/types` on a fresh
    /// checkout). A relative import that can only resolve there is left out,
    /// never warned about. Once the tree exists, the on-disk probe decides,
    /// so a typo still warns.
    fn reaches_a_generated_root(&self, from: &str) -> bool {
        let directory = directory_of(from);
        let roots = &self.config.root_dirs;
        roots.iter().any(|root| {
            within(&directory, root).is_some()
                && roots
                    .iter()
                    .filter(|other| *other != root)
                    .filter(|other| self.config.missing_root_dirs.contains(other))
                    .any(|other| {
                        !self
                            .nodes
                            .iter()
                            .any(|node| within(&directory_of(node), other).is_some())
                    })
        })
    }

    fn relative_bases(&self, from: &str, specifier: &str) -> Vec<String> {
        let directory = directory_of(from);
        let mut bases = vec![join(&directory, specifier)];
        let roots = &self.config.root_dirs;
        for root in roots {
            let Some(inside) = within(&directory, root) else {
                continue;
            };
            for other in roots.iter().filter(|other| *other != root) {
                bases.push(join(&join(other, &inside), specifier));
            }
        }
        bases
    }

    /// The candidate bases of the best `paths` match, if any pattern matches.
    fn alias_bases(&self, specifier: &str) -> Option<Vec<String>> {
        let (map, captured) = self
            .config
            .paths
            .iter()
            .filter_map(|map| match_pattern(map, specifier).map(|captured| (map, captured)))
            .max_by_key(|(map, _)| map.pattern.find('*').unwrap_or(map.pattern.len()))?;
        Some(
            map.targets
                .iter()
                .map(|target| join(&self.config.paths_base, &target.replacen('*', &captured, 1)))
                .collect(),
        )
    }

    fn base_url_or_external(&self, specifier: &str) -> Resolution {
        if let Some(base_url) = &self.config.base_url {
            let base = join(base_url, specifier);
            if let Some(found) = probe(&base, &|path| self.nodes.contains(path)) {
                return Resolution::File(found);
            }
        }
        Resolution::External(specifier.to_string())
    }
}

/// `directory` below `root`, relative to it; `None` when it is not inside.
fn within(directory: &str, root: &str) -> Option<String> {
    if root.is_empty() {
        return Some(directory.to_string());
    }
    if directory == root {
        return Some(String::new());
    }
    directory
        .strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}

/// What a `*` stands for when `specifier` matches `map`'s pattern.
fn match_pattern(map: &PathMap, specifier: &str) -> Option<String> {
    match map.pattern.split_once('*') {
        None => (map.pattern == specifier).then(String::new),
        Some((prefix, suffix)) => specifier
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
            .map(str::to_string),
    }
}

fn is_asset(specifier: &str) -> bool {
    let last = specifier.rsplit('/').next().unwrap_or(specifier);
    last.rsplit_once('.')
        .is_some_and(|(stem, extension)| !stem.is_empty() && ASSET_EXTENSIONS.contains(&extension))
}

/// The first existing file for `base` as TypeScript resolves it: a `.js`
/// specifier's source, the path itself, then `.ts`, `.tsx`, `.d.ts` appended,
/// then `index.*` inside it.
fn probe(base: &str, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if !base.is_empty() {
        for (javascript, sources) in SOURCE_FOR {
            if let Some(stem) = base.strip_suffix(javascript) {
                candidates.extend(sources.iter().map(|source| format!("{stem}{source}")));
            }
        }
        candidates.push(base.to_string());
        candidates.extend([".ts", ".tsx", ".d.ts"].map(|extension| format!("{base}{extension}")));
    }
    let inside = |name: &str| {
        if base.is_empty() {
            name.to_string()
        } else {
            format!("{base}/{name}")
        }
    };
    candidates.extend(["index.ts", "index.tsx", "index.d.ts"].map(inside));
    candidates.into_iter().find(|candidate| exists(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            base_url: None,
            paths: vec![PathMap {
                pattern: "~/*".to_string(),
                targets: vec!["./app/*".to_string()],
            }],
            paths_base: String::new(),
            root_dirs: vec![String::new(), ".react-router/types".to_string()],
            missing_root_dirs: Vec::new(),
        }
    }

    fn nodes(files: &[&str]) -> BTreeSet<String> {
        files.iter().map(|file| (*file).to_string()).collect()
    }

    fn resolved(files: &[&str], disk: &[&str], from: &str, specifier: &str) -> Resolution {
        let known = nodes(files);
        let config = config();
        let resolver = Resolver {
            config: &config,
            nodes: &known,
        };
        resolver.resolve(from, specifier, &|path| disk.contains(&path))
    }

    fn file(path: &str) -> Resolution {
        Resolution::File(path.to_string())
    }

    #[test]
    fn a_relative_import_appends_an_extension() {
        assert_eq!(
            resolved(&["app/a.ts", "app/b.tsx"], &[], "app/a.ts", "./b"),
            file("app/b.tsx")
        );
        assert_eq!(
            resolved(&["a.ts", "b.d.ts"], &[], "a.ts", "./b"),
            file("b.d.ts")
        );
    }

    #[test]
    fn a_directory_resolves_to_its_index() {
        assert_eq!(
            resolved(&["a.ts", "lib/index.ts"], &[], "a.ts", "./lib"),
            file("lib/index.ts")
        );
        assert_eq!(
            resolved(&["a.ts", "index.ts"], &[], "a.ts", "."),
            file("index.ts")
        );
    }

    #[test]
    fn a_js_specifier_maps_to_its_ts_source() {
        assert_eq!(
            resolved(&["a.ts", "b.ts"], &[], "a.ts", "./b.js"),
            file("b.ts")
        );
        assert_eq!(
            resolved(&["a.ts", "b.tsx"], &[], "a.ts", "./b.js"),
            file("b.tsx")
        );
        assert_eq!(
            resolved(&["a.ts", "b.mts"], &[], "a.ts", "./b.mjs"),
            file("b.mts")
        );
    }

    #[test]
    fn an_alias_with_a_dotted_name_appends_rather_than_replaces() {
        assert_eq!(
            resolved(
                &["app/lib/auth.server.ts"],
                &[],
                "app/routes/x.ts",
                "~/lib/auth.server"
            ),
            file("app/lib/auth.server.ts")
        );
    }

    #[test]
    fn root_dirs_that_hold_only_generated_files_leave_the_specifier_out() {
        let disk = [".react-router/types/app/routes/+types/home.ts"];
        assert_eq!(
            resolved(
                &["app/routes/home.tsx"],
                &disk,
                "app/routes/home.tsx",
                "./+types/home"
            ),
            Resolution::Left
        );
        assert_eq!(
            resolved(
                &["app/routes/home.tsx"],
                &[],
                "app/routes/home.tsx",
                "./+types/home"
            ),
            Resolution::Unresolved
        );
    }

    #[test]
    fn a_generated_root_not_produced_yet_leaves_the_specifier_out() {
        let known = nodes(&["app/routes/home.tsx"]);
        let config = Config {
            missing_root_dirs: vec![".react-router/types".to_string()],
            ..config()
        };
        let resolver = Resolver {
            config: &config,
            nodes: &known,
        };
        assert_eq!(
            resolver.resolve("app/routes/home.tsx", "./+types/home", &|_| false),
            Resolution::Left
        );
        assert_eq!(
            resolver.resolve("app/routes/home.tsx", "~/lib/gone", &|_| false),
            Resolution::Unresolved,
            "an alias is not covered by rootDirs"
        );
    }

    #[test]
    fn assets_are_left_without_a_probe() {
        for specifier in ["./x.css", "~/logo.svg", "./data.json", "./x.css?url"] {
            assert_eq!(
                resolved(&["a.ts"], &[], "a.ts", specifier),
                Resolution::Left,
                "{specifier}"
            );
        }
    }

    #[test]
    fn an_alias_to_nothing_is_unresolved_and_a_package_is_external() {
        assert_eq!(
            resolved(&["a.ts"], &[], "a.ts", "~/missing"),
            Resolution::Unresolved
        );
        assert_eq!(
            resolved(&["a.ts"], &[], "a.ts", "./missing"),
            Resolution::Unresolved
        );
        assert_eq!(
            resolved(&["a.ts"], &[], "a.ts", "@effect/sql"),
            Resolution::External("@effect/sql".to_string())
        );
    }

    #[test]
    fn the_longest_prefix_pattern_wins() {
        let known = nodes(&["a.ts", "wide/x.ts", "narrow/x.ts"]);
        let config = Config {
            paths: vec![
                PathMap {
                    pattern: "@/*".to_string(),
                    targets: vec!["wide/*".to_string()],
                },
                PathMap {
                    pattern: "@/lib/*".to_string(),
                    targets: vec!["narrow/*".to_string()],
                },
            ],
            ..Config::default()
        };
        let resolver = Resolver {
            config: &config,
            nodes: &known,
        };
        assert_eq!(
            resolver.resolve("a.ts", "@/lib/x", &|_| false),
            file("narrow/x.ts")
        );
    }
}
