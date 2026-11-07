---
canonical: fredericrous/decisions docs/plans/2026-10-31-design-is-an-optional-part.md
phases: [1]
status: active
branch: feat/conventions-presets-v2
---
# Design is an optional part: adr-lint #A (pointer)

This repository's slice of the ADR-0048 plan is section 1, "Conventions move
to `@aval-adr/eslint-config`": the `base`, `effect` and `tests` presets, the
decided bans under `adr-lint/no-restricted-imports`, the sync safety cap
(`>= 5.33, < 6`, both gates) and release **0.16.0** (the canonical body says
0.12.0; its Decision log renumbers it). Section 6 (#B, 0.17.0) is a later PR.

## Decision log

- 2026-11-07 — the canonical plan asks for a `resolved_problems` check that
  `adr-lint/no-restricted-imports` resolves to error "when decided bans
  exist". Not applicable here: sync never reads or writes `restrictedImports`
  (it appends `...adrLint(...)` only), so it cannot know that bans exist, and
  without them the rule is not configured at all. It is left to whichever
  change first makes sync write decided bans.
- The `base`, `effect` and `tests` presets were moved verbatim from Duro 5.45
  (single quotes, no semicolons), so the moved block keeps Duro's style
  beside this file's own. The package has no formatter config; a later
  reformat is cosmetic.

## Implementation review

- Round 1: approve-with-changes (77k, 82 s). Fixed: the brew upgrade line, holds-until on the entry-file guess. Not applicable: the level check (see Decision log). Deliberate: verbatim Duro style.
- Delta: approve (33k, 31 s). Kept: the holds-until names "#B" (low); it is this plan's next adr-lint PR, release 0.17.0.
- Verification: `make check` green on a8259a4 (109 s); `cargo test --test sync_typescript` green on e7ba68a. Next: release 0.16.0, then Duro v6.
