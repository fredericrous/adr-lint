# Full reviews: adr-lint becomes greenspotter

Panel 2026-11-24, every seat on opus; Codex last. Dispositions as recorded
with `greenbelay plan-review record`.

## Round 1 (body 4afd85599801, 2437 words)

- **backend**: approve-with-changes (55k, 97 s).
  - #1: accepted (`adds=ops`).
  - #2: rejected (t06 records the person's hard rename).
  - #3: rejected (this repository's `adr-lint.toml` holds only `[[cli.programs]]`).
  - #4, #5, #6, #8: accepted (homebrew-tap; sync row; no hint on `ruling-record`; `FETCH_HEAD`).
  - #7: accepted, merged with po#1.
  - #9: rejected (counts are computed).
- **lang:rust**: approve-with-changes (41k, 59 s).
  - #1: accepted (the fallback reaches every use of `config::FILE`).
  - #2: accepted (the note is returned as data and printed once).
  - #3: accepted, via unix#1 (`rename.sed` excluded).
  - #4: accepted (`concat!`).
- **lang:typescript**: approve-with-changes (45k, 65 s).
  - #1: accepted (npm token scope).
  - #2: accepted (`'aval-adr/` preset names).
  - #3: accepted (`.d.ts` type-check).
  - #4: accepted (package-lock).
  - #5: accepted, via rust#4.
  - #6: accepted (t10 moves the package and the comments in one commit).
- **unix**: approve-with-changes (44k, 64 s).
  - #1: accepted (`rename.sed` excluded in both places).
  - #2: accepted, via architect#2.
  - #3: accepted (the sync USAGE prose).
- **tui**: approve-with-changes (38k, 44 s).
  - #1: rejected (`learn` skips old rows instead).
  - #2: accepted (the both-files warning).
  - #3: accepted (`path:line` first).
- **po**: approve-with-changes (34k, 30 s).
  - #1: accepted (watch query and its end).
  - #2: accepted (owner and date for the reader tickets).
  - #3: rejected (without the refusal, `sync` adds a second preset).
- **architect**: approve-with-changes (44k, 75 s).
  - #1: accepted, blocking (`greengate.conf` excluded from the sed).
  - #2: accepted (the fallback's full reach).
  - #3: accepted (`learn` skips and counts old rows).

## Round 2 (body 3f37bb3e2e50, 2865 words)

- **lang:rust, architect, tui, po, unix**: approve-with-changes (28k–39k, 11–24 s each). All resolved their round-1 findings.
  - All five found the same thing: the old-record hint was still named in the hand edits and the allowlist. Accepted as one edit (unix's three spots).
  - Carried into implementation: rust#2 (bare `adrLint` guard), rust#3 (assert the skip line), tui#2 (exact skip wording), tui#3 (refusal width), tui#4 (`NO_COLOR`), po#2 (ticket owner and date).
  - unix#2: accepted (`gh run list` shows no logs).
- **lang:typescript**: approve-with-changes (28k, 12 s).
  - #1: rejected (only single-quoted `'aval-adr/` exists).
  - #2: carried (a cargo test for the refusal).
- **platform** (new seat, full review): approve-with-changes (51k, 72 s).
  - #1: accepted (tarball name and `@greenharness` error text).
  - #2: accepted (`--env release` secrets and deployment policy).
  - #3: accepted (`bump-tap` dry run on the seed).
- **backend**: approve-with-changes (63k, 96 s).
  - #1: rejected (seed before the tag, as greentopo did; platform#3 checks it).
  - #2: partly accepted (allowlist).
  - #3: accepted (daily probe and 7-day count).
  - #4: accepted (rollback after the tag).
  - #5: carried.
- **backend, on the final body** (3a3cc527f33a): approve-with-changes (40k, 37 s). Carried into implementation:
  - #1: show the npm scope with `npm access list packages @greenharness`;
  - #2: the seed cannot be installed until the tag;
  - #3: after the tag, fix forward rather than transfer back;
  - #4: a duplicate bullet in Behaviour 2.

## Codex (final, body 3a3cc527f33a)

- approve-with-changes.
  - #1, high: accepted. The ruling validator accepts `greenbelay` and `relais`, not `amont-agent` (`ruling.rs:42`).
  - #2, high: accepted. The `adrLint` preflight runs before any `sync` writer (`sync.rs:159`).
  - #3: rejected. The `prog/` prefix is in use: 22 branches on the decisions remote.
- **backend re-run** (94498b3b8c21): approve (25k, 26 s). Both Codex edits are resolved.
