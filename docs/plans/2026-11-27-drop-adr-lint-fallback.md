---
status: active
branch: chore/t22-drop-adr-lint-fallback
repos: [adr-lint]
adrs: [ADR-0073]
plan-id: p-01M8D1THH66BAYK144YW2729RE
---
# Drop the adr-lint.toml fallback

## Review panel

none — approve if removing the read is safe with census 0 on local mains.
📍 adr-lint · 1 PR · next: land, release 0.21.0. Panel: backend, lang:rust, lang:typescript, tui, unix (opus).
**Changed by review:** a census, a new test, the release steps and the full file list.
**Verdicts:** round 1: 5 approve-with-changes; round 2: backend rework on `adds=contract` (rejected: the pack only loses a glob), 3 carried; words 283 → 420.

## Picture

```
config::load --reads--> greenspotter.toml
candidates --head ls-tree --> greenspotter.toml
(adr-lint.toml: no read, no note)
greengate.pack/.conf globs --> greenspotter.toml
release 0.21.0 --> GitHub assets, tap, npm eslint-config
```

The picture shows the one read that stays.

Programme topo-spotter-rename, t22 (greenspotter slice). ADR-0073's one
fallback read of `adr-lint.toml` has served its purpose: the fleet sweep
(t10) renamed the files. Census 2026-11-27: `origin/main` of every local checkout under `~/Developer/Perso` has no `adr-lint.toml` (0). Only local checkouts were counted, not all ~25 pack consumers; the files left are on stale worktree branches and one checkout behind origin by 2 commits.

## Scope

- Remove `config::LEGACY_FILE`, the two-file read in `config::load`, the
  warning builders (`old_name_note`, `ignored_note`), `Config.note`,
  `validate_noted` and `Config::from_text_named`. `load` reads
  `greenspotter.toml` only; a missing file stays an empty config.
- `candidates --head` lists only `greenspotter.toml` in the commit.
- `check` and `candidates` no longer treat `adr-lint.toml` as a config or
  graph input.
- Delete `tests/legacy_config.rs` and the three tests of the old name in
  `tests/candidates_ambient.rs` and `tests/check_contract_rust.rs`.
- Drop the `adr-lint.toml` glob from `greengate.pack` and the vendored
  block of `greengate.conf`.
- Files: `src/config.rs`, `src/candidates.rs`, `src/check.rs`, `src/layers.rs`, `src/cli_probe.rs`, `tests/legacy_config.rs`, `tests/candidates_ambient.rs`, `tests/check_contract_rust.rs`, `tests/old_config_ignored.rs`, `greengate.pack`, `greengate.conf`, `CHANGELOG.md`.
- Add `tests/old_config_ignored.rs`: a scratch repo with only `adr-lint.toml` (a duplicate-layer contract, an error if read); `layers summary` exits 0 with no `warning:` and no old name on stderr.
- Release 0.21.0 (a removed read is a minor bump before 1.0), in a release commit after this PR: `Cargo.toml`, `Cargo.lock`, `npm/eslint-config/package.json` and its lockfile move together (release.yaml refuses a mismatch). CHANGELOG entry: `adr-lint.toml` is no longer read and its stderr warning is gone; the fix is `git mv adr-lint.toml greenspotter.toml`.

## Decisions

- The `sync` `adrLint` import refusal stays. It refuses an old ESLint
  config with a message naming the rename; it reads nothing and is not a
  fallback.
- `scripts/rename.sed` and CHANGELOG history stay.

## Non-goals

~25 consumer repos keep the old glob in their vendored pack block until
their next `greengate add`; no sweep.

## Verification

- `cargo test` has 0 failures; no test named `legacy` or `old_name` for the config remains.
- After release: `greenspotter --version` on this Mac prints 0.21.0.
- `cargo test --test old_config_ignored`: 1 passed.
- `git grep -n adr-lint.toml -- src tests greengate.pack greengate.conf` has no hit besides the concat in `tests/old_config_ignored.rs`.

## Code review

- adr-lint: backend, lang:rust (a repository holding only `adr-lint.toml` loses its contract; the census count is 0)

## Your steps

none.

<!-- panel: repos=adr-lint reviewers=backend,lang:rust,lang:typescript,tui,unix body-sha=8480bac84bd5 -->
