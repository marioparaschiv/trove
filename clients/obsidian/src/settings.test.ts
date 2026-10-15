import assert from "node:assert/strict";
import { test } from "node:test";
import { parseFolder, parseSettings } from "./settings.ts";

test("nothing saved gives the defaults", () => {
  assert.deepEqual(parseSettings(null), { binaryPath: "", categoryFolder: "Categories" });
});

test("reads saved settings", () => {
  assert.deepEqual(parseSettings({ binaryPath: "/usr/local/bin/trove" }), {
    binaryPath: "/usr/local/bin/trove",
    categoryFolder: "Categories",
  });
  assert.deepEqual(parseSettings({ binaryPath: "", categoryFolder: "Meta/Topics" }), {
    binaryPath: "",
    categoryFolder: "Meta/Topics",
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

test("rejects a category folder that isn't a folder in the vault", () => {
  for (const categoryFolder of ["", "/Categories", "../Categories", 7]) {
    assert.throws(() => parseSettings({ categoryFolder }), /categoryFolder must be a folder/);
  }
});

test("reads a typed folder as a vault folder path", () => {
  assert.equal(parseFolder(" /Meta/Topics/ "), "Meta/Topics");
  assert.equal(parseFolder("Meta//Topics"), "Meta/Topics");
  assert.equal(parseFolder("/"), undefined);
  assert.equal(parseFolder("Meta/../Topics"), undefined);
});
