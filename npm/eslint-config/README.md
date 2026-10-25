# @aval-adr/eslint-config

ESLint flat config for the decided code rules (ADR-0044) that a linter matches
exactly. Versioned with the `adr-lint` crate.

```js
import { adrLint, adrLintTyped } from "@aval-adr/eslint-config";

export default [
  ...adrLint({
    restrictedImports: [
      { files: ["src/domain/**"], paths: ["fs"], patterns: [{ group: ["@mui/*"] }] },
    ],
  }),
];
```

- `adrLint({ restrictedImports = [] })` returns the config array. Each entry is
  `{ files?, paths?, patterns? }`; entries with the same `files` (absent = all
  files) are concatenated into ONE `@typescript-eslint/no-restricted-imports`
  option, because flat config keeps only the last options of a rule. No entries,
  no such object.
- `adrLintTyped(options)` is `adrLint(options)` plus the typed rule
  `switch-exhaustiveness-check` and `parserOptions.projectService: true`; it
  needs a `tsconfig.json`. `adrLint` alone sets no typed rule and no parserOptions.

## Rules and the decided rule each enforces

| rule | decided rule |
|---|---|
| `linterOptions.reportUnusedDisableDirectives: "error"` | `general.no-disabled-safety` |
| `@typescript-eslint/ban-ts-comment` (`ts-expect-error: allow-with-description`, `ts-ignore`, `ts-nocheck` banned) | `general.no-disabled-safety` |
| `@eslint-community/eslint-comments/no-unlimited-disable`, `require-description`, `disable-enable-pair` | `general.no-disabled-safety` |
| `adr-lint/no-swallowed-catch` | `errors.never-swallowed` |
| `@typescript-eslint/no-restricted-imports` (from `restrictedImports`) | `architecture.dependency-rule`, `domain.independent-of-infrastructure` |
| `@typescript-eslint/switch-exhaustiveness-check` (typed only) | `types.exhaustive-matching` |

## Narrowed, and why

- **errors.never-swallowed:** ESLint's `no-empty` flags every empty block, and
  also `catch {} // why`, which the rule body allows. `adr-lint/no-swallowed-catch`
  reports only a `catch` with no statement and no comment anywhere in the clause
  (`catch (e) /* why */ {}` included) or on its closing-brace line.
- **general.no-disabled-safety:** `require-description` asks a reason only of the
  directives that suppress; `eslint-enable`, `global(s)`, `exported` and
  `eslint-env` suppress nothing and need none.

`switch-exhaustiveness-check` keeps its stock options, which are the rule body:
a missing case is reported even behind a silent `default`, and a full switch
whose `default` assigns to `never` passes.

## Deferred

- **components.acyclic-dependencies:** `import/no-cycle` is deferred. It needs
  `eslint-plugin-import-x` and a TS resolver in every consumer, and its lint
  time is unmeasured.

## Dependencies

- `@eslint-community/eslint-plugin-eslint-comments`: the maintained home of the
  disable-directive rules; owning them would mean parsing directive comments
  ourselves. It has no build script and no runtime dependencies beyond
  `escape-string-regexp` and `ignore`; if abandoned, the three rules move into
  `adr-lint/` here.
- `eslint` and `typescript-eslint` are peer dependencies the consumer already has.
