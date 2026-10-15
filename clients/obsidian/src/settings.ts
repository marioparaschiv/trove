export interface Settings {
  /** A `trove` binary to run instead of the pinned release the plugin downloads. Empty uses the release. */
  binaryPath: string;
  /** The vault folder category notes go in, `/`-separated, without leading or trailing `/`. */
  categoryFolder: string;
}

export const DEFAULT_SETTINGS: Settings = { binaryPath: "", categoryFolder: "Categories" };

/** Checks the plugin's saved data; `null` (nothing saved yet) gives the defaults. */
export function parseSettings(data: unknown): Settings {
  if (data === null) {
    return { ...DEFAULT_SETTINGS };
  }

  if (typeof data !== "object" || Array.isArray(data)) {
    throw new Error(`Trove settings must be an object, got ${JSON.stringify(data)}`);
  }

  const {
    binaryPath = DEFAULT_SETTINGS.binaryPath,
    categoryFolder = DEFAULT_SETTINGS.categoryFolder,
    ...rest
  } = data as Record<string, unknown>;
  const unknown = Object.keys(rest);
  if (unknown.length > 0) {
    throw new Error(`unknown Trove settings: ${unknown.join(", ")}`);
  }

  if (typeof binaryPath !== "string") {
    throw new Error(`Trove setting binaryPath must be a string, got ${JSON.stringify(binaryPath)}`);
  }

  if (typeof categoryFolder !== "string" || parseFolder(categoryFolder) !== categoryFolder) {
    throw new Error(
      `Trove setting categoryFolder must be a folder path inside the vault, got ${JSON.stringify(categoryFolder)}`,
    );
  }

  return { binaryPath, categoryFolder };
}

/** A folder path typed by the user as a vault folder (`/`-separated, no leading or trailing `/`); `undefined` when it isn't one. */
export function parseFolder(text: string): string | undefined {
  const parts = text
    .trim()
    .split("/")
    .filter((part) => part !== "");
  const valid =
    parts.length > 0 &&
    parts.every((part) => part !== "." && part !== ".." && !part.includes("\\"));

  return valid ? parts.join("/") : undefined;
}
