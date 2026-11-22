+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["adr-lint"]
+++
# f01 — names.reveal-intent: Python and Go exempt every loop variable

## Question

Which Python and Go binding positions should `names.reveal-intent` mark, so that only `i`, `j` and `k` stay exempt as in Rust?

## Look at

- `rules/candidates/names.reveal-intent/python.yml`: the rule's `inside` clauses cover assignment targets and function parameters only; `for` loop variables and tuple-unpacking targets are never marked.
- The Go variant of the same candidate: check it exempts loop variables the same way.
- Model the change on the Rust fix in adr-lint#61 (`rules/candidates/names.reveal-intent/rust.yml`, `CHANGELOG.md` Unreleased entry): only `i`, `j`, `k` stay unmarked, and the fixtures in `tests/rules/candidates/names.reveal-intent/` gain invalid cases for `for p in paths` and `for (key, v) in map`.

## Answer looks like

- `ast-grep test --skip-snapshot-tests` passes with the new Python and Go fixtures.
- A Python `for p in paths:` and `for key, v in items:` produce a candidate; `for i in range(n):` does not.
- The CHANGELOG Unreleased section names the change.

## Resolution

adr-lint#63 (1e6e85a), 2026-11-22. Gaps it left are f02.
