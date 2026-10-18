import { failure, type Schemas, type TroveClient } from "@trove/client";
import { type App, Modal, Notice, Setting } from "obsidian";
import { connectTo, message, vaultSource } from "./connection.ts";
import { appendLink } from "./links.ts";

type Suggestion = Schemas["Suggestion"];

/** How many suggestions one review walks through. */
const REVIEW_SIZE = 50;

/** Walks Trove's link suggestions one at a time: linking adds a wikilink to the note, dismissing records it. */
export class LinkSuggestions extends Modal {
  private readonly binary: Promise<string>;
  private readonly vaultPath: string;
  private readonly categoryFolder: string;
  private connection: { client: TroveClient; source: string } | undefined;
  private suggestions: Suggestion[] = [];
  private place = 0;
  private busy = false;

  constructor(app: App, binary: Promise<string>, vaultPath: string, categoryFolder: string) {
    super(app);
    this.binary = binary;
    this.vaultPath = vaultPath;
    this.categoryFolder = categoryFolder;
  }

  onOpen(): void {
    this.setTitle("Link suggestions");
    this.contentEl.createEl("p", { text: "Loading..." });
    void this.load();
  }

  onClose(): void {
    this.contentEl.empty();
  }

  private async load(): Promise<void> {
    try {
      const client = await connectTo(await this.binary);
      const source = await vaultSource(client, this.vaultPath);
      if (source === undefined) {
        throw new Error("this vault isn't a Trove source");
      }

      const { data, error, response } = await client.GET("/suggestions", {
        params: { query: { source, limit: REVIEW_SIZE, exclude_folder: this.categoryFolder } },
      });
      if (data === undefined) {
        throw failure("finding link suggestions", response, error);
      }

      this.connection = { client, source };
      this.suggestions = data.suggestions;
      this.render();
    } catch (error) {
      this.contentEl.empty();
      this.contentEl.createEl("p", { text: `Trove: ${message(error)}` });
    }
  }

  private render(): void {
    this.contentEl.empty();
    const suggestion = this.suggestions[this.place];
    if (suggestion === undefined) {
      this.setTitle("Link suggestions");
      this.contentEl.createEl("p", { text: "No more link suggestions." });

      return;
    }

    const { note, target, similarity } = suggestion;
    this.setTitle(`Link suggestion ${this.place + 1} of ${this.suggestions.length}`);
    new Setting(this.contentEl)
      .setName(`${note.title} → ${target.title}`)
      .setDesc(
        `Adds a link to ${target.path} at the end of ${note.path}. Similarity ${similarity.toFixed(2)}.`,
      );
    new Setting(this.contentEl)
      .addButton((button) =>
        button
          .setButtonText("Link")
          .setCta()
          .onClick(() => void this.decide(suggestion, "accepted")),
      )
      .addButton((button) =>
        button.setButtonText("Dismiss").onClick(() => void this.decide(suggestion, "dismissed")),
      )
      .addButton((button) => button.setButtonText("Skip").onClick(() => this.next()));
  }

  private next(): void {
    this.place += 1;
    this.render();
  }

  private async decide(suggestion: Suggestion, state: Schemas["SuggestionState"]): Promise<void> {
    if (this.busy || this.connection === undefined) {
      return;
    }

    this.busy = true;
    try {
      if (state === "accepted") {
        await this.link(suggestion);
      }

      const { client, source } = this.connection;
      const { error, response } = await client.POST("/suggestions/decisions", {
        body: { source, note: suggestion.note.path, target: suggestion.target.path, state },
      });
      if (!response.ok) {
        throw failure("recording the decision", response, error);
      }

      this.next();
    } catch (error) {
      new Notice(`Trove: ${message(error)}`);
    } finally {
      this.busy = false;
    }
  }

  private async link({ note, target }: Suggestion): Promise<void> {
    const file = this.app.vault.getFileByPath(note.path);
    const targetFile = this.app.vault.getFileByPath(target.path);
    if (file === null || targetFile === null) {
      throw new Error(`${file === null ? note.path : target.path} is no longer in the vault`);
    }

    const link = `[[${this.app.metadataCache.fileToLinktext(targetFile, file.path, true)}]]`;
    await this.app.vault.process(file, (text) => appendLink(text, link));
  }
}
