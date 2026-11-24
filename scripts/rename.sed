# adr-lint -> greenspotter (ADR-0072, ADR-0074). GNU sed -E; run once over the
# tracked text files, minus CHANGELOG.md, docs/plans/, scripts/rename.sed,
# .adr/packs/, greengate.conf, .claude/hooks/ and .claude/settings.json.
# Rules run in order; each is a row of docs/plans/2026-11-24-adr-lint-becomes-greenspotter.md.

# 0. the repository moved by transfer
s#fredericrous/adr-lint\b#greenharness/greenspotter#g
s#fredericrous/aval\b#greenharness/greentopo#g
# 1. npm scope and preset config names
s#@aval-adr/eslint-config#@greenharness/greenspotter-eslint-config#g
s#'aval-adr/#'greenspotter/preset-#g
# 2. MCP tool ids
s/mcp__aval__aval_/mcp__greentopo__greentopo_/g
s/mcp__aval__/mcp__greentopo__/g
# 3. environment
s/ADR_LINT_AVAL/GREENSPOTTER_GREENTOPO/g
s/AVAL/GREENTOPO/g
s/ADR_LINT/GREENSPOTTER/g
# 4. identifiers
s/adrLintTyped/greenspotterTyped/g
s/adrLintPlugin/greenspotterPlugin/g
s/AdrLintOptions/GreenspotterOptions/g
s/\bAdrLint\b/Greenspotter/g
s/\badrLint\b/greenspotter/g
# 5. test and symbol names
s/CARGO_BIN_EXE_adr-lint\b/CARGO_BIN_EXE_greenspotter/g
s/adr_lint/greenspotter/g
# 6. the config file, before the bare name
s/\badr-lint\.toml\b/greenspotter.toml/g
# 7. the bare name, formats, gate ids, temp files
s/\badr-lint\b/greenspotter/g
s/\\nadr-lint\b/\\ngreenspotter/g
# 8. greentopo forms
s/\baval\.pack\b/greentopo.pack/g
/^aval = "required"/!s/\baval\b/greentopo/g
s/\\naval\b/\\ngreentopo/g
s/\baval_/greentopo_/g
s/_aval_/_greentopo_/g
