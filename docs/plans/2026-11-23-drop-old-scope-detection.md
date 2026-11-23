---
canonical: greenrack:docs/plans/2026-11-23-drop-old-scope-detection.md
phases: [3]
status: active
---

# Drop the old design-system scope from detection (pointer)

Canonical plan: [greenrack/docs/plans/2026-11-23-drop-old-scope-detection.md](https://github.com/greenharness/greenrack/blob/main/docs/plans/2026-11-23-drop-old-scope-detection.md) (greenrack-rename t13).

This repository carries Phase 3: `adr-lint sync` stops naming the retired `@duro-app/eslint-config` as a refused source. It ships as a minor release.

Amended in review: the rule refuses an `adrLint` import from any package other than `@aval-adr/eslint-config` (see the Decision log), where the canonical plan said only the greenrack config.

## Code review

- adr-lint: backend, lang:rust (unix, tui left out: no output change)

## Decision log

- 2026-11-23 — Phase 3 is amended. The canonical plan said `DS_PACKAGES` names the greenrack eslint-config only. Three reviewers showed that this makes the sync accept a config that imports `adrLint` from `@duro-app/eslint-config` (it exported `adrLint` from 5.33.0) and write a second `import {adrLint}`: two bindings, a SyntaxError at load. The rule now refuses an `adrLint` specifier from any source that is not `@aval-adr/eslint-config`; `DS_PACKAGES` is gone. The canonical plan's Phase 3 text is corrected in its closing docs PR.

## Verification

Observed 2026-11-23 on adr-lint `270e8d8`:

- `cargo test --test sync_typescript`: passes. The retired-scope import and a `./local-lint.js` import are each refused (exit 1, file unchanged); the greenrack import is still refused; a `@duro-app/eslint-config` import without `adrLint` is left alone and synced.
- The pre-commit gate (clippy, cargo-test) passed on each commit.

## Implementation review

- implementation-review, backend, lang:rust: approve-with-changes in round 1, all three on the same defect (a second `adrLint` binding); fixed in `270e8d8` and re-reviewed as a delta.
