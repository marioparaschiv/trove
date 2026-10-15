import { type App, PluginSettingTab, Setting } from "obsidian";
import type TrovePlugin from "./main.ts";

export class TroveSettingTab extends PluginSettingTab {
  private readonly plugin: TrovePlugin;

  constructor(app: App, plugin: TrovePlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    this.containerEl.empty();

    new Setting(this.containerEl)
      .setName("Trove binary")
      .setDesc(
        "Absolute path to a trove executable to run instead of the release the plugin downloads. Leave empty to use the release. Applies when the plugin next loads.",
      )
      .addText((text) =>
        text.setValue(this.plugin.settings.binaryPath).onChange(async (value) => {
          this.plugin.settings.binaryPath = value.trim();
          await this.plugin.saveData(this.plugin.settings);
        }),
      );
  }
}
