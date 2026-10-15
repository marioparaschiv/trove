import { FileSystemAdapter, Notice, Plugin } from "obsidian";
import { DEFAULT_SETTINGS, parseSettings, type Settings } from "./settings.ts";
import { TroveSettingTab } from "./settings-tab.ts";
import { Switcher } from "./switcher.ts";

export default class TrovePlugin extends Plugin {
  settings: Settings = { ...DEFAULT_SETTINGS };

  async onload(): Promise<void> {
    try {
      this.settings = parseSettings(await this.loadData());
    } catch (error) {
      new Notice(`Trove: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }

    const adapter = this.app.vault.adapter;
    if (!(adapter instanceof FileSystemAdapter)) {
      throw new Error("Trove needs a vault on the local file system");
    }

    const vaultPath = adapter.getBasePath();

    this.addCommand({
      id: "open-switcher",
      name: "Open quick switcher",
      callback: () => new Switcher(this.app, this.settings.binaryPath, vaultPath).open(),
    });
    this.addSettingTab(new TroveSettingTab(this.app, this));
  }
}
