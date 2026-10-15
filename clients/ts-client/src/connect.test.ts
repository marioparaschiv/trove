import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { connect, readRuntimeFile } from "./index.ts";

const binary = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "target",
  "debug",
  process.platform === "win32" ? "trove.exe" : "trove",
);

function dataDir(t: TestContext, config = 'service_idle_timeout = "1m"\n'): string {
  const dir = mkdtempSync(join(tmpdir(), "trove-client-"));
  writeFileSync(join(dir, "config.toml"), config);

  t.after(() => {
    const runtime = readRuntimeFile(dir);
    if (runtime !== undefined) {
      process.kill(runtime.pid);
    }

    // Windows keeps the killed process's files locked for a moment.
    rmSync(dir, { recursive: true, force: true, maxRetries: 20 });
  });

  return dir;
}

test("starts the service when none runs, then reuses it", async (t) => {
  const dir = dataDir(t);
  assert.equal(readRuntimeFile(dir), undefined);

  const first = await connect({ binary, dataDir: dir });
  const status = await first.client.GET("/status");

  assert.equal(status.response.status, 200);
  assert.equal(status.data?.pid, first.runtime.pid);
  assert.deepEqual(status.data?.index, { files: 0, chunks: 0 });

  const second = await connect({ binary, dataDir: dir });
  const again = await second.client.GET("/status");

  assert.deepEqual(second.runtime, first.runtime);
  assert.equal(again.data?.pid, first.runtime.pid);
});

test("starts a new service when the runtime file's process is gone", async (t) => {
  const dir = dataDir(t);
  const exited = spawn(process.execPath, ["--eval", ""]);
  await once(exited, "exit");
  writeFileSync(
    join(dir, "runtime.json"),
    JSON.stringify({ port: 1, token: "stale", pid: exited.pid, version: "0.0.0" }),
  );

  const { client, runtime } = await connect({ binary, dataDir: dir });
  const status = await client.GET("/status");

  assert.notEqual(runtime.pid, exited.pid);
  assert.equal(status.data?.pid, runtime.pid);
});

test("reports why the service failed to start", async (t) => {
  const dir = dataDir(t, 'service_idle_timeout = "soon"\n');

  await assert.rejects(connect({ binary, dataDir: dir }), /service_idle_timeout/);
});
