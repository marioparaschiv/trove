import { posix, win32 } from "node:path";
import type { TroveResult } from "./suggestions.ts";

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
