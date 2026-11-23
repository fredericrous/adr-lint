---
canonical: greenrack:docs/plans/2026-11-23-drop-old-scope-detection.md
phases: [3]
status: active
---

# Drop the old design-system scope from detection (pointer)

Canonical plan: [greenrack/docs/plans/2026-11-23-drop-old-scope-detection.md](https://github.com/greenharness/greenrack/blob/main/docs/plans/2026-11-23-drop-old-scope-detection.md) (greenrack-rename t13).

This repository carries Phase 3: `adr-lint sync` stops refusing an `adrLint` import from the retired `@duro-app/eslint-config`; only `@greenharness/greenrack-eslint-config` is refused. It ships as a minor release.

## Code review

- adr-lint: backend, lang:rust (unix, tui left out: no output change)
