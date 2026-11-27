---
canonical: decisions:docs/plans/2026-11-27-topo3-tool-repos.md
phases: [2]
status: done
---
Part of [Move the tool repositories to greentopo 3.0.0](https://github.com/fredericrous/decisions/blob/main/docs/plans/2026-11-27-topo3-tool-repos.md)
(topo-spotter-rename t24).

## Code review

- adr-lint: none — CI pin, re-vendored pack, regenerated hook and gate block, no code

## Implementation review
- approve (opus, 47k, 39 s); no findings; coverage exit 0 against the re-vendored corpus.

Pack: sha256 `ef66e76480492ccd93dc6f042efa00142550d97666d8973f262ecb4b91c662e4`, from decisions `57b484bb50c0`, the same in all seven repositories.
