import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { after, before, describe, type TestContext, test } from "node:test";
import { connect, readRuntimeFile } from "@trove/client";
import {
  assetUrl,
  binaryPath,
  installBinary,
  parseReleasePin,
  type ReleaseAsset,
  type ReleasePin,
  releasePlatform,
} from "./release.ts";

const REPO_ROOT = join(import.meta.dirname, "..", "..", "..");
const PIN = {
  version: "0.1.0",
  repository: "marioparaschiv/trove",
  assets: {
    "linux-x86_64": {
      file: "trove-0.1.0-linux-x86_64.tar.gz",
      sha256: "5e97518f7bd701927d8f2010fd37d4969abcb499c2a6f5eae00086073a421265",
    },
  },
};

function tempDir(t: TestContext, prefix: string): string {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  // Windows keeps a killed process's files locked for a moment.
  t.after(() => rmSync(dir, { recursive: true, force: true, maxRetries: 20 }));

  return dir;
}

test("a development build has no pin", () => {
  assert.equal(parseReleasePin(undefined), undefined);
});

test("reads the pin bundled at release", () => {
  assert.deepEqual(parseReleasePin(JSON.stringify(PIN)), PIN);
});

test("rejects a malformed pin", () => {
  const asset = PIN.assets["linux-x86_64"];
  const bad = [
    "[]",
    JSON.stringify({ ...PIN, version: "latest" }),
    JSON.stringify({ ...PIN, repository: "../trove" }),
    JSON.stringify({ ...PIN, extra: true }),
    JSON.stringify({ ...PIN, assets: { "linux-arm64": asset } }),
    JSON.stringify({ ...PIN, assets: { "linux-x86_64": { ...asset, sha256: "abc" } } }),
    JSON.stringify({ ...PIN, assets: { "linux-x86_64": { ...asset, file: "../trove.tar.gz" } } }),
  ];
  for (const text of bad) {
    assert.throws(() => parseReleasePin(text), /invalid release pin/, text);
  }
});

test("maps Node's platform names to release platforms", () => {
  assert.equal(releasePlatform("linux", "x64"), "linux-x86_64");
  assert.equal(releasePlatform("darwin", "arm64"), "macos-arm64");
  assert.equal(releasePlatform("win32", "x64"), "windows-x86_64");
  assert.equal(releasePlatform("darwin", "x64"), undefined);
});

test("downloads from the tagged GitHub release", () => {
  assert.equal(
    assetUrl(PIN, PIN.assets["linux-x86_64"]),
    "https://github.com/marioparaschiv/trove/releases/download/v0.1.0/trove-0.1.0-linux-x86_64.tar.gz",
  );
});

/** This checkout's debug build, packaged the way the release workflow packages a release build. */
function packageDebugBuild(dist: string): {
  pin: ReleasePin;
  asset: ReleaseAsset;
  archive: string;
} {
  const platform = releasePlatform(process.platform, process.arch);
  if (platform === undefined) {
    throw new Error(`no release platform for ${process.platform} ${process.arch}`);
  }

  const script = join(REPO_ROOT, "scripts", "package-release.sh");
  const debug = join(REPO_ROOT, "target", "debug");
  const archive = execFileSync("bash", [script, platform, PIN.version, debug, dist], {
    encoding: "utf8",
  }).trim();
  const [sha256 = ""] = readFileSync(`${archive}.sha256`, "utf8").split(" ");
  const asset = { file: basename(archive), sha256 };

  return { pin: { ...PIN, assets: { [platform]: asset } }, asset, archive };
}

describe("installing a release", () => {
  const dist = mkdtempSync(join(tmpdir(), "trove-dist-"));
  after(() => rmSync(dist, { recursive: true, force: true }));
  let release: ReturnType<typeof packageDebugBuild>;
  before(() => {
    release = packageDebugBuild(dist);
  });
  /** Stands in for the download from GitHub. */
  const download = async () => new Uint8Array(readFileSync(release.archive)).buffer;

  test("installs the pinned release, which then starts", async (t) => {
    const folder = tempDir(t, "trove-bin-");
    const dataDir = mkdtempSync(join(tmpdir(), "trove-data-"));
    t.after(() => {
      const runtime = readRuntimeFile(dataDir);
      if (runtime !== undefined) {
        process.kill(runtime.pid);
      }
      rmSync(dataDir, { recursive: true, force: true, maxRetries: 20 });
    });

    const binary = await installBinary({
      ...release,
      folder,
      platform: process.platform,
      download,
    });

    assert.equal(binary, binaryPath(release.pin, folder, process.platform));
    assert.deepEqual(readdirSync(folder), [release.pin.version]);
    const { client } = await connect({ binary, dataDir });
    const status = await client.GET("/status");
    assert.equal(status.response.status, 200);
    assert.equal(status.data?.version, release.pin.version);
  });

  test("rejects an archive with the wrong hash and installs nothing", async (t) => {
    const folder = tempDir(t, "trove-bin-");
    const asset = { ...release.asset, sha256: "0".repeat(64) };

    await assert.rejects(
      installBinary({ ...release, asset, folder, platform: process.platform, download }),
      {
        message: `${asset.file} has SHA-256 ${release.asset.sha256}, expected ${asset.sha256}; it was not installed`,
      },
    );
    assert.deepEqual(readdirSync(folder), []);
    assert.equal(existsSync(binaryPath(release.pin, folder, process.platform)), false);
  });
});
