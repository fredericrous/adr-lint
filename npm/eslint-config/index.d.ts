import type { Linter } from "eslint";

export interface RestrictedImport {
  /** Globs the restriction applies to; absent means all files. */
  files?: string[];
  paths?: Array<
    string | { name: string; message?: string; importNames?: string[] }
  >;
  patterns?: Array<
    string | { group: string[]; message?: string; caseSensitive?: boolean }
  >;
}

export interface GreenspotterOptions {
  restrictedImports?: RestrictedImport[];
}

export function greenspotter(options?: GreenspotterOptions): Linter.Config[];
export function greenspotterTyped(options?: GreenspotterOptions): Linter.Config[];

export const base: Linter.Config[];
export const effect: Linter.Config[];
/** The `@typescript-eslint/no-restricted-imports` entry `effect` sets. */
export const serverImportBans: Linter.RuleEntry;
export const tests: Linter.Config[];
