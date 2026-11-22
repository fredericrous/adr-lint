# Follow-ups

## Destination

Work noticed while doing something else in adr-lint is a ticket here, and each one is closed, or knowingly dropped, by a merged change.

## Notes

- Lines filed from the repository inbox at a session boundary (worktree-task Handoff).
- One pull request per ticket, unless two tickets touch the same function.

## Decisions so far

- [f01](f01-names-candidates-python-go.md): Python and Go mark weak names in every loop, unpacking and parameter position; only `i`, `j`, `k` exempt (adr-lint#63).
- [f02](f02-names-candidates-remaining-binders.md): the remaining binders are marked too: Python `with`/`except` alias and walrus, Go variadic, type switch, select `:=` receive, `const`, interface method parameters, Rust trait method signatures. The ticket's `k` example stays exempt as a loop index; the fixtures use `p`.

## Handoff

2026-11-22, boundary: f02 closed by the `fix/names-remaining-binders` PR: every binding position #63 left unmarked is now marked in Python, Go and Rust, with a valid and an invalid fixture each; it ships in the next adr-lint release (Unreleased in the CHANGELOG). No ticket is open in this programme.

## Not yet specified

## Out of scope
