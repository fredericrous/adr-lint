# adr-lint
Checks a tree against the decided code rules a linter matches exactly; marks candidate lines for reviewers (ADR-0044)

## Commands

`adr-lint coverage [-C <dir>] [--map <path>] [--json]` checks every active
governs:code constraint (from `aval --json rules --level constraint --governs
code --full`) against `map/rules.toml`; `ADR_LINT_AVAL` names the aval program.
Exit 0 none, 1 problems, 2 usage, 3 could not run.

`adr-lint check [--rule <id>] [--json] [--no-color] [--] [PATH…]` runs the
exact ast-grep rules embedded in the binary (`rules/<rule-id>/<lang>.yml`,
fixtures in `tests/rules`, `make rules`) over the named files, else
`$AMONT_FILES`, else the staged files (a path named in argv or
`$AMONT_FILES` that git lists as deleted is skipped). `--json` findings are `{path, line, col, rule, message, detail, text}`,
`text` being the source line with whitespace collapsed. It needs exactly the ast-grep version in `AST_GREP_VERSION`
(`ADR_LINT_AST_GREP` names the program). A finding is waived with
`adr-lint: allow <rule-id> -- <reason>` on its line or the line above. Exit
0 none, 1 findings, 2 usage, 3 could not run. `adr-lint help <command>`
prints a command's usage.

`adr-lint candidates --base <ref> [--json] [--rule <id>] [--max <n>]` runs the
candidate ast-grep rules (`rules/candidates/<rule-id>/<lang>.yml`, apart from
the exact rules so `check` never reports one) over the files changed since the
merge-base of HEAD and `<ref>` and prints only the matches on added lines, each
with a stable id, for a reviewer to rule on. Candidates are not findings: exit
0 either way, 2 usage, 3 could not run. `--json` prints one versioned
`adr-lint.candidates/1` document.

A candidate rule may carry `metadata: commentExempt: same-line`, as the exact
rules do: `candidates` then drops a match with a comment on the same line after
it (`//` and `/* */`, or `#` in Python), by the same function `check` uses. The
`errors.never-swallowed` candidates use it: a Rust `let _ = <call>;` and a
`<expr>.ok();` statement, a Go `_ = <call>` or `_ = err`, and a Python
`except …: pass`, each listed only when no comment follows it on its line.

Generated files are skipped by both commands: a file is generated when, among its first 15 lines, a comment line contains `@generated`, or contains both `generated` and `DO NOT EDIT` (case-insensitive).

`adr-lint ruling-record --check <file>` validates a file of reviewers' ruling
records (`adr-lint.ruling/1`, JSON Lines): exit 0 valid, 1 the first bad line
named, 2 usage, 3 file unreadable. Both formats are documented in
[docs/formats.md](docs/formats.md).

## Install

```sh
brew install fredericrous/tap/adr-lint
```

Or download `adr-lint-<version>-<target>.tar.gz` from the
[releases](https://github.com/fredericrous/adr-lint/releases) (targets:
`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`), check it against `SHA256SUMS`, and put the
`adr-lint` binary on your `PATH`.

`adr-lint check` and `candidates` need ast-grep at the version in
`AST_GREP_VERSION` (`brew install ast-grep`, or its releases page).

## ESLint preset

`npm/eslint-config/` is the npm package `@aval-adr/eslint-config`: `adrLint()`
and `adrLintTyped()` flat-config factories carrying the TypeScript lint settings
that match the decided rules exactly. It is versioned with this crate and
published on each `v*` tag; see its README.

## Releasing

Bump `version` in `Cargo.toml` and add a `## v<version>` section to
`CHANGELOG.md` in a pull request, and merge it. Then push a `v<version>` tag:
the release workflow checks the tag against `Cargo.toml`, builds the four
targets, publishes the GitHub release with `SHA256SUMS`, and, when the
`PUBLISH_TAP` variable is `true`, rewrites `Formula/adr-lint.rb` in
`fredericrous/homebrew-tap` with `scripts/bump-tap.py` and checks that brew
installs it. The seed formula is `packaging/homebrew/adr-lint.rb`.
