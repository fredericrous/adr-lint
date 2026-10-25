# Formats

adr-lint owns two documented formats (ADR-0044, `harness.tool-boundaries`).
Other tools read them only through the version token each carries, never by
parsing anything else of adr-lint's (ADR-0039, `harness.cross-tool-reads`).
The owner is the lower, more stable tool, so the readers and writers conform
to this page.

Stability promise, for both formats: within one major version of a token
(`/1`), a field is never removed and never re-meant; a field may be added. A
breaking change gets a new major token (`/2`), and a reader that meets a token
it does not know refuses the data instead of guessing.

Paths in every adr-lint output use `/` separators on every OS.

## `adr-lint.candidates/1`

Written by `adr-lint candidates --json`; read by amont-agent and relais, which
put the candidates in a reviewer's brief. One JSON object on stdout:

```json
{
  "format": "adr-lint.candidates/1",
  "base": "<merge-base sha>",
  "head": "<HEAD sha>",
  "candidates": [
    {"id": "<64 hex>", "rule": "names.reveal-intent", "pattern": "<message>",
     "path": "src/a.rs", "line": 6, "col": 9, "text": "let x = 2;"}
  ],
  "overflow": {"<rule id>": 3}
}
```

- `format`: the version token.
- `base`: the merge-base of HEAD and the `--base` ref, the commit the change
  is measured from. `head`: the HEAD commit the diff was read at.
- `candidates`: the matches on lines the diff adds, at most `--max`.
  - `id`: lowercase hex sha256 of the rule id, a NUL byte, the repository-
    relative path with `/` separators, a NUL byte, the line text with
    whitespace trimmed and collapsed to single spaces, a NUL byte and the
    decimal occurrence number of the line among the diff's added lines in
    that file with the same normalised text, in file order (1 for the
    first), whether or not an earlier one was itself a candidate. The same
    edit has the same id on any machine; identical added lines in one file
    have distinct ids.
  - `rule`: the aval rule id. `pattern`: the message of the pattern that
    matched. `path`: repository-relative, `/` separators. `line`, `col`:
    1-based position of the match. `text`: the normalised line text.
- `overflow`: per rule id, how many candidates `--max` cut. Always present;
  `{}` when nothing was cut.

A candidate is a line to look at, not a finding.

## `adr-lint.ruling/1`

JSON Lines: one object per line, written when a reviewer rules on a candidate.
Written by amont-agent and relais (the `tool` field says which). The only
reader today is adr-lint, through `adr-lint ruling-record --check`; no other
tool reads these records.

```json
{"format": "adr-lint.ruling/1", "review": "<reviewer id>", "tool": "relais",
 "repo": "adr-lint", "tree": "<64 hex>", "candidate": "<64 hex>",
 "rule": "names.reveal-intent", "pattern": "<pattern message>",
 "ruling": "fine", "clause": "<rule-body clause>", "at": "2026-10-24T12:00:00Z"}
```

- `format`: the version token.
- `review`: the reviewer's id, such as a Claude Code agent id or a relais
  dispatch id.
- `tool`: `"amont-agent"` or `"relais"`, the tool that wrote the record.
- `repo`: the repository name.
- `tree`: the 64-hex content id of the tree the review bound.
- `candidate`: the 64-hex candidate `id` ruled on.
- `rule`, `pattern`: the candidate's rule id and pattern message.
- `ruling`: `"violation"` or `"fine"`.
- `clause`: for `fine`, the rule-body clause quoted that makes the line fine;
  for `violation`, the finding text.
- `at`: an RFC 3339 UTC time in the profile `YYYY-MM-DDTHH:MM:SS[.fraction]Z`
  (uppercase `T` and `Z`, no numeric offset), as `2026-10-24T12:00:00Z`.

Records are keyed by (`review`, `candidate`). A reader keeps the last record
for a key, so a reviewer corrects a ruling by writing a new line.

`adr-lint ruling-record --check <file>` validates a file: exit 0 when every
line is a valid record, 1 naming the first bad line and why, 2 on a usage
error, 3 when the file cannot be read.
