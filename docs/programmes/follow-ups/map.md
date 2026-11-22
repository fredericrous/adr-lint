# Follow-ups

## Destination

Work noticed while doing something else in adr-lint is a ticket here, and each one is closed, or knowingly dropped, by a merged change.

## Notes

- Lines filed from the repository inbox at a session boundary (worktree-task Handoff).
- One pull request per ticket, unless two tickets touch the same function.

## Decisions so far

- [f01](f01-names-candidates-python-go.md): Python and Go mark weak names in every loop, unpacking and parameter position; only `i`, `j`, `k` exempt (adr-lint#63).

## Handoff

2026-11-22, boundary: f01 closed by adr-lint#63 (1e6e85a): the Python and Go `names.reveal-intent` candidates mark weak names in loop, unpacking and parameter positions, and Rust closure and pattern parameters. Released with the Rust fix (#61) in the next adr-lint release. f02 files the positions #63 left unmarked (Python `with … as`, `except … as`, walrus; Go variadic, type switch, select receive, `const`; trait and interface method signatures). Next: f02.

## Not yet specified

## Out of scope
