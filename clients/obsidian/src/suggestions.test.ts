import assert from "node:assert/strict";
import { test } from "node:test";
import { type FileSuggestion, inVault, merge, type TroveResult } from "./suggestions.ts";

const retryBackoff: TroveResult = {
  source: "/home/ada/Vault",
  path: "Engineering/Retry backoff.md",
  title: "Retry backoff",
  heading_path: ["Retry backoff", "Jitter"],
  score: 3.4,
  why: ["meaning"],
};

const kyoto: TroveResult = {
  source: "/home/ada/Vault",
  path: "Travel/Kyoto.md",
  title: "Kyoto",
  heading_path: ["Kyoto"],
  score: 2.9,
  why: ["meaning", "words"],
};

test("keeps only results from the vault's folder", () => {
  const otherSource = { ...kyoto, source: "/home/ada/Papers" };

  assert.deepEqual(inVault([retryBackoff, otherSource], "/home/ada/Vault/", "linux"), [
    retryBackoff,
  ]);
});

test("compares Windows folders without regard to case or separators", () => {
  const fromTrove = { ...retryBackoff, source: "C:\\Users\\Ada\\Vault" };
  const otherDrive = { ...kyoto, source: "D:\\Users\\Ada\\Vault" };

  assert.deepEqual(inVault([fromTrove, otherDrive], "c:/users/ada/vault", "win32"), [fromTrove]);
});

test("compares other folders with case", () => {
  assert.deepEqual(inVault([retryBackoff], "/home/ada/vault", "darwin"), []);
});

test("lists filename matches first, then Trove results for other notes", () => {
  const files: FileSuggestion[] = [
    { kind: "file", path: "Travel/Kyoto.md", match: { score: -0.2, matches: [[7, 12]] } },
  ];

  assert.deepEqual(merge(files, [kyoto, retryBackoff]), [
    ...files,
    { kind: "trove", result: retryBackoff },
  ]);
});
