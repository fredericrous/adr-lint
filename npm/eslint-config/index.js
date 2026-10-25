import eslintComments from "@eslint-community/eslint-plugin-eslint-comments";
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
        if (sourceCode.getCommentsInside(node.body).length > 0) return;
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

const adrLintPlugin = { rules: { "no-swallowed-catch": noSwallowedCatch } };

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
  return [...groups.values()].map((group) => ({
    ...(group.files === undefined ? {} : { files: group.files }),
    rules: {
      "@typescript-eslint/no-restricted-imports": [
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
        "@eslint-community/eslint-comments/require-description": "error",
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
        "@typescript-eslint/switch-exhaustiveness-check": [
          "error",
          {
            considerDefaultExhaustiveForUnions: true,
            requireDefaultForNonUnion: false,
          },
        ],
      },
    },
  ];
}
