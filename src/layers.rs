//! `adr-lint layers`: shows the `[architecture]` contract of `adr-lint.toml`
//! (ADR-0056) and changes nothing. `summary` is at most four short lines
//! (`adr-lint.layers-summary/1`, docs/formats.md); `explain` is the whole
//! contract, or what it says about one file.

use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

use adr_lint::config;
use adr_lint::contract::{Contract, Entry, OnlyIn};

use crate::check;
use crate::coverage::Failure;
use crate::graph;

pub(crate) const USAGE: &str = "\
usage: adr-lint layers summary [-C <dir>] [--no-color]
       adr-lint layers explain [-C <dir>] [--no-color] [PATH]

Shows the `[architecture]` contract of adr-lint.toml (ADR-0056). Read-only.

  summary  at most four lines: the layers inner first, the first `only_in`
           entry and where the detail is; nothing when there is no contract
  explain  the whole contract; with a PATH, that file's layer, the layers it
           may import, the `only_in` entries that apply to it and its open
           findings, as `adr-lint check PATH` prints them

flags:
  -C <dir>      read adr-lint.toml in <dir> (default .)
  --no-color    no colour (also off when stdout is not a terminal, NO_COLOR is
                set, or TERM=dumb); `summary` never prints colour
  -h, --help    this text

exit status:
  0  printed (explain: findings included), or no `[architecture]`
  2  usage error, or a PATH outside the tree
  3  could not run (the contract is unreadable or invalid, a tool is missing,
     the -C directory cannot be entered, or, with a PATH, ast-grep is missing)
";

/// Every summary line is at most this many columns.
const WIDTH: usize = 80;
const LAYERS_PREFIX: &str = "  layers   ";
const ONLY_IN_PREFIX: &str = "  only in  ";
/// The most columns the `only_in` imports take on line 3, so the paths keep
/// a share of it.
const IMPORTS_BUDGET: usize = 40;

#[derive(Debug, PartialEq, Eq)]
enum Subcommand {
    Summary,
    Explain,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    subcommand: Subcommand,
    directory: Option<String>,
    no_color: bool,
    path: Option<String>,
}

fn parse_options(args: &[String]) -> Result<Options, Failure> {
    let subcommand = match args.first().map(String::as_str) {
        Some("summary") => Subcommand::Summary,
        Some("explain") => Subcommand::Explain,
        Some(other) => {
            return Err(Failure::Usage(format!(
                "unknown subcommand `{other}` (summary, explain)"
            )))
        }
        None => return Err(Failure::Usage("needs a subcommand".to_string())),
    };
    let mut options = Options {
        subcommand,
        directory: None,
        no_color: false,
        path: None,
    };
    let mut remaining = args[1..].iter();
    let mut operands_only = false;
    while let Some(arg) = remaining.next() {
        if operands_only || arg == "-" || !arg.starts_with('-') {
            if options.path.replace(arg.clone()).is_some() {
                return Err(Failure::Usage("takes at most one PATH".to_string()));
            }
        } else if arg == "--" {
            operands_only = true;
        } else if arg == "--no-color" {
            options.no_color = true;
        } else if arg == "-C" {
            let directory = remaining
                .next()
                .ok_or_else(|| Failure::Usage("`-C` needs a directory".to_string()))?;
            options.directory = Some(directory.clone());
        } else if let Some(attached) = arg.strip_prefix("-C") {
            options.directory = Some(attached.to_string());
        } else {
            return Err(Failure::Usage(format!("unknown flag `{arg}`")));
        }
    }
    if options.subcommand == Subcommand::Summary && options.path.is_some() {
        return Err(Failure::Usage("`summary` takes no PATH".to_string()));
    }
    Ok(options)
}

pub(crate) fn run(args: &[String]) -> Result<(), Failure> {
    let options = parse_options(args)?;
    if let Some(directory) = &options.directory {
        std::env::set_current_dir(directory)
            .map_err(|cause| Failure::CouldNotRun(format!("cannot enter {directory}: {cause}")))?;
    }
    let contract = config::validate(Path::new("."))
        .map_err(|cause| Failure::CouldNotRun(cause.to_string()))?;
    let Some(contract) = contract else {
        return Ok(());
    };
    let text = match options.subcommand {
        Subcommand::Summary => summary(&contract),
        Subcommand::Explain => explain(&contract, &options)?,
    };
    match std::io::stdout().write_all(text.as_bytes()) {
        Ok(()) => Ok(()),
        Err(cause) if cause.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(cause) => Err(Failure::CouldNotRun(format!(
            "cannot write to stdout: {cause}"
        ))),
    }
}

/// `items` joined by `separator` in at most `budget` columns; a list cut to
/// fit ends with `, +N more`.
fn fit(items: &[String], separator: &str, budget: usize) -> String {
    let total = items.len();
    for shown in (0..=total).rev() {
        let mut text = items[..shown].join(separator);
        if shown < total {
            if shown > 0 {
                text.push_str(", ");
            }
            text.push_str(&format!("+{} more", total - shown));
        }
        if text.chars().count() <= budget {
            return text;
        }
    }
    // Not even `+N more` fits: the budget is below its own width.
    String::new()
}

fn texts(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.text.clone()).collect()
}

fn locations(list: &OnlyIn) -> Vec<String> {
    let mut places = texts(&list.paths);
    places.extend(texts(&list.modules));
    places
}

fn only_in_line(list: &OnlyIn, width: usize) -> String {
    let imports = fit(&texts(&list.imports), ", ", IMPORTS_BUDGET);
    let taken = ONLY_IN_PREFIX.chars().count() + imports.chars().count() + ": ".len();
    let places = fit(&locations(list), ", ", width.saturating_sub(taken));
    format!("{ONLY_IN_PREFIX}{imports}: {places}")
}

fn summary(contract: &Contract) -> String {
    let names: Vec<String> = contract
        .layers
        .iter()
        .map(|layer| layer.name.clone())
        .collect();
    let mut text = String::from("architecture (adr-lint.toml, ADR-0056):\n");
    text.push_str(&format!(
        "{LAYERS_PREFIX}{}\n",
        fit(&names, " -> ", WIDTH - LAYERS_PREFIX.len())
    ));
    if let Some(first) = contract.only_in.first() {
        text.push_str(&only_in_line(first, WIDTH));
        text.push('\n');
    }
    text.push_str("  detail   adr-lint layers explain [PATH]\n");
    text
}

fn listed(label: &str, entries: &[Entry]) -> Option<String> {
    (!entries.is_empty()).then(|| format!("    {label:<8} {}\n", texts(entries).join(", ")))
}

fn contract_text(contract: &Contract) -> String {
    let mut text = String::from("architecture (adr-lint.toml, ADR-0056):\n");
    text.push_str("  layers, inner first\n");
    for (index, layer) in contract.layers.iter().enumerate() {
        text.push_str(&format!(
            "  {}. {} (adr-lint.toml:{})\n",
            index + 1,
            layer.name,
            layer.line
        ));
        text.extend(listed("paths", &layer.paths));
        text.extend(listed("modules", &layer.modules));
    }
    for list in &contract.only_in {
        text.push_str(&format!("  only in (adr-lint.toml:{})\n", list.line));
        text.extend(listed("imports", &list.imports));
        text.extend(listed("paths", &list.paths));
        text.extend(listed("modules", &list.modules));
    }
    if !contract.ambient.paths.is_empty() || !contract.ambient.modules.is_empty() {
        text.push_str("  ambient reads allowed in\n");
        text.extend(listed("paths", &contract.ambient.paths));
        text.extend(listed("modules", &contract.ambient.modules));
    }
    text.push_str(&format!(
        "  cycles   {}\n",
        if contract.check_cycles { "on" } else { "off" }
    ));
    text
}

/// `path` as a `/`-separated path under the current directory, or a usage
/// failure when it leads outside.
fn inside_tree(path: &str) -> Result<String, Failure> {
    let outside = || Failure::Usage(format!("`{path}` is outside the tree"));
    let here = std::env::current_dir().map_err(|cause| {
        Failure::CouldNotRun(format!("cannot read the current directory: {cause}"))
    })?;
    let given = Path::new(path);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        here.join(given)
    };
    let resolved = std::fs::canonicalize(&joined).unwrap_or_else(|_| lexical(&joined));
    let root = std::fs::canonicalize(&here).unwrap_or(here);
    let relative = resolved.strip_prefix(&root).map_err(|_| outside())?;
    let text = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    if text.is_empty() {
        return Err(outside());
    }
    Ok(text)
}

/// `path` with `.` and `..` folded away, without touching the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded
}

fn explain(contract: &Contract, options: &Options) -> Result<String, Failure> {
    let mut text = contract_text(contract);
    let Some(path) = &options.path else {
        return Ok(text);
    };
    let file = inside_tree(path)?;
    let ast_grep = std::env::var(check::AST_GREP_VARIABLE)
        .unwrap_or_else(|_| check::DEFAULT_AST_GREP.to_string());
    check::check_version(&ast_grep)?;
    let site = graph::site_of_file(&ast_grep, contract, &file)?;
    text.push_str(&format!("\nfile     {file}\n"));
    match site.layer {
        Some(index) => {
            let names: Vec<&str> = contract.layers[..=index]
                .iter()
                .map(|layer| layer.name.as_str())
                .collect();
            text.push_str(&format!(
                "layer    {} ({} of {})\n",
                contract.layers[index].name,
                index + 1,
                contract.layers.len()
            ));
            text.push_str(&format!("may import  {}\n", names.join(", ")));
        }
        None => text.push_str("layer    none (no layer claims this file)\n"),
    }
    let module = site.module.as_deref().unwrap_or("");
    for list in contract
        .only_in
        .iter()
        .filter(|list| list.allows(module, &file))
    {
        text.push_str(&format!(
            "only in  {}: {} (adr-lint.toml:{})\n",
            texts(&list.imports).join(", "),
            locations(list).join(", "),
            list.line
        ));
    }
    let style = if options.no_color {
        check::Style::Plain
    } else {
        check::Style::Ansi
    };
    let findings = check::contract_report_of(std::slice::from_ref(&file), style)?;
    if findings.is_empty() {
        text.push_str("findings none\n");
    } else {
        text.push_str("findings\n");
        text.push_str(&findings);
    }
    Ok(text)
}
