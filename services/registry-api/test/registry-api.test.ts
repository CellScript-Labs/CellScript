import { describe, expect, it } from "vitest";
import type { SignChallengeResponseData } from "@joyid/ckb";
import {
  AUTH_ACTION,
  AUTH_PROTOCOL,
  AUTH_REVOKE_CAPABILITY_ACTION,
  DEFAULT_REGISTRY_ORIGIN,
  PUBLISH_ACTION,
  PUBLISH_PROTOCOL,
  canonicalJson,
  capabilityKeyId,
  joyidPrincipalIdFromBinding,
  type CapabilityAuthorisationPayload,
  type CapabilityRevocationPayload,
  type PublishPayload,
} from "../src/domain";
import { MemoryRegistryStore, createApp, type SnapshotWriter } from "../src/index";

const now = new Date("2026-06-23T12:00:00Z");

function authPayload(principalId = "0x1111111111111111111111111111111111111111"): CapabilityAuthorisationPayload {
  return {
    protocol: AUTH_PROTOCOL,
    action: AUTH_ACTION,
    registry_origin: DEFAULT_REGISTRY_ORIGIN,
    principal_type: "joyid_ckb",
    principal_id: principalId,
    capability_pubkey: `p256-spki:${principalId.slice(2)}`,
    requested_scopes: ["publish:cellscript/demo"],
    capability_expires_at: "2026-09-21T12:00:00Z",
    nonce: "0x1111111111111111",
    issued_at: "2026-06-23T12:00:00Z",
    expires_at: "2026-06-23T12:10:00Z",
    cli_version: "cellc 0.20.0",
  };
}

function joyidSignature(
  payload: CapabilityAuthorisationPayload,
  challenge = canonicalJson(payload),
  pubkey = payload.principal_id.startsWith("0x") ? payload.principal_id.slice(2) : "pubkey",
): SignChallengeResponseData {
  return {
    challenge,
    signature: "sig",
    message: "message",
    pubkey,
    keyType: "main_key",
    alg: -7,
  };
}

function revokePayload(keyId: string, principalId = "0x1111111111111111111111111111111111111111"): CapabilityRevocationPayload {
  return {
    protocol: AUTH_PROTOCOL,
    action: AUTH_REVOKE_CAPABILITY_ACTION,
    registry_origin: DEFAULT_REGISTRY_ORIGIN,
    principal_type: "joyid_ckb",
    principal_id: principalId,
    capability_key_id: keyId,
    nonce: "0x3333333333333333",
    issued_at: "2026-06-23T12:00:00Z",
    expires_at: "2026-06-23T12:10:00Z",
    cli_version: "cellc 0.20.0",
  };
}

function joyidRevocationSignature(
  payload: CapabilityRevocationPayload,
  challenge = canonicalJson(payload),
  pubkey = payload.principal_id.startsWith("0x") ? payload.principal_id.slice(2) : "pubkey",
): SignChallengeResponseData {
  return {
    challenge,
    signature: "sig",
    message: "message",
    pubkey,
    keyType: "main_key",
    alg: -7,
  };
}

async function publishPayload(keyId: string): Promise<PublishPayload> {
  return {
    protocol: PUBLISH_PROTOCOL,
    action: PUBLISH_ACTION,
    registry_origin: DEFAULT_REGISTRY_ORIGIN,
    namespace: "cellscript",
    name: "demo",
    version: "1.2.3",
    source_hash: `0x${"ab".repeat(32)}`,
    manifest_hash: `0x${"cd".repeat(32)}`,
    capability_key_id: keyId,
    nonce: "0x2222222222222222",
    issued_at: "2026-06-23T12:00:00Z",
    expires_at: "2026-06-23T12:10:00Z",
    cli_version: "cellc 0.20.0",
    registry_entry: {
      namespace: "cellscript",
      name: "demo",
      version: "1.2.3",
      repository: "https://github.com/cellscript/demo",
    },
  };
}

function base64(value: string): string {
  return btoa(value);
}

function utf8(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

function testApp(store = new MemoryRegistryStore(), writer?: SnapshotWriter) {
  const snapshots: Array<{ key: string; body: Uint8Array; contentType: string }> = [];
  const snapshotWriter =
    writer ??
    ({
      async put(key, body, options) {
        snapshots.push({ key, body, contentType: options.contentType });
      },
    } satisfies SnapshotWriter);
  const app = createApp({
    store,
    now: () => now,
    joyidVerifier: { verifySignature: async () => true },
    capabilityVerifier: { verify: async () => true },
    snapshotWriter,
  });
  return { app, store, snapshots };
}

async function post(
  app: ReturnType<typeof createApp>,
  path: string,
  body: unknown,
  env: Record<string, unknown> = {},
  headers: Record<string, string> = {},
): Promise<Response> {
  return app.fetch(
    new Request(`https://api.registry.cellscript.dev${path}`, {
      method: "POST",
      headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.5", ...headers },
      body: JSON.stringify(body),
    }),
    { REGISTRY_ORIGIN: DEFAULT_REGISTRY_ORIGIN, ...env },
  );
}

async function get(
  app: ReturnType<typeof createApp>,
  path: string,
  env: Record<string, unknown> = {},
  headers: Record<string, string> = {},
): Promise<Response> {
  return app.fetch(
    new Request(`https://api.registry.cellscript.dev${path}`, {
      method: "GET",
      headers: { "cf-connecting-ip": "203.0.113.5", ...headers },
    }),
    { REGISTRY_ORIGIN: DEFAULT_REGISTRY_ORIGIN, ...env },
  );
}

describe("registry api", () => {
  it("reports readiness only when production bindings are configured", async () => {
    const app = createApp();
    const missing = await get(app, "/ready");
    expect(missing.status).toBe(503);
    expect(await missing.json()).toMatchObject({
      status: "not_ready",
      checks: {
        store: "missing_hyperdrive",
        object_store: "missing_r2",
        admin_token: "missing_secret",
      },
    });

    const ready = await get(app, "/ready", {
      HYPERDRIVE: {},
      REGISTRY_OBJECTS: {},
      REGISTRY_ADMIN_TOKEN: "secret",
    });
    expect(ready.status).toBe(200);
    expect(await ready.json()).toMatchObject({
      status: "ready",
      checks: {
        store: "configured",
        object_store: "configured",
        admin_token: "configured",
      },
    });
  });

  it("rejects JoyID signatures that do not bind the canonical capability payload", async () => {
    const { app } = testApp();
    const payload = authPayload();
    const response = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload, "different challenge"),
    });

    expect(response.status).toBe(401);
    const body = await response.json() as any;
    expect(body.error.code).toBe("joyid_challenge_mismatch");
  });

  it("rejects JoyID signatures whose signer does not match principal_id", async () => {
    const { app } = testApp();
    const payload = authPayload("0x1111111111111111111111111111111111111111");
    const response = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload, canonicalJson(payload), "2222222222222222222222222222222222222222"),
    });

    expect(response.status).toBe(401);
    const body = await response.json() as any;
    expect(body.error.code).toBe("joyid_principal_mismatch");
  });

  it("does not let invalid JoyID signatures consume principal quota", async () => {
    const store = new MemoryRegistryStore();
    const app = createApp({
      store,
      now: () => now,
      joyidVerifier: { verifySignature: async () => false },
      capabilityVerifier: { verify: async () => true },
      snapshotWriter: {
        async put() {},
      },
    });
    const payload = authPayload();
    const response = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });

    expect(response.status).toBe(401);
    expect((await response.json() as any).error.code).toBe("joyid_signature_invalid");
    expect(store.quotaEvents.some((event) => event.quotaKey === `principal:${payload.principal_type}:${payload.principal_id}`)).toBe(false);
  });

  it("accepts hashed JoyID principal bindings", async () => {
    const { app } = testApp();
    const pubkey = "33".repeat(32);
    const principalId = await joyidPrincipalIdFromBinding("main_key", pubkey);
    const payload = authPayload(principalId);
    const response = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload, canonicalJson(payload), pubkey),
    });

    expect(response.status).toBe(201);
    const body = await response.json() as any;
    expect(body.principal_id).toBe(principalId);
  });

  it("creates a capability, claims namespace, stores snapshot, and admits source_published publish", async () => {
    const { app, store, snapshots } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(capabilityResponse.status).toBe(201);
    const capability = await capabilityResponse.json() as any;
    expect(capability.key_id).toBe(await capabilityKeyId(payload.capability_pubkey));

    const claimResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "cellscript",
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(claimResponse.status).toBe(202);
    expect((await claimResponse.json() as any).status).toBe("review_pending");

    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const publishResponse = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });

    expect(publishResponse.status).toBe(202);
    const body = await publishResponse.json() as any;
    expect(body.status).toBe("source_published");
    expect(body.direct_url).toBe("https://registry.cellscript.dev/packages/cellscript/demo/versions/1.2.3.json");
    expect(snapshots).toHaveLength(2);
    const sourceSnapshot = snapshots.find((snapshot) => snapshot.key.startsWith("source-snapshots/"));
    const staticEntry = snapshots.find((snapshot) => snapshot.key === "packages/cellscript/demo/versions/1.2.3.json");
    expect(sourceSnapshot?.key).toContain("source-snapshots/cellscript/demo/1.2.3/");
    expect(staticEntry).toBeTruthy();
    const staticBody = JSON.parse(utf8(staticEntry!.body)) as any;
    expect(staticBody.kind).toBe("cellscript.registry.package_version");
    expect(staticBody.coordinate).toBe("cellscript/demo@1.2.3");
    expect(staticBody.status).toBe("source_published");
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.status).toBe("source_published");
    expect(store.capabilities.get(capability.key_id)?.last_used_at).toBeTruthy();
    expect(store.auditEvents.some((event) => event.event_type === "capability.used" && event.capability_key_id === capability.key_id)).toBe(true);
    expect(store.auditEvents.some((event) => event.event_type === "publish.accepted")).toBe(true);
  });

  it("serves package-version JSON from the static registry read path without the write store", async () => {
    const app = createApp({
      registryObjectReader: {
        async get(key) {
          expect(key).toBe("packages/cellscript/demo/versions/1.2.3.json");
          return {
            body: JSON.stringify({ schema_version: 1, coordinate: "cellscript/demo@1.2.3", status: "source_published" }),
            contentType: "application/json; charset=utf-8",
            etag: "\"static-entry\"",
          };
        },
      },
    });

    const response = await app.fetch(new Request("https://registry.cellscript.dev/packages/cellscript/demo/versions/1.2.3.json"));
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toContain("max-age=60");
    expect(response.headers.get("etag")).toBe("\"static-entry\"");
    expect((await response.json() as any).coordinate).toBe("cellscript/demo@1.2.3");
  });

  it("replays a successful publish response for the same Idempotency-Key without rewriting objects", async () => {
    const { app, store, snapshots } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const body = {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    };
    const first = await post(app, "/v1/packages/cellscript/demo/versions", body, {}, { "idempotency-key": "publish-key-0001" });
    expect(first.status).toBe(202);
    const firstBody = await first.json() as any;

    const replay = await post(app, "/v1/packages/cellscript/demo/versions", body, {}, { "idempotency-key": "publish-key-0001" });
    expect(replay.status).toBe(202);
    expect(replay.headers.get("x-idempotency-status")).toBe("replayed");
    const replayBody = await replay.json() as any;
    expect(replayBody.direct_url).toBe(firstBody.direct_url);
    expect(replayBody.snapshot_hash).toBe(firstBody.snapshot_hash);
    expect(snapshots).toHaveLength(2);
  });

  it("rejects conflicting publish payloads that reuse an Idempotency-Key", async () => {
    const { app, store } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const first = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    }, {}, { "idempotency-key": "publish-key-0002" });
    expect(first.status).toBe(202);

    const changed = {
      ...publish,
      version: "1.2.4",
      source_hash: `0x${"ef".repeat(32)}`,
      registry_entry: { ...publish.registry_entry, version: "1.2.4" },
    };
    const conflict = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: changed,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("changed source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "changed source snapshot".length,
        source_hash: changed.source_hash,
      },
    }, {}, { "idempotency-key": "publish-key-0002" });
    expect(conflict.status).toBe(409);
    expect((await conflict.json() as any).error.code).toBe("idempotency_key_conflict");
  });

  it("blocks publish nonce replay before another version can write source objects", async () => {
    const { app, store, snapshots } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const first = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });
    expect(first.status).toBe(202);

    const replayedNonce = {
      ...publish,
      version: "1.2.4",
      source_hash: `0x${"ef".repeat(32)}`,
      registry_entry: { ...publish.registry_entry, version: "1.2.4" },
    };
    const replay = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: replayedNonce,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("replayed nonce source"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "replayed nonce source".length,
        source_hash: replayedNonce.source_hash,
      },
    });
    expect(replay.status).toBe(409);
    expect((await replay.json() as any).error.code).toBe("nonce_replay");
    expect(snapshots).toHaveLength(2);
    expect(store.auditEvents.some((event) => event.event_type === "nonce.replay_blocked")).toBe(true);
  });

  it("releases publish idempotency reservation when static registry object write fails before admission", async () => {
    const store = new MemoryRegistryStore();
    const writes: Array<{ key: string; body: Uint8Array; contentType: string }> = [];
    let failStaticWrites = true;
    const app = createApp({
      store,
      now: () => now,
      joyidVerifier: { verifySignature: async () => true },
      capabilityVerifier: { verify: async () => true },
      snapshotWriter: {
        async put(key, body, options) {
          if (failStaticWrites && key.startsWith("packages/")) {
            throw new Error("static registry object write failed");
          }
          writes.push({ key, body, contentType: options.contentType });
        },
      } satisfies SnapshotWriter,
    });
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const sourceSnapshot = {
      content_base64: base64("source snapshot"),
      content_type: "application/vnd.cellscript.source+tar",
      size_bytes: "source snapshot".length,
      source_hash: publish.source_hash,
    };
    const idempotencyKey = "publish-key-static-fail";
    const response = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: sourceSnapshot,
    }, {}, { "idempotency-key": idempotencyKey });

    expect(response.status).toBe(500);
    expect((await response.json() as any).error.code).toBe("internal_error");
    expect(writes).toHaveLength(1);
    expect(writes[0]?.key).toContain("source-snapshots/cellscript/demo/1.2.3/");
    expect(store.snapshots.size).toBe(0);
    expect(store.packageVersions.has("cellscript/demo@1.2.3")).toBe(false);
    expect(store.idempotencyKeys.has(`publish:${idempotencyKey}`)).toBe(false);
    expect(store.capabilities.get(capability.key_id)?.last_used_at).toBeFalsy();
    expect(store.auditEvents.some((event) => event.event_type === "capability.used")).toBe(false);
    expect(store.auditEvents.some((event) => event.event_type === "publish.accepted")).toBe(false);

    failStaticWrites = false;
    const retryPublish = { ...publish, nonce: "0x4444444444444444" };
    const retry = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: retryPublish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: sourceSnapshot,
    }, {}, { "idempotency-key": idempotencyKey });

    expect(retry.status).toBe(202);
    expect((await retry.json() as any).status).toBe("source_published");
    expect(store.packageVersions.has("cellscript/demo@1.2.3")).toBe(true);
    expect(store.idempotencyKeys.get(`publish:${idempotencyKey}`)?.status).toBe("completed");
    expect(store.capabilities.get(capability.key_id)?.last_used_at).toBeTruthy();
    expect(store.auditEvents.some((event) => event.event_type === "capability.used")).toBe(true);
    expect(store.auditEvents.some((event) => event.event_type === "publish.accepted")).toBe(true);
  });

  it("allows audited admin review and quarantine transitions with an admin token", async () => {
    const { app, store, snapshots } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;

    const claimResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "cellscript",
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(claimResponse.status).toBe(202);
    expect((await claimResponse.json() as any).status).toBe("review_pending");

    const adminEnv = { REGISTRY_ADMIN_TOKEN: "secret" };
    const adminHeaders = { authorization: "Bearer secret", "x-registry-admin-actor": "ops@example.com" };
    const approveResponse = await post(
      app,
      "/v1/admin/namespaces/cellscript/status",
      { status: "active", review_reason: "approved core namespace" },
      adminEnv,
      adminHeaders,
    );
    expect(approveResponse.status).toBe(200);
    expect((await approveResponse.json() as any).status).toBe("active");

    const publish = await publishPayload(capability.key_id);
    const publishResponse = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });
    expect(publishResponse.status).toBe(202);

    const quarantineResponse = await post(
      app,
      "/v1/admin/packages/cellscript/demo/versions/1.2.3/status",
      { status: "quarantined", reason: "manual review" },
      adminEnv,
      adminHeaders,
    );
    expect(quarantineResponse.status).toBe(200);
    expect((await quarantineResponse.json() as any).status).toBe("quarantined");
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.status).toBe("quarantined");
    const staticEntryWrites = snapshots.filter((snapshot) => snapshot.key === "packages/cellscript/demo/versions/1.2.3.json");
    expect(staticEntryWrites).toHaveLength(2);
    expect(JSON.parse(utf8(staticEntryWrites.at(-1)!.body)).status).toBe("quarantined");
    expect(store.auditEvents.some((event) => event.event_type === "admin.namespace.status_updated")).toBe(true);
    expect(store.auditEvents.some((event) => event.event_type === "admin.package_version.status_updated")).toBe(true);
  });

  it("does not change DB package status when a suppressive static update fails", async () => {
    const store = new MemoryRegistryStore();
    const snapshots: Array<{ key: string; body: Uint8Array; contentType: string }> = [];
    let failStaticWrites = false;
    const app = createApp({
      store,
      now: () => now,
      joyidVerifier: { verifySignature: async () => true },
      capabilityVerifier: { verify: async () => true },
      snapshotWriter: {
        async put(key, body, options) {
          if (failStaticWrites && key.startsWith("packages/")) {
            throw new Error("static registry object write failed");
          }
          snapshots.push({ key, body, contentType: options.contentType });
        },
      } satisfies SnapshotWriter,
    });
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });
    const publish = await publishPayload(capability.key_id);
    const publishResponse = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });
    expect(publishResponse.status).toBe(202);

    failStaticWrites = true;
    const response = await post(
      app,
      "/v1/admin/packages/cellscript/demo/versions/1.2.3/status",
      { status: "quarantined", reason: "manual review" },
      { REGISTRY_ADMIN_TOKEN: "secret" },
      { authorization: "Bearer secret" },
    );

    expect(response.status).toBe(500);
    expect((await response.json() as any).error.code).toBe("internal_error");
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.status).toBe("source_published");
    expect(store.auditEvents.some((event) => event.event_type === "admin.package_version.status_updated")).toBe(false);
    const staticEntryWrites = snapshots.filter((snapshot) => snapshot.key === "packages/cellscript/demo/versions/1.2.3.json");
    expect(staticEntryWrites).toHaveLength(1);
    expect(JSON.parse(utf8(staticEntryWrites[0]!.body)).status).toBe("source_published");
  });

  it("rejects publish when the capability principal does not own the namespace", async () => {
    const { app, store } = testApp();
    const ownerPayload = authPayload("0x1111111111111111111111111111111111111111");
    const otherPayload = authPayload("0x2222222222222222222222222222222222222222");
    await post(app, "/v1/capabilities", {
      payload: otherPayload,
      joyid_signature: joyidSignature(otherPayload),
    });
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: ownerPayload.principal_id,
    });
    const keyId = await capabilityKeyId(otherPayload.capability_pubkey);
    const publish = await publishPayload(keyId);

    const response = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });

    expect(response.status).toBe(403);
    expect((await response.json() as any).error.code).toBe("namespace_owner_mismatch");
  });

  it("records auth failure audit events for invalid capability signatures", async () => {
    const store = new MemoryRegistryStore();
    const snapshots: Array<{ key: string; body: Uint8Array; contentType: string }> = [];
    const app = createApp({
      store,
      now: () => now,
      joyidVerifier: { verifySignature: async () => true },
      capabilityVerifier: { verify: async () => false },
      snapshotWriter: {
        async put(key, body, options) {
          snapshots.push({ key, body, contentType: options.contentType });
        },
      },
    });
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const publish = await publishPayload(capability.key_id);
    const response = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });

    expect(response.status).toBe(401);
    expect((await response.json() as any).error.code).toBe("capability_signature_invalid");
    expect(snapshots).toHaveLength(0);
    const event = store.auditEvents.find((entry) => entry.event_type === "auth.failure");
    expect(event?.data).toMatchObject({
      path: "/v1/packages/cellscript/demo/versions",
      status: 401,
      code: "capability_signature_invalid",
    });
  });

  it("rejects namespace claims by a different JoyID principal", async () => {
    const { app } = testApp();
    const first = {
      ...authPayload("0x1111111111111111111111111111111111111111"),
      requested_scopes: ["publish:alpha/demo"],
    };
    const second = {
      ...authPayload("0x2222222222222222222222222222222222222222"),
      requested_scopes: ["publish:alpha/demo"],
    };

    const firstResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "alpha",
      payload: first,
      joyid_signature: joyidSignature(first),
    });
    expect(firstResponse.status).toBe(201);

    const secondResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "alpha",
      payload: second,
      joyid_signature: joyidSignature(second),
    });
    expect(secondResponse.status).toBe(409);
    expect((await secondResponse.json() as any).error.code).toBe("namespace_already_claimed");
  });

  it("applies a cooldown between new namespace claims for the same JoyID principal", async () => {
    const { app, store } = testApp();
    const principalId = "0x1111111111111111111111111111111111111111";
    const first = {
      ...authPayload(principalId),
      requested_scopes: ["publish:alpha/demo"],
      nonce: "0xaaaaaaaaaaaaaaaa",
    };
    const second = {
      ...authPayload(principalId),
      requested_scopes: ["publish:bravo/demo"],
      nonce: "0xbbbbbbbbbbbbbbbb",
    };

    const firstResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "alpha",
      payload: first,
      joyid_signature: joyidSignature(first),
    });
    expect(firstResponse.status).toBe(201);

    const secondResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "bravo",
      payload: second,
      joyid_signature: joyidSignature(second),
    });
    expect(secondResponse.status).toBe(429);
    expect((await secondResponse.json() as any).error.code).toBe("namespace_claim_cooldown");
    expect(store.auditEvents.some((event) => event.event_type === "namespace_claim.cooldown_blocked")).toBe(true);
  });

  it("exposes token-gated audit events for registry operations", async () => {
    const { app } = testApp();
    const payload = {
      ...authPayload("0x1111111111111111111111111111111111111111"),
      requested_scopes: ["publish:alpha/demo"],
    };
    const claim = await post(app, "/v1/namespaces/claim", {
      namespace: "alpha",
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(claim.status).toBe(201);

    const unauthorized = await get(app, "/v1/admin/audit-events", { REGISTRY_ADMIN_TOKEN: "secret" });
    expect(unauthorized.status).toBe(401);
    expect((await unauthorized.json() as any).error.code).toBe("admin_unauthorized");

    const invalidLimit = await get(
      app,
      "/v1/admin/audit-events?limit=999",
      { REGISTRY_ADMIN_TOKEN: "secret" },
      { authorization: "Bearer secret" },
    );
    expect(invalidLimit.status).toBe(400);
    expect((await invalidLimit.json() as any).error.code).toBe("invalid_audit_limit");

    const response = await get(
      app,
      "/v1/admin/audit-events?event_type=namespace.claimed&limit=10",
      { REGISTRY_ADMIN_TOKEN: "secret" },
      { authorization: "Bearer secret" },
    );
    expect(response.status).toBe(200);
    const body = await response.json() as any;
    expect(body.events).toHaveLength(1);
    expect(body.events[0]).toMatchObject({
      event_type: "namespace.claimed",
      principal_type: "joyid_ckb",
      principal_id: payload.principal_id,
      namespace: "alpha",
    });
    expect(body.events[0].id).toBeTruthy();
    expect(body.events[0].created_at).toBeTruthy();
  });

  it("rate-limits capability creation by request IP before JoyID becomes the only spam control", async () => {
    const { app } = testApp();
    let response: Response | undefined;
    for (let i = 0; i < 121; i += 1) {
      const principalId = `0x${(i + 1).toString(16).padStart(40, "0")}`;
      const payload = authPayload(principalId);
      response = await post(app, "/v1/capabilities", {
        payload,
        joyid_signature: joyidSignature(payload),
      });
    }

    expect(response?.status).toBe(429);
    expect((await response!.json() as any).error.code).toBe("rate_limited");
  });

  it("runs scheduled cleanup for expired replay and quota state", async () => {
    const { app, store } = testApp();
    store.usedNonces.set("old-nonce", {
      protocol: "cellscript-registry-publish-v1",
      action: "publish",
      nonce: "0xaaaaaaaaaaaaaaaa",
      request_id: "old-request",
      expires_at: "2026-06-23T11:59:00Z",
      created_at: "2026-06-23T11:50:00Z",
    });
    store.usedNonces.set("live-nonce", {
      protocol: "cellscript-registry-publish-v1",
      action: "publish",
      nonce: "0xbbbbbbbbbbbbbbbb",
      request_id: "live-request",
      expires_at: "2026-06-23T12:01:00Z",
      created_at: "2026-06-23T11:50:00Z",
    });
    store.idempotencyKeys.set("old-key", {
      key: "old-key",
      request_hash: "old-hash",
      request_id: "old-request",
      status: "processing",
      expires_at: "2026-06-23T11:59:00Z",
      created_at: "2026-06-23T11:50:00Z",
      completed_at: null,
    });
    store.idempotencyKeys.set("live-key", {
      key: "live-key",
      request_hash: "live-hash",
      request_id: "live-request",
      status: "processing",
      expires_at: "2026-06-23T12:01:00Z",
      created_at: "2026-06-23T11:50:00Z",
      completed_at: null,
    });
    store.quotaEvents = [
      { quotaKey: "old-quota", bucket: "publish", at: "2026-06-21T11:59:00Z" },
      { quotaKey: "live-quota", bucket: "publish", at: "2026-06-21T12:01:00Z" },
    ];

    await app.scheduled(
      { scheduledTime: now.getTime(), cron: "*/15 * * * *" } as ScheduledController,
      { CLEANUP_QUOTA_EVENT_RETENTION_HOURS: "48" },
    );

    expect(store.usedNonces.has("old-nonce")).toBe(false);
    expect(store.usedNonces.has("live-nonce")).toBe(true);
    expect(store.idempotencyKeys.has("old-key")).toBe(false);
    expect(store.idempotencyKeys.has("live-key")).toBe(true);
    expect(store.quotaEvents).toEqual([{ quotaKey: "live-quota", bucket: "publish", at: "2026-06-21T12:01:00Z" }]);
    const event = store.auditEvents.find((entry) => entry.event_type === "maintenance.cleanup");
    expect(event?.data).toMatchObject({
      used_nonces_deleted: 1,
      idempotency_keys_deleted: 1,
      quota_events_deleted: 1,
    });
  });

  it("revokes a capability with JoyID and blocks later publish", async () => {
    const { app, store } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(capabilityResponse.status).toBe(201);
    const capability = await capabilityResponse.json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: payload.principal_id,
    });

    const revoke = revokePayload(capability.key_id);
    const revokeResponse = await post(app, `/v1/capabilities/${capability.key_id}/revoke`, {
      payload: revoke,
      joyid_signature: joyidRevocationSignature(revoke),
      reason: "rotated",
    });
    expect(revokeResponse.status).toBe(200);
    expect((await revokeResponse.json() as any).status).toBe("revoked");
    expect(store.capabilities.get(capability.key_id)?.revoked_at).toBeTruthy();

    const publish = await publishPayload(capability.key_id);
    const publishResponse = await post(app, "/v1/packages/cellscript/demo/versions", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source+tar",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });

    expect(publishResponse.status).toBe(401);
    expect((await publishResponse.json() as any).error.code).toBe("capability_revoked");
    expect(store.auditEvents.some((event) => event.event_type === "capability.revoked")).toBe(true);
  });

  it("does not allow a replayed capability creation to reactivate a revoked key", async () => {
    const { app, store } = testApp();
    const payload = authPayload();
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(capabilityResponse.status).toBe(201);
    const capability = await capabilityResponse.json() as any;

    const revoke = revokePayload(capability.key_id);
    const revokeResponse = await post(app, `/v1/capabilities/${capability.key_id}/revoke`, {
      payload: revoke,
      joyid_signature: joyidRevocationSignature(revoke),
      reason: "rotated",
    });
    expect(revokeResponse.status).toBe(200);

    const replayCreate = await post(app, "/v1/capabilities", {
      payload,
      joyid_signature: joyidSignature(payload),
    });
    expect(replayCreate.status).toBe(409);
    expect((await replayCreate.json() as any).error.code).toBe("nonce_replay");
    expect(store.capabilities.get(capability.key_id)?.revoked_at).toBeTruthy();
  });
});
