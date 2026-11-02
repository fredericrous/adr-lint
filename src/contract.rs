//! The `[architecture]` section of `adr-lint.toml` (`adr-lint.contracts/1`,
//! docs/formats.md): layers inner first, `only_in` lists, the places ambient
//! reads are allowed, and the cycle check. Parsing validates; every error
//! names the `adr-lint.toml` line it comes from.

use std::fmt;

use toml_edit::{Document, Item, TableLike, Value};

use crate::config::FILE;

/// A contract error, already worded with its `adr-lint.toml:N` location.
#[derive(Debug, PartialEq, Eq)]
pub struct ContractError(String);

impl ContractError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(&self.0)
    }
}

/// One string of the contract and the line it is written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub line: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
    pub line: usize,
    /// Globs, `!`-prefixed ones exclude.
    pub paths: Vec<Entry>,
    /// Rust `crate::` paths or Python dotted modules.
    pub modules: Vec<Entry>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OnlyIn {
    pub line: usize,
    pub imports: Vec<Entry>,
    pub paths: Vec<Entry>,
    pub modules: Vec<Entry>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ambient {
    pub paths: Vec<Entry>,
    pub modules: Vec<Entry>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Contract {
    /// Inner first: a layer may import itself and the layers before it.
    pub layers: Vec<Layer>,
    pub only_in: Vec<OnlyIn>,
    pub ambient: Ambient,
    pub check_cycles: bool,
}

impl Layer {
    fn matches_path(&self, path: &str) -> bool {
        let included = self
            .paths
            .iter()
            .filter(|entry| !entry.text.starts_with('!'))
            .any(|entry| glob_matches(&entry.text, path));
        let excluded = self
            .paths
            .iter()
            .filter_map(|entry| entry.text.strip_prefix('!'))
            .any(|pattern| glob_matches(pattern, path));
        included && !excluded
    }

    fn module_entry(&self, module: &str) -> Option<&Entry> {
        self.modules
            .iter()
            .filter(|entry| module_within(module, &entry.text))
            .max_by_key(|entry| entry.text.len())
    }
}

impl Contract {
    /// The layer a file belongs to: the one whose globs match it and whose
    /// `!` excludes do not. Two layers matching is a contract error.
    pub fn layer_of_path(&self, path: &str) -> Result<Option<&Layer>, ContractError> {
        let mut matching = self.layers.iter().filter(|layer| layer.matches_path(path));
        let first = matching.next();
        match (first, matching.next()) {
            (Some(one), Some(other)) => Err(ContractError(format!(
                "{FILE}:{}: path `{path}` is in layer `{}` and in layer `{}` ({FILE}:{})",
                one.line, one.name, other.name, other.line
            ))),
            (found, _) => Ok(found),
        }
    }

    /// The layer owning a module: the one with the longest entry that is the
    /// module or a prefix of it, on `::` or `.` boundaries.
    pub fn layer_of_module(&self, module: &str) -> Option<&Layer> {
        self.layers
            .iter()
            .filter_map(|layer| {
                layer
                    .module_entry(module)
                    .map(|entry| (entry.text.len(), layer))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, layer)| layer)
    }
}

/// `module` is `prefix` or lies inside it.
fn module_within(module: &str, prefix: &str) -> bool {
    module == prefix
        || module
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with("::") || rest.starts_with('.'))
}

fn is_literal(pattern: &str) -> bool {
    !pattern.contains(['*', '?', '['])
}

/// A glob over `/`-separated paths: `**` matches any number of segments
/// (also none), `*` any run of characters inside one segment, `?` one.
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&pattern, &path)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((segment, rest)) => path.split_first().is_some_and(|(name, remaining)| {
            segment_matches(segment.as_bytes(), name.as_bytes()) && segments_match(rest, remaining)
        }),
    }
}

fn segment_matches(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|skip| segment_matches(rest, &name[skip..])),
        Some((b'?', rest)) => !name.is_empty() && segment_matches(rest, &name[1..]),
        Some((byte, rest)) => name.first() == Some(byte) && segment_matches(rest, &name[1..]),
    }
}

/// Parses and validates the `[architecture]` section of `text`; `None` when
/// there is none.
pub fn parse(text: &str) -> Result<Option<Contract>, ContractError> {
    let document = Document::parse(text.to_string())
        .map_err(|cause| ContractError(format!("{FILE}: {cause}")))?;
    let Some(section) = document.as_item().get("architecture") else {
        return Ok(None);
    };
    let reader = Reader { text };
    let table = reader.table(section, "architecture", 1)?;
    reader.only_keys(
        table,
        &["layers", "only_in", "ambient", "cycles"],
        "architecture",
    )?;
    let layers = reader.layers(table)?;
    let only_in = reader.only_in(table)?;
    let ambient = reader.ambient(table)?;
    let check_cycles = reader.cycles(table)?;
    check_overlaps(&layers)?;
    Ok(Some(Contract {
        layers,
        only_in,
        ambient,
        check_cycles,
    }))
}

struct Reader<'a> {
    text: &'a str,
}

impl Reader<'_> {
    fn line_at(&self, span: Option<std::ops::Range<usize>>) -> usize {
        span.map_or(1, |range| {
            1 + self.text.as_bytes()[..range.start.min(self.text.len())]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
        })
    }

    fn error(&self, line: usize, message: &str) -> ContractError {
        ContractError(format!("{FILE}:{line}: {message}"))
    }

    fn table<'t>(
        &self,
        item: &'t Item,
        name: &str,
        fallback_line: usize,
    ) -> Result<&'t dyn TableLike, ContractError> {
        item.as_table_like().ok_or_else(|| {
            let line = item
                .span()
                .map_or(fallback_line, |_| self.line_at(item.span()));
            self.error(line, &format!("`{name}` must be a table"))
        })
    }

    fn only_keys(
        &self,
        table: &dyn TableLike,
        known: &[&str],
        name: &str,
    ) -> Result<(), ContractError> {
        for (key, _) in table.iter() {
            if !known.contains(&key) {
                let line = table
                    .get_key_value(key)
                    .map_or(1, |(found, _)| self.line_at(found.span()));
                return Err(self.error(
                    line,
                    &format!(
                        "unknown key `{key}` in `{name}` (known: {})",
                        known.join(", ")
                    ),
                ));
            }
        }
        Ok(())
    }

    fn line_of_key(&self, table: &dyn TableLike, key: &str) -> usize {
        table
            .get_key_value(key)
            .map_or(1, |(found, _)| self.line_at(found.span()))
    }

    /// The entries of an optional array-of-strings key.
    fn strings(
        &self,
        table: &dyn TableLike,
        key: &str,
        context: &str,
    ) -> Result<Vec<Entry>, ContractError> {
        let Some(item) = table.get(key) else {
            return Ok(Vec::new());
        };
        let line = self.line_of_key(table, key);
        let Some(array) = item.as_value().and_then(Value::as_array) else {
            return Err(self.error(
                line,
                &format!("{context}: `{key}` must be an array of strings"),
            ));
        };
        array
            .iter()
            .map(|value| {
                let at = self.line_at(value.span());
                value
                    .as_str()
                    .map(|text| Entry {
                        text: text.to_string(),
                        line: at,
                    })
                    .ok_or_else(|| {
                        self.error(at, &format!("{context}: `{key}` must hold only strings"))
                    })
            })
            .collect()
    }

    fn tables_of<'t>(
        &self,
        parent: &'t dyn TableLike,
        key: &str,
    ) -> Result<Vec<(&'t dyn TableLike, usize)>, ContractError> {
        let Some(item) = parent.get(key) else {
            return Ok(Vec::new());
        };
        let line = self.line_of_key(parent, key);
        if let Some(array) = item.as_array_of_tables() {
            return Ok(array
                .iter()
                .map(|table| (table as &dyn TableLike, self.line_at(table.span())))
                .collect());
        }
        if let Some(array) = item.as_value().and_then(Value::as_array) {
            return array
                .iter()
                .map(|value| {
                    value
                        .as_inline_table()
                        .map(|table| (table as &dyn TableLike, self.line_at(value.span())))
                        .ok_or_else(|| self.error(line, &format!("`{key}` must hold tables")))
                })
                .collect();
        }
        Err(self.error(
            line,
            &format!("`architecture.{key}` must be an array of tables ([[architecture.{key}]])"),
        ))
    }

    fn layers(&self, table: &dyn TableLike) -> Result<Vec<Layer>, ContractError> {
        let mut layers: Vec<Layer> = Vec::new();
        for (layer, line) in self.tables_of(table, "layers")? {
            self.only_keys(layer, &["name", "paths", "modules"], "architecture.layers")?;
            let name = layer
                .get("name")
                .and_then(Item::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| self.error(line, "a layer needs `name` as a non-empty string"))?
                .to_string();
            if let Some(first) = layers.iter().find(|earlier| earlier.name == name) {
                return Err(self.error(
                    line,
                    &format!(
                        "layer `{name}` is declared twice (first at {FILE}:{})",
                        first.line
                    ),
                ));
            }
            let context = format!("layer `{name}`");
            let paths = self.strings(layer, "paths", &context)?;
            let modules = self.strings(layer, "modules", &context)?;
            if let Some(entry) = modules.iter().find(|entry| entry.text.starts_with('!')) {
                return Err(self.error(
                    entry.line,
                    &format!("{context}: `!` excludes apply to `paths` only"),
                ));
            }
            let has_positive = paths.iter().any(|entry| !entry.text.starts_with('!'));
            if !has_positive && modules.is_empty() {
                return Err(self.error(
                    line,
                    &format!("{context} needs at least one `paths` glob or `modules` entry"),
                ));
            }
            layers.push(Layer {
                name,
                line,
                paths,
                modules,
            });
        }
        Ok(layers)
    }

    fn only_in(&self, table: &dyn TableLike) -> Result<Vec<OnlyIn>, ContractError> {
        let mut lists = Vec::new();
        for (list, line) in self.tables_of(table, "only_in")? {
            self.only_keys(
                list,
                &["imports", "paths", "modules"],
                "architecture.only_in",
            )?;
            let imports = self.strings(list, "imports", "only_in")?;
            let paths = self.strings(list, "paths", "only_in")?;
            let modules = self.strings(list, "modules", "only_in")?;
            if imports.is_empty() {
                return Err(self.error(line, "an `only_in` entry needs a non-empty `imports`"));
            }
            if paths.is_empty() && modules.is_empty() {
                return Err(self.error(
                    line,
                    "an `only_in` entry needs `paths` or `modules` saying where the imports may appear",
                ));
            }
            lists.push(OnlyIn {
                line,
                imports,
                paths,
                modules,
            });
        }
        Ok(lists)
    }

    fn ambient(&self, table: &dyn TableLike) -> Result<Ambient, ContractError> {
        let Some(item) = table.get("ambient") else {
            return Ok(Ambient::default());
        };
        let ambient = self.table(
            item,
            "architecture.ambient",
            self.line_of_key(table, "ambient"),
        )?;
        self.only_keys(ambient, &["paths", "modules"], "architecture.ambient")?;
        Ok(Ambient {
            paths: self.strings(ambient, "paths", "ambient")?,
            modules: self.strings(ambient, "modules", "ambient")?,
        })
    }

    fn cycles(&self, table: &dyn TableLike) -> Result<bool, ContractError> {
        let Some(item) = table.get("cycles") else {
            return Ok(true);
        };
        let line = self.line_of_key(table, "cycles");
        let cycles = self.table(item, "architecture.cycles", line)?;
        self.only_keys(cycles, &["check"], "architecture.cycles")?;
        match cycles.get("check") {
            None => Ok(true),
            Some(value) => value
                .as_bool()
                .ok_or_else(|| self.error(line, "`cycles.check` must be true or false")),
        }
    }
}

/// A module in two layers, or a path two layers both claim, is an error that
/// names both entries. Two globs can overlap without any file proving it, so
/// the static check covers equal entries and a literal path matched by the
/// other layer; `Contract::layer_of_path` catches the rest per file.
fn check_overlaps(layers: &[Layer]) -> Result<(), ContractError> {
    for (index, earlier) in layers.iter().enumerate() {
        for later in &layers[index + 1..] {
            for first in &earlier.modules {
                if let Some(second) = later.modules.iter().find(|entry| entry.text == first.text) {
                    return Err(conflict("module", first, earlier, second, later));
                }
            }
            for (owner, other) in [(earlier, later), (later, earlier)] {
                for entry in positive_paths(owner) {
                    let clash = other
                        .paths
                        .iter()
                        .find(|candidate| candidate.text == entry.text)
                        .or_else(|| {
                            (is_literal(&entry.text) && other.matches_path(&entry.text))
                                .then(|| {
                                    positive_paths(other).find(|candidate| {
                                        glob_matches(&candidate.text, &entry.text)
                                    })
                                })
                                .flatten()
                        });
                    if let Some(second) = clash {
                        return Err(if std::ptr::eq(owner, earlier) {
                            conflict("path", entry, owner, second, other)
                        } else {
                            conflict("path", second, other, entry, owner)
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn positive_paths(layer: &Layer) -> impl Iterator<Item = &Entry> {
    layer
        .paths
        .iter()
        .filter(|entry| !entry.text.starts_with('!'))
}

fn conflict(
    kind: &str,
    first: &Entry,
    first_layer: &Layer,
    second: &Entry,
    second_layer: &Layer,
) -> ContractError {
    ContractError(format!(
        "{FILE}:{}: {kind} `{}` of layer `{}` is also in layer `{}` ({kind} `{}` at {FILE}:{})",
        first.line, first.text, first_layer.name, second_layer.name, second.text, second.line
    ))
}
