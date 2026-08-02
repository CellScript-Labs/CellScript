import { constants as fsConstants } from "node:fs";
import { access, mkdir, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { randomUUID } from "node:crypto";

import { createApp, type Env } from "./index";
import { FilesystemObjectStore } from "./filesystem-object-store";
import { SqlRegistryStore } from "./sql-store";

const port = integerEnv("PORT", 8787, 1, 65_535);
const databaseUrl = requiredEnv("DATABASE_URL");
const objectRoot = resolve(requiredEnv("REGISTRY_OBJECTS_DIR"));
const adminToken = requiredEnv("REGISTRY_ADMIN_TOKEN");
const maxIncomingBodyBytes = integerEnv("MAX_INCOMING_BODY_BYTES", 7 * 1024 * 1024, 1_024, 64 * 1024 * 1024);
const requireVerifierReady = process.env["REQUIRE_REGISTRY_VERIFIER_READY"] === "true";
const verifierHeartbeatPath = resolve(process.env["REGISTRY_VERIFIER_SHARED_HEARTBEAT"] ?? `${objectRoot}/.health/verifier-ready`);
const verifierHeartbeatMaxAgeSeconds = integerEnv("REGISTRY_VERIFIER_HEARTBEAT_MAX_AGE_SECONDS", 120, 30, 600);

await mkdir(objectRoot, { recursive: true, mode: 0o750 });
const managedObjectPrefixes = ["source-snapshots", "artifacts"].map((prefix) => resolve(objectRoot, prefix));
for (const prefix of managedObjectPrefixes) {
  await mkdir(prefix, { recursive: true, mode: 0o750 });
  await access(prefix, fsConstants.R_OK | fsConstants.W_OK);
}

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
  ...(process.env["CKB_MAINNET_RPC_URL"] ? { CKB_MAINNET_RPC_URL: process.env["CKB_MAINNET_RPC_URL"] } : {}),
  ...(process.env["REGISTRY_TYPE_SCRIPT_JSON"]
    ? { REGISTRY_TYPE_SCRIPT_JSON: process.env["REGISTRY_TYPE_SCRIPT_JSON"] }
    : {}),
  ...(process.env["REGISTRY_TYPE_SCRIPT_CELL_DEP_JSON"]
    ? { REGISTRY_TYPE_SCRIPT_CELL_DEP_JSON: process.env["REGISTRY_TYPE_SCRIPT_CELL_DEP_JSON"] }
    : {}),
  ...(process.env["REGISTRY_COMMITMENT_LOCK_SCRIPT_JSON"]
    ? { REGISTRY_COMMITMENT_LOCK_SCRIPT_JSON: process.env["REGISTRY_COMMITMENT_LOCK_SCRIPT_JSON"] }
    : {}),
  ...(process.env["REGISTRY_COMMITMENT_LOCK_CELL_DEP_JSON"]
    ? { REGISTRY_COMMITMENT_LOCK_CELL_DEP_JSON: process.env["REGISTRY_COMMITMENT_LOCK_CELL_DEP_JSON"] }
    : {}),
  ...(process.env["REGISTRY_REPRODUCER_POLICY_JSON"]
    ? { REGISTRY_REPRODUCER_POLICY_JSON: process.env["REGISTRY_REPRODUCER_POLICY_JSON"] }
    : {}),
  ...(process.env["CKB_REGISTRY_SCAN_MAX_CELLS"]
    ? { CKB_REGISTRY_SCAN_MAX_CELLS: process.env["CKB_REGISTRY_SCAN_MAX_CELLS"] }
    : {}),
  ...(process.env["CKB_MIN_CONFIRMATIONS"]
    ? { CKB_MIN_CONFIRMATIONS: process.env["CKB_MIN_CONFIRMATIONS"] }
    : {}),
};

const app = createApp({
  store,
  snapshotWriter: objectStore,
  registryObjectReader: objectStore,
  readinessCheck: async () => {
    await access(objectRoot, fsConstants.R_OK | fsConstants.W_OK);
    for (const prefix of managedObjectPrefixes) {
      await access(prefix, fsConstants.R_OK | fsConstants.W_OK);
    }
    const checks: Record<string, string> = { object_store: "ready", runtime: "ready" };
    if (requireVerifierReady) {
      const heartbeat = await stat(verifierHeartbeatPath);
      if (!heartbeat.isFile() || Date.now() - heartbeat.mtimeMs > verifierHeartbeatMaxAgeSeconds * 1_000) {
        throw new Error("registry verifier heartbeat is stale");
      }
      checks["verifier"] = "ready";
    }
    return checks;
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

let maintenanceRunning = false;
const runMaintenance = () => {
  if (maintenanceRunning) {
    log("maintenance.skipped", { reason: "previous_run_active" });
    return;
  }
  maintenanceRunning = true;
  app.scheduled({} as ScheduledController, env)
    .catch((error) => {
      log("maintenance.failed", { error: error instanceof Error ? error.message : "unknown error" });
    })
    .finally(() => {
      maintenanceRunning = false;
    });
};
void runMaintenance();
const maintenanceInterval = setInterval(runMaintenance, 15 * 60 * 1000);
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
