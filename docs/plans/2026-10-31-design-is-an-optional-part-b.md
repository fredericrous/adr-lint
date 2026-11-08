---
canonical: fredericrous/decisions docs/plans/2026-10-31-design-is-an-optional-part.md
phases: [6]
status: active
branch: feat/sync-own-package
---
# Design is an optional part: adr-lint #B (pointer)

This repository's second slice of the ADR-0048 plan is section 6:

- the TypeScript sync targets `@aval-adr/eslint-config` (at least 0.16.0;
  the canonical body says 0.12, renumbered by its Decision log);
- it adds `adrLint` to an import from that package, or inserts one;
- it appends `...adrLint()` with no `stack`;
- it no longer reads or edits a `@duro-app/eslint-config` import, so both
  Duro gates and the 0.16 cap go;
- the release is **0.17.0**.

## Implementation review

- Round 1: approve-with-changes (52k, 24 s). Fixed: the stale `5_33` test names, and `stack` is now matched as a property key.
- Delta: approve (56k, 9 s). Deliberate: a quoted `"stack":` key is not refused, because that form is unusual in a flat config.
- Verification:
  - `make check` passes on b5db705, and `cargo test --test sync_typescript` passes on 2272faa.
  - Scratch repo with only `@aval-adr/eslint-config ^0.16.0` installed: `sync` writes `import {adrLint} from "@aval-adr/eslint-config"` and `...adrLint()` and exits 0; `sync --check` exits 0; `eslint --print-config` shows `adr-lint/no-swallowed-catch` and `ban-ts-comment` at error (no decided bans, so `adr-lint/no-restricted-imports` is unconfigured).
  - Cosmetic: the inserted import has no semicolon in a config that uses them.
- Next: release 0.17.0.
