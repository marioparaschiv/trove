import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { isRecord, type Schemas } from "@trove/client";

export type SetupEvent = Schemas["SetupEvent"];

/** Reads one line of `trove setup --json`. */
export function parseSetupEvent(line: string): SetupEvent {
  const event = toSetupEvent(JSON.parse(line));
  if (event === undefined) {
    throw new Error(`unexpected setup output ${line}`);
  }

  return event;
}

function toSetupEvent(value: unknown): SetupEvent | undefined {
  if (!isRecord(value)) {
    return undefined;
  }

  const { event, name, bytes, from, total, message, seconds, path } = value;
  switch (event) {
    case "file":
      return typeof name === "string" &&
        typeof bytes === "number" &&
        (from === "cache" || from === "download")
        ? { event, name, bytes, from }
        : undefined;
    case "progress":
      return typeof name === "string" && typeof bytes === "number" && typeof total === "number"
        ? { event, name, bytes, total }
        : undefined;
    case "waiting":
      return { event };
    case "verifying":
      return typeof name === "string" ? { event, name } : undefined;
    case "retry":
      return typeof message === "string" && typeof seconds === "number"
        ? { event, message, seconds }
        : undefined;
    case "done":
      return typeof path === "string" ? { event, path } : undefined;
    case "error":
      return typeof message === "string" ? { event, message } : undefined;
    default:
      return undefined;
  }
}

/** What a setup event tells the user. */
export function describeSetupEvent(event: SetupEvent): string {
  switch (event.event) {
    case "file":
      return event.from === "cache"
        ? "Copying the model from the Hugging Face cache"
        : `Downloading the model (${Math.round(event.bytes / 1_000_000)} MB)`;
    case "progress":
      return `Downloading the model: ${Math.floor((event.bytes * 100) / event.total)}%`;
    case "waiting":
      return "Waiting for a network connection";
    case "verifying":
      return "Checking the model";
    case "retry":
      return `${event.message}; trying again in ${event.seconds} s`;
    case "done":
      return "Model ready";
    case "error":
      return event.message;
  }
}

/** Runs `trove setup --json`, passing each event to `onEvent`. Fails with setup's error when it fails. */
export function runSetup(
  binary: string,
  dataDir: string,
  onEvent: (event: SetupEvent) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, ["setup", "--json"], {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
      env: { ...process.env, TROVE_DATA_DIR: dataDir },
    });

    let failure: Error | undefined;
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    createInterface({ input: child.stdout }).on("line", (line) => {
      try {
        const event = parseSetupEvent(line);
        if (event.event === "error") {
          failure ??= new Error(event.message);
        }
        onEvent(event);
      } catch (error) {
        failure ??= error instanceof Error ? error : new Error(String(error));
      }
    });

    child.on("error", (error) => reject(new Error(`failed to run ${binary}`, { cause: error })));
    child.on("close", (code, signal) => {
      if (code === 0 && failure === undefined) {
        resolve();
      } else {
        reject(failure ?? new Error(`trove setup exited with ${code ?? signal}: ${stderr.trim()}`));
      }
    });
  });
}
