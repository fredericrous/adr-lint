import effectPlugin from "@effect/eslint-plugin";
import js from "@eslint/js";
import eslintComments from "@eslint-community/eslint-plugin-eslint-comments";
import prettier from "eslint-config-prettier";
import tseslint from "typescript-eslint";

const SOURCE_FILES = ["**/*.{ts,tsx,mts,cts,js,jsx,mjs,cjs}"];

const noSwallowedCatch = {
  meta: {
    type: "problem",
    schema: [],
    messages: {
      swallowed:
        "An empty catch swallows the error: handle it, or say in a comment why the failure is irrelevant.",
    },
  },
  create(context) {
    const { sourceCode } = context;
    return {
      CatchClause(node) {
        if (node.body.body.length > 0) return;
        // A comment anywhere in the clause, `catch (e) /* why */ {}` included.
        if (sourceCode.getCommentsInside(node).length > 0) return;
        const closingBrace = sourceCode.getLastToken(node.body);
        const next = sourceCode.getTokenAfter(closingBrace, {
          includeComments: true,
        });
        const commentOnClosingLine =
          next !== null &&
          (next.type === "Line" || next.type === "Block") &&
          next.loc.start.line === closingBrace.loc.end.line;
        if (commentOnClosingLine) return;
        context.report({ node, messageId: "swallowed" });
      },
    };
  },
};

const adrLintPlugin = {
  rules: {
    "no-swallowed-catch": noSwallowedCatch,
    // typescript-eslint's rule under adr-lint's own id, so a preset's
    // `@typescript-eslint/no-restricted-imports` does not replace the decided bans.
    "no-restricted-imports": tseslint.plugin.rules["no-restricted-imports"],
  },
};

function restrictedImportConfigs(restrictedImports) {
  const groups = new Map();
  for (const entry of restrictedImports) {
    const key = entry.files === undefined ? "" : JSON.stringify(entry.files);
    const group = groups.get(key) ?? {
      files: entry.files,
      paths: [],
      patterns: [],
    };
    group.paths.push(...(entry.paths ?? []));
    group.patterns.push(...(entry.patterns ?? []));
    groups.set(key, group);
  }
  // A group without `files` covers the source files only: the parser and the
  // `@typescript-eslint` plugin are registered for those, and a consumer that
  // also lints .json or .md would otherwise get a missing-plugin error.
  return [...groups.values()].map((group) => ({
    files: group.files ?? SOURCE_FILES,
    rules: {
      "adr-lint/no-restricted-imports": [
        "error",
        { paths: group.paths, patterns: group.patterns },
      ],
    },
  }));
}

export function adrLint({ restrictedImports = [] } = {}) {
  return [
    {
      name: "adr-lint/base",
      files: SOURCE_FILES,
      languageOptions: { parser: tseslint.parser },
      linterOptions: { reportUnusedDisableDirectives: "error" },
      plugins: {
        "@typescript-eslint": tseslint.plugin,
        "@eslint-community/eslint-comments": eslintComments,
        "adr-lint": adrLintPlugin,
      },
      rules: {
        "@typescript-eslint/ban-ts-comment": [
          "error",
          {
            "ts-expect-error": "allow-with-description",
            "ts-ignore": true,
            "ts-nocheck": true,
            "ts-check": false,
          },
        ],
        "@eslint-community/eslint-comments/no-unlimited-disable": "error",
        // Only the directives that suppress need a reason: the rule's default
        // would also demand one on eslint-enable, global(s), exported and
        // eslint-env, which suppress nothing (general.no-disabled-safety).
        "@eslint-community/eslint-comments/require-description": [
          "error",
          { ignore: ["eslint-enable", "global", "globals", "exported", "eslint-env"] },
        ],
        "@eslint-community/eslint-comments/disable-enable-pair": "error",
        "adr-lint/no-swallowed-catch": "error",
      },
    },
    ...restrictedImportConfigs(restrictedImports),
  ];
}

export function adrLintTyped(options = {}) {
  return [
    ...adrLint(options),
    {
      name: "adr-lint/typed",
      files: SOURCE_FILES,
      languageOptions: { parserOptions: { projectService: true } },
      rules: {
        // The stock options are the rule body: a missing case is reported even
        // behind a silent `default`, and a full switch whose `default`
        // assigns to `never` passes (allowDefaultCaseForExhaustiveSwitch).
        "@typescript-eslint/switch-exhaustiveness-check": "error",
      },
    },
  ];
}

export const base = tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    name: 'aval-adr/base',
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
          varsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },
  prettier,
)

const KYSELY_MESSAGE =
  'DB access in this stack is @effect/sql (SqlClient + Migrator + SqlSchema) — Kysely duplicates it outside the Effect runtime.'

const OTEL_BARREL_MESSAGE =
  'Import the subpath you need (e.g. @effect/opentelemetry/NodeSdk) — the barrel re-exports WebSdk, which statically imports @opentelemetry/sdk-trace-web and crashes a Node service at module load.'

const OTEL_BARREL_PATH = {
  allowTypeImports: true,
  name: '@effect/opentelemetry',
  message: OTEL_BARREL_MESSAGE,
}

const KYSELY_PATTERN = {
  allowTypeImports: true,
  group: ['kysely', 'kysely-*', '@kysely/*'],
  message: KYSELY_MESSAGE,
}

export const effect = tseslint.config(...base, {
  name: 'aval-adr/effect-policy',
  plugins: {'@effect': effectPlugin},
  rules: {
    '@effect/no-import-from-barrel-package': 'warn',
    // See the react preset: type imports erase, so they bypass nothing here.
    // The OTel barrel ban exists because WebSdk's transitive imports crash a
    // Node service *at module load* — which a type import never triggers.
    'no-restricted-imports': 'off',
    '@typescript-eslint/no-restricted-imports': [
      'error',
      {
        paths: [OTEL_BARREL_PATH],
        patterns: [KYSELY_PATTERN],
      },
    ],
  },
})

const TESTID_MESSAGE =
  'Select by what users and assistive tech perceive — getByRole/getByLabel/getByText — not test ids. If nothing selectable exists, the control is not accessible; fix the component.'

const CSS_LOCATOR_MESSAGE =
  'CSS class/id locators are implementation details that drift. Use getByRole/getByLabel/getByText; gate readiness on aria-busy over a named region, not a class hook.'

export const tests = [
  {
    name: 'aval-adr/tests',
    files: [
      '**/*.test.{ts,tsx,js,jsx}',
      '**/*.spec.{ts,tsx,js,jsx}',
      '**/e2e/**/*.{ts,tsx,js,jsx}',
      '**/__tests__/**/*.{ts,tsx,js,jsx}',
    ],
    rules: {
      'no-restricted-syntax': [
        'error',
        {
          selector: 'CallExpression[callee.property.name=/^(get|query|find)(All)?ByTestId$/]',
          message: TESTID_MESSAGE,
        },
        {
          selector: 'CallExpression[callee.name=/^(get|query|find)(All)?ByTestId$/]',
          message: TESTID_MESSAGE,
        },
        {
          selector:
            'CallExpression[callee.property.name="locator"] > Literal.arguments:first-child[value=/^[.#]/]',
          message: CSS_LOCATOR_MESSAGE,
        },
      ],
    },
  },
]
