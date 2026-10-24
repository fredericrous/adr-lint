---
canonical: decisions:docs/plans/2026-10-24-code-rules-reach-the-reviewer.md
phases: [3]
status: active
---
# Code rules reach the reviewer (pointer)

Canonical plan: [decisions/docs/plans/2026-10-24-code-rules-reach-the-reviewer.md](https://github.com/fredericrous/decisions/blob/main/docs/plans/2026-10-24-code-rules-reach-the-reviewer.md) (ADR-0044).

This repository is adr-lint, which Phase 3 creates. The person moved it from Forgejo to GitHub, and its eslint preset from the Forgejo registry to public npm as `eslint-config-adr-lint` (2026-10-24). The reason: Forgejo needs a client certificate that GitHub CI and public npm consumers of `@duro-app/eslint-config` do not have.

This first PR is the scaffold: toolchain pin, the fleet decisions pack, the gate, CI and a CLI that answers `--help` and `--version`. The commands come in the PRs that follow.
