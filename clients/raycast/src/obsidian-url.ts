import { basename } from "node:path";
import type { Schemas } from "@trove/client";

/**
 * An `obsidian://open` link to the result's note at its best heading.
 *
 * Obsidian names a vault after its folder, and a vault is registered with Trove as one source,
 * so the source folder's name is the vault name.
 */
export function obsidianUrl({ source, path, heading_path }: Schemas["SearchResult"]): string {
  const note = path.replace(/\.md$/, "");
  const heading = heading_path.at(-1);
  const file = heading === undefined ? note : `${note}#${heading}`;

  // Not URLSearchParams: it encodes spaces as `+`, which Obsidian keeps as a literal `+`.
  return `obsidian://open?vault=${encodeURIComponent(basename(source))}&file=${encodeURIComponent(file)}`;
}
