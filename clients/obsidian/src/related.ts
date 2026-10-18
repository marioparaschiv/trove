import { failure } from "@trove/client";
import { Keymap, MarkdownView, Notice, type Plugin } from "obsidian";
import { connectTo, message, noteLink, vaultSource } from "./connection.ts";
import type { TroveResult } from "./suggestions.ts";
import { inVault } from "./vault-paths.ts";

interface Section {
  el: HTMLElement;
  /** The note the section lists notes for. */
  path: string | undefined;
}

/** The related notes at the bottom of each Markdown view, built from the classes of Obsidian's backlinks section. */
export class RelatedNotes {
  private readonly plugin: Plugin;
  private readonly binary: Promise<string>;
  private readonly vaultPath: string;
  private readonly sections = new Map<MarkdownView, Section>();

  constructor(plugin: Plugin, binary: Promise<string>, vaultPath: string) {
    this.plugin = plugin;
    this.binary = binary;
    this.vaultPath = vaultPath;
  }

  /** Shows the section in every Markdown view and keeps it current as notes and modes change. */
  register(): void {
    const { workspace } = this.plugin.app;
    const refresh = () => this.refresh();
    workspace.onLayoutReady(refresh);
    this.plugin.registerEvent(workspace.on("file-open", refresh));
    this.plugin.registerEvent(workspace.on("layout-change", refresh));
    this.plugin.register(() => {
      for (const section of this.sections.values()) {
        section.el.remove();
      }
      this.sections.clear();
    });
  }

  private refresh(): void {
    const views = new Set<MarkdownView>();
    for (const leaf of this.plugin.app.workspace.getLeavesOfType("markdown")) {
      if (leaf.view instanceof MarkdownView) {
        views.add(leaf.view);
        this.show(leaf.view);
      }
    }

    for (const [view, section] of this.sections) {
      if (!views.has(view)) {
        section.el.remove();
        this.sections.delete(view);
      }
    }
  }

  private show(view: MarkdownView): void {
    // Where Obsidian puts the backlinks section in each mode.
    const container =
      view.getMode() === "preview"
        ? view.previewMode.containerEl.querySelector(".mod-footer")
        : view.contentEl.querySelector(".cm-sizer");
    if (view.file === null || container === null) {
      return;
    }

    let section = this.sections.get(view);
    if (section === undefined) {
      section = { el: createDiv({ cls: "embedded-backlinks" }), path: undefined };
      this.sections.set(view, section);
    }

    if (section.el.parentElement !== container) {
      container.appendChild(section.el);
    }

    if (section.path !== view.file.path) {
      section.path = view.file.path;
      void this.load(view, section, view.file.path);
    }
  }

  private async load(view: MarkdownView, section: Section, path: string): Promise<void> {
    let results: TroveResult[];
    try {
      results = await this.related(path);
    } catch (error) {
      if (section.path === path) {
        this.render(view, section, [], `Trove: ${message(error)}`);
      }

      return;
    }

    if (section.path === path) {
      this.render(view, section, results, "No related notes.");
    }
  }

  private async related(path: string): Promise<TroveResult[]> {
    const client = await connectTo(await this.binary);
    const source = await vaultSource(client, this.vaultPath);
    if (source === undefined) {
      throw new Error("this vault isn't a Trove source");
    }

    const { data, error, response } = await client.GET("/related", {
      params: { query: { source, path } },
    });
    // A note Trove hasn't indexed yet.
    if (response.status === 404) {
      return [];
    }

    if (data === undefined) {
      throw failure("finding related notes", response, error);
    }

    return inVault(data.results, this.vaultPath, process.platform);
  }

  private render(
    view: MarkdownView,
    section: Section,
    results: TroveResult[],
    empty: string,
  ): void {
    section.el.empty();
    const pane = section.el.createDiv({ cls: "backlink-pane" });
    const header = pane.createDiv({ cls: "tree-item-self" });
    header.createDiv({ cls: "tree-item-inner", text: "Related notes" });
    header
      .createDiv({ cls: "tree-item-flair-outer" })
      .createSpan({ cls: "tree-item-flair", text: String(results.length) });

    const list = pane.createDiv({ cls: "search-result-container" });
    if (results.length === 0) {
      list.createDiv({ cls: "search-empty-state", text: empty });

      return;
    }

    for (const result of results) {
      const item = list.createDiv({ cls: "tree-item search-result" });
      item
        .createDiv({ cls: "tree-item-self search-result-file-title is-clickable" })
        .createDiv({ cls: "tree-item-inner", text: result.title });
      if (result.heading_path.length > 0) {
        item
          .createDiv({ cls: "search-result-file-matches" })
          .createDiv({ cls: "search-result-file-match", text: result.heading_path.join(" > ") });
      }

      item.addEventListener("click", (evt) => {
        const link = noteLink(result);
        this.plugin.app.workspace
          .openLinkText(link, view.file?.path ?? "", Keymap.isModEvent(evt))
          .catch((error: unknown) => new Notice(`Trove: couldn't open ${link}: ${message(error)}`));
      });
    }
  }
}
