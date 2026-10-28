//! `adr-lint sync`: writes the settings that make a linter refuse the
//! suppressions general.linters-police-suppressions forbids. Rust and Python
//! are synced; their halves are in `rust` and `python`.

mod python;
mod rust;

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

flags:
  -C <dir>    the repository (default .)
  --check     write nothing; exit 1 when a setting is missing or weaker, or a
              member does not opt in
  -h, --help  this text

exit status:
  0  in sync, synced, or nothing to do
  1  a weaker level (sync), or anything missing or weaker (--check)
  2  usage error
  3  could not run (cargo failing, a config that is a symlink, unreadable or not
     strict JSON)
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
    Ok(rust_status.max(python_status))
}
