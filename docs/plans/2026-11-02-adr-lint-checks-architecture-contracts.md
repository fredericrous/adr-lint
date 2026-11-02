---
canonical: decisions:docs/plans/2026-11-02-adr-lint-checks-architecture-contracts.md
phases: [1, 2, 3, 4, 4b, 4c, 5, 6]
status: active
---
# adr-lint checks a repository's architecture contracts (pointer)

Canonical plan: [decisions/docs/plans/2026-11-02-adr-lint-checks-architecture-contracts.md](https://github.com/fredericrous/decisions/blob/main/docs/plans/2026-11-02-adr-lint-checks-architecture-contracts.md) (ADR-0056).

This repository carries every phase after Phase 0. The phases are: the `[architecture]` contract, the Rust, TypeScript, Python and Go import graphs reported as `adr-lint check` findings, `layers summary` and `layers explain`, the ambient-read candidates, and the 0.15.0 release.

## Implementation review

Approve-with-changes in round 1 and approve-with-changes in the delta. implementation-review and the backend, lang:rust, lang:typescript, lang:python, lang:go and unix profiles: the profiles gave backend, Python and unix approve, and Rust, TypeScript and Go approve-with-changes, with nothing blocking in the delta.
Fixed in 12f909c, aa8db57 and f500098: output parameters, the flag argument, placeholder names, fail-closed config reads, rename widening, the two-layer error in `check`, Go directories, ambient patterns, TS resolution.
deliberate: the medium and low items carried into adr-lint#42 (go.mod under testdata, with none in the fleet; TS flags to variants; a Rust sort clone; Go alias docs; git locale; ambient module maps; explain of a missing file exits 3).
Observed verification and phase ticks: the canonical plan in decisions, updated by the closing decisions PR before this merges.
