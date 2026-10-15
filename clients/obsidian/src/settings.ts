export interface Settings {
  /** A `trove` binary to run instead of the pinned release the plugin downloads. Empty uses the release. */
  binaryPath: string;
}

export const DEFAULT_SETTINGS: Settings = { binaryPath: "" };

/** Checks the plugin's saved data; `null` (nothing saved yet) gives the defaults. */
export function parseSettings(data: unknown): Settings {
  if (data === null) {
    return { ...DEFAULT_SETTINGS };
  }

  if (typeof data !== "object" || Array.isArray(data)) {
    throw new Error(`Trove settings must be an object, got ${JSON.stringify(data)}`);
  }

  const { binaryPath = DEFAULT_SETTINGS.binaryPath, ...rest } = data as Record<string, unknown>;
  const unknown = Object.keys(rest);
  if (unknown.length > 0) {
    throw new Error(`unknown Trove settings: ${unknown.join(", ")}`);
  }

  if (typeof binaryPath !== "string") {
    throw new Error(`Trove setting binaryPath must be a string, got ${JSON.stringify(binaryPath)}`);
  }

  return { binaryPath };
}
