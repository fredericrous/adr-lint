# Changelog

## Unreleased

- BREAKING: candidate ids now also hash the 1-based column of the match, so
  two matches on one line (two `.unwrap()` calls) get distinct ids. This
  changes every id.
- The failures.expected-vs-defect candidate patterns no longer match in test
  files nor inside a Rust `mod` carrying `#[cfg(test)]`.
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
