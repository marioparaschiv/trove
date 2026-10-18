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

/** Filename matches first, then the Trove results for notes not already listed. */
export function merge(files: FileSuggestion[], trove: TroveResult[]): Suggestion[] {
  const listed = new Set(files.map((file) => file.path));
  const added = trove
    .filter((result) => !listed.has(result.path))
    .map((result): Suggestion => ({ kind: "trove", result }));

  return [...files, ...added];
}
