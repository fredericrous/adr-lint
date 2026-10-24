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
`$AMONT_FILES`, else the staged files. It needs exactly the ast-grep version in `AST_GREP_VERSION`
(`ADR_LINT_AST_GREP` names the program). A finding is waived with
`adr-lint: allow <rule-id> -- <reason>` on its line or the line above. Exit
0 none, 1 findings, 2 usage, 3 could not run. `adr-lint help <command>`
prints a command's usage.
