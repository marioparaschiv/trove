import assert from "node:assert/strict";
import { test } from "node:test";
import type { TroveResult } from "./suggestions.ts";
import { inVault } from "./vault-paths.ts";

function result(source: string, path: string): TroveResult {
  return { source, path, title: path, heading_path: [], score: 1, why: ["meaning"] };
}

test("keeps only results from the vault's folder", () => {
  const retryBackoff = result("/home/ada/Vault", "Engineering/Retry backoff.md");
  const otherSource = result("/home/ada/Papers", "Travel/Kyoto.md");

  assert.deepEqual(inVault([retryBackoff, otherSource], "/home/ada/Vault/", "linux"), [
    retryBackoff,
  ]);
});

test("compares Windows folders without regard to case or separators", () => {
  const fromTrove = result("C:\\Users\\Ada\\Vault", "Engineering/Retry backoff.md");
  const otherDrive = result("D:\\Users\\Ada\\Vault", "Travel/Kyoto.md");

  assert.deepEqual(inVault([fromTrove, otherDrive], "c:/users/ada/vault", "win32"), [fromTrove]);
});

test("compares other folders with case", () => {
  const retryBackoff = result("/home/ada/Vault", "Engineering/Retry backoff.md");

  assert.deepEqual(inVault([retryBackoff], "/home/ada/vault", "darwin"), []);
});
