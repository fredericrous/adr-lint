---
canonical: decisions:docs/plans/2026-10-24-code-rules-reach-the-reviewer.md
phases: [3, 4, 5, 6, 7]
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

## Candidate ids per match, and test code (phase 4, second adr-lint PR)

Running amont-agent's brief on its own branch showed two problems:
- two matches on one line (two `unwrap()` calls) got the same id, because the ordinal counted lines and not matches;
- `failures.expected-vs-defect` filled 35 of the 40 slots with `unwrap`s in test code, where an `unwrap` is the test's own assertion.

The id now also hashes the match's column. The candidate rule leaves out test files and `#[cfg(test)]` modules, which narrows it; ADR-0044 forbids only broadening a rule.

### Verification (ids per match)

On the release binary, in a scratch repository, input → expected → actual:
- A line with two `.unwrap()` calls, `a.unwrap() + b.unwrap()` → two candidates with distinct ids → distinct, at cols 44 and 57; the single-letter parameters `a` and `b` are separate names.reveal-intent candidates (cols 6 and 21).
- An `.unwrap()` inside `#[cfg(test)] mod tests { … }` → no candidate → none.
- `make check`, with a pattern-id test that runs the manifest reader on a scratch Cargo.toml → pass → pass.

### Implementation review (ids per match)

- approve-with-changes (4 findings), then approve-with-changes on the Delta.
- Fixed:
  - the CHANGELOG names the wider tests.no-real-waits (`tests.rs`);
  - the manifest pattern-id test runs the reader.
- deliberate, for the next adr-lint PR: the test comment says "line 3" for the added line 4.
- carried to the decisions PR: plan line 225 gains the column, and a 2026-10-25 Decision-log entry records the column and the test-code exclusion.
- 53k + 58k tokens. The relais run (`run-65ea4ff942b98-7aff`) was approved after its own review; I fixed its two low findings.

## Phase 4: `candidates --head`

relais records a candidate with `git commit-tree` and never moves HEAD, so `candidates` gains `--head <rev>`. It measures `<merge-base of rev and --base>...<rev>` and reads the changed files from `<rev>`'s objects, through one `git cat-file --batch`.

On the release binary, input → expected → actual:
- A scratch repo with HEAD at base, a dirty working tree and a commit-tree candidate, `--head <candidate>` → the same ids as the run with that commit checked out → `610663331e19`, `6998f43dd19b`, equal.
- `--head nope` → exit 3 naming the rev → exit 3, `cannot resolve --head \`nope\``.
- A 200-file commit-tree candidate, `--head --max 40` → under relais's 5 s guidance timeout → 6.5 s with one `cat-file` per file; 0.56 s (p of 3: 1.07, 0.58, 0.56) with `--batch`; 40 listed, 160 overflow.
- Mutation: the working-tree filter changed to `std::fs::metadata` → the symlink test fails → fails at tests/candidates.rs:473.
- `make check` → pass → pass.

### Implementation review (`--head`)

- approve-with-changes (3 findings), then approve on the Delta; 51k + 63k tokens, 84 s + 50 s.
- Fixed:
  - the symlink test can now fail;
  - `regular_files_in` has a unit test;
  - the CHANGELOG names the symlink change.
- Carried to the decisions PR: the `--head` Decision-log line and this pilot record.
- Found by its timing check, and fixed: the spawn per file. The relais run (`run-65ea8bb3e545f-11c23`) was approved after its own review, and its low finding was fixed.

## Phase 5: the pack row and `check --json` text

amont-agent's write-time hook (behaviour 4) needs three things from adr-lint 0.5.0:
- **The row it gates on.** `amont.pack` declares the exact rules' `check` row, `pre-commit adr-lint-check *.rs,*.ts,*.tsx,*.go,*.py+.adr.yaml warn files adr-lint check --`. It ships as `warn` (behaviour 2), and opts in only where a decisions corpus exists, since `check` exits 3 without one. The candidate rows and the rest of Phase 3 stay open.
- **The line text.** Each `check --json` finding carries `text`, the line normalised as the candidate id normalises it, so the hook can journal (rule, path, line) without reimplementing that normalisation.
- **The deleted-path fix carried from Phase 3.** A path git lists as deleted, named in argv or in `$AMONT_FILES`, is skipped, not exit 3.

## Verification (phase 5)

On the release binary, in a scratch repository with a decisions corpus, input → expected → actual:
- `git rm src/b.rs`, then `AMONT_FILES=src/b.rs adr-lint check` and `adr-lint check -- src/b.rs` → exit 0, nothing listed → both exit 0, where 0.4.0 exited 3 ("cannot read src/b.rs").
- The same staged deletion from `src/`, `check -- b.rs` and `check -- ./b.rs` → 0 → 0, 0.
- `check -- no/such.rs`, a path git never tracked → exit 3 naming it → exit 3, `cannot read no/such.rs`.
- `check --json` on a reasonless `#[ignore]` → rule `tests.first-properties`, `text` `#[ignore]` → as expected.
- `amont add <a repo holding this amont.pack> --dry-run` → declares the row → `pre-commit  adr-lint-check  *.rs,*.ts,*.tsx,*.go,*.py+.adr.yaml  warn  files adr-lint check --`.
  - After a real add, `amont list --json` lists id `pre-commit-adr-lint-check` with status `unusable` and `command: null` until the row is trusted. So amont-agent's gate matches the id, not the command.
  - The untrusted row shows no scope, so `+.adr.yaml` as an opt-in file rests on amont's docs/custom-checks.md and on homelab's trusted `+kustomization.yaml` row (`scope_opt_in`). Look again once the row is trusted in a real repository.
- `make check` → pass → pass, again after fce9e87.

### Implementation review (phase 5)

- approve-with-changes (1 high, 1 low), then approve-with-changes on the Delta, with the high one owed to this commit; 45k + 48k tokens, 54 s + 20 s.
- Fixed: a Windows `src\b.rs` now matches git's deleted set (fce9e87); this Verification record is filled.
- The relais run (`run-65ead80dfb337-146e5`) was approved after its own review. Its low finding, a reason comment one line above its `.ok()`, was fixed by hand.

## Phase 6: the eslint preset, `@aval-adr/eslint-config`

duro's opt-in `adrLint` export needs a published preset first. The person chose the `@aval-adr` npm scope aval already publishes in (2026-10-25). So the package is `@aval-adr/eslint-config`, not `eslint-config-adr-lint`. It lives in `npm/eslint-config/`, is versioned with the crate, and is published by the release workflow under `--provenance`, armed by the `PUBLISH_NPM` variable as aval's is.

What it holds, checked against each rule body before building:
- `adrLint({restrictedImports})` returns a flat config:
  - `reportUnusedDisableDirectives: "error"`;
  - `ban-ts-comment`, allowing only `@ts-expect-error` with a description;
  - eslint-comments `no-unlimited-disable`, `require-description` and `disable-enable-pair` (general.linters-police-suppressions);
  - one merged `@typescript-eslint/no-restricted-imports` per `files` scope.
- `adrLintTyped(…)` adds `switch-exhaustiveness-check` with `projectService`.

Narrowed, each because the stock rule is broader than its body:
- **errors.never-swallowed:** `no-empty` flags every empty block, and also `catch {} // why`, which the body allows. The preset ships its own rule instead: an empty `catch` with no comment inside it or on its line.
- **general.no-disabled-safety:** `require-description` ignores `eslint-enable`, `global(s)`, `exported` and `eslint-env`, which suppress nothing.
- **types.exhaustive-matching:** not narrowed after all. The first build set `considerDefaultExhaustiveForUnions: true`, but the stock options already pass a full switch whose `default` assigns to `never`, and they report a missing case behind a silent `default`, which the body wants caught. The review found this; a fixture now proves it.
- **components.acyclic-dependencies:** `import/no-cycle` is deferred. It needs `eslint-plugin-import-x` and a TS resolver in every consumer, none of the ten `@duro-app/eslint-config` consumers uses either today, and its lint time is unmeasured. A TS cycle stays the reviewer's until phase 7 measures it.

## Verification (phase 6)

Input → expected → actual:
- `npm test` in npm/eslint-config, 22 cases through ESLint's API → all pass → 22/22. They cover:
  - must-fire and must-pass for each rule;
  - the `@mui/*` and `kysely` bans merged into one option;
  - two `files` scopes giving two objects;
  - a file-less group scoped to the source files;
  - `catch (e) /* why */ {}` passes;
  - a bare `eslint-enable` passes;
  - typed: a missing member with no default fires, a missing member behind a silent `default` fires, and a `default` assigning to `never` passes;
  - it loads beside `typescript-eslint`'s recommended configs.
- `make check`, which now runs `npm-test` → pass → pass.
- `npm pack --dry-run` → the entry files only → README.md, index.d.ts, index.js, package.json (3.0 kB).
- The guard's `sed` on npm/eslint-config/package.json → `0.5.0`, equal to Cargo.toml → `0.5.0`. It runs before any publish, so a mismatch stops the release first.
- `npm whoami` with the person's token → an account in the `@aval-adr` scope → `zougi`.
- The token is set as `NPM_TOKEN` in the `release` environment (deployment policy: tag `v*`) and removed from the repository level → `gh secret list` at repository level is empty, and `--env release` lists NPM_TOKEN and TAP_DEPLOY_KEY. `PUBLISH_NPM` is `true`.

### Implementation review (phase 6)

- approve-with-changes (2 blocking, 1 high, 3 medium, 1 low), then approve-with-changes on the Delta, with this record owed to this commit; 74k + 82k tokens, 280 s + 29 s.
- Fixed:
  - the fixtures' single-letter names (both adr-lint candidates, ruled violations);
  - publish-npm in the `release` environment;
  - `require-description` ignoring directives that suppress nothing;
  - a file-less import group scoped to the source files;
  - comments anywhere in a catch clause;
  - `switch-exhaustiveness-check` back on its stock options. The first build's `considerDefaultExhaustiveForUnions: true` was a loosening, not a narrowing: it let a missing case behind a silent `default` through.
- The four dependency candidates were ruled `fine`, and are argued in the PR.
- The relais run (`run-65eb07928217f-3602`) stopped on `verification_inputs_changed`. It was applied and verified by hand, and recorded as salvaged at b573f13.

## Phase 7: what the measurement asked of adr-lint 0.7.0

The report-only survey of the owned repositories (95 exact findings and about 630 native ones, over 49 repositories) found two things to fix in adr-lint before any cleanup:
- **Generated files.** A MockGen file's `m_2` receiver was flagged, and nobody can fix generated code at the source. `check` and `candidates` now skip a file whose first 15 lines carry a comment with a standalone `@generated` tag, or one that starts with a generator's sentence ("Code generated", "Generated by", …) and says `DO NOT EDIT`; a comment that merely discusses the markers does not count. A non-UTF-8 changed file is tested from a lossy decoding, so `candidates` does not start failing on it.
- **errors.never-swallowed.** Its `Checked by:` line was corrected in decisions (2026-10-26): clippy `let_underscore_must_use`, errcheck check-blank and ruff `S110` cannot read the same-line comment the body allows. The Rust `let _ =` and `.ok();`, Go `_ =` and Python `except: pass` become candidate patterns, which honour `commentExempt: same-line` as `check`'s exact rules do.

## Verification (phase 7, adr-lint)

Input → expected → actual:
- `make check`, after each change → pass → pass. Two runs failed first, both explained: one existing test (`rule_filters_the_findings…`) exited 3 when the machine's load average reached 255 and `check`'s 30 s subprocess deadline expired (it passed alone, and the full run passed once the 1-minute load fell below 40); and my own `--help` sentence put double quotes inside a Rust string, which the build rejected.
- `adr-lint coverage` against the pack refreshed with decisions#97 → exit 1, one problem, `errors.never-swallowed: candidate class has no mechanism in the map` → exit 1 as expected; after the four candidate rules and the map entry → exit 0.
- The Go pattern, reproduced before the fix: a scratch file with `s.x = g()`, `m[k] = g()`, `*p = g()` and `_ = g()` → only `_ = g()` marked → 4 candidates, 3 of them false. After the fix → only line 7 marked → `[7]`; `ast-grep test` 68/68, with the three field, index and pointer assignments and `x, _ = g()` as valid fixtures.
- The generated-file test, reproduced before the fix: `adr-lint check -- src/generated.rs` → the file is checked → exit 0 silent, so adr-lint skipped its own source because a doc comment mentioned the marker in backticks. After the fix, `is_generated(include_str!("generated.rs"))` is false (a unit test), as are a backticked or quoted `@generated`, `not@generated` and "Both generated and DO NOT EDIT must appear."; Go MockGen, prost, protoc (Python) and protoc-gen-es headers are still generated.
- Mutation checks, each by removing the guard and running only its test:
  - the generated-file skip removed from `check` → `a_mockgen_go_file_is_exempt…` fails → it fails (rc 101);
  - the lossy read in `candidates` replaced by `read_to_string` → `a_changed_file_that_is_not_utf8…` fails → it fails (rc 101);
  - Python's `#` comment style switched off → NO integration test fails (rc 0, twice): tree-sitter-python takes a trailing comment into the `except` clause, so the rule's own `pass`-only match stops matching, and the commented case is dropped by the rule, not by the exemption. The unit test `a_hash_starts_a_comment_in_python_and_nowhere_else` fails (rc 101). So the Python `#` handling is covered by that unit test and by no integration test, and the integration cases record the contract only.
- The commit gate passes 25 checks on every commit.

### Implementation review (phase 7, adr-lint)

- approve-with-changes, then four Deltas, each approve-with-changes with only fixes or this record owed; 46k + 55k + 57k + 60k tokens. That is more passes than the template allows (a round and one delta): the findings were all fixed in code, the verdict was never `rework`, and each Delta bound a changed tree.
- Fixed:
  - the Go pattern matching `s.x = g()`, `m[k] = g()` and `*p = g()`;
  - the generated-file test skipping the module that documents it;
  - three descriptions (`--help`, README, CHANGELOG) of the older, wider test, plus its "for example" list;
  - `misses` naming Go `v, _ := f()`, Rust `let _ = res;` and a Python comment line above `pass`.
- The relais runs `run-65eb81e6bdb71-6a89` and `run-65eb89f06a03d-ee13` each stopped at review: the worker wrote the code and skipped the integration tests and the README. Both were finished by hand and recorded as salvaged.

## Phase 7: `adr-lint sync`, the Rust half

Every native-linter cleanup is accepted against the synced presets, so `sync` comes before the cleanup. The Rust half writes the clippy lints of general.linters-police-suppressions: `allow_attributes` and `allow_attributes_without_reason`, both `deny`, so a suppression is `#[expect(lint, reason = "…")]`.
- **Where it writes.** In `[workspace.lints.clippy]` for a workspace, and every member then needs `lints.workspace = true`, or the workspace table reaches nothing; in `[lints.clippy]` for a single package.
- **How it writes.** It edits with `toml_edit`, so the person's formatting and comments stand. A stronger level already there (`forbid`) is kept. A weaker one is a conflict, printed as `file:line`, and nothing is written. The file is written to a temporary file and renamed, keeping its mode, and a symlinked manifest is refused.
- **When it skips.** A `rust-version` below 1.81, where `reason =` does not exist, is skipped with a note.
- **`--check`** writes nothing, and says what is missing or weaker.

Two notes for the pilot:
- **Where `sync --check` runs** is decided when relais adopts it. It is listed as an exact check under general.no-disabled-safety, so it belongs in CI.
- **A known limit.** A clippy group set to `allow` at a higher priority than the two keys (`restriction = { level = "allow", priority = 1 }`) overrides the two denies, and `sync` reads only the two keys, so it does not see that.

## Verification (phase 7, sync)

Input → expected → actual:
- `make check` → pass → pass: 12 sync tests, 10 from the contract and two from the review. The first run after the review fixes failed five `check` tests with exit 3 at load average 381, where their 30 s subprocess deadline expired. I had chained the commit with `;`, so it committed before I read that. Re-run on the same commit at load 33, it passed.
- A pilot on a copy of relais's real workspace (`git archive` of its manifests and lock):
  - `--check` → exit 1, naming exactly the three gaps (both keys and the member's opt-in) → as expected;
  - `sync` → both manifests written, only new lines added → as expected;
  - `--check` again → 0 → 0;
  - a second `sync` → `already in sync` → as expected;
  - `cargo metadata` → accepts the synced manifests → it does.
- End to end, a scratch workspace with one reasonless `#[allow(dead_code)]`: after `sync`, `cargo clippy` → refused → exit 101, `#[allow] attribute found` and `allow attribute without specifying a reason`. The control, the same workspace without `sync` → passes → exit 0. So the refusal comes from what `sync` wrote.
- The review's bug, where a conflict was told from a missing key by searching the printed text for " is missing": the regression test, run against 7bf66f9, the commit before the fix, → fails → `left: Some(0)`: sync exits 0 and writes over the conflict. On the fix → passes → it passes. The worker's first version of that test named the member `is missing` and passed on the old code too, because the old search needed a space before "is missing". The member is now `member is missing`.
- The toml_edit argument, checked against `cargo metadata`: none of toml_edit or its four new crates (toml_writer, hashbrown, indexmap, equivalent) has a build script; toml_edit and toml_writer are toml-rs's, hashbrown rust-lang's, indexmap and equivalent indexmap-rs's.

### Implementation review (phase 7, sync)

- approve-with-changes, then two Deltas (approve-with-changes), with only fixes or this record owed; 65k + 88k tokens and the last pass.
- Fixed:
  - findings as a `Finding` enum, not searched text (the bug above);
  - `Role` with three variants, no `bool`;
  - the editing functions as methods, not `&mut` arguments;
  - names that say what they hold;
  - every `.ok();` with its reason on its line;
  - a member-conflict test;
  - "each file atomically" in the docs;
  - the toml_edit argument, which first claimed sync was never on a trust path, false for a hook binary.
- adr-lint listed 12 candidates in its own new code. The reviewer ruled 10 of them violations, and all 10 are fixed: the list fell to 3, two of them fine and the dependency entry now argued.
- The last Delta (tree `bd98a259`) answered approve-with-changes. Its blocker was this record, which this commit writes. One new low finding remains: the Cargo.toml comment compares sync to `toml` parsing "the rules map", but the map is adr-lint's own file. It is deliberate for now, because changing the tree reopens the review. The fix is carried into the next adr-lint PR, the TypeScript half of `sync`.
- The relais runs `run-65ebbb72cfe00-1a0d` and `run-65ebc187859be-109b` stopped on `verification_inputs_changed` (Cargo.toml) and were verified by hand and recorded as salvaged.

## Phase 7: the cleanup's first finding in adr-lint

The 2026-10-27 survey found one exact finding in adr-lint itself, and it is false. `adr-lint.waiver-without-reason` fired on src/check.rs:852, a test's string literal `"# adr-lint: allow x.y because"` (src/check.rs:852 at 0.8.0). With no `--`, the old parser read an empty reason; after `--`, the closing quote passed as one. `parse_waiver` accepts `//`, `#` or `/*` before the marker in any language, so the `#` inside a Rust string reads as a comment leader. The fix makes the leader follow the file's comment style (`#` for .py; `//` or `/*` otherwise), as `is_comment_line` already does.

This PR also carries the low finding from sync's last review. The Cargo.toml comment compares sync to `toml` parsing "the rules map", but the map is adr-lint's own file.

## Verification (phase 7, waiver comment style)

Input → expected → actual:
- **The regression test** `a_hash_in_a_rust_string_is_not_a_waiver_but_a_python_comment_is`:
  - on the old src/check.rs → fails → fails, `string.rs:2:8: adr-lint.waiver-without-reason`;
  - on the fix → passes → passes.
- **adr-lint 0.8.0 against this build**, on its own src/check.rs and tests/check.rs → 1 false finding each before, 0 after → as expected.
- **`make check`** → passes → passes; the gate passed 25 checks on each commit.
- **How it was built.** relais (`run-65ece543db56f-2a82`) stopped on `verification_inputs_changed`, because the contract's Cargo.toml comment edit is one of the profile's inputs. I applied the candidate and verified it by hand; it is recorded as salvaged. The worker's integration test was wrong twice, and both are fixed:
  - it counted the rule id three times in the grouped output;
  - its `--` fixture never reproduced the bug.
- **Residual**, named by a `holds-until:` above `has_leader`: a `//` (or `#` in Python) inside a string literal before the marker still reads as a leader.

### Implementation review (phase 7, waiver comment style)

- **Verdicts.** approve-with-changes, then approve-with-changes on the Delta, with only this record owed; 60k + 63k tokens, 61 s + 15 s.
- **Fixed:**
  - the Cargo.toml comment now says "the first TOML parser", since `ruling-record --check` already parses a reviewer's JSON;
  - the `holds-until:` for the string gap;
  - this record, and the corrected literal.
- **Deliberate.** The Delta's optional low finding asks the `holds-until:` to add that such a string can also raise a false waiver-without-reason from any line. It is carried to the next adr-lint PR, because changing the comment now would reopen the reviewed tree.

## Phase 7: `adr-lint sync`, the Python half

The 2026-10-27 cleanups showed the exact checker catches file-level headers but not the native settings that make a linter refuse a suppression. general.linters-police-suppressions asks, for Python, for ruff `PGH003`, `PGH004` and `RUF100`, and for pyright `enableTypeIgnoreComments = false` and `reportUnnecessaryTypeIgnoreComment = "error"`. Of the six owned repositories that carry Python, none selects `PGH`, none sets either pyright key, and two (homelab, stealth-fetch) configure neither tool.

**What `sync` does for Python**, in the repository root given by `-C`:
- **Ruff.** The config is the first of `.ruff.toml`, `ruff.toml`, and `pyproject.toml` when it has `[tool.ruff]`, which is ruff's own order. The lint table is `[lint]` (`[tool.ruff.lint]` in pyproject), or the top level when the file still uses the deprecated top-level `select`.
  - A code is enabled when a `select` or `extend-select` entry is a prefix of it, or is `ALL`.
  - An `ignore` entry that is a prefix of the code and at least as specific as the select that enables it is a conflict. It is reported with its line, and nothing is written.
  - A `per-file-ignores` entry that lists the code or a prefix of it is a conflict too.
  - A missing code is appended to `select`, or to `extend-select` when there is no `select`, so a config that relied on ruff's defaults keeps them.
- **Pyright.** The config is `pyrightconfig.json` when it exists, which pyright prefers, else `[tool.pyright]` in pyproject.toml.
  - `enableTypeIgnoreComments` must be `false`, and `reportUnnecessaryTypeIgnoreComment` must be `"error"` or `true`. A different value is a conflict.
  - A missing key is added. In TOML that goes through `toml_edit`, as in the Rust half. In JSON it is a textual insertion before the closing brace that copies the file's own indentation, and the result is parsed again before it is written.
  - A pyrightconfig.json that is not strict JSON (comments, trailing commas) is refused with exit 3 and a message, never rewritten.
- **A tool with no config is not configured.** Writing one would switch on a linter the repository never chose. It prints `sync: no ruff config, ruff skipped`, and the same for pyright, only when the repository has a `pyproject.toml` or one of the two configs.
- Writes are atomic per file and keep its mode. A symlinked config is refused. Both tools in one pyproject.toml are one read and one write.
- Exit statuses are the Rust half's. When both halves run, the exit status is the larger of the two.

**Layout.** `src/sync.rs` keeps the options, the usage text, the atomic write and the dispatch. The Rust half moves unchanged to `src/sync/rust.rs`, in a commit that changes no behaviour, and the Python half is `src/sync/python.rs`.

**Known limits, to be named in the code:** a nested ruff config that does not extend the root's is not read; a pyright `executionEnvironments` entry can override the two keys; `ruff.toml` `extend` chains are not followed.

## Verification (phase 7, sync Python)

Input → expected → actual:
- **`make check`** → pass → pass: 41 tests in tests/sync_python.rs and the 12 Rust sync tests unedited. The gate passed 25 checks on every commit.
- **The move of the Rust half** (src/sync.rs to src/sync/rust.rs) → no behaviour change → a line diff shows only module headers, imports, `mod rust;` and the `run` signature differing; the 12 original tests pass unedited.
- **The seven real configs** (copies of origin/main files, run through the built binary):

  | Repository | Files | `--check` before | `sync` | `--check` after |
  |---|---|---|---|---|
  | trade-agents | ruff.toml, pyrightconfig.json | 1, four gaps | wrote both | 0 |
  | sre-agent, gtm-agent, governance-audit, agentic-platform | pyproject.toml | 1, four gaps | wrote it | 0 |
  | stealth-fetch | pyproject.toml, no tool tables | 0 | `ruff skipped`, `pyright skipped` | 0 |
  | homelab | none at the root | 0 | silent | 0 |

  Each diff only adds `PGH003`/`PGH004` to `select`, plus the two pyright keys. `RUF` was already selected in all five, so `RUF100` was never added. The JSON path ran on trade-agents' real file and stays valid JSON.
- **Real ruff 0.16.0 and pyright 1.1.411** on a probe (`import os  # noqa`, `x: int = "a"  # type: ignore`, and a `# pyright: ignore[reportAssignmentType]` on a valid line), run against trade-agents' and sre-agent's configs before and after `sync`:
  - before: ruff reports nothing; pyright honours `# type: ignore`, so the type error is hidden;
  - after: ruff reports `PGH004` and `PGH003`; pyright reports the type error and flags the unnecessary `# pyright: ignore` (`reportUnnecessaryTypeIgnoreComment`).
- **Ruff's rules, measured with 0.16.0 and then pinned by tests:**
  - on a tie, `ignore` wins (`select = ["PGH003"]` with `ignore = ["PGH003"]` reports nothing);
  - the more specific entry wins in both directions (`select = ["PGH"]` + `ignore = ["PGH003"]` is off; `select = ["PGH003"]` + `ignore = ["PGH"]` is on);
  - for one key, `[lint]` replaces the top-level value outright (`select`, `ignore`, `extend-select` and `per-file-ignores` all behave so); different keys combine (a top-level `extend-select` beside `[lint] select` is on; a top-level `extend-ignore` beside `[lint] ignore` is off).
- **Bugs found on review of the relais output, each reproduced by a failing test first:**
  1. A comment on the last entry of a multi-line `select` (sre-agent's `"T20",    # print`) moved onto the new last entry. Two tests failed on the old code; both pass on the fix.
  2. `audit_ruff` unioned the top-level and `[lint]` settings, so a top-level `ignore = ["PGH003"]` beside `[lint] select` made `--check` say "in sync" while ruff had PGH003 off, and a top-level `select = ["PGH"]` beside `[lint] select = ["E"]` hid a missing PGH. Both found by the first review. Four of six new tests failed on the old code.
  3. A new `[lint] extend-select` would have replaced a top-level `extend-select = ["B"]`. Found while measuring, and now pinned by a test.
- **Deliberate breakage of the code**, to see whether the tests notice: 11 valid mutations. Nine were caught at once. Two survived and were closed with new tests: a select/ignore tie (`>=` to `>`), and a file with only top-level settings and no `select` (the key would go to `[lint]`). A twelfth, which swapped an empty path for an empty path, changed nothing and is discarded.
- **A relais stop:** the worker changed src/main.rs (+2/-2), outside the contract's write scope. The change is right (the top-level help still said Rust was the only language synced), so it was accepted, and the run is recorded as salvaged.
- **The CI aval pin** moved from 1.10.0 to 1.11.0 in the first commit, because the refreshed pack's session hook is rewritten by 1.11.0 and the CI job runs `aval hook install --check`.

Not verified, and named in the code as `holds-until:` markers: a nested ruff config that does not extend the root's (the packages of agentic-platform and gtm-agent, homelab's services), `extend` chains, and pyright `executionEnvironments`. In each, `--check` can say "in sync" while a tool reads settings that `sync` never saw. The guard that compares the re-parsed JSON with the original cannot fire on strict JSON; it stays as a guard. The Go (`nolintlint` in `.golangci.yml`) and TypeScript (`eslint.config.*`) halves are not built.

### Implementation review (phase 7, sync Python)

- **Verdicts.** approve-with-changes three times, with only this record owed by the end; 92k + 106k + 114k tokens, 266 s + 30 s + 21 s.
- **Round 1, fixed:**
  - the three limits became `holds-until:` markers, and the `extend` wording was widened;
  - `value` became `new_entry`;
  - `apply` returns an enum and `run` records the change;
  - a top-level ruff setting beside `[lint]`;
  - JSON: CRLF, the conflict line at the top-level key, and a re-parse that compares the whole object.
- **Delta 1, fixed:** the two settings tables merge per key, not as a union (found by the reviewer, then measured).
- **Candidates.** All eight were ruled: the `&mut` document editors are fine, two violations (the `value` name, `apply`'s two result channels) were fixed, and the two `_ =>` arms are in the moved Rust half.
- **Counts corrected.** My Delta-1 message said "12 broken on purpose, 3 survivors"; the record above has the true figures.

## Phase 7: `adr-lint sync`, the Go half

general.linters-police-suppressions asks, for Go, for golangci-lint `nolintlint` with `require-explanation`, `require-specific` and `allow-unused: false`. Measured with golangci-lint 2.4.0: `nolintlint` is off under `version: "2"` defaults; enabled, it reports an unused `//nolint:govet`; with `require-specific` it reports a bare `//nolint`. Six owned Go repositories have a `.golangci.yml`, all `version: "2"`, in three shapes: `enable` added to the defaults (four), `default: none` plus `enable` (garage-operator), and no `linters:` at all (vault-transit-unseal-operator).

**What `sync` does.**
- **The file.** The first of `.golangci.yml`, `.golangci.yaml`, `.golangci.toml` and `.golangci.json` in the root given by `-C`. Only YAML is edited; a TOML or JSON golangci config, or one without `version: "2"`, is refused with exit 3 and the reason.
- **The edit.** It goes through `yaml-edit`, which edits losslessly and keeps the person's comments and layout.
- **Enabling the linter.** `nolintlint` is enabled unless `linters.default` is `all`, which already covers it. It is appended to `linters.enable`, which is created under `linters` when absent, as is `linters` itself.
- **Settings.** It writes `linters.settings.nolintlint` with `require-explanation: true`, `require-specific: true` and `allow-unused: false`, adding only the keys that are missing.
- **Conflicts**, reported with their line and nothing written:
  - `nolintlint` listed in `linters.disable`;
  - any of the three settings at the weaker value;
  - an `linters.exclusions.rules` entry whose `linters` list names `nolintlint`.
- **Re-check.** The output is re-parsed, and the audit must find nothing; otherwise exit 3 and nothing is written.

**What `--check` does.** It reports the same findings without writing. When `golangci-lint` is on PATH, it also runs `golangci-lint config verify` (30 s deadline) in the root, so the edited file is valid for the installed version. When it is not on PATH, `--check` says so on stderr.

**The dependency** is `yaml-edit`, argued beside its declaration in Cargo.toml. Writes are atomic and keep the file's mode; a symlinked config is refused.

## Verification (phase 7, sync Go)

Input → expected → actual:
- **`make check`** → pass → pass. The gate passed 25 checks on every commit. One `make check` failed two unrelated `check` tests with exit 3 at load average 388; both passed alone at load 41 (a timeout under load, as before).
- **The six real `.golangci.yml`** (copies of HEAD) → sync writes, `--check` is 0 afterwards, golangci-lint 2.4.0 `config verify` passes → all six do. The diffs only add lines:
  - +6 lines in the five with a `linters:` mapping;
  - +8 in vault-transit-unseal-operator, which had no `linters:`.
  
  Every comment line in duro-operator is unchanged.
- **Real golangci-lint 2.4.0 on a probe** (a bare `//nolint` and an unused `//nolint:govet`, with no standard-library import) under duro-operator's config → before: 0 issues; after sync: both flagged → as expected. A probe that imports `os` cannot be checked locally: golangci-lint 2.4.0 rejects the installed Go's export data ("version 4 is greater than maximum supported version 2"), a local toolchain limit.
- **`linters.default`, measured on 2.4.0.** `all` enables nolintlint (it flags the bare `//nolint` once sync's settings are present), so `enable` is not added there. golangci-lint lists nolintlint as `fast`, so under `default: fast` the added `enable` entry is redundant but harmless. `standard` and `none` need it.
- **Shapes, measured on the binary before any fix:**
  - CRLF: kept on every inserted line, as expected;
  - tab indentation: produced a mix of tabs and spaces, a bug, now refused with exit 3 (also a tab after spaces before a key);
  - a `linters:` holding only a comment: the comment survives but lands below the inserted keys, a known placement pinned by a test;
  - a trailing comment on the last `enable` item: kept on that item.
  
  Each shape is now pinned with exact bytes.
- **An independent losslessness check.** Before writing, every original line must survive in order, checked line by line and sharing no code with yaml-edit; only the `enable:` flow list may gain `nolintlint`. The unit test `only_the_enable_list_may_gain_an_item` fails when that exemption is widened back to any line ending in `]` (the second relais review's catch).
- **A quoted `"true"`** → reported as a string, and nothing written → as expected. golangci-lint's own `config verify` rejects it too ("got string, want boolean").

### Implementation review (phase 7, sync Go)

- **Verdicts.** approve-with-changes, then approve-with-changes on the Delta, with only this record owed; 57k + 65k tokens.
- **Fixed, round 1:**
  - tabs refused;
  - CRLF, comment-only and trailing-comment shapes pinned;
  - the independent losslessness check;
  - the exclusions limit as a `holds-until:`.
- **Fixed, from the relais reviews:**
  - the Audit wrapper, an inaccurate lifetime comment, the exit table, two names, the quoted-boolean message;
  - the `]` exemption narrowed to `enable:`;
  - a tab after spaces.
- **Deliberate.**
  - The Delta's low finding: the `enable:` exemption is not tied to `linters`, so it also matches `formatters.enable`. It fails closed for every other shape and only lets through a yaml-edit misreading, and changing it would reopen the reviewed tree. Carried to the next adr-lint PR.
  - The tab check would refuse a block-scalar line that starts with a tab and contains a colon: a safe false positive.
- **The dependency.** `yaml-edit` was added in its own commit with its argument; the candidate (Cargo.toml) was ruled fine.

## Phase 7: `adr-lint sync`, the TypeScript half

general.linters-police-suppressions asks, for TypeScript:
- `linterOptions.reportUnusedDisableDirectives: "error"`;
- `@typescript-eslint/ban-ts-comment`, allowing only a described `@ts-expect-error`;
- the eslint-comments rules `no-unlimited-disable`, `require-description` and `disable-enable-pair`.

`adrLint()` in `@duro-app/eslint-config` (5.33.0) already sets all of them, and it rides that package's version. So `sync` only adds the call, once per repository. None of the ten owned configs calls it yet. Each imports named presets from `@duro-app/eslint-config`: nine export an array, and application-landscape exports `tseslint.config(...)`.

**What `sync` does.**
- It reads the first of `eslint.config.{js,mjs,cjs,ts,mts,cts}` in the root given by `-C`.
- It locates the edit points with the ast-grep that adr-lint already requires (0.51.0), which gives tree-sitter spans, so no new crate is needed.
- The edit is textual, at those byte ranges:
  - `adrLint` is added to the `import { … } from "@duro-app/eslint-config"` specifiers;
  - `...adrLint({ stack: [...] })` becomes the LAST element of the default-exported array, or the last argument of `tseslint.config(...)`, so its rules win over earlier entries. `stack` lists which of `react` and `effect` that import already names, in that order, and is `[]` when neither.
- The result is re-parsed with ast-grep and must contain exactly one `adrLint(` call; otherwise exit 3 and nothing is written.

**What `--check` does.**
- **Statically:** the import and the call must be present.
- **Against ESLint:** when the repository's own `node_modules/.bin/eslint` exists, it also runs `eslint --print-config` on one sample file, the first `.ts` or `.tsx` that git lists (a 30 s deadline, through `process::run`). Every required setting must be at `error`, so a later config object that switches one off is caught. The required settings are `reportUnusedDisableDirectives` and the four rules, plus `ban-ts-comment`'s `ts-expect-error` option.
- When ESLint is not installed, `--check` says so on stderr (`sync: eslint not installed; rule resolution not checked`) rather than passing silently.

**A version gate.** `adrLint` exists in `@duro-app/eslint-config` 5.33.0 and later, and on 2026-10-28 nine of the ten repositories had 5.1.0 to 5.11.0, declared or installed. Against those, the edit makes ESLint fail to load (`SyntaxError: … does not provide an export named 'adrLint'`, measured in duro-app with 5.9.0 installed). So before writing, and in `--check`:
- `sync` reads the installed version from `node_modules/@duro-app/eslint-config/package.json`, else the lowest version the repository's `package.json` range admits;
- below 5.33.0, the run is refused with exit 1, naming the version found and the one needed (`upgrade @duro-app/eslint-config to 5.33.0 or later first`), and nothing is written;
- a `workspace:` dependency, as in duro-design-system, which is the package, passes the gate;
- a range it cannot read is refused with exit 3.

`sync` never edits `package.json` or a lockfile; upgrading is the repository's own change.

**Refused, with exit 3 and the reason:**
- no installed or declared `@duro-app/eslint-config` version at all (added on review: the gate must not wave a write through when it cannot see a version, which is exactly when the import can break ESLint);
- the package imported by more than one declaration, for example a trailing `import type`, where `adrLint` could land type-only (added on review);
- an `eslint.config.cjs`: `module.exports` is not edited (added on review);
- no `@duro-app/eslint-config` import (for `@aval-adr/eslint-config`, or a repository on neither, that is a later step);
- a default export that is neither an array literal nor `tseslint.config(...)`/`defineConfig(...)`;
- a symlinked config.

A config that already calls `adrLint` is in sync. Writes are atomic and keep the file's mode. Exit statuses are those of the other halves, and with several halves `run` returns the larger.

## Verification (phase 7, sync TypeScript)

Input → expected → actual:
- **`make check`** → pass → pass: 44 tests in tests/sync_typescript.rs and the Rust and Python sync tests unedited. The gate passed 25 checks on every commit.
- **The ten real eslint configs** (copies of HEAD) → sync writes, `--check` is 0 afterwards, `node --check` parses each result → all ten do. Each diff is +2/-1:
  - `adrLint` is added to the existing `@duro-app/eslint-config` import, keeping its quotes and spacing;
  - `...adrLint({ stack: [...] })` becomes the last element, with `stack` matching the presets imported (`["react", "effect"]` for duro-app and application-landscape, `[]` for duro-design-system). application-landscape's `tseslint.config(...)` gets it as the last argument.
- **The gap no review saw**, found by hand. `adrLint` exists from @duro-app/eslint-config 5.33.0. On 2026-10-28, nine of the ten repositories had 5.1.0 to 5.11.0, declared or installed; duro-design-system is the package (`workspace:`). In duro-app (5.9.0 installed), `import { react, adrLint } from '@duro-app/eslint-config'` gave `SyntaxError: ... does not provide an export named 'adrLint'`, so the unguarded edit would have broken ESLint in nine repositories. That is the version gate.
- **The gate on duro-app's real files.** With 5.9.0 installed → exit 1 naming 5.9.0 and 5.33.0, and the file unchanged → as expected. With `^5.33.0` declared → written → as expected.
- **End to end with the published preset.** In a scratch repository with @duro-app/eslint-config 5.35.0, ESLint 9.39.5 and typescript 5, holding duro-app's real config:
  - `sync` writes;
  - `eslint --print-config` resolves `reportUnusedDisableDirectives` and the four rules to 2 (error);
  - `adr-lint sync --check` exits 0, naming the sample file;
  - ESLint reports a bare `/* eslint-disable */` three times (no-unlimited-disable, require-description, disable-enable-pair);
  - control: the unsynced config lets the same file through with 0 findings.
  
  The scratch run first failed for two setup reasons of mine (typescript missing; no `.gitignore`, so git listed `node_modules`), both fixed before the run that counts.
- **Tests able to fail**, by deliberate breakage:
  - the gate never refusing, the declared range ignored, and an alias accepted are each caught by the 8 gate and alias tests, which I wrote by hand after the second relais run implemented the gate without testing it;
  - a broken ESLint whose failure is swallowed is caught by `a_broken_eslint_is_reported_with_its_own_error_not_as_ignored_files`.

### Implementation review (phase 7, sync TypeScript)

- **Verdicts.** approve-with-changes, then approve-with-changes on the Delta, with only this record owed; 67k + 85k tokens.
- **Fixed:**
  - no version found is refused, not written (high);
  - a failing `git ls-files` exits 3 with git's message;
  - `--check` tries up to 20 samples, names the one used, and carries ESLint's last failure;
  - `>`, prerelease, union and x-ranges read correctly or are refused;
  - two import declarations are refused, and so is `.cjs`;
  - the weak subpath test is strengthened.
- **From the relais reviews:**
  - the unbound alias;
  - the absolute eslint path;
  - the README and USAGE wording;
  - the swallowed ESLint failure;
  - the exit table.
- **Deliberate.**
  - The Delta's low residue: a process failure, unlike output that is not JSON, still moves to the next sample, so a hanging ESLint can cost up to 20 × 30 s before exit 3. It is bounded, and changing it would reopen the reviewed tree; carried to the next adr-lint PR (stop at the first non-zero exit, since an ignored file exits 0).
  - `nthChild` on ast-grep 0.51.0 is evidenced by the ten real configs, not tested in isolation.
- **Rollout.** In nine of the ten TypeScript repositories `sync` will refuse until @duro-app/eslint-config is upgraded to 5.33.0 or later (the latest is 5.35.0). That upgrade is the first step of each repository's TypeScript cleanup.

