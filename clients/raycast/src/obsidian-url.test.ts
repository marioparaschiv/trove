import assert from "node:assert/strict";
import { test } from "node:test";
import { obsidianUrl } from "./obsidian-url.ts";

const source = process.platform === "win32" ? "C:\\Users\\me\\Notes Vault" : "/home/me/Notes Vault";

test("opens the note at its best heading", () => {
  const url = obsidianUrl({
    source,
    path: "Software/Retry backoff.md",
    title: "Retry backoff",
    heading_path: ["Retry backoff", "Jitter"],
    score: 3.4,
  });

  assert.equal(url, "obsidian://open?vault=Notes%20Vault&file=Software%2FRetry%20backoff%23Jitter");
});

test("opens the note at the top when the match is before the first heading", () => {
  const url = obsidianUrl({
    source,
    path: "Nature/Birding journal.md",
    title: "Birding journal",
    heading_path: [],
    score: 2.9,
  });

  assert.equal(url, "obsidian://open?vault=Notes%20Vault&file=Nature%2FBirding%20journal");
});

test("encodes characters that would end or split a query value", () => {
  const url = obsidianUrl({
    source,
    path: "Cooking/Bread & butter + jam.md",
    title: "Bread & butter + jam",
    heading_path: ["Ratios: 100% hydration?"],
    score: 3.1,
  });

  assert.equal(
    url,
    "obsidian://open?vault=Notes%20Vault&file=Cooking%2FBread%20%26%20butter%20%2B%20jam%23Ratios%3A%20100%25%20hydration%3F",
  );
});
