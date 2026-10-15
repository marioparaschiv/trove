import type { Schemas } from "@trove/client";
import { categoryChain, type NoteState } from "./category-notes.ts";

export type CategoryPin = Schemas["CategoryPin"];

/** Every note's category properties, by path. */
export type NoteStates = ReadonlyMap<string, NoteState>;

export function inFolder(path: string, folder: string): boolean {
  return path.startsWith(`${folder}/`);
}

/** A category's name: its category note's file name. */
export function categoryName(categoryNote: string): string {
  return categoryNote.replace(/^.*\//, "").replace(/\.md$/, "");
}

/** The notes in a category or below it, by path. */
export function notesIn(categoryNote: string, states: NoteStates, folder: string): string[] {
  const notes: string[] = [];
  for (const [path, { category }] of states) {
    if (inFolder(path, folder) || category === undefined) {
      continue;
    }

    const chain = categoryChain(category, (above) => states.get(above)?.parent);
    if (chain.includes(categoryNote)) {
      notes.push(path);
    }
  }

  return notes.sort();
}

/** Pins a category under its category note's name, found in later runs by the notes in it now. */
export function categoryPin(
  categoryNote: string,
  states: NoteStates,
  folder: string,
  replaces: string[] = [],
): CategoryPin {
  return {
    type: "category",
    name: categoryName(categoryNote),
    notes: notesIn(categoryNote, states, folder),
    replaces,
  };
}

/**
 * The pins a hand edit of a note's properties makes, with `states` already showing the edit: a note's `category` set to
 * a category note moves the note there, and a category note's `parent` changed moves the category.
 */
export function editPins(
  path: string,
  before: NoteState | undefined,
  after: NoteState,
  states: NoteStates,
  folder: string,
): CategoryPin[] {
  if (inFolder(path, folder)) {
    const parent = after.parent;
    if (parent === before?.parent || (parent !== undefined && !inFolder(parent, folder))) {
      return [];
    }

    const pins = [categoryPin(path, states, folder)];
    if (parent !== undefined) {
      pins.push(categoryPin(parent, states, folder));
    }
    pins.push({
      type: "parent",
      category: categoryName(path),
      parent: parent === undefined ? null : categoryName(parent),
    });

    return pins;
  }

  const category = after.category;
  if (category === undefined || category === before?.category || !inFolder(category, folder)) {
    return [];
  }

  return [
    categoryPin(category, states, folder),
    { type: "note", path, category: categoryName(category) },
  ];
}

/** The pin a renamed category note makes, with `states` from before the rename. */
export function renamePin(
  oldPath: string,
  newPath: string,
  states: NoteStates,
  folder: string,
): CategoryPin {
  return {
    type: "category",
    name: categoryName(newPath),
    notes: notesIn(oldPath, states, folder),
    replaces: [categoryName(oldPath)],
  };
}

/** The pin merging the category of the note `from` into that of `into`. */
export function mergePin(
  from: string,
  into: string,
  states: NoteStates,
  folder: string,
): CategoryPin {
  const notes = new Set([...notesIn(into, states, folder), ...notesIn(from, states, folder)]);

  return {
    type: "category",
    name: categoryName(into),
    notes: [...notes].sort(),
    replaces: [categoryName(from)],
  };
}

/** `states` after the note at `oldPath` moved to `newPath`, which Obsidian follows by updating the links to it. */
export function renamedStates(
  states: NoteStates,
  oldPath: string,
  newPath: string,
): Map<string, NoteState> {
  const follow = (path: string | undefined) => (path === oldPath ? newPath : path);
  const renamed = new Map<string, NoteState>();
  for (const [path, { category, parent }] of states) {
    renamed.set(follow(path) ?? path, { category: follow(category), parent: follow(parent) });
  }

  return renamed;
}
