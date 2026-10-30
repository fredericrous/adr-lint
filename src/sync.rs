//! `adr-lint sync`: writes the settings that make a linter refuse the
//! suppressions general.linters-police-suppressions forbids. Rust, Python, Go
//! and TypeScript are synced; their halves are in `rust`, `python`, `go` and
//! `typescript`.

mod go;
mod python;
mod rust;
mod typescript;

use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::Item;

use crate::coverage::Failure;

pub(crate) const USAGE: &str = "\
usage: adr-lint sync [--check] [-C <dir>]

Rust: writes clippy's `allow_attributes` and `allow_attributes_without_reason`
at `deny` into <dir>/Cargo.toml (`[lints.clippy]`, or `[workspace.lints.clippy]`
with `lints.workspace = true` added to every workspace member).
Python: makes ruff select PGH003, PGH004 and RUF100 (in `.ruff.toml`,
`ruff.toml` or pyproject.toml `[tool.ruff]`, appending to `select`, or to
`extend-select` when there is none) and pyright set
`enableTypeIgnoreComments = false` and
`reportUnnecessaryTypeIgnoreComment = \"error\"` (in pyrightconfig.json, else
pyproject.toml `[tool.pyright]`). A tool with no config is skipped, not
configured. Formatting and comments are kept. A key already at a strong value
is kept; an ignore or per-file-ignore of a covered ruff code, or a weaker
pyright value, is a conflict: it is reported with its line and nothing is
written. A pyrightconfig.json that is not strict JSON is refused.
Go: makes golangci-lint enable `nolintlint` with `require-explanation: true`,
`require-specific: true` and `allow-unused: false`, in `.golangci.yml` or
`.golangci.yaml` (the first of the four golangci names; a TOML or JSON config,
or one without `version: \"2\"`, is refused). `nolintlint` is appended to
`linters.enable` (not needed under `default: all`) and the settings are added
under `linters.settings.nolintlint`, only the missing keys. `nolintlint` in
`linters.disable` or in an `exclusions.rules` entry's `linters`, or a weaker
setting, is a conflict: reported with its line, nothing written. `--check` also
runs `golangci-lint config verify` when golangci-lint is on PATH, and says so
when it is not.
TypeScript: adds `adrLint` to the `@duro-app/eslint-config` import of the first
of eslint.config.{js,mjs,ts,mts,cts} and `...adrLint({ stack: [...] })` as
the last element of its default-exported array (or the last argument of
`tseslint.config(...)`/`defineConfig(...)`); `stack` lists `react` and `effect`
when the import names them; a preset the config spreads only as
`...preset.map((c) => ({ ...c, files: X }))` (once) becomes
`{ name: \"preset\", files: X }` (X an identifier or array literal, copied as
written). `adrLint` needs @duro-app/eslint-config 5.33.0, a scoped stack
5.38.0. Edit points are found with ast-grep. `--check` also
runs the repository's `node_modules/.bin/eslint --print-config` on the .ts/.tsx
files git lists (not .d.ts; up to 20, until ESLint prints a config; it names the
file used), and reports a required setting that is not at error. A config with no
such import, another kind of default export, an aliased `adrLint` import, or a
symlink is refused, as is an eslint.config.cjs (module.exports is not
edited) or a package imported by two declarations. A @duro-app/eslint-config older than the minimum the written call needs (the installed
version, else the lowest the package.json range admits; `workspace:` passes) is
refused with exit 1 and nothing is written.

flags:
  -C <dir>    the repository (default .)
  --check     write nothing; exit 1 when a setting is missing or weaker, or a
              member does not opt in
  -h, --help  this text

exit status:
  0  in sync, synced, or nothing to do
  1  a weaker level (sync), or anything missing or weaker (--check), or a
     failing `golangci-lint config verify` (--check)
  2  usage error
  3  could not run (cargo failing, a config that is a symlink, unreadable, not
     strict JSON, a golangci-lint config that is not YAML version 2, an
     @duro-app/eslint-config range it cannot read or no version of it at all, an
     ESLint config sync will not edit such as .cjs, a failing `git ls-files`, or no
     listed file ESLint prints a config for)
";

const RULE: &str = "general.linters-police-suppressions";
const COMMENT: &str = "# adr-lint sync: general.linters-police-suppressions\n";

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

fn line_of(raw: &str, setting: Option<&Item>) -> usize {
    let start = setting.and_then(Item::span).map_or(0, |span| span.start);
    raw[..start.min(raw.len())].matches('\n').count() + 1
}

fn lookup<'a>(root: &'a Item, path: &[&str]) -> Option<&'a Item> {
    path.iter().try_fold(root, |parent, segment| {
        parent.as_table_like().and_then(|table| table.get(segment))
    })
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
        fs::remove_file(&temporary).ok(); // the temporary file is abandoned either way; the write error is reported
        return Err(failed(cause));
    }
    Ok(())
}

/// Runs the command; the exit status is the `Ok` value (0 or 1).
pub(crate) fn run(args: &[String]) -> Result<u8, Failure> {
    let options = parse_options(args)?;
    let rust_status = rust::run(&options)?;
    let python_status = python::run(&options)?;
    let go_status = go::run(&options)?;
    let typescript_status = typescript::run(&options)?;
    Ok(rust_status
        .max(python_status)
        .max(go_status)
        .max(typescript_status))
}
