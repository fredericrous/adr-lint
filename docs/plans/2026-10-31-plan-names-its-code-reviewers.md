---
canonical: decisions:docs/plans/2026-10-31-plan-names-its-code-reviewers.md
phases: [1b]
status: active
---
# The plan names its code reviewers (pointer)

Canonical plan: [decisions/docs/plans/2026-10-31-plan-names-its-code-reviewers.md](https://github.com/fredericrous/decisions/blob/main/docs/plans/2026-10-31-plan-names-its-code-reviewers.md) (ADR-0052).

This repository carries Phase 1b: `adr-lint coverage` checks the `**Reviewed by:**` line of each judge rule.

## Implementation review

- Phase 1b: approve-with-changes, then approve on the delta (62k + 60k tokens, 58 s + 13 s). Fixed: the empty-entry message, and the four `coverage` cases recorded.
- Deliberate: the release goes in its own `chore(release)` PR, as this repository does. The optional test of the `unix,, tui` message is skipped; the variant is already tested.
- Verification, on a scratch copy of the corpus: all present → rc 0. Line removed, `nobody`, or a line on the exact rule `architecture.dependency-rule` → rc 1, each naming the rule.
