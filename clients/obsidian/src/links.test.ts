import assert from "node:assert/strict";
import { test } from "node:test";
import { appendLink } from "./links.ts";

test("adds the link as its own paragraph at the end", () => {
  assert.equal(
    appendLink("# Espresso\n\nGrind finer.\n", "[[Pour over]]"),
    "# Espresso\n\nGrind finer.\n\n[[Pour over]]\n",
  );
});

test("ends a last line that has no line break first", () => {
  assert.equal(appendLink("Grind finer.", "[[Pour over]]"), "Grind finer.\n\n[[Pour over]]\n");
});

test("an empty note gets just the link", () => {
  assert.equal(appendLink("", "[[Pour over]]"), "[[Pour over]]\n");
});
