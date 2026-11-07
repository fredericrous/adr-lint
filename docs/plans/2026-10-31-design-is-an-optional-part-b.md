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
