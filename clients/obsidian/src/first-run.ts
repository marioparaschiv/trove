import { existsSync } from "node:fs";
import { join } from "node:path";
import { defaultDataDir, failure } from "@trove/client";
import { Notice, requestUrl } from "obsidian";
import { connectTo, vaultSource } from "./connection.ts";
import {
  assetUrl,
  binaryPath,
  installBinary,
  parseReleasePin,
  releasePlatform,
} from "./release.ts";
import type { Settings } from "./settings.ts";
import { describeSetupEvent, runSetup } from "./setup.ts";

/** Downloaded releases live in the data folder, so they stay on this device and out of the synced vault. */
const RELEASES_FOLDER = "bin";
const DONE_NOTICE_MS = 5_000;

/**
 * Gets Trove ready for this vault and returns the binary to run: the one set in the settings, else the pinned release
 * (downloaded and checked on first run). Then puts the model in place and registers and indexes the vault if it isn't
 * a source yet, each with a notice while it runs.
 */
export async function prepareTrove(settings: Settings, vaultPath: string): Promise<string> {
  const dataDir = defaultDataDir();
  const binary = settings.binaryPath !== "" ? settings.binaryPath : await pinnedBinary(dataDir);
  await setUpModel(binary, dataDir);
  await addVault(binary, vaultPath);

  return binary;
}

async function pinnedBinary(dataDir: string): Promise<string> {
  const pin = parseReleasePin(process.env.TROVE_RELEASE_PIN);
  if (pin === undefined) {
    throw new Error(
      "this build of the plugin has no pinned release; set the trove binary in its settings",
    );
  }

  const platform = releasePlatform(process.platform, process.arch);
  const asset = platform && pin.assets[platform];
  if (asset === undefined) {
    throw new Error(`Trove ${pin.version} has no build for ${process.platform} ${process.arch}`);
  }

  const folder = join(dataDir, RELEASES_FOLDER);
  const binary = binaryPath(pin, folder, process.platform);
  if (existsSync(binary)) {
    return binary;
  }

  const notice = new Notice(`Trove: downloading Trove ${pin.version}`, 0);
  try {
    await installBinary({
      pin,
      asset,
      folder,
      platform: process.platform,
      download: async (asset) => (await requestUrl(assetUrl(pin, asset))).arrayBuffer,
    });
  } finally {
    notice.hide();
  }

  return binary;
}

/** Shows a notice only when setup has something to do; a rerun with the model in place only reports `done`. */
async function setUpModel(binary: string, dataDir: string): Promise<void> {
  let notice: Notice | undefined;
  try {
    await runSetup(binary, dataDir, (event) => {
      if (event.event === "done" && notice === undefined) {
        return;
      }

      const text = `Trove: ${describeSetupEvent(event)}`;
      notice = notice === undefined ? new Notice(text, 0) : notice.setMessage(text);
    });
  } catch (error) {
    notice?.hide();
    throw error;
  }

  const shown = notice;
  if (shown !== undefined) {
    window.setTimeout(() => shown.hide(), DONE_NOTICE_MS);
  }
}

async function addVault(binary: string, vaultPath: string): Promise<void> {
  const client = await connectTo(binary);
  if ((await vaultSource(client, vaultPath)) !== undefined) {
    return;
  }

  const added = await client.POST("/sources", { body: { path: vaultPath } });
  if (added.data === undefined) {
    throw failure("adding the vault", added.response, added.error);
  }

  const notice = new Notice("Trove: indexing this vault", 0);
  try {
    const { data, error, response } = await client.POST("/index", { body: {} });
    if (data === undefined) {
      throw failure("indexing the vault", response, error);
    }

    notice.setMessage(`Trove: indexed ${data.files} notes`);
    window.setTimeout(() => notice.hide(), DONE_NOTICE_MS);
  } catch (error) {
    notice.hide();
    throw error;
  }
}
