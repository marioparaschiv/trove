import assert from "node:assert/strict";
import { test } from "node:test";
import { parseSettings } from "./settings.ts";

test("nothing saved gives the defaults", () => {
  assert.deepEqual(parseSettings(null), { binaryPath: "" });
});

test("reads saved settings", () => {
  assert.deepEqual(parseSettings({ binaryPath: "/usr/local/bin/trove" }), {
    binaryPath: "/usr/local/bin/trove",
  });
});

test("rejects unknown settings", () => {
  assert.throws(() => parseSettings({ binaryPath: "", binary: "trove" }), /unknown.*binary/);
});

test("rejects a binary path that isn't a string", () => {
  assert.throws(() => parseSettings({ binaryPath: 42 }), /binaryPath must be a string/);
});

test("rejects data that isn't an object", () => {
  assert.throws(() => parseSettings("trove"), /must be an object/);
});
