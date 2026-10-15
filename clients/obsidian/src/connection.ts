import { connect, type TroveClient } from "@trove/client";
import { stripHeadingForLink } from "obsidian";
import { obsidianFetch } from "./obsidian-fetch.ts";
import { isSameFolder, type TroveResult } from "./suggestions.ts";

/** Connects to Trove, starting it when it isn't running. */
export async function connectTo(binary: string): Promise<TroveClient> {
  const { client } = await connect({ binary, fetch: obsidianFetch });

  return client;
}

/** The source folder registered for the vault, as Trove spells it; `undefined` when the vault isn't one. */
export async function vaultSource(
  client: TroveClient,
  vaultPath: string,
): Promise<string | undefined> {
  const { data, error, response } = await client.GET("/sources");
  if (data === undefined) {
    throw failure("listing sources", response, error);
  }

  return data.find((source) => isSameFolder(source.path, vaultPath, process.platform))?.path;
}

/** A link to a Trove result's note, at its heading. */
export function noteLink({ path, heading_path }: TroveResult): string {
  return [path, ...heading_path.map(stripHeadingForLink)].join("#");
}

export function failure(
  action: string,
  response: Response,
  error: { error: string } | undefined,
): Error {
  return new Error(
    `${action} failed with ${response.status}: ${error?.error ?? response.statusText}`,
  );
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
