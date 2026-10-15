import { posix, win32 } from "node:path";
import type { Schemas } from "@trove/client";
import type { SearchResult as FuzzyMatch } from "obsidian";

export type TroveResult = Schemas["SearchResult"];

/** A note whose path matches the query; `match` is `null` for recent files shown on an empty query. */
export interface FileSuggestion {
  kind: "file";
  path: string;
  match: FuzzyMatch | null;
}

export type Suggestion = FileSuggestion | { kind: "trove"; result: TroveResult };

/** Whether two absolute folder paths name the same folder, the way `platform` compares paths. */
export function isSameFolder(a: string, b: string, platform: NodeJS.Platform): boolean {
  if (platform === "win32") {
    return win32.resolve(a).toLowerCase() === win32.resolve(b).toLowerCase();
  }

  return posix.resolve(a) === posix.resolve(b);
}

/** Keeps the results from the vault's folder; Trove may index other sources too. */
export function inVault(
  results: TroveResult[],
  vaultPath: string,
  platform: NodeJS.Platform,
): TroveResult[] {
  return results.filter((result) => isSameFolder(result.source, vaultPath, platform));
}

/** Filename matches first, then the Trove results for notes not already listed. */
export function merge(files: FileSuggestion[], trove: TroveResult[]): Suggestion[] {
  const listed = new Set(files.map((file) => file.path));
  const added = trove
    .filter((result) => !listed.has(result.path))
    .map((result): Suggestion => ({ kind: "trove", result }));

  return [...files, ...added];
}
