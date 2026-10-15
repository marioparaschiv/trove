import { Action, ActionPanel, getPreferenceValues, List } from "@raycast/api";
import { usePromise } from "@raycast/utils";
import { connect, type TroveClient } from "@trove/client";
import { useRef, useState } from "react";
import { obsidianUrl } from "./obsidian-url.ts";

export default function SearchVault() {
  const { troveBinary } = getPreferenceValues<Preferences.SearchVault>();
  const [query, setQuery] = useState("");
  const { data: connection, isLoading } = usePromise(connect, [{ binary: troveBinary }], {
    failureToastOptions: { title: "Couldn't start Trove" },
  });

  if (connection === undefined) {
    return <List isLoading={isLoading} searchText={query} onSearchTextChange={setQuery} throttle />;
  }

  return <Results client={connection.client} query={query} onQueryChange={setQuery} />;
}

interface ResultsProps {
  client: TroveClient;
  query: string;
  onQueryChange: (query: string) => void;
}

function Results({ client, query, onQueryChange }: ResultsProps) {
  const abortable = useRef<AbortController>(null);
  const hasQuery = query.trim() !== "";
  const { data, isLoading } = usePromise(
    async (text: string) => {
      const { data, error } = await client.POST("/search", {
        body: { query: text },
        signal: abortable.current?.signal,
      });
      if (data === undefined) {
        throw new Error(error.error);
      }

      return data.results;
    },
    [query],
    { execute: hasQuery, abortable, failureToastOptions: { title: "Search failed" } },
  );

  // usePromise keeps the last results when it stops executing, so a cleared query would show them.
  const results = hasQuery ? (data ?? []) : [];

  return (
    <List
      isLoading={isLoading}
      searchText={query}
      onSearchTextChange={onQueryChange}
      searchBarPlaceholder="Search your vault by meaning"
      throttle
    >
      {results.map((result) => (
        <List.Item
          key={`${result.source}/${result.path}`}
          title={result.title}
          subtitle={result.heading_path.join(" > ")}
          accessories={[{ text: result.path }]}
          actions={
            <ActionPanel>
              <Action.Open title="Open in Obsidian" target={obsidianUrl(result)} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
