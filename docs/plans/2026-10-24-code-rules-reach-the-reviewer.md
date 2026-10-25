---
canonical: decisions:docs/plans/2026-10-24-code-rules-reach-the-reviewer.md
phases: [3, 4]
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

### Verification (check)

On the release binary, run on scratch files inside this repository (aval needs its corpus), input → expected → actual:
- A Rust `#[ignore]` and a `trait ParserTrait`; a TS `interface IUser`, a bare `it.skip(...)`, and a waived `it.skip(...)` → four findings in two groups, each group ending with `aval rule <id>`, the waived skip dropped, exit 1 → as expected.
- A clean `fn clean() {}` → exit 0 → exit 0.
- `interface IOError {}`, `interface IPAddress {}`, `interface IUser {}` → only `IUser` flagged → only line 3.
- `test.skip(isWin, "not on windows")` and `test.skip("x", () => {})` → only the second flagged → only line 2.
- `o.m_x` (a foreign field) and `let m_count = 1` → only the declaration flagged → only `c.rs:2:14`.
- Findings print the rule's statement from aval → they do.
- `check missing.rs` → exit 3 naming the path → exit 3, "cannot read missing.rs".
- `ADR_LINT_AST_GREP=/nonexistent` → exit 3 with the install command → exit 3; `--rule=nope` → exit 2 → exit 2; `help check` → check's usage → it.
- `make check` (with `ast-grep test` 16/16) and `adr-lint coverage` on the live corpus → pass, exit 0 → pass, 0.

### Implementation review (check)

- approve-with-changes (9 findings, 2 blocking), then approve on the Delta.
- Fixed:
  - missing or unreadable paths exit 3;
  - a `Style` enum, not a flag argument;
  - the `I` prefix and `m_` rules no broader than names.reveal-intent;
  - a reasoned Playwright skip passes;
  - findings carry aval's statement;
  - the ast-grep version is written once;
  - tests fail under CI without ast-grep;
  - comment detection ignores string literals and other rules' waivers.
- deliberate: the `misses` text of names.reveal-intent and tests.first-properties does not yet list Python `self.m_x`, TS `this.m_x`, `IPv4Address` (still flagged) and a template-string Playwright reason. The next map edit lists them.
- carried to the amont pack rows: a deleted path in `$AMONT_FILES` would exit 3.
- 95k + 112k tokens. Code by relais runs `run-65e9dd9fc8a3c-43c1` and `run-65e9e06008c5d-7479`, both salvaged after a verification-inputs stop, verified by hand.

## Candidates (fourth PR)

`adr-lint candidates --base <ref> [--json]` prints the candidate lines a diff adds, for a reviewer to rule on (behaviour 3). Each carries a stable id, sha256 of (rule id, path, normalised line text). The patterns are ast-grep rules for the `candidate` classes in `map/rules.toml`. The map's `misses` text gets the four cases the check review listed.

### Verification (candidates)

On the release binary, in a scratch two-commit TypeScript repository carrying the decisions pack, input → expected → actual:
- The base has `keep()` and an empty `package.json` dependencies table. The change adds `run(verbose: boolean)` with `const data = 1` and a `left-pad` dependency → three candidates, all on added lines, and nothing for the unchanged `keep()` → the three, grouped by rule, each with `aval rule <id>`.
- `--json` → an array of `{id, rule, pattern, path, line, col, text}` with 64-hex ids → as expected.
- `--max 1` → the first candidate, then `… and N more in <rule>` per rule → as expected.
- `--base nope` → exit 3 naming git's error → exit 3; no `--base` → exit 2 → exit 2.
- `make check` (fixtures under `tests/rules/candidates/`) and `adr-lint coverage` on the live corpus → pass, exit 0 → pass, 0.

### Implementation review (candidates)

- approve-with-changes (6 findings), then approve-with-changes on the Delta, with every code finding resolved.
- Fixed by a relais review-fix run (`run-65e9e8de97f66-1119e`, approved after its own review):
  - `_ =>` only beside non-literal arms;
  - test files recognised by their naming conventions;
  - Go `bool` results not flagged;
  - a version bump not counted as new;
  - a Go retry bound that is a counter or a deadline.
- deliberate, for the next map edit: a removed non-dependency key such as a `[features]` entry `serde = [...]` can hide a new `serde` dependency; the counter and deadline reads in the Go retry rule are shape-based (already in `misses`).
- carried to phase 4: two identical added lines in one file share an id; whether ast-grep's paths match git's `/` paths on Windows.
- 119k + 134k tokens. The first run (`run-65e9e26ee32ac-a257`) was salvaged after its second reviewer timed out; this review stood in for it.

## Release (fifth PR)

A tag `v*` builds the four tap platforms, publishes a GitHub release with `SHA256SUMS`, and rewrites `Formula/adr-lint.rb` in fredericrous/homebrew-tap. The workflow and `scripts/bump-tap.py` are ported from fleet-lsp's. The person approved (2026-10-25) a write deploy key on the tap, `adr-lint release: tap publish`, whose private half lives only in this repository's `release` environment as `TAP_DEPLOY_KEY`, with `PUBLISH_TAP=true`, as for the sibling tools. v0.1.0 ships `coverage`, `check` and `candidates`, so phase 4 can start; the other commands follow in later releases.

### Verification (release)

Before the merge, input → expected → actual:
- `make check` with `tests/test_bump_tap.py` (rewrite the seed; refuse a missing pair, another version's checksums, a target the release did not publish; a second run writes the same bytes) → pass → pass, and the same tests pass under `PYTHONOPTIMIZE=1`.
- The seed formula in the tap → `Formula/adr-lint.rb` on fredericrous/homebrew-tap main → merged as homebrew-tap#10.

After the merge, recorded in the decisions PR that records Phase 3 (GitHub dispatches a workflow only from the default branch):
- `gh workflow run release.yaml -f tag=v0.0.9` from main → `guard` fails, no `build` job;
- a dispatch with no tag → four assets plus `SHA256SUMS`, no publish job.

### Implementation review (release)

- approve-with-changes (6 findings, none blocking), then approve on the Delta.
- Fixed:
  - the dispatch tag reaches the shell as `env`;
  - `bump-tap.py` refuses through `sys.exit`, so `-O` cannot strip its checks, and runs with `-I`;
  - an MIT LICENSE is shipped in each archive;
  - tests for idempotence and a missing target.
- 58k + 74k tokens. The relais run (`run-65ea35d9fe65c-2b38`) was salvaged after a verification-inputs stop and verified by hand.

## Formats (phase 4, first PR)

Phase 4 puts candidates in the reviewer briefs, so the formats amont-agent and relais read must be fixed first. adr-lint owns them (ADR-0044, `harness.cross-tool-reads`):
- the `candidates --json` document;
- the ruling-record JSONL that the reviewers' rulings are written in.

Both are documented with a version token. Two items carried from the candidates review land here:
- identical added lines in one file get distinct ids;
- paths are normalised to `/` on every platform.

### Verification (formats)

On the release binary, input → expected → actual:
- On a scratch repo, two identical added lines `export const data = 1;` in `b.ts`, via `candidates --json` → an `adr-lint.candidates/1` document, two candidates with distinct ids, each with a `pattern_id` → as expected; both carry `names.reveal-intent/typescript-weak-name`.
- The versioned document → `format`, `base`, `head`, `candidates` and `overflow` (`{}` when nothing was cut) → as expected.
- `ruling-record --check` on `{}` → exit 1 naming line 1 → exit 1, `line 1: missing field \`format\``.
- `ruling-record --check` on a file whose line 2 is not UTF-8 → exit 1, `line 2: not UTF-8` → by integration test.
- `make check` → pass → pass.

### Implementation review (formats)

- approve-with-changes (6 findings), then approve-with-changes on the Delta, with all six resolved.
- Fixed:
  - a stable `pattern_id` (the ast-grep rule id) in both formats;
  - readers ignore unknown fields;
  - `tool` and `ruling` are closed sets;
  - the id's stability stated, with the renumbering case;
  - blank lines, cross-file order and Unicode whitespace stated.
- deliberate, for the next adr-lint PR:
  - the page says `learn` groups by `pattern_id` before `learn` ships;
  - no test asserts `pattern_id`'s value;
  - the page does not yet say that renaming a pattern id is a breaking change.
- carried to the decisions PR: the canonical plan's id formula (line 225) gains the ordinal, and its Decision log records the ordinal, `/` paths, the versioned document and `pattern_id`, before any consumer reads them.
- 63k + 78k tokens. The relais run (`run-65ea3cc9561bd-b59b`) was accepted by its own review; I fixed its one low finding.
