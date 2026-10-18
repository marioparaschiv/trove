import { failure } from "@trove/client";
import { type App, FuzzySuggestModal, Notice, type TAbstractFile, TFile } from "obsidian";
import { noteState } from "./categories.ts";
import { linkTo, type NoteState } from "./category-notes.ts";
import {
  type CategoryPin,
  editPins,
  inFolder,
  mergePin,
  renamedStates,
  renamePin,
} from "./category-pins.ts";
import { connectTo, message, vaultSource } from "./connection.ts";
import type TrovePlugin from "./main.ts";

/**
 * Turns manual category edits into Trove pins, so later runs keep them: renaming a category note, setting a note's
 * `category` or a category note's `parent` by hand, and the "Merge category into..." command.
 */
export class CategoryEdits {
  private readonly plugin: TrovePlugin;
  private readonly binary: Promise<string>;
  private readonly vaultPath: string;
  /** Every note's category properties as last seen, to tell which property an edit changed. */
  private states = new Map<string, NoteState>();
  /** The last send: Trove keeps the pin that arrives last, so each send waits for the one before. */
  private sending: Promise<void> = Promise.resolve();

  constructor(plugin: TrovePlugin, binary: Promise<string>, vaultPath: string) {
    this.plugin = plugin;
    this.binary = binary;
    this.vaultPath = vaultPath;
  }

  register(): void {
    const { app } = this.plugin;
    app.workspace.onLayoutReady(() => {
      for (const file of app.vault.getMarkdownFiles()) {
        this.see(file.path);
      }
    });
    this.plugin.registerEvent(app.metadataCache.on("changed", (file) => this.changed(file)));
    this.plugin.registerEvent(
      app.vault.on("rename", (file, oldPath) => this.renamed(file, oldPath)),
    );
    this.plugin.registerEvent(app.vault.on("delete", (file) => this.states.delete(file.path)));
    this.plugin.addCommand({
      id: "merge-category",
      name: "Merge category into...",
      checkCallback: (checking) => {
        const file = app.workspace.getActiveFile();
        if (file === null || !inFolder(file.path, this.folder())) {
          return false;
        }
        if (!checking) {
          new CategoryPicker(
            app,
            this.folder(),
            file,
            (into) => void this.merge(file, into),
          ).open();
        }

        return true;
      },
    });
  }

  /** Sets a category property as the plugin, which is not a manual edit. */
  async setLink(file: TFile, property: "category" | "parent", target: string): Promise<void> {
    const state = this.states.get(file.path) ?? { category: undefined, parent: undefined };
    this.states.set(file.path, { ...state, [property]: target });
    await this.plugin.app.fileManager.processFrontMatter(
      file,
      (frontmatter: Record<string, unknown>) => {
        frontmatter[property] = linkTo(target);
      },
    );
  }

  private folder(): string {
    return this.plugin.settings.categoryFolder;
  }

  private see(path: string): { before: NoteState | undefined; after: NoteState } | undefined {
    const after = noteState(this.plugin.app, path);
    if (after === undefined) {
      return undefined;
    }

    const before = this.states.get(path);
    this.states.set(path, after);

    return { before, after };
  }

  private changed(file: TFile): void {
    const seen = this.see(file.path);
    // Only the note being edited counts: other notes change when sync brings another device's edits.
    if (seen === undefined || this.plugin.app.workspace.getActiveFile()?.path !== file.path) {
      return;
    }

    const pins = editPins(file.path, seen.before, seen.after, this.states, this.folder());
    if (pins.length > 0) {
      this.pin(pins);
    }
  }

  private renamed(file: TAbstractFile, oldPath: string): void {
    if (!(file instanceof TFile) || file.extension !== "md") {
      return;
    }

    const folder = this.folder();
    const isCategory = inFolder(oldPath, folder) && inFolder(file.path, folder);
    if (isCategory) {
      this.pin([renamePin(oldPath, file.path, this.states, folder)]);
    }
    this.states = renamedStates(this.states, oldPath, file.path);
  }

  private async merge(from: TFile, into: TFile): Promise<void> {
    const { app } = this.plugin;
    try {
      await this.send([mergePin(from.path, into.path, this.states, this.folder())]);

      for (const [path, { category, parent }] of [...this.states]) {
        const file = app.vault.getFileByPath(path);
        if (file === null) {
          continue;
        }
        if (category === from.path) {
          await this.setLink(file, "category", into.path);
        }
        if (parent === from.path) {
          await this.setLink(file, "parent", into.path);
        }
      }
      await app.fileManager.trashFile(from);
      new Notice(`Trove: merged ${from.basename} into ${into.basename}`);
    } catch (error) {
      new Notice(`Trove: ${message(error)}`);
    }
  }

  private pin(pins: CategoryPin[]): void {
    this.send(pins).catch((error: unknown) => new Notice(`Trove: ${message(error)}`));
  }

  private send(pins: CategoryPin[]): Promise<void> {
    const sent = this.sending.then(() => this.put(pins));
    // The caller reports the failure; the next send goes ahead regardless.
    this.sending = sent.catch(() => undefined);

    return sent;
  }

  private async put(pins: CategoryPin[]): Promise<void> {
    const client = await connectTo(await this.binary);
    const source = await vaultSource(client, this.vaultPath);
    if (source === undefined) {
      throw new Error("this vault isn't a Trove source");
    }

    const { error, response } = await client.PUT("/categories/pins", { body: { source, pins } });
    if (!response.ok) {
      throw failure("pinning the category edit", response, error);
    }
  }
}

/** Picks the category note to merge another one into. */
class CategoryPicker extends FuzzySuggestModal<TFile> {
  private readonly folder: string;
  private readonly from: TFile;
  private readonly onChoose: (into: TFile) => void;

  constructor(app: App, folder: string, from: TFile, onChoose: (into: TFile) => void) {
    super(app);
    this.folder = folder;
    this.from = from;
    this.onChoose = onChoose;
    this.setPlaceholder(`Merge ${from.basename} into...`);
  }

  getItems(): TFile[] {
    return this.app.vault
      .getMarkdownFiles()
      .filter((file) => file !== this.from && inFolder(file.path, this.folder));
  }

  getItemText(file: TFile): string {
    return file.basename;
  }

  onChooseItem(file: TFile): void {
    this.onChoose(file);
  }
}
