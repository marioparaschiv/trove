import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { describeSetupEvent, parseSetupEvent, runSetup, type SetupEvent } from "./setup.ts";

const binary = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "target",
  "debug",
  process.platform === "win32" ? "trove.exe" : "trove",
);

const REVISION = "0123456789abcdef0123456789abcdef01234567";
const MODEL = Buffer.from("not a real model, but setup only checks its size and hash\n".repeat(50));

test("reads each kind of setup event", () => {
  const events: SetupEvent[] = [
    { event: "file", name: "model.gguf", bytes: 309_855_456, from: "download" },
    { event: "progress", name: "model.gguf", bytes: 10, total: 40 },
    { event: "waiting" },
    { event: "verifying", name: "model.gguf" },
    { event: "retry", message: "connection refused", seconds: 4 },
    { event: "done", path: "/data/models/model.gguf" },
    { event: "error", message: "disk full" },
  ];
  for (const event of events) {
    assert.deepEqual(parseSetupEvent(JSON.stringify(event)), event);
  }
});

test("rejects output that isn't a setup event", () => {
  for (const line of [
    "Downloading",
    "{}",
    '{"event":"progress","name":"model.gguf","bytes":"10","total":40}',
    '{"event":"finished"}',
  ]) {
    assert.throws(() => parseSetupEvent(line), Error, line);
  }
});

test("describes progress for a notice", () => {
  assert.equal(
    describeSetupEvent({ event: "file", name: "m", bytes: 309_855_456, from: "download" }),
    "Downloading the model (310 MB)",
  );
  assert.equal(
    describeSetupEvent({ event: "progress", name: "m", bytes: 103_000_000, total: 309_855_456 }),
    "Downloading the model: 33%",
  );
  assert.equal(
    describeSetupEvent({ event: "retry", message: "connection refused", seconds: 8 }),
    "connection refused; trying again in 8 s",
  );
});

/** A data folder whose model is a small file served by a local stand-in for Hugging Face. */
async function fixtureSetup(t: TestContext): Promise<string> {
  const server = createServer((request, response) => {
    if (request.url === `/trove/fixture/resolve/${REVISION}/model.gguf`) {
      response.end(MODEL);
    } else {
      response.writeHead(404).end();
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => server.close());

  const dataDir = mkdtempSync(join(tmpdir(), "trove-setup-"));
  t.after(() => rmSync(dataDir, { recursive: true, force: true }));
  const hfCache = mkdtempSync(join(tmpdir(), "trove-hf-"));
  t.after(() => rmSync(hfCache, { recursive: true, force: true }));
  writeFileSync(
    join(dataDir, "config.toml"),
    [
      "[model]",
      'repo = "trove/fixture"',
      `revision = "${REVISION}"`,
      'file = "model.gguf"',
      `size = ${MODEL.length}`,
      `sha256 = "${createHash("sha256").update(MODEL).digest("hex")}"`,
      "",
    ].join("\n"),
  );

  const env = { HF_ENDPOINT: process.env.HF_ENDPOINT, HF_HUB_CACHE: process.env.HF_HUB_CACHE };
  process.env.HF_ENDPOINT = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  process.env.HF_HUB_CACHE = hfCache;
  t.after(() => {
    for (const [key, value] of Object.entries(env)) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
  });

  return dataDir;
}

test("runs setup and reports its events", async (t) => {
  const dataDir = await fixtureSetup(t);
  const events: SetupEvent[] = [];

  await runSetup(binary, dataDir, (event) => events.push(event));

  const kinds = events.map((event) => event.event);
  assert.equal(kinds[0], "file");
  assert.deepEqual(kinds.slice(-2), ["verifying", "done"]);

  events.length = 0;
  await runSetup(binary, dataDir, (event) => events.push(event));
  assert.deepEqual(
    events.map((event) => event.event),
    ["done"],
  );
});

test("fails with setup's error", async (t) => {
  const dataDir = await fixtureSetup(t);
  process.env.HF_ENDPOINT = "huggingface.co";

  await assert.rejects(
    runSetup(binary, dataDir, () => {}),
    /HF_ENDPOINT must be an http/,
  );
});
