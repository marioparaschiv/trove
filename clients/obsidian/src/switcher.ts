import { connect, type TroveClient } from "@trove/client";
import {
  type App,
  debounce,
  type SearchResult as FuzzyMatch,
  Keymap,
  Notice,
  Platform,
  prepareFuzzySearch,
  renderResults,
  SuggestModal,
  stripHeadingForLink,
} from "obsidian";
import { obsidianFetch } from "./obsidian-fetch.ts";
import {
  type FileSuggestion,
  inVault,
  isSameFolder,
  merge,
  type Suggestion,
  type TroveResult,
} from "./suggestions.ts";

const SEARCH_DELAY_MS = 150;

/** A quick switcher like `Ctrl+O` that merges in Trove's results once they arrive. */
export class Switcher extends SuggestModal<Suggestion> {
  private readonly vaultPath: string;
  private readonly connection: Promise<TroveClient>;
  private failed = false;
  private troveResults: { query: string; results: TroveResult[] } | undefined;
  private readonly searchSoon = debounce(
    (query: string) => void this.search(query),
    SEARCH_DELAY_MS,
    true,
  );

  constructor(app: App, binaryPath: string, vaultPath: string) {
    super(app);
    this.vaultPath = vaultPath;
    this.setPlaceholder("Find a note by name or meaning...");
    this.setInstructions([
      { command: "↑↓", purpose: "to navigate" },
      { command: "↵", purpose: "to open" },
      { command: Platform.isMacOS ? "cmd ↵" : "ctrl ↵", purpose: "to open in new tab" },
      { command: "esc", purpose: "to dismiss" },
    ]);

    this.connection = this.start(binaryPath);
    this.connection.catch((error: unknown) => this.fail(error));
  }

  getSuggestions(query: string): Suggestion[] {
    const trimmed = query.trim();
    const files = this.fileMatches(trimmed);
    if (trimmed === "" || this.failed) {
      return files;
    }

    if (this.troveResults?.query === trimmed) {
      return merge(files, this.troveResults.results);
    }

    this.searchSoon(trimmed);

    return files;
  }

  renderSuggestion(suggestion: Suggestion, el: HTMLElement): void {
    el.addClass("mod-complex");
    const content = el.createDiv({ cls: "suggestion-content" });
    const title = content.createDiv({ cls: "suggestion-title" });

    if (suggestion.kind === "file") {
      if (suggestion.match === null) {
        title.setText(withoutExtension(suggestion.path));
      } else {
        renderResults(title, withoutExtension(suggestion.path), suggestion.match);
      }

      return;
    }

    const { path, heading_path } = suggestion.result;
    title.setText(withoutExtension(path));
    if (heading_path.length > 0) {
      content.createDiv({ cls: "suggestion-note", text: heading_path.join(" > ") });
    }
  }

  onChooseSuggestion(suggestion: Suggestion, evt: MouseEvent | KeyboardEvent): void {
    const linktext = linkTo(suggestion);
    this.app.workspace
      .openLinkText(linktext, "", Keymap.isModEvent(evt))
      .catch((error: unknown) => new Notice(`Trove: couldn't open ${linktext}: ${message(error)}`));
  }

  onClose(): void {
    super.onClose();
    this.searchSoon.cancel();
  }

  private async start(binaryPath: string): Promise<TroveClient> {
    if (binaryPath === "") {
      throw new Error("set the path to the trove binary in the Trove settings");
    }

    const { client } = await connect({ binary: binaryPath, fetch: obsidianFetch });
    void this.prewarm(client);

    const sources = await client.GET("/sources");
    if (sources.data === undefined) {
      throw failure("listing sources", sources.response, sources.error);
    }

    if (
      !sources.data.some((source) => isSameFolder(source.path, this.vaultPath, process.platform))
    ) {
      throw new Error(
        `this vault isn't a Trove source; run trove sources add "${this.vaultPath}", then trove index`,
      );
    }

    return client;
  }

  private async prewarm(client: TroveClient): Promise<void> {
    try {
      const { data, error, response } = await client.POST("/prewarm");
      if (data === undefined) {
        throw failure("loading the model", response, error);
      }
    } catch (error) {
      this.fail(error);
    }
  }

  private async search(query: string): Promise<void> {
    try {
      const client = await this.connection;
      const { data, error, response } = await client.POST("/search", { body: { query } });
      if (data === undefined) {
        throw failure("search", response, error);
      }

      if (query !== this.inputEl.value.trim()) {
        return;
      }

      this.troveResults = {
        query,
        results: inVault(data.results, this.vaultPath, process.platform),
      };
      // SuggestModal recomputes its suggestions on input.
      this.inputEl.dispatchEvent(new Event("input"));
    } catch (error) {
      this.fail(error);
    }
  }

  private fileMatches(query: string): FileSuggestion[] {
    const { vault, workspace } = this.app;
    if (query === "") {
      return workspace
        .getLastOpenFiles()
        .filter((path) => vault.getFileByPath(path)?.extension === "md")
        .map((path) => ({ kind: "file", path, match: null }));
    }

    const fuzzy = prepareFuzzySearch(query);
    const matches: { path: string; match: FuzzyMatch }[] = [];
    for (const file of vault.getMarkdownFiles()) {
      const match = fuzzy(withoutExtension(file.path));
      if (match !== null) {
        matches.push({ path: file.path, match });
      }
    }

    return matches
      .sort((a, b) => b.match.score - a.match.score)
      .map(({ path, match }) => ({ kind: "file", path, match }));
  }

  /** Shows the first failure; the switcher then works on filenames alone. */
  private fail(error: unknown): void {
    if (this.failed) {
      return;
    }

    this.failed = true;
    new Notice(`Trove: ${message(error)}`);
  }
}

/** A link to the note, at the matched heading for Trove results. */
function linkTo(suggestion: Suggestion): string {
  if (suggestion.kind === "file") {
    return suggestion.path;
  }

  const { path, heading_path } = suggestion.result;

  return [path, ...heading_path.map(stripHeadingForLink)].join("#");
}

function withoutExtension(path: string): string {
  return path.endsWith(".md") ? path.slice(0, -".md".length) : path;
}

function failure(action: string, response: Response, error: { error: string } | undefined): Error {
  return new Error(
    `${action} failed with ${response.status}: ${error?.error ?? response.statusText}`,
  );
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
