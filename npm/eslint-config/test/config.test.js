import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { ESLint } from "eslint";
import { adrLint, adrLintTyped } from "../index.js";

const fixtureDir = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "fixture",
);
const RESTRICTED = "@typescript-eslint/no-restricted-imports";

async function lint(config, code, fileName = "sample.ts") {
  const eslint = new ESLint({
    overrideConfigFile: true,
    overrideConfig: config,
    cwd: fixtureDir,
  });
  const [result] = await eslint.lintText(code, {
    filePath: path.join(fixtureDir, fileName),
  });
  return result.messages.map((message) => message.ruleId);
}

const base = adrLint();
const withConsole = [...base, { rules: { "no-console": "error" } }];

test("ts-ignore fires; ts-expect-error with a description passes", async () => {
  assert.ok(
    (await lint(base, "// @ts-ignore\nconst a: number = 'x';\n")).includes(
      "@typescript-eslint/ban-ts-comment",
    ),
  );
  assert.deepEqual(
    await lint(
      base,
      "// @ts-expect-error -- upstream types lack X\nexport const a: number = 'x';\n",
    ),
    [],
  );
});

test("ts-nocheck fires", async () => {
  assert.ok(
    (await lint(base, "// @ts-nocheck\nexport {};\n")).includes(
      "@typescript-eslint/ban-ts-comment",
    ),
  );
});

test("a blanket eslint-disable fires", async () => {
  assert.ok(
    (await lint(base, "/* eslint-disable */\nexport {};\n")).includes(
      "@eslint-community/eslint-comments/no-unlimited-disable",
    ),
  );
});

test("a disable without a description fires; with one passes", async () => {
  assert.ok(
    (
      await lint(
        withConsole,
        "// eslint-disable-next-line no-console\nconsole.log(1);\n",
      )
    ).includes("@eslint-community/eslint-comments/require-description"),
  );
  assert.deepEqual(
    await lint(
      withConsole,
      "// eslint-disable-next-line no-console -- reason\nconsole.log(1);\n",
    ),
    [],
  );
});

test("an unused disable directive fires", async () => {
  const ids = await lint(
    base,
    "// eslint-disable-next-line no-console -- reason\nexport const a = 1;\n",
  );
  assert.equal(ids.length, 1);
});

test("an unpaired eslint-disable fires disable-enable-pair", async () => {
  const ids = await lint(
    withConsole,
    "/* eslint-disable no-console -- reason */\nconsole.log(1);\n",
  );
  assert.ok(
    ids.includes("@eslint-community/eslint-comments/disable-enable-pair"),
  );
});

test("an empty catch with no comment fires", async () => {
  assert.deepEqual(await lint(base, "try { f(); } catch (e) {}\n"), [
    "adr-lint/no-swallowed-catch",
  ]);
});

test("a catch with a comment inside passes", async () => {
  assert.deepEqual(
    await lint(
      base,
      "try { f(); } catch (e) { /* the file may not exist yet */ }\n",
    ),
    [],
  );
});

test("a catch with a comment on the closing-brace line passes", async () => {
  assert.deepEqual(
    await lint(base, "try { f(); } catch {} // optional cache\n"),
    [],
  );
});

test("a catch with a statement passes", async () => {
  assert.deepEqual(
    await lint(base, "try { f(); } catch (e) { g(e); }\n"),
    [],
  );
});

test("an empty non-catch block is not reported", async () => {
  assert.deepEqual(await lint(base, "if (x) {}\n"), []);
});

test("restrictedImports entries without files merge into one option", async () => {
  const config = adrLint({
    restrictedImports: [
      { patterns: [{ group: ["@mui/*"] }] },
      { patterns: [{ group: ["kysely"] }] },
    ],
  });
  assert.equal(config.filter((entry) => entry.rules?.[RESTRICTED]).length, 1);
  const ids = await lint(
    config,
    "import a from '@mui/material';\nimport b from 'kysely';\nexport { a, b };\n",
  );
  assert.equal(ids.filter((id) => id === RESTRICTED).length, 2);
});

test("entries with different files make two objects", () => {
  const config = adrLint({
    restrictedImports: [
      { files: ["src/domain/**"], paths: ["fs"] },
      { files: ["src/ui/**"], paths: ["pg"] },
      { files: ["src/domain/**"], paths: ["net"] },
    ],
  });
  const restricting = config.filter((entry) => entry.rules?.[RESTRICTED]);
  assert.equal(restricting.length, 2);
  assert.deepEqual(restricting[0].rules[RESTRICTED][1].paths, ["fs", "net"]);
});

test("no entries emit no restricted-imports object", () => {
  assert.equal(
    adrLint().filter((entry) => entry.rules?.[RESTRICTED]).length,
    0,
  );
});

test("adrLint alone sets no parserOptions and no typed rule", () => {
  for (const entry of adrLint()) {
    assert.equal(entry.languageOptions?.parserOptions, undefined);
    assert.equal(
      entry.rules?.["@typescript-eslint/switch-exhaustiveness-check"],
      undefined,
    );
  }
});

const typed = adrLintTyped();

async function lintFixture(config, fileName) {
  const source = await readFile(path.join(fixtureDir, fileName), "utf8");
  return lint(config, source, fileName);
}

test("typed: a switch missing a member without default fires", async () => {
  assert.ok(
    (await lintFixture(typed, "missing.ts")).includes(
      "@typescript-eslint/switch-exhaustiveness-check",
    ),
  );
});

test("typed: a default assigning to never passes", async () => {
  assert.deepEqual(await lintFixture(typed, "exhaustive.ts"), []);
});

test("typed: a missing member behind a silent default fires", async () => {
  assert.ok(
    (await lintFixture(typed, "silent-default.ts")).includes(
      "@typescript-eslint/switch-exhaustiveness-check",
    ),
  );
});

test("a comment between catch and its brace passes", async () => {
  assert.deepEqual(
    await lint(base, "try { f(); } catch (e) /* the cache is optional */ {}\n"),
    [],
  );
});

test("an eslint-enable without a description passes", async () => {
  assert.deepEqual(
    await lint(
      withConsole,
      "/* eslint-disable no-console -- the CLI prints */\nconsole.log(1);\n/* eslint-enable no-console */\n",
    ),
    [],
  );
});

test("a group without files covers the source files only", () => {
  const [group] = adrLint({ restrictedImports: [{ paths: ["fs"] }] }).filter(
    (entry) => entry.rules?.[RESTRICTED],
  );
  assert.ok(Array.isArray(group.files) && group.files.length > 0);
});

test("it loads beside typescript-eslint's recommended configs", async () => {
  const tseslint = (await import("typescript-eslint")).default;
  const config = [...tseslint.configs.recommended, ...adrLint()];
  assert.deepEqual(await lint(config, "export const sides = 4;\n"), []);
});
