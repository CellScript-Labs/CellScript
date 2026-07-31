import { constants as fsConstants } from "node:fs";
import { access, mkdir, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, resolve, sep } from "node:path";
import { randomUUID } from "node:crypto";

import { createApp, type Env, type RegistryObjectRead, type RegistryObjectReader, type SnapshotWriter } from "./index";
import { sha256Hex } from "./domain";
import { SqlRegistryStore } from "./sql-store";

class FilesystemObjectStore implements SnapshotWriter, RegistryObjectReader {
  constructor(private readonly root: string) {}

  async put(key: string, body: Uint8Array, _options: { contentType: string; metadata: Record<string, string> }): Promise<void> {
    const path = this.pathFor(key);
    await mkdir(dirname(path), { recursive: true, mode: 0o750 });
    const temporary = `${path}.tmp-${randomUUID()}`;
    try {
      await writeFile(temporary, body, { mode: 0o640, flag: "wx" });
      await rename(temporary, path);
    } catch (error) {
      await unlink(temporary).catch(() => undefined);
      throw error;
    }
  }

  async get(key: string): Promise<RegistryObjectRead | null> {
    try {
      const body = await readFile(this.pathFor(key));
      return {
        body,
        contentType: contentTypeFor(key),
        etag: `"sha256-${await sha256Hex(body)}"`,
      };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
  }

  private pathFor(key: string): string {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9._/-]{0,1023}$/.test(key) || key.split("/").includes("..")) {
      throw new Error("registry object key is invalid");
    }
    const path = resolve(this.root, key);
    if (path !== this.root && !path.startsWith(`${this.root}${sep}`)) {
      throw new Error("registry object key escapes the configured root");
    }
    return path;
  }
}

const port = integerEnv("PORT", 8787, 1, 65_535);
const databaseUrl = requiredEnv("DATABASE_URL");
const objectRoot = resolve(requiredEnv("REGISTRY_OBJECTS_DIR"));
const adminToken = requiredEnv("REGISTRY_ADMIN_TOKEN");
const maxIncomingBodyBytes = integerEnv("MAX_INCOMING_BODY_BYTES", 7 * 1024 * 1024, 1_024, 64 * 1024 * 1024);

await mkdir(objectRoot, { recursive: true, mode: 0o750 });

const store = new SqlRegistryStore({ connectionString: databaseUrl });
const objectStore = new FilesystemObjectStore(objectRoot);
const env: Env = {
  REGISTRY_ADMIN_TOKEN: adminToken,
  REGISTRY_ORIGIN: process.env["REGISTRY_ORIGIN"] ?? "https://api.registry.cellscript.dev",
  STATIC_REGISTRY_ORIGIN: process.env["STATIC_REGISTRY_ORIGIN"] ?? "https://registry.cellscript.dev",
  ENVIRONMENT: process.env["ENVIRONMENT"] ?? "production",
  ...(process.env["MAX_JSON_BODY_BYTES"] ? { MAX_JSON_BODY_BYTES: process.env["MAX_JSON_BODY_BYTES"] } : {}),
  ...(process.env["MAX_SNAPSHOT_BYTES"] ? { MAX_SNAPSHOT_BYTES: process.env["MAX_SNAPSHOT_BYTES"] } : {}),
  ...(process.env["CLEANUP_QUOTA_EVENT_RETENTION_HOURS"]
    ? { CLEANUP_QUOTA_EVENT_RETENTION_HOURS: process.env["CLEANUP_QUOTA_EVENT_RETENTION_HOURS"] }
    : {}),
  ...(process.env["NAMESPACE_CLAIM_COOLDOWN_SECONDS"]
    ? { NAMESPACE_CLAIM_COOLDOWN_SECONDS: process.env["NAMESPACE_CLAIM_COOLDOWN_SECONDS"] }
    : {}),
};

const app = createApp({
  store,
  snapshotWriter: objectStore,
  registryObjectReader: objectStore,
  readinessCheck: async () => {
    await access(objectRoot, fsConstants.R_OK | fsConstants.W_OK);
    return { object_store: "ready", runtime: "ready" };
  },
});

const server = createServer(async (request, response) => {
  const startedAt = Date.now();
  const requestId = request.headers["x-request-id"]?.toString() ?? randomUUID();
  try {
    const protocol = firstHeader(request.headers["x-forwarded-proto"]) ?? "http";
    const host = firstHeader(request.headers.host) ?? `127.0.0.1:${port}`;
    const url = new URL(request.url ?? "/", `${protocol}://${host}`);
    const headers = new Headers();
    for (const [name, value] of Object.entries(request.headers)) {
      if (value === undefined) continue;
      if (Array.isArray(value)) {
        for (const item of value) headers.append(name, item);
      } else {
        headers.set(name, value);
      }
    }
    headers.set("x-request-id", requestId);
    const method = request.method ?? "GET";
    const body = method === "GET" || method === "HEAD" ? undefined : await readIncomingBody(request, maxIncomingBodyBytes);
    const requestInit: RequestInit = { method, headers };
    if (body) requestInit.body = body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength) as ArrayBuffer;
    const registryResponse = await app.fetch(new Request(url, requestInit), env);
    response.statusCode = registryResponse.status;
    registryResponse.headers.forEach((value, name) => response.setHeader(name, value));
    if (method === "HEAD" || !registryResponse.body) {
      response.end();
    } else {
      response.end(Buffer.from(await registryResponse.arrayBuffer()));
    }
    log("request.completed", {
      request_id: requestId,
      method,
      path: url.pathname,
      status: registryResponse.status,
      duration_ms: Date.now() - startedAt,
    });
  } catch (error) {
    const tooLarge = error instanceof IncomingBodyTooLargeError;
    log("request.failed", {
      request_id: requestId,
      error: error instanceof Error ? error.message : "unknown error",
      duration_ms: Date.now() - startedAt,
    });
    if (!response.headersSent) {
      response.statusCode = tooLarge ? 413 : 500;
      response.setHeader("content-type", "application/json; charset=utf-8");
      response.setHeader("x-content-type-options", "nosniff");
    }
    response.end(JSON.stringify({
      request_id: requestId,
      error: {
        code: tooLarge ? "request_body_too_large" : "node_adapter_error",
        message: tooLarge ? "request body exceeds the configured limit" : "internal error",
      },
    }));
  }
});

server.requestTimeout = 30_000;
server.headersTimeout = 15_000;
server.keepAliveTimeout = 5_000;
server.listen(port, "0.0.0.0", () => log("server.started", { port, object_root: objectRoot }));

const maintenanceInterval = setInterval(() => {
  app.scheduled({} as ScheduledController, env).catch((error) => {
    log("maintenance.failed", { error: error instanceof Error ? error.message : "unknown error" });
  });
}, 15 * 60 * 1000);
maintenanceInterval.unref();

for (const signal of ["SIGTERM", "SIGINT"] as const) {
  process.on(signal, () => {
    clearInterval(maintenanceInterval);
    log("server.stopping", { signal });
    server.close((error) => {
      if (error) {
        log("server.stop_failed", { error: error.message });
        process.exitCode = 1;
      }
    });
  });
}

class IncomingBodyTooLargeError extends Error {}

async function readIncomingBody(request: import("node:http").IncomingMessage, maximumBytes: number): Promise<Uint8Array> {
  const chunks: Buffer[] = [];
  let receivedBytes = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    receivedBytes += buffer.byteLength;
    if (receivedBytes > maximumBytes) throw new IncomingBodyTooLargeError();
    chunks.push(buffer);
  }
  return Buffer.concat(chunks);
}

function firstHeader(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}

function contentTypeFor(key: string): string {
  if (key.endsWith(".json")) return "application/json; charset=utf-8";
  if (key.endsWith(".tar.gz")) return "application/gzip";
  if (key.endsWith(".tar")) return "application/x-tar";
  return "application/octet-stream";
}

function requiredEnv(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function integerEnv(name: string, fallback: number, minimum: number, maximum: number): number {
  const raw = process.env[name];
  if (!raw) return fallback;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer between ${minimum} and ${maximum}`);
  }
  return value;
}

function log(event: string, data: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify({ timestamp: new Date().toISOString(), event, ...data })}\n`);
}
