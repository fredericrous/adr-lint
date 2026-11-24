---
status: active
branch: feat/adr-lint-becomes-greenspotter
repos: [adr-lint, decisions, homebrew-tap]
adrs: [ADR-0072, ADR-0073, ADR-0074]
plan-id: p-01M85PXQJQC5VEWYDNF9Z3JJXA
---
# adr-lint becomes greenspotter

## Review panel

👉 **Decide:** none. Approve if one fallback read of `adr-lint.toml` with a warning, like greentopo's `aval:` packs (ADR-0073), is acceptable.
📍 adr-lint, decisions, homebrew-tap · 3 phases · 2 releases · next: Phase 1, the transfer. Panel: backend, lang:rust, lang:typescript, unix, tui, po, architect, platform (opus-5-5), codex.
**Changed by review:** the sed leaves out `greengate.conf` and itself; the fallback reaches every reader; `sync` refuses before any writer.
**Verdicts:** round 2: 8 approve-with-changes; backend approve on the final body; 16 carried; words 2437 → 3152 (×1.29, exempt high fixes past round 1's ×1.18); remove 0, merge 0.
📄 Full reviews: [2026-11-24-adr-lint-becomes-greenspotter.reviews.md](2026-11-24-adr-lint-becomes-greenspotter.reviews.md)

## Picture

```
fredericrous/adr-lint --transfer+rename--> greenharness/greenspotter
  |                                         (old URLs redirect: 3 pack
  |                                          shas, CI pins 0.8.0/0.15.x)
  +- scripts/rename.sed + hand edits --> PR --> tag v0.20.0
  |     default program: greentopo           +- GitHub assets
  |     formats: greenspotter.*/1            +- npm @greenharness/
  |     config: greenspotter.toml               greenspotter-eslint-config
  |     (adr-lint.toml: one fallback read)   +- tap greenspotter.rb
  +- decisions: map note + tickets owed before the machine switch
       (relais and greenbelay readers, greenrack eslint-config)
not here: machine switch (t09), fleet sweep + pins (t10)
```

The picture shows the one repository move, the release it produces, the
one fallback read that keeps unswept repositories' contracts, and the
reader renames this release makes owed in two other tools.

## Context

Programme topo-spotter-rename (map
`decisions/docs/programmes/topo-spotter-rename/map.md`), ticket t08. The
greentopo build is done (greentopo 2.0.0, 2026-11-17), and the download
watch (t07) closed 2026-11-24 with 0 failures. ADR-0072 renames adr-lint to
greenspotter at `greenharness/greenspotter`, by sed, with no shipped alias
(ADR-0073 `harness.renames`, a tool with no outside users). ADR-0074 puts
its npm package at `@greenharness/greenspotter-eslint-config`. t06 decided
that the stored names change too: `greenspotter.candidates/1`,
`greenspotter.ruling/1`, `greenspotter.contracts/1`, and gate ids
`greenspotter-*`, with old rows kept as history.

The template is greentopo's plan
(`greenharness/greentopo:docs/plans/2026-11-16-aval-becomes-greentopo.md`)
and its `scripts/rename.sed`. Its lesson applies here: the first pass
missed name forms, so this plan lists every form found by
`git grep -ohE '[A-Za-z_]*[Aa][Dd][Rr][-_]?[Ll][Ii][Nn][Tt][A-Za-z_]*'`
before the first build.

Fleet facts, measured 2026-11-24 on the live checkouts:

- 21 repositories pin this repository's gate pack, at three shas:
  `3c0b5fe8` (18 repositories), `f790339a` (customer-vision, trade-agents)
  and `e4d11197` (relais). They fetch it as `github:fredericrous/adr-lint`.
- Four CI workflows download a pinned release from
  `--repo fredericrous/adr-lint`: greenbelay (0.8.0), relais (0.15.0),
  customer-vision and trade-agents (0.15.1).
- relais (`context/candidates.rs:17`, `runner/review/candidates.rs:17`) and
  greenbelay (`greenbelay-process/src/candidates_block.rs:20,22`) match
  `adr-lint.candidates/1` and `adr-lint.ruling/1` exactly and refuse any
  other format. They also spawn `adr-lint` by name (`RELAIS_ADR_LINT`,
  `GREENBELAY_ADR_LINT`).
- greengate names `pre-push-adr-lint-cli-probe` only in a test fixture
  (`greengate-runtime/src/live.rs:1663`). No code branches on it.
- No fleet repository sets `ADR_LINT_*` (the one hit is decisions prose).
- The release workflow has no crates.io job, so no crate is published,
  and none was before.

## Goal

`greenharness/greenspotter` exists, transferred from
`fredericrous/adr-lint`, and greenspotter 0.20.0 is released under that
name:

- the binary `greenspotter`, whose default program for decisions is
  `greentopo`;
- the npm package `@greenharness/greenspotter-eslint-config`, exporting
  `greenspotter()` and `greenspotterTyped()`, with rule ids
  `greenspotter/*`;
- the tap formula `greenspotter`;
- the config file `greenspotter.toml`;
- formats `greenspotter.*/1`.

No live name in the repository says `adr-lint`, `adr_lint`, `adrLint` or
`aval`, except the allowlist named in Verification.

## Non-goals

- relais and greenbelay readers of the new format ids and binary name.
  Each is a ticket filed here (Phase 3, step 4), and both block t09.
- The machine switch (`/usr/local/bin/adr-lint`, the tap rename map
  `adr-lint → greenspotter`, removing `Formula/adr-lint.rb`) is t09.
- The fleet sweep (t10): it renames `adr-lint.toml`, the gate lines and
  pins, `adrLint` imports and `adr-lint/*` rule ids in eslint-disable
  comments (in the same commit as the package move, since `reportUnusedDisableDirectives` errors on an unknown rule id), `# adr-lint sync:` comments, and removes this plan's fallback
  read. The pins include this repository's own `greengate.conf` pack
  block and its `severity` line. They move in t10, after t09 has put
  `greenspotter` on PATH, because a gate naming a missing binary refuses
  pushes.
- greenrack moving off `@aval-adr/eslint-config` (its 7.x release, owed
  in the map).
- Old published packages (`@aval-adr/eslint-config`, the `adr-lint`
  formula) are left as they are (t03).
- relais.toml's `[integrations] aval = "required"` stays until t11.
- The two stale worktrees, `adr-lint-wt-adopt-suppressions` (behind #49)
  and `adr-lint-wt-conventions` (behind #50, with uncommitted edits), are
  left as they are. After this merge they replay `scripts/rename.sed`
  before a rebase.

## Code review

- adr-lint: backend, lang:rust, lang:typescript, unix, tui, platform (platform: `release.yaml` and `adr.yaml` change; lang:typescript: `npm/eslint-config`)
- decisions: none — markdown only
- homebrew-tap: platform (`Formula/greenspotter.rb`, a copy of `adr-lint.rb` through its own PR)

## Behaviour

1. **One fallback read of `adr-lint.toml`.** This is ADR-0073's "one
   fallback read for committed files until a fleet sweep has renamed
   them", the same allowance greentopo used for `aval:` packs.
   - Without it, `config.rs:34` would read a repository that has only
     `adr-lint.toml` as an empty config. That silently drops its
     architecture contracts and its cli-probe programs.
   - `config::FILE` is `greenspotter.toml`. `LEGACY_FILE` is
     `adr-lint.toml`, the only `adr-lint` left in code, with a comment
     naming the sweep that removes it.
   - When only `adr-lint.toml` exists, it is read and one line goes to
     stderr:
     ``warning: adr-lint.toml is the old name; run `git mv adr-lint.toml greenspotter.toml` ``.
     The exit status is unchanged and nothing extra goes to stdout. Under
     `--json` the line goes on stderr too; no JSON shape changes.
   - When both files exist, `greenspotter.toml` is read and the warning
     says ``adr-lint.toml is ignored; run `git rm adr-lint.toml` ``, and
     when the two texts differ it adds ``(its contents differ from
     greenspotter.toml; merge them first)``.
   - Every reader goes through `config.rs`: check, candidates (`--head`
     reads the file at a commit), cli-probe, layers, sync. Each names the
     file it actually read in its messages (`greenspotter.toml:N` or
     `adr-lint.toml:N`): `config::load` returns the name it read, and
     `contract.rs` (lines 167, 240, 281, 407, 556) and `graph/*.rs` take that
     name instead of the `FILE` constant. `check.rs:402,539` treat either
     name as the config file, so an edit to `adr-lint.toml` alone still
     widens `check`.
   - The gate pack's trigger globs list both names on both lines that
     name the file: the pre-commit check (`greengate.pack:2`) and cli-probe
     (`+greenspotter.toml,adr-lint.toml`), so the sweep's first push still
     runs both.
   - Tests:
     - (the legacy note is returned by `config::load` as data and printed once by the binary, never `eprintln!` in the library; asserted through `CARGO_BIN_EXE_greenspotter`: exactly one stderr line, `--json` stdout byte-identical)
     - only `adr-lint.toml`: its contract is enforced, with one stderr
       warning;
     - both files: `greenspotter.toml` wins, with one warning;
     - `candidates --head` at a commit that has only `adr-lint.toml`;
     - `cli-probe` with only `adr-lint.toml`;
     - `layers` prints the file it read.
2. **Formats** become `greenspotter.candidates/1`, `greenspotter.ruling/1`
   and `greenspotter.contracts/1`. They are written and read under the
   new ids only (t06). For an old row:
   - `ruling-record --check` refuses an `adr-lint.ruling/1` record, as it
     refuses any unknown token today (`ruling.rs:124`);
   - `learn --from` skips such rows as history (ADR-0076: read as unknown)
     and prints one stderr line with their count, so a ledger that holds
     old and new rows can still be learned from (test: a mixed file exits 0);
   - a row written under an old name is refused as an unknown format, as ADR-0076 says, with no hint.
   - Old `rulings.jsonl` files stay as history. Nothing in the fleet runs
     `learn` on a schedule.
3. **The decisions program**: `greentopo` from PATH, or
   `$GREENSPOTTER_GREENTOPO`. `src/aval.rs` becomes `src/greentopo.rs`, the
   arguments are unchanged, and the JSON contract is greentopo 2.0.0's,
   the same as aval 1.x. Other env vars: `GREENSPOTTER_AST_GREP`,
   `GREENSPOTTER_MESSAGE`.
4. **`sync`** writes
   ``import {greenspotter} from "@greenharness/greenspotter-eslint-config"``
   and `...greenspotter()`. The minimum version it demands is 0.20.0, so
   the hint is `pnpm add -D @greenharness/greenspotter-eslint-config@^0.20.0`
   (a carried-over 0.16.0 with a caret could never resolve on 0.x).
   - An existing `adrLint` import, from any package, is refused with exit 1
     and nothing written, naming the fix
     (``<path>:<line>: rename the adrLint import to greenspotter from @greenharness/greenspotter-eslint-config; adr-lint/* rule ids in eslint-disable comments become greenspotter/*``),
     with the path first, as every other sync failure has it (src/sync/typescript.rs:324).
     Without the refusal, sync would add a second preset beside it.
     Implement this as a read-only legacy-import preflight in `sync::run`,
     before any Rust, Python, Go or TypeScript writer runs; detecting it
     must return exit 1 immediately. Test a mixed Rust/TypeScript repository
     whose Cargo.toml needs sync edits and whose ESLint config imports
     `adrLint`: require the migration diagnostic, exit 1, and every file
     byte-identical before and after.
   - The Cargo.toml comment it writes is `# greenspotter sync: …`.
5. **Everything else is a rename**: the same commands, flags, JSON fields
   and exit codes. Version 0.20.0, because the binary, the formats and the
   package all change, and a breaking change in 0.x bumps the minor.

## Phases

### Phase 1: transfer (the agent)

Files: none in the tree; the live checkout's `.git/config` remote.

1. Pre-flight, before the transfer:
   - `gh api repos/greenharness/greenspotter` returns 404;
   - `npm view @greenharness/greenspotter-eslint-config` returns 404;
   - `gh secret list -R fredericrous/adr-lint`, `gh secret list --env release`,
     `gh variable list` and `gh variable list --env release` are recorded
     (`TAP_DEPLOY_KEY`, `NPM_TOKEN`, `PUBLISH_*`), with
     `gh api repos/fredericrous/adr-lint/environments/release/deployment-branch-policies`
     (the `v*` tag rule), to compare after;
   - `NPM_TOKEN` is confirmed to hold publish rights on the `@greenharness`
     scope (a token limited to `@aval-adr` is rotated before the tag), and
     `release.yaml:315`'s message names `@greenharness`.
2. `gh api -X POST repos/fredericrous/adr-lint/transfer -f new_owner=greenharness -f new_name=greenspotter`.
3. `git -C ~/Developer/Perso/adr-lint remote set-url origin git@github.com:greenharness/greenspotter.git`,
   and the same for this worktree, which shares the config. The
   directory keeps its name until the sweep.
4. Check the redirects the fleet depends on, before any edit:
   - `gh release download v<V> --repo fredericrous/adr-lint --pattern 'adr-lint-<V>-x86_64-unknown-linux-gnu.tar.gz'`
     for V in 0.8.0, 0.15.0, 0.15.1;
   - `git fetch https://github.com/fredericrous/adr-lint <sha>` in a
     scratch repository, then `git show FETCH_HEAD:greengate.pack`, for the
     three pack shas;
   - `git ls-remote https://github.com/fredericrous/adr-lint` resolves.

   Any failure: transfer back (Rollback) and stop.

### Phase 2: rename in the repository (this worktree)

`scripts/rename.sed` is committed, then run with GNU `sed -E -i` on text
files only, NUL-separated
(`git grep -Ilz '' | xargs -0`; xargs here is BSD). It excludes
`CHANGELOG.md`, `docs/plans/`, `scripts/rename.sed` itself (else the pass rewrites its own patterns and the stale worktrees replay a no-op), `.adr/packs/` (vendored), `greengate.conf` (its pinned pack block and `severity` line run the 0.19 binary until t10),
`.claude/hooks/` and `.claude/settings.json` (regenerated). The rules run
in this order:

| # | Expression (`\|` is a markdown escape; the file has a bare `\|`) |
|---|---|
| 0 | `s#fredericrous/adr-lint\b#greenharness/greenspotter#g; s#fredericrous/aval\b#greenharness/greentopo#g` |
| 1 | `s#@aval-adr/eslint-config#@greenharness/greenspotter-eslint-config#g; s#'aval-adr/#'greenspotter/preset-#g` (the preset config names, which rule 8 would turn into `greentopo-adr/`) |
| 2 | `s/mcp__aval__aval_/mcp__greentopo__greentopo_/g; s/mcp__aval__/mcp__greentopo__/g` |
| 3 | `s/ADR_LINT_AVAL/GREENSPOTTER_GREENTOPO/g; s/AVAL/GREENTOPO/g; s/ADR_LINT/GREENSPOTTER/g` |
| 4 | `s/adrLintTyped/greenspotterTyped/g; s/adrLintPlugin/greenspotterPlugin/g; s/AdrLintOptions/GreenspotterOptions/g; s/\bAdrLint\b/Greenspotter/g; s/\badrLint\b/greenspotter/g` |
| 5 | `s/CARGO_BIN_EXE_adr-lint\b/CARGO_BIN_EXE_greenspotter/g; s/adr_lint/greenspotter/g` (catches `refused_as_foreign_adr_lint`, `a_…_adr_lint_…` test names) |
| 6 | `s/\badr-lint\.toml\b/greenspotter.toml/g` |
| 7 | `s/\badr-lint\b/greenspotter/g; s/\\nadr-lint\b/\\ngreenspotter/g` (the format ids, gate ids and `.adr-lint-sync.tmp` included) |
| 8 | the greentopo forms: `s/\baval\.pack\b/greentopo.pack/g`, then `/^aval = "required"/!s/\baval\b/greentopo/g`, `s/\\naval\b/\\ngreentopo/g`, `s/\baval_/greentopo_/g`, `s/_aval_/_greentopo_/g` |

- Rule 3 runs `ADR_LINT_AVAL` first, so the result is
  `GREENSPOTTER_GREENTOPO`, not `GREENSPOTTER_AVAL`.
- Rule 6 runs before rule 7, so `adr-lint.toml` is named once.
- Each rule's hit count is printed before and after. A rule that matched
  before and still matches after, outside the allowlist, stops the run.

Then `git mv`:

- `adr-lint.toml` → `greenspotter.toml`;
- `src/aval.rs` → `src/greentopo.rs`;
- `packaging/homebrew/adr-lint.rb` → `greenspotter.rb`;
- `tests/fixtures/**` paths whose names hold `adr-lint`, if any.

Hand edits:

- the fallback read (Behaviour 1) in `src/config.rs` and its callers: `Config` carries the name it read and `contract::parse` takes it (`contract.rs:10`); `check.rs:402` and `check.rs:539` match both names; `candidates.rs:819-829` `--head` tries `greenspotter.toml` then `adr-lint.toml`; `cli_probe.rs:182` and `graph*.rs` name the file read; a test that staging only `adr-lint.toml` widens `check`;
  restoring `LEGACY_FILE = "adr-lint.toml"` and the trigger glob in
  `greengate.pack` after the sed;
- the `learn --from` skip of old-format rows with its one stderr count line (Behaviour 2) in `src/ruling.rs`; no hint on `ruling-record --check`;
- the ruling validator's tool identities become `greenbelay` and `relais` (`amont-agent`, greenbelay's old name, is dropped; its rows are old-format history): update `src/ruling.rs`'s `TOOLS`, diagnostics, usage, fixtures and `docs/formats.md`. Test producer-shaped `greenspotter.ruling/1` records from both tools through `ruling-record --check` and `learn --from`; require both to succeed. The greenbelay reader ticket also verifies its emitted records against these consumers before t09;
- `sync` (Behaviour 4): its `USAGE` prose and exit-status lines (`sync.rs:44-82`) rewritten by hand for the new rule; the 0.20.0 floor and hint, and the `adrLint`
  refusal, whose literal survives sed by being built with `concat!("adr", "Lint")`
  or kept on an allowlisted line;
- `Cargo.toml` and `npm/eslint-config/package.json`: version 0.20.0;
  `Cargo.lock` is regenerated by `cargo build`, and `npm/eslint-config/package-lock.json` by `npm install --package-lock-only`;
- `.github/workflows/adr.yaml`: download greentopo 2.0.0 from
  `greenharness/greentopo` (asset `greentopo-2.0.0-x86_64-unknown-linux-gnu.tar.gz`),
  run `greentopo check`, `greentopo traits --check`,
  `greentopo hook install --check`;
- `.github/workflows/release.yaml`: the tap step targets
  `tap/Formula/greenspotter.rb`, and the job summary names the new
  package; the tarball line (`release.yaml:323`) becomes
  `greenharness-greenspotter-eslint-config-$VERSION.tgz` (sed rule 8 would
  write `greentopo-adr-…`), and the `npm whoami` error (`release.yaml:315`)
  names the `@greenharness` scope; `git grep -n 'greentopo-adr'` then finds
  nothing;
- `scripts/bump-tap.py` and `tests/test_bump_tap.py`: `class Greenspotter`;
- `.claude/hooks/`: the installed aval 1.x `aval hook uninstall --write`,
  then `greentopo hook install` with the greentopo 2.0.0 release binary
  (downloaded to a scratch directory), then `hook install --check`
  exits 0;
- `AGENTS.md` and `CLAUDE.md`: regenerated with `greengate agents-md`,
  never sed;
- `CHANGELOG.md`: a 0.20.0 entry naming the rename, the fallback read, the
  new format ids and what an old ruling row does.

This repository's `greengate.conf` keeps its pinned `adr-lint` pack block
(the 0.19 binary on PATH runs it until t09; see Non-goals). Its
`adr-lint-cli-probe` gate then finds no `adr-lint.toml` and probes
nothing until t10. This is an accepted gap, written in the PR.

Run, in the worktree:

- `cargo test`;
- `cargo clippy --all-targets -- -D warnings`;
- `cargo fmt --check`;
- `python3 -I tests/test_bump_tap.py`;
- `npm test` in `npm/eslint-config`;
- `npm pack --dry-run` there;
- a type-check of a two-line consumer importing `{greenspotter, greenspotterTyped}` against `index.d.ts`, exit 0.

Then commit, review (F4b), land, open the PR, and merge on green.

### Phase 3: release 0.20.0

Files: the tap's `Formula/greenspotter.rb`, and
`decisions/docs/programmes/topo-spotter-rename/` (map and new tickets).

1. Tap seed, before the tag: `Formula/greenspotter.rb` in homebrew-tap, a
   copy of `adr-lint.rb` with the class, URLs, assets, `bin.install` and
   test renamed, through its own PR. `bump-tap.py` rewrites a formula and
   never creates one. Its URLs must match the renamed script's pattern
   (`https://github.com/greenharness/greenspotter/releases/download/v0.19.0/greenspotter-0.19.0-<target>.tar.gz`, four pairs).
   Before the tag, the renamed `bump-tap.py 0.20.0` runs on a copy of the seed
   with a four-line fake SHA256SUMS and prints `formula -> 0.20.0 (4 targets)`. `Formula/adr-lint.rb` stays at 0.19.0, and there is
   no `formula_renames.json` entry (t09 adds it).
2. Tag `v0.20.0` through `tag-release`, and read each job's `conclusion`.
3. Piloted runs with the released binaries, both downloaded to a scratch
   directory (macOS asset), in this repository at the merged main:
   - `GREENSPOTTER_GREENTOPO=<scratch>/greentopo <scratch>/greenspotter check`;
   - `<scratch>/greenspotter cli-probe`;
   - `<scratch>/greenspotter candidates --json --base origin/main~1`, whose
     `format` is `greenspotter.candidates/1`;
   - `greenspotter check` in a scratch copy holding only `adr-lint.toml`:
     one warning, and its contract enforced.
4. In decisions, through a `prog/topo-spotter-rename/build-greenspotter`
   branch and PR:
   - a map Notes line (transferred, 0.20.0 released, the fallback read
     owed to t10);
   - t08 closed;
   - two new tickets that block t09:
     - **relais**: reads `greenspotter.*/1`, spawns `greenspotter`
       (`RELAIS_GREENSPOTTER`), CI pins 0.20.0 from `greenharness/greenspotter`;
     - **greenbelay**: the same (`GREENBELAY_GREENSPOTTER`);
   - t09 gains: install greenspotter with the relais and greenbelay
     releases together, the tap rename map, and keeping `adr-lint` 0.19 as
     the real binary on PATH until then;
   - t10 gains: the greenrack 7.x move to the new eslint-config, and the
     fallback-read removal.

## Verification

| Check | Expected |
|---|---|
| `gh api repos/greenharness/greenspotter -q .full_name` | `greenharness/greenspotter` |
| old-path `gh release download` 0.8.0, 0.15.0, 0.15.1; `git fetch` of the three pack shas plus `git show FETCH_HEAD:greengate.pack`; `git ls-remote` | all succeed via redirect |
| `gh secret list` / `gh variable list -R greenharness/greenspotter`, each also with `--env release`, and the `release` environment's deployment-branch-policies | the same as before the transfer |
| `rename.sed` hit counts per rule, before → after | each > 0 → 0, outside the allowlist |
| `git grep -nIiE 'adr-lint\|adr_lint\|adrlint\|aval' -- ':!CHANGELOG.md' ':!docs/plans' ':!.adr/packs' ':!scripts/rename.sed'` | allowlist only: `LEGACY_FILE` and its tests and glob, the `adrLint` refusal and its tests, the `adr-lint.ruling/1` literal in the mixed-ledger test, the legacy warning strings and fallback matches, relais.toml's `aval = "required"`, `greengate.conf`'s pinned pack block and `severity` line |
| `cargo test` | exit 0; the five fallback tests and the mixed-ledger `learn` test `ok` |
| clippy `-D warnings`, `cargo fmt --check`, `test_bump_tap.py`, `npm test` | each exits 0 |
| `npm pack --dry-run` in `npm/eslint-config` | `@greenharness/greenspotter-eslint-config@0.20.0` |
| `target/debug/greenspotter --help`, `--version`, cli-probe on itself | names `greenspotter` only; help ≤ 80 columns |
| piloted: `target/debug/greenspotter check` with only `adr-lint.toml` (scratch copy) | its contract enforced, one `warning:` line on stderr, stdout unchanged |
| piloted: `target/debug/greenspotter ruling-record --check` on an `adr-lint.ruling/1` row | refused as an unknown format, naming the line |
| `jq '.hooks.SessionStart' .claude/settings.json` | one `greentopo-heads.sh` entry; `greentopo hook install --check` exits 0 |
| tap seed PR merged before the tag | merged |
| release run `conclusion` | `success` for build, publish, npm, tap, verify-brew |
| `npm view @greenharness/greenspotter-eslint-config version`, tap `Formula/greenspotter.rb` | 0.20.0 |
| released binaries: `greenspotter check` against the released greentopo 2.0.0, `cli-probe`, `candidates --json` | exit 0; `format` is `greenspotter.candidates/1` |
| piloted: a scratch repo importing `adrLint` from `@aval-adr/eslint-config`, then `greenspotter sync` | exit 1, file unchanged, message `rename the adrLint import to greenspotter from @greenharness/greenspotter-eslint-config` |
| `brew info fredericrous/tap/adr-lint` | still 0.19.0 |
| `git ls-remote https://github.com/greenharness/greenspotter refs/tags/v0.20.0` and that tag's `greengate.pack` | resolves; the gate ids are `greenspotter-*` (the pin regeneration itself is t10) |
| decisions PR | merged; `relais programme check` exits 0 |

## After release

- **Daily until t10** (the redirect is the only thing holding the pins
  until t10 repoints them): probe the three old download paths and the
  three pack shas with the Phase 1 step 4 commands, and record each result
  in the map. Expected 0 failures.
- **For the first 7 days**, also count fleet runs that failed at the
  download step or a pack fetch:
  `gh run list -R <repo> --status failure --json databaseId,createdAt`
  per pinned repository, then
  `gh run view <id> -R <repo> --log-failed | grep -c 'fredericrous/adr-lint'`
  for each. Expected 0.
- If any probe or run fails and no fix lands within a day, transfer back.

## Rollback

- Transfer: `gh api -X POST repos/greenharness/greenspotter/transfer -f new_owner=fredericrous -f new_name=adr-lint`.
  Nothing is created at the old name, so this stays possible.
- Before the tag: close the PR. After the tag: a 0.20.1 fix. A transfer back does not unpublish the npm package, the tap formula or the release, so it is not a rollback of the release. Tags are never
  moved.

## Decision log

- 2026-11-24: one fallback read of `adr-lint.toml`, as greentopo's for
  `aval:` packs (ADR-0073), because without it greenspotter silently drops
  an unswept repository's contracts. The alternative was a loud refusal.
- 2026-11-24: version 0.20.0, not 1.0.0: a rename is no stability claim.
- 2026-11-24: the reader renames in relais and greenbelay are new tickets
  that block t09. Until t09, `adr-lint` 0.19 stays the real binary on PATH. The two reader tickets carry an owner and a target date in the map, and the programme map checks them 14 days after the 0.20.0 release.

<!-- panel: repos=adr-lint,decisions,homebrew-tap adds=contract,ops reviewers=backend,lang:rust,lang:typescript,unix,tui,po,architect,platform body-sha=94498b3b8c21 -->
