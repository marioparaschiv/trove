import assert from "node:assert/strict";
import { test } from "node:test";
import type { NoteState } from "./category-notes.ts";
import {
  editPins,
  mergePin,
  type NoteStates,
  notesIn,
  renamedStates,
  renamePin,
} from "./category-pins.ts";

const FOLDER = "Categories";
const KITCHEN = "Categories/Kitchen.md";
const BREAD = "Categories/Bread.md";
const CHEESE = "Categories/Cheese.md";
const SAILING = "Categories/Sailing.md";

function state(category?: string, parent?: string): NoteState {
  return { category, parent };
}

/** The vault after applying categories: kitchen split into bread and cheese, and sailing. */
function vault(): Map<string, NoteState> {
  return new Map([
    [KITCHEN, state()],
    [BREAD, state(undefined, KITCHEN)],
    [CHEESE, state(undefined, KITCHEN)],
    [SAILING, state()],
    ["Bread/Proofing dough.md", state(BREAD)],
    ["Cheese/Brining feta.md", state(CHEESE)],
    ["Cheese/Mozzarella stretching.md", state(BREAD)],
    ["Fermentation.md", state(KITCHEN)],
    ["Sailing/Tacking.md", state(SAILING)],
    ["Unsorted.md", state()],
  ]);
}

function edited(states: NoteStates, path: string, after: NoteState): Map<string, NoteState> {
  return new Map([...states, [path, after]]);
}

test("a category's notes include its subcategories' notes", () => {
  assert.deepEqual(notesIn(KITCHEN, vault(), FOLDER), [
    "Bread/Proofing dough.md",
    "Cheese/Brining feta.md",
    "Cheese/Mozzarella stretching.md",
    "Fermentation.md",
  ]);
  assert.deepEqual(notesIn(CHEESE, vault(), FOLDER), ["Cheese/Brining feta.md"]);
});

test("setting a note's category by hand pins the note and the category it moved to", () => {
  const path = "Cheese/Mozzarella stretching.md";
  const before = vault().get(path);
  const states = edited(vault(), path, state(CHEESE));

  assert.deepEqual(editPins(path, before, state(CHEESE), states, FOLDER), [
    {
      type: "category",
      name: "Cheese",
      notes: ["Cheese/Brining feta.md", path],
      replaces: [],
    },
    { type: "note", path, category: "Cheese" },
  ]);
});

test("edits that don't move a note into a category note pin nothing", () => {
  const path = "Bread/Proofing dough.md";
  assert.deepEqual(editPins(path, state(BREAD), state(BREAD), vault(), FOLDER), []);
  assert.deepEqual(editPins(path, state(BREAD), state(undefined), vault(), FOLDER), []);
  assert.deepEqual(editPins(path, state(BREAD), state("Unsorted.md"), vault(), FOLDER), []);
  assert.deepEqual(
    editPins(BREAD, state(undefined, KITCHEN), state(BREAD, KITCHEN), vault(), FOLDER),
    [],
  );
});

test("changing a category note's parent pins the category under its new parent", () => {
  const states = edited(vault(), CHEESE, state(undefined, SAILING));

  assert.deepEqual(
    editPins(CHEESE, state(undefined, KITCHEN), state(undefined, SAILING), states, FOLDER),
    [
      { type: "category", name: "Cheese", notes: ["Cheese/Brining feta.md"], replaces: [] },
      {
        type: "category",
        name: "Sailing",
        notes: ["Cheese/Brining feta.md", "Sailing/Tacking.md"],
        replaces: [],
      },
      { type: "parent", category: "Cheese", parent: "Sailing" },
    ],
  );

  const top = edited(vault(), CHEESE, state());
  assert.deepEqual(editPins(CHEESE, state(undefined, KITCHEN), state(), top, FOLDER).at(-1), {
    type: "parent",
    category: "Cheese",
    parent: null,
  });
});

test("renaming a category note pins its notes under the new name, replacing the old one", () => {
  assert.deepEqual(renamePin(BREAD, "Categories/Baking.md", vault(), FOLDER), {
    type: "category",
    name: "Baking",
    notes: ["Bread/Proofing dough.md", "Cheese/Mozzarella stretching.md"],
    replaces: ["Bread"],
  });
});

test("merging pins both categories' notes under the kept name", () => {
  assert.deepEqual(mergePin(BREAD, CHEESE, vault(), FOLDER), {
    type: "category",
    name: "Cheese",
    notes: ["Bread/Proofing dough.md", "Cheese/Brining feta.md", "Cheese/Mozzarella stretching.md"],
    replaces: ["Bread"],
  });
});

test("a renamed note's links follow it", () => {
  const states = renamedStates(vault(), BREAD, "Categories/Baking.md");

  assert.equal(states.has(BREAD), false);
  assert.deepEqual(states.get("Categories/Baking.md"), state(undefined, KITCHEN));
  assert.deepEqual(states.get("Bread/Proofing dough.md"), state("Categories/Baking.md"));
  assert.deepEqual(notesIn("Categories/Baking.md", states, FOLDER), [
    "Bread/Proofing dough.md",
    "Cheese/Mozzarella stretching.md",
  ]);
});
