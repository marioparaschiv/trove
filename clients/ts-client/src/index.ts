import { spawn } from "node:child_process";
import { closeSync, mkdirSync, openSync, readFileSync, watch } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import createClient, { type Client, type ClientOptions } from "openapi-fetch";
import type { components, paths } from "./schema.ts";

export type Schemas = components["schemas"];
export type RuntimeFile = Schemas["RuntimeFile"];
export type TroveClient = Client<paths>;

export interface Connection {
  client: TroveClient;
  runtime: RuntimeFile;
}

export interface ConnectOptions {
  /** The `trove` binary, run as `trove serve` when no service is running. */
  binary: string;
  /** The data folder. Defaults to the one the binary uses without `TROVE_DATA_DIR` set. */
  dataDir?: string;
  /** Sends the requests. Defaults to the global `fetch`. */
  fetch?: ClientOptions["fetch"];
}

const RUNTIME_FILE = "runtime.json";
const SERVE_LOG = "serve.log";
const START_TIMEOUT_MS = 30_000;

/** Connects to the running `trove serve`, starting it first when none runs. */
export async function connect({
  binary,
  dataDir = defaultDataDir(),
  fetch,
}: ConnectOptions): Promise<Connection> {
  const found = readRuntimeFile(dataDir);
  const runtime = found && isAlive(found.pid) ? found : await start(binary, dataDir);

  return {
    runtime,
    client: createClient<paths>({
      baseUrl: `http://127.0.0.1:${runtime.port}`,
      headers: { Authorization: `Bearer ${runtime.token}` },
      fetch,
    }),
  };
}

/** The error a failed request gives, with the status and the server's message when it sent one. */
export function failure(
  action: string,
  response: Response,
  error: Schemas["ErrorBody"] | undefined,
): Error {
  return new Error(
    `${action} failed with ${response.status}: ${error?.error ?? response.statusText}`,
  );
}

/** The data folder the `trove` binary uses: `TROVE_DATA_DIR`, else the OS's local data folder. */
export function defaultDataDir(): string {
  const fromEnv = process.env.TROVE_DATA_DIR;
  if (fromEnv !== undefined) {
    if (!isAbsolute(fromEnv)) {
      throw new Error(`TROVE_DATA_DIR must be an absolute path, got ${JSON.stringify(fromEnv)}`);
    }

    return fromEnv;
  }

  switch (process.platform) {
    case "win32": {
      const localAppData = process.env.LOCALAPPDATA;
      if (localAppData === undefined) {
        throw new Error("LOCALAPPDATA is not set, so the Trove data folder can't be found");
      }

      return join(localAppData, "Trove", "data");
    }
    case "darwin":
      return join(homedir(), "Library", "Application Support", "Trove");
    default: {
      const xdg = process.env.XDG_DATA_HOME;
      const dataHome =
        xdg !== undefined && isAbsolute(xdg) ? xdg : join(homedir(), ".local", "share");

      return join(dataHome, "trove");
    }
  }
}

/** Reads and checks the runtime file; `undefined` when there is none. */
export function readRuntimeFile(dataDir: string): RuntimeFile | undefined {
  const path = join(dataDir, RUNTIME_FILE);
  let text: string;
  try {
    text = readFileSync(path, "utf8");
  } catch (error) {
    if (isErrnoException(error) && error.code === "ENOENT") {
      return undefined;
    }

    throw error;
  }

  const value: unknown = JSON.parse(text);
  if (!isRuntimeFile(value)) {
    throw new Error(`invalid runtime file ${path}: ${text}`);
  }

  return value;
}

function isRuntimeFile(value: unknown): value is RuntimeFile {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const { port, token, pid, version, ...rest } = value as Record<string, unknown>;

  return (
    Object.keys(rest).length === 0 &&
    typeof port === "number" &&
    Number.isInteger(port) &&
    port > 0 &&
    port <= 65_535 &&
    typeof token === "string" &&
    token !== "" &&
    typeof pid === "number" &&
    Number.isInteger(pid) &&
    pid > 0 &&
    typeof version === "string"
  );
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);

    return true;
  } catch (error) {
    if (isErrnoException(error) && error.code === "ESRCH") {
      return false;
    }

    // EPERM: the process exists but belongs to someone else.
    if (isErrnoException(error) && error.code === "EPERM") {
      return true;
    }

    throw error;
  }
}

function isErrnoException(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

/** Starts `trove serve` detached, so it outlives this process, and waits for its runtime file. */
function start(binary: string, dataDir: string): Promise<RuntimeFile> {
  mkdirSync(dataDir, { recursive: true });

  // Watch before spawning so the runtime file can't appear unnoticed.
  const watcher = watch(dataDir);
  const logPath = join(dataDir, SERVE_LOG);
  const log = openSync(logPath, "w");
  const child = spawn(binary, ["serve"], {
    detached: true,
    stdio: ["ignore", "ignore", log],
    windowsHide: true,
    env: { ...process.env, TROVE_DATA_DIR: dataDir },
  });
  closeSync(log);
  child.unref();

  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () =>
        finish(
          new Error(
            `trove serve did not start within ${START_TIMEOUT_MS / 1000} s; see ${logPath}`,
          ),
        ),
      START_TIMEOUT_MS,
    );

    const finish = (result: RuntimeFile | Error) => {
      watcher.close();
      clearTimeout(timer);
      child.removeAllListeners();

      if (result instanceof Error) {
        reject(result);
      } else {
        resolve(result);
      }
    };

    const check = () => {
      try {
        const runtime = readRuntimeFile(dataDir);
        if (runtime !== undefined && isAlive(runtime.pid)) {
          finish(runtime);
        }
      } catch (error) {
        finish(error instanceof Error ? error : new Error(String(error)));
      }
    };

    watcher.on("change", (_event, filename) => {
      if (filename === RUNTIME_FILE) {
        check();
      }
    });
    watcher.on("error", finish);
    child.on("error", (error) => finish(new Error(`failed to run ${binary}`, { cause: error })));
    // Exit code 0 means another service won the race to start; its runtime file is on the way.
    child.on("exit", (code, signal) => {
      if (code !== 0) {
        const stderr = readFileSync(logPath, "utf8").trim();
        finish(new Error(`trove serve exited with ${code ?? signal}: ${stderr}`));
      }
    });

    check();
  });
}
