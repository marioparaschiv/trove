import assert from "node:assert/strict";
import { test } from "node:test";
import type { Schemas } from "@trove/client";
import {
  type CategoryChange,
  categoryChain,
  describeChange,
  linkTarget,
  linkTo,
  type NoteState,
  planCategories,
} from "./category-notes.ts";

const VAULT = "/home/ada/Vault";

function note(path: string, source = VAULT): Schemas["CategoryNote"] {
  return { source, path, title: path.replace(/^.*\//, "").replace(/\.md$/, "") };
}

// Shaped like `GET /categories` on the categories fixture: a kitchen category split into bread and cheese, and sailing.
const CATEGORIES: Schemas["Category"][] = [
  {
    id: 3,
    parent: null,
    name: "dough, cheese, kitchen",
    start_here: note("Kitchen projects.md"),
    notes: [],
  },
  {
    id: 0,
    parent: 3,
    name: "dough, loaf, levain",
    start_here: note("Bread/Proofing dough.md"),
    notes: [note("Bread/Proofing dough.md"), note("Kitchen projects.md")],
  },
  {
    id: 1,
    parent: 3,
    name: "curds, rennet, whey",
    start_here: note("Cheese/Rennet and curds.md"),
    notes: [note("Cheese/Rennet and curds.md")],
  },
  {
    id: 2,
    parent: null,
    name: "wind, boat, mainsail",
    start_here: note("Sailing/Tacking.md"),
    notes: [note("Sailing/Tacking.md")],
  },
];

/** A vault in memory that applies changes the way the plugin does. */
class FakeVault {
  readonly notes = new Map<string, NoteState>();

  constructor(paths: string[]) {
    for (const path of paths) {
      this.notes.set(path, { category: undefined, parent: undefined });
    }
  }

  plan(categories = CATEGORIES): CategoryChange[] {
    return planCategories(categories, {
      folder: "Categories",
      inVault: (source) => source === VAULT,
      state: (path) => this.notes.get(path),
    });
  }

  apply(changes: CategoryChange[]): void {
    for (const change of changes) {
      if (change.kind === "create") {
        this.notes.set(change.path, { category: undefined, parent: change.parent });
      } else {
        const state = this.notes.get(change.path);
        assert.ok(state, `${change.path} is in the vault`);
        this.notes.set(change.path, {
          ...state,
          [change.kind === "set-parent" ? "parent" : "category"]: change.to,
        });
      }
    }
  }
}

const NOTES = [
  "Bread/Proofing dough.md",
  "Kitchen projects.md",
  "Cheese/Rennet and curds.md",
  "Sailing/Tacking.md",
];

test("plans a category note per category and a category on each note", () => {
  assert.deepEqual(new FakeVault(NOTES).plan(), [
    { kind: "create", path: "Categories/dough, cheese, kitchen.md", parent: undefined },
    {
      kind: "create",
      path: "Categories/dough, loaf, levain.md",
      parent: "Categories/dough, cheese, kitchen.md",
    },
    {
      kind: "set-category",
      path: "Bread/Proofing dough.md",
      from: undefined,
      to: "Categories/dough, loaf, levain.md",
    },
    {
      kind: "set-category",
      path: "Kitchen projects.md",
      from: undefined,
      to: "Categories/dough, loaf, levain.md",
    },
    {
      kind: "create",
      path: "Categories/curds, rennet, whey.md",
      parent: "Categories/dough, cheese, kitchen.md",
    },
    {
      kind: "set-category",
      path: "Cheese/Rennet and curds.md",
      from: undefined,
      to: "Categories/curds, rennet, whey.md",
    },
    { kind: "create", path: "Categories/wind, boat, mainsail.md", parent: undefined },
    {
      kind: "set-category",
      path: "Sailing/Tacking.md",
      from: undefined,
      to: "Categories/wind, boat, mainsail.md",
    },
  ]);
});

test("applying the plan twice changes nothing the second time", () => {
  const vault = new FakeVault(NOTES);
  vault.apply(vault.plan());

  assert.deepEqual(vault.plan(), []);
});

test("changes only what differs from the vault", () => {
  const vault = new FakeVault(NOTES);
  vault.apply(vault.plan());
  vault.notes.set("Sailing/Tacking.md", {
    category: "Categories/curds, rennet, whey.md",
    parent: undefined,
  });
  vault.notes.set("Categories/curds, rennet, whey.md", { category: undefined, parent: undefined });

  assert.deepEqual(vault.plan(), [
    {
      kind: "set-parent",
      path: "Categories/curds, rennet, whey.md",
      from: undefined,
      to: "Categories/dough, cheese, kitchen.md",
    },
    {
      kind: "set-category",
      path: "Sailing/Tacking.md",
      from: "Categories/curds, rennet, whey.md",
      to: "Categories/wind, boat, mainsail.md",
    },
  ]);
});

test("leaves out notes and categories from other sources and notes no longer in the vault", () => {
  const categories: Schemas["Category"][] = [
    { ...CATEGORIES[3], notes: [note("Sailing/Tacking.md"), note("Gone.md")] },
    {
      id: 7,
      parent: null,
      name: "papers",
      start_here: note("Paper.md", "/home/ada/Papers"),
      notes: [note("Paper.md", "/home/ada/Papers")],
    },
  ];

  assert.deepEqual(new FakeVault(["Sailing/Tacking.md"]).plan(categories), [
    { kind: "create", path: "Categories/wind, boat, mainsail.md", parent: undefined },
    {
      kind: "set-category",
      path: "Sailing/Tacking.md",
      from: undefined,
      to: "Categories/wind, boat, mainsail.md",
    },
  ]);
});

test("gives categories with the same name their own notes", () => {
  const categories: Schemas["Category"][] = [
    { ...CATEGORIES[2], parent: null, name: "Wind: boat" },
    { ...CATEGORIES[3], name: "wind  boat" },
  ];
  const created = new FakeVault([])
    .plan(categories)
    .filter((change) => change.kind === "create")
    .map((change) => change.path);

  assert.deepEqual(created, ["Categories/Wind  boat.md", "Categories/wind  boat 2.md"]);
});

test("links to a note by its path without the extension", () => {
  assert.equal(linkTo("Categories/wind, boat.md"), "[[Categories/wind, boat]]");
});

test("reads the link target of a property", () => {
  assert.equal(linkTarget("[[Categories/Sailing]]"), "Categories/Sailing");
  assert.equal(linkTarget("[[Sailing#Tides|tides]]"), "Sailing");
  assert.equal(linkTarget(["[[Sailing]]", "[[Bread]]"]), "Sailing");
  assert.equal(linkTarget("Sailing"), undefined);
  assert.equal(linkTarget(42), undefined);
  assert.equal(linkTarget(undefined), undefined);
});

test("follows parents up to the top-level category", () => {
  const parents = new Map([
    ["Categories/Bread.md", "Categories/Kitchen.md"],
    ["Categories/Kitchen.md", undefined],
  ]);

  assert.deepEqual(
    categoryChain("Categories/Bread.md", (path) => parents.get(path)),
    ["Categories/Kitchen.md", "Categories/Bread.md"],
  );
});

test("stops at a loop of parents", () => {
  const parents = new Map([
    ["A.md", "B.md"],
    ["B.md", "A.md"],
  ]);

  assert.deepEqual(
    categoryChain("A.md", (path) => parents.get(path)),
    ["B.md", "A.md"],
  );
});

test("describes each change for the preview", () => {
  const changes: CategoryChange[] = [
    { kind: "create", path: "Categories/Bread.md", parent: "Categories/Kitchen.md" },
    { kind: "create", path: "Categories/Kitchen.md", parent: undefined },
    {
      kind: "set-parent",
      path: "Categories/Bread.md",
      from: undefined,
      to: "Categories/Kitchen.md",
    },
    {
      kind: "set-category",
      path: "Bread/Proofing dough.md",
      from: "Categories/Cheese.md",
      to: "Categories/Bread.md",
    },
  ];

  assert.deepEqual(changes.map(describeChange), [
    "Create Categories/Bread.md under Categories/Kitchen.md",
    "Create Categories/Kitchen.md",
    "Categories/Bread.md: parent (none) → Categories/Kitchen.md",
    "Bread/Proofing dough.md: category Categories/Cheese.md → Categories/Bread.md",
  ]);
});
