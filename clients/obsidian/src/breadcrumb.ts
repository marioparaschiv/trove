import { Keymap, MarkdownView, Notice, type Plugin } from "obsidian";
import { noteState } from "./categories.ts";
import { categoryChain } from "./category-notes.ts";
import { categoryName } from "./category-pins.ts";
import { message } from "./connection.ts";

interface Breadcrumb {
  el: HTMLElement;
  /** The note and category path last shown, to skip rendering the same again. */
  shown: string | undefined;
}

/**
 * The category path above each note's title, read from its `category` property (or `parent`, for a category note)
 * and the `parent` of each category note up to the top level. Built from the classes of Obsidian's own header
 * breadcrumbs.
 */
export class Breadcrumbs {
  private readonly plugin: Plugin;
  private readonly breadcrumbs = new Map<MarkdownView, Breadcrumb>();

  constructor(plugin: Plugin) {
    this.plugin = plugin;
  }

  register(): void {
    const { workspace, metadataCache } = this.plugin.app;
    const refresh = () => this.refresh();
    workspace.onLayoutReady(refresh);
    this.plugin.registerEvent(workspace.on("file-open", refresh));
    this.plugin.registerEvent(workspace.on("layout-change", refresh));
    this.plugin.registerEvent(metadataCache.on("changed", refresh));
    this.plugin.register(() => {
      for (const breadcrumb of this.breadcrumbs.values()) {
        breadcrumb.el.remove();
      }
      this.breadcrumbs.clear();
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

    for (const [view, breadcrumb] of this.breadcrumbs) {
      if (!views.has(view)) {
        breadcrumb.el.remove();
        this.breadcrumbs.delete(view);
      }
    }
  }

  private show(view: MarkdownView): void {
    // Where Obsidian puts the inline title in each mode.
    const container =
      view.getMode() === "preview"
        ? view.previewMode.containerEl.querySelector(".mod-header")
        : view.contentEl.querySelector(".cm-sizer");
    if (view.file === null || container === null) {
      return;
    }

    let breadcrumb = this.breadcrumbs.get(view);
    if (breadcrumb === undefined) {
      breadcrumb = { el: createDiv({ cls: "view-header-title-parent" }), shown: undefined };
      this.breadcrumbs.set(view, breadcrumb);
    }

    const title = container.querySelector(".inline-title");
    if (title !== null && breadcrumb.el.nextElementSibling !== title) {
      title.before(breadcrumb.el);
    } else if (title === null && breadcrumb.el.parentElement !== container) {
      container.prepend(breadcrumb.el);
    }

    const path = view.file.path;
    const chain = this.chain(path);
    const shown = [path, ...chain].join("\n");
    if (breadcrumb.shown !== shown) {
      breadcrumb.shown = shown;
      this.render(breadcrumb.el, path, chain);
    }
  }

  private chain(path: string): string[] {
    const { app } = this.plugin;
    const state = noteState(app, path);
    const start = state?.category ?? state?.parent;

    return start === undefined
      ? []
      : categoryChain(start, (category) => noteState(app, category)?.parent);
  }

  private render(el: HTMLElement, sourcePath: string, chain: string[]): void {
    el.empty();
    el.toggle(chain.length > 0);
    chain.forEach((path, place) => {
      if (place > 0) {
        el.createSpan({ cls: "view-header-breadcrumb-separator", text: "/" });
      }

      el.createSpan({ cls: "view-header-breadcrumb", text: categoryName(path) }).addEventListener(
        "click",
        (evt) => {
          this.plugin.app.workspace
            .openLinkText(path, sourcePath, Keymap.isModEvent(evt))
            .catch(
              (error: unknown) => new Notice(`Trove: couldn't open ${path}: ${message(error)}`),
            );
        },
      );
    });
  }
}
