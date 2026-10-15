import type { Schemas } from "@trove/client";

type Category = Schemas["Category"];

/** The category properties a note has now, each as the path of the note its link resolves to. */
export interface NoteState {
  category: string | undefined;
  parent: string | undefined;
}

export type CategoryChange =
  | { kind: "create"; path: string; parent: string | undefined }
  | { kind: "set-parent"; path: string; from: string | undefined; to: string }
  | { kind: "set-category"; path: string; from: string | undefined; to: string };

export interface PlanOptions {
  /** The folder category notes go in, inside the vault. */
  folder: string;
  /** Whether a note in a Trove response is from this vault's source. */
  inVault: (source: string) => boolean;
  /** The note's category properties, or `undefined` when there is no such note in the vault. */
  state: (path: string) => NoteState | undefined;
}

/**
 * Embedded in every category note. In a base code block `this` is the note the block is in (Obsidian's "Bases syntax"
 * help page: "Access properties with `this`"), and a link equals a file it resolves to; `list()` also covers a
 * property holding several links.
 */
export const CATEGORY_NOTE_BODY = `## Subcategories

\`\`\`base
filters:
  and:
    - list(parent).contains(this)
views:
  - type: table
    name: Subcategories
\`\`\`

## Notes

\`\`\`base
filters:
  and:
    - list(category).contains(this)
views:
  - type: table
    name: Notes
\`\`\`
`;

// Characters Obsidian refuses in file names or that break links.
const UNSAFE_IN_NAME = /[\\/:*?"<>|#^[\]]/g;

/**
 * What applying Trove's categories would change: a category note per category (with `parent` linking its parent
 * category note) and `category` on each note. Notes and category notes that already say the same are left alone, so
 * applying the plan twice changes nothing the second time.
 */
export function planCategories(categories: Category[], options: PlanOptions): CategoryChange[] {
  const kept = inVaultCategories(categories, options.inVault);
  const named = categoryNotePaths(kept, options.folder);
  const pathOf = new Map(named.map(({ category, path }) => [category.id, path]));
  const changes: CategoryChange[] = [];

  for (const { category, path } of named) {
    const parentId = category.parent ?? undefined;
    const parent = parentId === undefined ? undefined : pathOf.get(parentId);
    const current = options.state(path);
    if (current === undefined) {
      changes.push({ kind: "create", path, parent });
    } else if (parent !== undefined && current.parent !== parent) {
      changes.push({ kind: "set-parent", path, from: current.parent, to: parent });
    }

    for (const note of category.notes) {
      const state = options.inVault(note.source) ? options.state(note.path) : undefined;
      if (state !== undefined && state.category !== path) {
        changes.push({ kind: "set-category", path: note.path, from: state.category, to: path });
      }
    }
  }

  return changes;
}

/** The categories with a note from this vault, themselves or in a subcategory. Parents come before subcategories. */
function inVaultCategories(
  categories: Category[],
  inVault: (source: string) => boolean,
): Category[] {
  const kept = new Set<number>();
  for (const category of [...categories].reverse()) {
    if (kept.has(category.id) || category.notes.some((note) => inVault(note.source))) {
      kept.add(category.id);
      if (category.parent !== null && category.parent !== undefined) {
        kept.add(category.parent);
      }
    }
  }

  return categories.filter((category) => kept.has(category.id));
}

/** A path in `folder` for each category, named after it; later categories with a taken name get a number. */
function categoryNotePaths(
  categories: Category[],
  folder: string,
): { category: Category; path: string }[] {
  const named: { category: Category; path: string }[] = [];
  const taken = new Set<string>();
  for (const category of categories) {
    const name = category.name.replace(UNSAFE_IN_NAME, " ").trim() || "Category";
    let path = `${folder}/${name}.md`;
    for (let number = 2; taken.has(path.toLowerCase()); number++) {
      path = `${folder}/${name} ${number}.md`;
    }
    taken.add(path.toLowerCase());
    named.push({ category, path });
  }

  return named;
}

/** One line of the preview. */
export function describeChange(change: CategoryChange): string {
  switch (change.kind) {
    case "create":
      return change.parent === undefined
        ? `Create ${change.path}`
        : `Create ${change.path} under ${change.parent}`;
    case "set-parent":
      return `${change.path}: parent ${change.from ?? "(none)"} → ${change.to}`;
    case "set-category":
      return `${change.path}: category ${change.from ?? "(none)"} → ${change.to}`;
  }
}

/** The property value linking to the note at `path`. */
export function linkTo(path: string): string {
  return `[[${path.replace(/\.md$/, "")}]]`;
}

/** The link path a property value links to (`"[[Note#Heading|Alias]]"` gives `Note`); the first of a list. */
export function linkTarget(value: unknown): string | undefined {
  const first: unknown = Array.isArray(value) ? value[0] : value;
  if (typeof first !== "string") {
    return undefined;
  }

  const target = /^\s*\[\[([^\]|#]+)[^\]]*\]\]\s*$/.exec(first)?.[1]?.trim();

  return target === "" ? undefined : target;
}

/** The category notes from the top level down to `category`, following `parentOf`; a loop ends the path. */
export function categoryChain(
  category: string,
  parentOf: (path: string) => string | undefined,
): string[] {
  const chain: string[] = [];
  for (let at: string | undefined = category; at !== undefined && !chain.includes(at); ) {
    chain.unshift(at);
    at = parentOf(at);
  }

  return chain;
}
