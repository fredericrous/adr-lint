# adr-lint
Checks a tree against the decided code rules a linter matches exactly; marks candidate lines for reviewers (ADR-0044)

## Commands

`adr-lint coverage [-C <dir>] [--map <path>] [--json]` checks every active
governs:code constraint (from `aval --json rules --level constraint --governs
code --full`) against `map/rules.toml`; `ADR_LINT_AVAL` names the aval program.
Exit 0 none, 1 problems, 2 usage, 3 could not run.
