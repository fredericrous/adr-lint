---
canonical: decisions:docs/plans/2026-10-24-code-rules-reach-the-reviewer.md
phases: [3]
status: active
---
# Code rules reach the reviewer (pointer)

Canonical plan: [decisions/docs/plans/2026-10-24-code-rules-reach-the-reviewer.md](https://github.com/fredericrous/decisions/blob/main/docs/plans/2026-10-24-code-rules-reach-the-reviewer.md) (ADR-0044).

This repository is adr-lint, which Phase 3 creates. The person moved it from Forgejo to GitHub, and its eslint preset from the Forgejo registry to public npm as `eslint-config-adr-lint` (2026-10-24). The reason: Forgejo needs a client certificate that GitHub CI and public npm consumers of `@duro-app/eslint-config` do not have.

This first PR is the scaffold: toolchain pin, the fleet decisions pack, the gate, CI and a CLI that answers `--help` and `--version`. The commands come in the PRs that follow.

## Verification

From the release binary (`cargo build --release`), input → expected → actual:
- `adr-lint --bogus --help` → exit 0, usage on stdout → exit 0, `usage: adr-lint <command> [flags]`.
- `adr-lint --bogus --version` and `adr-lint check -V` → exit 0, `adr-lint 0.1.0` → both exit 0, `adr-lint 0.1.0`.
- `adr-lint frobnicate` → exit 2, empty stdout, "unknown command" → exit 2, `adr-lint: unknown command \`frobnicate\``.
- `adr-lint --bogus` → exit 2, "unknown flag" → exit 2, `adr-lint: unknown flag \`--bogus\``.
- `make check`, `aval check`, `aval traits --check` → pass → pass; 44 records, no findings.

## Implementation review

- approve-with-changes, then approve-with-changes on the Delta. Fixed: version honoured anywhere on the line, CI runs `make check` on Linux for a pull request and adds macOS on main (ADR-0041), a `holds-until:` on the unmeasured floor, unknown flag versus command.
- deliberate: `Cargo.toml` says the floor builds on any 2021-edition toolchain, which `rust-version = "1.85"` contradicts; the first dependency PR measures the floor and rewrites that comment.
- 66k + 72k tokens.

## Coverage (second PR)

`map/rules.toml` holds the mechanism for each `exact` and `candidate` class a rule body's `**Checked by:**` line names. It is TOML, not the YAML the canonical plan names: the Rust YAML crate is deprecated, and TOML needs no new kind of parser. `adr-lint coverage` checks the map against `aval rules --governs code --full --json` (behaviour 6).

### Verification (coverage)

On the release binary, input → expected → actual:
- `adr-lint coverage` on the live corpus → exit 0 → exit 0; `--json` → `[]` → `[]`.
- `ADR_LINT_AVAL=/nonexistent adr-lint coverage` → exit 3, one stderr line → exit 3, `cannot run \`/nonexistent\``.
- `adr-lint coverage --bogus --help` → exit 0, coverage's usage → exit 0.
- `make check` → pass → pass; `make msrv` → `cargo check --locked` on 1.85.0 → pass.

Behaviour 6, by test (`tests/coverage.rs` runs the binary against a fake aval's fixture corpus; the others are unit tests of the pure functions):
- a constraint with no Checked-by line → exit 1 naming it (binary test);
- an exact class with no mechanism → exit 1 (binary test);
- a map id that is not an active rule → exit 1 (binary test);
- an unknown class → a problem (unit test, `checked_by`);
- a rule listing exact and candidate, with its candidate mechanism removed → a problem (unit test, `problems_of_a_named_table`).

A grandchild holding aval's stdout open → the read gives up at the deadline → unit test `a_grandchild_holding_stdout_open_cannot_outlast_the_deadline`, an error in under 3 s with a 300 ms deadline.

### Implementation review (coverage)

- approve-with-changes, then approve-with-changes on the Delta. Fixed: pipe reads bounded by the deadline, map text no broader than the bodies, the override `holds-until:` in the map, `make msrv` in CI with the version read from Cargo.toml, wording.
- deliberate, fixed in the `check` PR: `make msrv` exits 0 on SKIPPED even under CI; the aval unit test leaves its scratch dir and a `sleep 5`; a sub-second deadline prints "within 0 s".
- 71k + 88k tokens.

## Check (third PR)

`adr-lint check` runs the exact ast-grep rules of behaviour 2. It also brings in what the coverage review carried:
- `make msrv` fails under CI when it would skip;
- the aval test removes its scratch dir;
- a sub-second deadline reads in milliseconds;
- the ADR job runs `adr-lint coverage` on the live corpus;
- a check that map text does not drift from the rule bodies.
