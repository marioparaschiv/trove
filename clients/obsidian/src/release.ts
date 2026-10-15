import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rename, rm, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { promisify } from "node:util";
import { isRecord } from "@trove/client";

/** The platforms releases are built for, by Node's platform and architecture. */
const PLATFORMS = {
  "linux-x64": "linux-x86_64",
  "darwin-arm64": "macos-arm64",
  "win32-x64": "windows-x86_64",
} as const;

export type Platform = (typeof PLATFORMS)[keyof typeof PLATFORMS];

export interface ReleaseAsset {
  /** The archive's file name in the release. */
  file: string;
  /** The archive's SHA-256, lowercase hex. */
  sha256: string;
}

/** The release of `trove` this build of the plugin uses, bundled into it when the release is built. */
export interface ReleasePin {
  version: string;
  /** The GitHub repository the release is in, `owner/name`. */
  repository: string;
  assets: Partial<Record<Platform, ReleaseAsset>>;
}

const execFileAsync = promisify(execFile);

/** Checks the bundled pin; `undefined` (a development build) gives `undefined`. */
export function parseReleasePin(text: string | undefined): ReleasePin | undefined {
  if (text === undefined) {
    return undefined;
  }

  const value: unknown = JSON.parse(text);
  if (!isRecord(value)) {
    throw new Error(`invalid release pin ${text}`);
  }

  const { version, repository, assets, ...rest } = value;
  if (
    Object.keys(rest).length > 0 ||
    typeof version !== "string" ||
    !/^\d+\.\d+\.\d+$/.test(version) ||
    typeof repository !== "string" ||
    !/^[A-Za-z0-9-]+\/[\w.-]+$/.test(repository) ||
    !isRecord(assets)
  ) {
    throw new Error(`invalid release pin ${text}`);
  }

  const pinned: ReleasePin["assets"] = {};
  for (const [name, asset] of Object.entries(assets)) {
    const platform = Object.values(PLATFORMS).find((known) => known === name);
    if (platform === undefined || !isAsset(asset)) {
      throw new Error(`invalid release pin asset ${name}: ${JSON.stringify(asset)}`);
    }

    pinned[platform] = asset;
  }

  return { version, repository, assets: pinned };
}

function isAsset(value: unknown): value is ReleaseAsset {
  if (!isRecord(value)) {
    return false;
  }

  const { file, sha256, ...rest } = value;

  return (
    Object.keys(rest).length === 0 &&
    typeof file === "string" &&
    /^\w[\w.-]*$/.test(file) &&
    typeof sha256 === "string" &&
    /^[0-9a-f]{64}$/.test(sha256)
  );
}

/** The release platform for Node's `platform` and `arch`; `undefined` when there is no build for it. */
export function releasePlatform(platform: NodeJS.Platform, arch: string): Platform | undefined {
  const key = `${platform}-${arch}`;

  return key in PLATFORMS ? PLATFORMS[key as keyof typeof PLATFORMS] : undefined;
}

export function assetUrl(pin: ReleasePin, asset: ReleaseAsset): string {
  return `https://github.com/${pin.repository}/releases/download/v${pin.version}/${asset.file}`;
}

/** Where the pinned `trove` goes inside `folder`. A binary there has always passed its hash check. */
export function binaryPath(pin: ReleasePin, folder: string, platform: NodeJS.Platform): string {
  return join(folder, pin.version, platform === "win32" ? "trove.exe" : "trove");
}

export interface InstallOptions {
  pin: ReleasePin;
  asset: ReleaseAsset;
  /** The folder releases are installed in, one subfolder per version. */
  folder: string;
  platform: NodeJS.Platform;
  download: (asset: ReleaseAsset) => Promise<ArrayBuffer>;
}

/**
 * Downloads the pinned archive, checks its SHA-256 and unpacks it to {@link binaryPath}. The archive is unpacked
 * beside the version folder and renamed into place, so a damaged or partial download never takes its name.
 */
export async function installBinary({
  pin,
  asset,
  folder,
  platform,
  download,
}: InstallOptions): Promise<string> {
  const bytes = Buffer.from(await download(asset));
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  if (sha256 !== asset.sha256) {
    throw new Error(
      `${asset.file} has SHA-256 ${sha256}, expected ${asset.sha256}; it was not installed`,
    );
  }

  await mkdir(folder, { recursive: true });
  const work = await mkdtemp(join(folder, `.${pin.version}-`));
  try {
    const archive = join(work, asset.file);
    const unpacked = join(work, "files");
    await writeFile(archive, bytes);
    await mkdir(unpacked);
    await execFileAsync(tarCommand(platform), ["-xf", archive, "-C", unpacked], {
      windowsHide: true,
    });

    const binary = binaryPath(pin, folder, platform);
    if (!existsSync(join(unpacked, basename(binary)))) {
      throw new Error(`${asset.file} has no ${basename(binary)}`);
    }

    try {
      await rename(unpacked, dirname(binary));
    } catch (error) {
      // Another window installed the same version first.
      if (!existsSync(binary)) {
        throw error;
      }
    }

    return binary;
  } finally {
    await rm(work, { recursive: true, force: true });
  }
}

/** Windows' own `tar` (bsdtar) also unpacks zip files; another `tar` earlier on the PATH may not. */
function tarCommand(platform: NodeJS.Platform): string {
  if (platform !== "win32") {
    return "tar";
  }

  return join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
}
