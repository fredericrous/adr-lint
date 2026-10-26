//! `adr-lint sync`: writes the clippy lints that police suppressions into a
//! repository's Cargo.toml (general.linters-police-suppressions). Rust is the
//! only language synced so far.

use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{value, Document, DocumentMut, Item, Table, TableLike};

use crate::coverage::Failure;
use crate::process;

pub(crate) const USAGE: &str = "\
usage: adr-lint sync [--check] [-C <dir>]

Writes clippy's `allow_attributes` and `allow_attributes_without_reason` at
`deny` into <dir>/Cargo.toml (`[lints.clippy]`, or `[workspace.lints.clippy]`
with `lints.workspace = true` added to every workspace member), keeping the
formatting and comments of the file. A key already at deny or forbid is kept;
a weaker one is reported and nothing is written. Rust is the only language
synced so far.

flags:
  -C <dir>    the repository (default .)
  --check     write nothing; exit 1 when a key is missing or weaker, or a
              member does not opt in
  -h, --help  this text

exit status:
  0  in sync, synced, or nothing to do for Rust
  1  a weaker level (sync), or anything missing or weaker (--check)
  2  usage error
  3  could not run (cargo failing, a manifest that is a symlink or unreadable)
";

const RULE: &str = "general.linters-police-suppressions";
const COMMENT: &str = "# adr-lint sync: general.linters-police-suppressions\n";
const KEYS: [&str; 2] = ["allow_attributes", "allow_attributes_without_reason"];
const MINIMUM_RUST: (u64, u64) = (1, 81);
const METADATA: [&str; 4] = ["metadata", "--no-deps", "--format-version", "1"];

#[derive(Debug, PartialEq, Eq)]
struct Options {
    dir: PathBuf,
    check: bool,
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options {
        dir: PathBuf::from("."),
        check: false,
    };
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        if arg == "--check" {
            options.check = true;
        } else if arg == "-C" || (arg.starts_with("-C") && !arg.starts_with("--")) {
            let attached = arg.strip_prefix("-C").filter(|rest| !rest.is_empty());
            let dir = match attached {
                Some(dir) => dir.to_string(),
                None => remaining
                    .next()
                    .cloned()
                    .ok_or_else(|| Failure::Usage("`-C` needs a value".to_string()))?,
            };
            options.dir = PathBuf::from(dir);
        } else if arg.starts_with('-') {
            return Err(Failure::Usage(format!("unknown flag `{arg}`")));
        } else {
            return Err(Failure::Usage(format!("unexpected argument `{arg}`")));
        }
    }
    Ok(options)
}

/// One manifest being read, and edited when the run is not a check.
struct Manifest {
    path: PathBuf,
    findings: Vec<String>,
    document: DocumentMut,
    roles: Vec<Role>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Holds the lints: `lints.clippy`, or `workspace.lints.clippy`.
    Target { workspace: bool },
    /// A workspace member: must inherit with `lints.workspace = true`.
    Member,
}

/// A level that keeps the suppression lints strong.
fn is_strong(level: &str) -> bool {
    level == "deny" || level == "forbid"
}

fn level_of(item: &Item) -> Option<&str> {
    item.as_str().or_else(|| {
        item.as_table_like()
            .and_then(|table| table.get("level"))
            .and_then(Item::as_str)
    })
}

fn line_of(raw: &str, item: Option<&Item>) -> usize {
    let start = item.and_then(Item::span).map_or(0, |span| span.start);
    raw[..start.min(raw.len())].matches('\n').count() + 1
}

fn lookup<'a>(root: &'a Item, path: &[&str]) -> Option<&'a Item> {
    path.iter().try_fold(root, |item, segment| {
        item.as_table_like().and_then(|table| table.get(segment))
    })
}

fn target_path(workspace: bool) -> Vec<&'static str> {
    if workspace {
        vec!["workspace", "lints", "clippy"]
    } else {
        vec!["lints", "clippy"]
    }
}

fn inherits_workspace_lints(root: &Item) -> bool {
    lookup(root, &["lints", "workspace"]).and_then(Item::as_bool) == Some(true)
}

/// Findings of one manifest, each as the line the person is shown.
fn audit(path: &Path, raw: &str, role: Role) -> Result<Vec<String>, Failure> {
    let parsed = Document::parse(raw.to_string())
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", path.display())))?;
    let root = parsed.as_item();
    let shown = path.display();
    let mut findings = Vec::new();
    match role {
        Role::Target { workspace } => {
            let table = lookup(root, &target_path(workspace));
            for key in KEYS {
                let item = table.and_then(Item::as_table_like).and_then(|t| t.get(key));
                match item {
                    None => findings.push(format!("{shown}: clippy.{key} is missing")),
                    Some(item) => match level_of(item) {
                        Some(level) if is_strong(level) => {}
                        other => findings.push(format!(
                            "{shown}:{}: clippy.{key} is {}, weaker than deny ({RULE})",
                            line_of(raw, Some(item)),
                            other.unwrap_or("not a level"),
                        )),
                    },
                }
            }
        }
        Role::Member => match lookup(root, &["lints"]) {
            None => findings.push(format!("{shown}: lints.workspace = true is missing")),
            Some(_) if inherits_workspace_lints(root) => {}
            Some(lints) => findings.push(format!(
                "{shown}:{}: lints are set here without `workspace = true`, so the workspace lints do not apply ({RULE})",
                line_of(raw, Some(lints)),
            )),
        },
    }
    Ok(findings)
}

fn is_conflict(finding: &str) -> bool {
    !finding.contains(" is missing")
}

/// Walks to the table at `path`, creating what is absent.
fn table_at<'a>(
    document: &'a mut DocumentMut,
    path: &[&str],
) -> Result<&'a mut dyn TableLike, String> {
    let mut current: &mut dyn TableLike = document.as_table_mut();
    for (index, segment) in path.iter().enumerate() {
        if current.get(segment).is_none() {
            let mut table = Table::new();
            table.set_implicit(index + 1 < path.len());
            current.insert(segment, Item::Table(table));
        }
        current = current
            .get_mut(segment)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| format!("`{segment}` is not a table"))?;
    }
    Ok(current)
}

fn apply(manifest: &mut Manifest) -> Result<(), Failure> {
    let path = manifest.path.display().to_string();
    let failed = |cause: String| Failure::CouldNotRun(format!("{path}: {cause}"));
    for role in manifest.roles.clone() {
        apply_role(&mut manifest.document, role).map_err(failed)?;
    }
    Ok(())
}

fn apply_role(document: &mut DocumentMut, role: Role) -> Result<(), String> {
    match role {
        Role::Target { workspace } => {
            let table = table_at(document, &target_path(workspace))?;
            let mut commented = false;
            for key in KEYS {
                if table.get(key).is_some() {
                    continue;
                }
                table.insert(key, value("deny"));
                if !commented {
                    commented = true;
                    if let Some((mut name, _)) = table.get_key_value_mut(key) {
                        name.leaf_decor_mut().set_prefix(COMMENT);
                    }
                }
            }
        }
        Role::Member => {
            let table = table_at(document, &["lints"])?;
            table.insert("workspace", value(true));
        }
    }
    Ok(())
}

fn load(path: &Path, roles: Vec<Role>) -> Result<Manifest, Failure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", path.display())))?;
    if metadata.file_type().is_symlink() {
        return Err(Failure::CouldNotRun(format!(
            "{} is a symlink; refusing to rewrite it",
            path.display()
        )));
    }
    let raw = fs::read_to_string(path)
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", path.display())))?;
    let mut findings = Vec::new();
    for role in &roles {
        findings.extend(audit(path, &raw, *role)?);
    }
    let document = raw
        .parse::<DocumentMut>()
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", path.display())))?;
    Ok(Manifest {
        path: path.to_path_buf(),
        findings,
        document,
        roles,
    })
}

/// The `rust-version` of the root manifest, `[package]` or `[workspace.package]`.
fn rust_version(raw: &str) -> Option<String> {
    let parsed = Document::parse(raw.to_string()).ok()?;
    let root = parsed.as_item();
    ["package", "workspace"]
        .iter()
        .find_map(|head| {
            let path: &[&str] = if *head == "package" {
                &["package", "rust-version"]
            } else {
                &["workspace", "package", "rust-version"]
            };
            lookup(root, path).and_then(Item::as_str)
        })
        .map(str::to_string)
}

fn below_minimum(version: &str) -> bool {
    let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(major), Some(minor)) => (major, minor) < MINIMUM_RUST,
        _ => false,
    }
}

/// The manifests of the workspace members, as `cargo metadata` lists them.
fn member_manifests(dir: &Path) -> Result<Vec<PathBuf>, Failure> {
    let command = format!("cargo {}", METADATA.join(" "));
    let failed = |cause: String| Failure::CouldNotRun(format!("`{command}` failed: {cause}"));
    let printed = process::run("cargo", &METADATA, dir, process::DEADLINE)
        .map_err(|cause| failed(cause.into_message()))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&printed).map_err(|cause| failed(cause.to_string()))?;
    let packages = parsed
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| failed("no `packages` array in its output".to_string()))?;
    Ok(packages
        .iter()
        .filter_map(|package| package.get("manifest_path")?.as_str())
        .map(PathBuf::from)
        .collect())
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Writes `text` over `path` through a temporary file beside it, keeping
/// the permission bits of the file it replaces.
fn write_atomically(path: &Path, text: &str) -> Result<(), Failure> {
    let failed = |cause: std::io::Error| {
        Failure::CouldNotRun(format!("cannot write {}: {cause}", path.display()))
    };
    let temporary = path.with_file_name(format!(
        ".{}.adr-lint-sync.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Cargo.toml")
    ));
    let permissions = fs::metadata(path).map_err(failed)?.permissions();
    let written = fs::write(&temporary, text)
        .and_then(|()| fs::set_permissions(&temporary, permissions))
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(cause) = written {
        // The temporary file is abandoned either way; the write error is reported.
        fs::remove_file(&temporary).ok();
        return Err(failed(cause));
    }
    Ok(())
}

/// Runs the command; the exit status is the `Ok` value (0 or 1).
pub(crate) fn run(args: &[String]) -> Result<u8, Failure> {
    let options = parse_options(args)?;
    let root_path = options.dir.join("Cargo.toml");
    match fs::symlink_metadata(&root_path) {
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("sync: no Cargo.toml, nothing to do for Rust");
            return Ok(0);
        }
        Err(cause) => {
            return Err(Failure::CouldNotRun(format!(
                "{}: {cause}",
                root_path.display()
            )))
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(Failure::CouldNotRun(format!(
                "{} is a symlink; refusing to rewrite it",
                root_path.display()
            )))
        }
        Ok(_) => {}
    }
    let raw = fs::read_to_string(&root_path)
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", root_path.display())))?;
    if let Some(version) = rust_version(&raw).filter(|version| below_minimum(version)) {
        eprintln!(
            "sync: rust-version {version} is below 1.81, where #[expect(…, reason)] exists; Rust skipped"
        );
        return Ok(0);
    }
    let parsed = Document::parse(raw.clone())
        .map_err(|cause| Failure::CouldNotRun(format!("{}: {cause}", root_path.display())))?;
    let is_workspace = parsed
        .as_item()
        .as_table_like()
        .is_some_and(|t| t.contains_key("workspace"));
    let has_package = parsed
        .as_item()
        .as_table_like()
        .is_some_and(|t| t.contains_key("package"));
    if !is_workspace && inherits_workspace_lints(parsed.as_item()) {
        eprintln!(
            "sync: {} inherits its lints from a workspace above; run sync there",
            root_path.display()
        );
        return Ok(0);
    }

    let mut root_roles = vec![Role::Target {
        workspace: is_workspace,
    }];
    if is_workspace && has_package {
        root_roles.push(Role::Member);
    }
    let mut manifests = vec![load(&root_path, root_roles)?];
    if is_workspace {
        for member in member_manifests(&options.dir)? {
            if !same_file(&member, &root_path) {
                manifests.push(load(&member, vec![Role::Member])?);
            }
        }
    }

    let findings: Vec<&String> = manifests.iter().flat_map(|m| m.findings.iter()).collect();
    if options.check {
        for finding in &findings {
            eprintln!("{finding}");
        }
        return Ok(u8::from(!findings.is_empty()));
    }
    let conflicts: Vec<&&String> = findings.iter().filter(|f| is_conflict(f)).collect();
    if !conflicts.is_empty() {
        for conflict in conflicts {
            eprintln!("{conflict}");
        }
        return Ok(1);
    }
    if findings.is_empty() {
        eprintln!("sync: already in sync");
        return Ok(0);
    }
    for manifest in &mut manifests {
        if manifest.findings.is_empty() {
            continue;
        }
        apply(manifest)?;
        write_atomically(&manifest.path, &manifest.document.to_string())?;
        eprintln!("sync: wrote {}", manifest.path.display());
    }
    Ok(0)
}
