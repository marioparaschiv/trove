import assert from "node:assert/strict";
import { test } from "node:test";
import { sliceBytes } from "./index.ts";

test("slices by UTF-8 bytes, not UTF-16 indices", () => {
  // "é" is two bytes, "💧" four bytes and two UTF-16 units.
  const text = "café 💧 pressure";

  assert.equal(sliceBytes(text, 0, 5), "café");
  assert.equal(sliceBytes(text, 6, 10), "💧");
  assert.equal(sliceBytes(text, 11, 19), "pressure");
  assert.equal(sliceBytes(text, 11), "pressure");
});
