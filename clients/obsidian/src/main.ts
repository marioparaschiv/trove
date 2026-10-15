import { FileSystemAdapter, Notice, Plugin } from "obsidian";
import { Breadcrumbs } from "./breadcrumb.ts";
import { CategoryCommands } from "./categories.ts";
import { CategoryEdits } from "./category-edits.ts";
import { message } from "./connection.ts";
import { prepareTrove } from "./first-run.ts";
import { LinkSuggestions } from "./link-suggestions.ts";
import { RelatedNotes } from "./related.ts";
import { DEFAULT_SETTINGS, parseSettings, type Settings } from "./settings.ts";
import { TroveSettingTab } from "./settings-tab.ts";
import { Switcher } from "./switcher.ts";

export default class TrovePlugin extends Plugin {
  settings: Settings = { ...DEFAULT_SETTINGS };

  async onload(): Promise<void> {
    try {
      this.settings = parseSettings(await this.loadData());
    } catch (error) {
      new Notice(`Trove: ${message(error)}`);
      throw error;
    }

    const adapter = this.app.vault.adapter;
    if (!(adapter instanceof FileSystemAdapter)) {
      throw new Error("Trove needs a vault on the local file system");
    }

    const vaultPath = adapter.getBasePath();
    const binary = new Promise<void>((resolve) => this.app.workspace.onLayoutReady(resolve)).then(
      () => prepareTrove(this.settings, vaultPath),
    );
    binary.catch((error: unknown) => new Notice(`Trove: ${message(error)}`, 0));

    this.addCommand({
      id: "open-switcher",
      name: "Open quick switcher",
      callback: () => new Switcher(this.app, binary, vaultPath).open(),
    });
    this.addCommand({
      id: "review-link-suggestions",
      name: "Review link suggestions",
      callback: () =>
        new LinkSuggestions(this.app, binary, vaultPath, this.settings.categoryFolder).open(),
    });
    const categoryEdits = new CategoryEdits(this, binary, vaultPath);
    categoryEdits.register();
    new CategoryCommands(this, binary, vaultPath, categoryEdits).register();
    new RelatedNotes(this, binary, vaultPath).register();
    new Breadcrumbs(this).register();
    this.addSettingTab(new TroveSettingTab(this.app, this));
  }
}
