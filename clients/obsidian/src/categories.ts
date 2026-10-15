import { type App, Modal, Notice, type TFile } from "obsidian";
import {
  CATEGORY_NOTE_BODY,
  type CategoryChange,
  describeChange,
  linkTarget,
  linkTo,
  type NoteState,
  planCategories,
} from "./category-notes.ts";
import { connectTo, failure, message, vaultSource } from "./connection.ts";
import type TrovePlugin from "./main.ts";
import { isSameFolder } from "./suggestions.ts";

/** The "Preview categories" and "Apply categories" commands. */
export class CategoryCommands {
  private readonly plugin: TrovePlugin;
  private readonly binary: Promise<string>;
  private readonly vaultPath: string;

  constructor(plugin: TrovePlugin, binary: Promise<string>, vaultPath: string) {
    this.plugin = plugin;
    this.binary = binary;
    this.vaultPath = vaultPath;
  }

  register(): void {
    this.plugin.addCommand({
      id: "preview-categories",
      name: "Preview categories",
      callback: () => void this.preview(),
    });
    this.plugin.addCommand({
      id: "apply-categories",
      name: "Apply categories",
      callback: () => void this.apply(),
    });
  }

  private async preview(): Promise<void> {
    try {
      new CategoryPreview(this.plugin.app, await this.plan()).open();
    } catch (error) {
      new Notice(`Trove: ${message(error)}`);
    }
  }

  private async apply(): Promise<void> {
    try {
      const changes = await this.plan();
      await applyChanges(this.plugin.app, changes, this.plugin.settings.categoryFolder);
      new Notice(
        changes.length === 0
          ? "Trove: categories are up to date"
          : `Trove: applied ${changes.length} category changes`,
      );
    } catch (error) {
      new Notice(`Trove: ${message(error)}`);
    }
  }

  private async plan(): Promise<CategoryChange[]> {
    const client = await connectTo(await this.binary);
    if ((await vaultSource(client, this.vaultPath)) === undefined) {
      throw new Error("this vault isn't a Trove source");
    }

    const folder = this.plugin.settings.categoryFolder;
    const { data, error, response } = await client.GET("/categories", {
      params: { query: { exclude_folder: folder } },
    });
    if (data === undefined) {
      throw failure("building categories", response, error);
    }

    const { app } = this.plugin;
    return planCategories(data.categories, {
      folder,
      inVault: (source) => isSameFolder(source, this.vaultPath, process.platform),
      state: (path) => noteState(app, path),
    });
  }
}

/** Lists what applying the categories would change, and changes nothing. */
class CategoryPreview extends Modal {
  private readonly changes: CategoryChange[];

  constructor(app: App, changes: CategoryChange[]) {
    super(app);
    this.changes = changes;
  }

  onOpen(): void {
    this.setTitle("Category changes");
    if (this.changes.length === 0) {
      this.contentEl.createEl("p", { text: "Categories are up to date." });

      return;
    }

    this.contentEl.createEl("p", {
      text: `Applying categories would make ${this.changes.length} changes. No file moves.`,
    });
    const list = this.contentEl.createEl("ul");
    for (const change of this.changes) {
      list.createEl("li", { text: describeChange(change) });
    }
  }

  onClose(): void {
    this.contentEl.empty();
  }
}

async function applyChanges(app: App, changes: CategoryChange[], folder: string): Promise<void> {
  if (
    changes.some((change) => change.kind === "create") &&
    app.vault.getFolderByPath(folder) === null
  ) {
    await app.vault.createFolder(folder);
  }

  for (const change of changes) {
    if (change.kind === "create") {
      const file = await app.vault.create(change.path, CATEGORY_NOTE_BODY);
      if (change.parent !== undefined) {
        await setLink(app, file, "parent", change.parent);
      }

      continue;
    }

    const file = app.vault.getFileByPath(change.path);
    if (file === null) {
      throw new Error(`${change.path} is no longer in the vault`);
    }

    await setLink(app, file, change.kind === "set-parent" ? "parent" : "category", change.to);
  }
}

function setLink(app: App, file: TFile, property: string, target: string): Promise<void> {
  return app.fileManager.processFrontMatter(file, (frontmatter: Record<string, unknown>) => {
    frontmatter[property] = linkTo(target);
  });
}

/** The note's `category` and `parent` as the paths their links resolve to; `undefined` when there is no such note. */
export function noteState(app: App, path: string): NoteState | undefined {
  const file = app.vault.getFileByPath(path);
  if (file === null) {
    return undefined;
  }

  const frontmatter = app.metadataCache.getFileCache(file)?.frontmatter;

  return {
    category: resolveLink(app, frontmatter?.category, path),
    parent: resolveLink(app, frontmatter?.parent, path),
  };
}

/** The path of the note a property's link names: the note it resolves to, else the path it spells. */
function resolveLink(app: App, value: unknown, sourcePath: string): string | undefined {
  const target = linkTarget(value);
  if (target === undefined) {
    return undefined;
  }

  const resolved = app.metadataCache.getFirstLinkpathDest(target, sourcePath);

  return resolved?.path ?? (target.endsWith(".md") ? target : `${target}.md`);
}
