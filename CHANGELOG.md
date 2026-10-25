# Changelog

## Unreleased

- New npm package `@aval-adr/eslint-config` (`npm/eslint-config/`): `adrLint` and
  `adrLintTyped` flat-config factories for the decided code rules a linter
  matches exactly, published by the release workflow when `PUBLISH_NPM` is `true`.

## v0.5.0

- `amont.pack` declares the `adr-lint check` row, for amont and for amont-agent's write-time hook,
  at severity `warn`.
- `adr-lint check --json` findings carry `text`: the source line with
  whitespace trimmed and collapsed, normalized as `candidates` ids are.
- `check` skips a named or `$AMONT_FILES` path that is missing and that git
  reports as deleted, instead of exiting 3; other missing paths still exit 3.

## v0.4.0

- `adr-lint candidates --head <ref>` measures the change from the merge-base
  of <ref> and `--base` to <ref>, reading every changed file's text from
  <ref>'s git objects instead of the working tree, so a commit that is not
  checked out can be listed. The JSON `head` is then the sha of <ref>.
- A changed symlink is no longer scanned in either mode; before, the
  working-tree run followed a symlink to a source file.

## v0.3.0

- BREAKING: candidate ids now also hash the 1-based column of the match, so
  two matches on one line (two `.unwrap()` calls) get distinct ids. This
  changes every id.
- The failures.expected-vs-defect candidate patterns no longer match in test
  files nor inside a Rust `mod` carrying `#[cfg(test)]`.
- A file named `tests.rs` (Rust's out-of-line test module) now counts as a
  test file, for failures.expected-vs-defect and also for tests.no-real-waits,
  whose sleeps in `tests.rs` are now candidates.
- docs/formats.md: `adr-lint learn` is described as not yet shipped, and
  renaming a pattern id is documented as a breaking change within `/1`.

## v0.2.0

- Candidate ids now include the occurrence number of the line among the
  file's added identical lines, so identical added lines get distinct ids.
  This changes every id; acceptable before any consumer exists.
- Paths in all output use `/` separators.
- `adr-lint candidates --json` prints a versioned `adr-lint.candidates/1`
  document `{format, base, head, candidates, overflow}` instead of a bare array.
- docs/formats.md documents the candidates document and the
  `adr-lint.ruling/1` ruling-record JSONL.
- `adr-lint ruling-record --check <file>` validates a ruling-record file.

## v0.1.0

The first release (decisions ADR-0044).


- `adr-lint coverage`: checks every active governs:code constraint against `map/rules.toml`.
- `adr-lint check`: runs the exact ast-grep rules embedded in the binary.
- `adr-lint candidates`: prints candidate matches on added lines for reviewers.
