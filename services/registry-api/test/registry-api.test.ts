import { describe, expect, it } from "vitest";
import type { SignChallengeResponseData } from "@joyid/ckb";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { blake2b } from "@noble/hashes/blake2.js";
import {
  AUTH_ACTION,
  AUTH_PROTOCOL,
  AUTH_REVOKE_CAPABILITY_ACTION,
  DEPLOYMENT_ACTION,
  DEPLOYMENT_PROTOCOL,
  DEFAULT_REGISTRY_ORIGIN,
  PUBLISH_ACTION,
  PUBLISH_PROTOCOL,
  canonicalJson,
  capabilityKeyId,
  ckbBlake2bHex,
  ckbScriptHash,
  ckbSecp256k1PrincipalIdFromPublicKey,
  joyidPrincipalIdFromBinding,
  validatePublishPayload,
  type CapabilityAuthorisationPayload,
  type CapabilityRevocationPayload,
  type CkbSecp256k1Signature,
  type DeploymentPayload,
  type PublishPayload,
} from "../src/domain";
import { MemoryRegistryStore, createApp, parseDepGroupOutPoints, type AppDeps, type SnapshotWriter } from "../src/index";

const now = new Date("2026-06-23T12:00:00Z");
const ckbPrivateKey = Uint8Array.from({ length: 32 }, (_, index) => index === 31 ? 7 : 0);

function bytesHex(value: Uint8Array): string {
  return `0x${[...value].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

function depGroupData(outPoints: Array<{ tx_hash_byte: number; index: number }>): string {
  const bytes = new Uint8Array(4 + outPoints.length * 36);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, outPoints.length, true);
  outPoints.forEach((outPoint, item) => {
    const offset = 4 + item * 36;
    bytes.fill(outPoint.tx_hash_byte, offset, offset + 32);
    view.setUint32(offset + 32, outPoint.index, true);
  });
  return bytesHex(bytes);
}

describe("DepGroup decoding", () => {
  it("decodes canonical Molecule OutPointVec data", () => {
    expect(parseDepGroupOutPoints(depGroupData([
      { tx_hash_byte: 0x11, index: 3 },
      { tx_hash_byte: 0xab, index: 0xffff_fffe },
    ]))).toEqual([
      { tx_hash: `0x${"11".repeat(32)}`, index: 3 },
      { tx_hash: `0x${"ab".repeat(32)}`, index: 0xffff_fffe },
    ]);
  });

  it("rejects empty and non-canonical DepGroup data", () => {
    expect(() => parseDepGroupOutPoints("0x00000000")).toThrow(/canonical non-empty/);
    expect(() => parseDepGroupOutPoints("0x01000000aa")).toThrow(/canonical non-empty/);
  });
});

async function ckbAuthPayload(): Promise<CapabilityAuthorisationPayload> {
  const publicKey = bytesHex(secp256k1.getPublicKey(ckbPrivateKey, true));
  return {
    ...authPayload(),
    principal_type: "ckb_secp256k1",
    principal_id: await ckbSecp256k1PrincipalIdFromPublicKey(publicKey),
  };
}

function ckbWalletSignature(
  payload: CapabilityAuthorisationPayload | CapabilityRevocationPayload,
): CkbSecp256k1Signature {
  const challenge = canonicalJson(payload);
  const message = new TextEncoder().encode(`Nervos Message:${challenge}`);
  const messageHash = blake2b(message, {
    dkLen: 32,
    personalization: new TextEncoder().encode("ckb-default-hash"),
  });
  const recovered = secp256k1.sign(messageHash, ckbPrivateKey, { format: "recovered", prehash: false });
  const ckbSignature = new Uint8Array(65);
  ckbSignature.set(recovered.subarray(1), 0);
  ckbSignature[64] = recovered[0] ?? 0;
  return {
    scheme: "ckb_secp256k1",
    challenge,
    signature: bytesHex(ckbSignature),
    public_key: bytesHex(secp256k1.getPublicKey(ckbPrivateKey, true)),
  };
}

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
    cli_version: "cellc 0.23.0",
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
    cli_version: "cellc 0.23.0",
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
    cli_version: "cellc 0.23.0",
    artifact: {
      kind: "source_library",
      profile: "cellscript_source",
      consumption_mode: "dependency",
      language: "cellscript",
    },
    registry_entry: {
      schema_version: 1,
      namespace: "cellscript",
      name: "demo",
      artifact: {
        kind: "source_library",
        profile: "cellscript_source",
        consumption_mode: "dependency",
        language: "cellscript",
      },
      repository: "https://github.com/cellscript/demo",
      versions: [{
        version: "1.2.3",
        tag: "v1.2.3",
        source_hash: `0x${"ab".repeat(32)}`,
        cellscript_version: "0.23.0",
        edition: "2026",
        compatibility_profile_hash: "ef".repeat(32),
        dependencies: {},
        verification_status: "pending",
        deployment_status: "not_applicable",
        availability_status: "active",
      }],
    },
  };
}

async function ckbExecutablePublishPayload(keyId: string): Promise<PublishPayload> {
  const payload = await publishPayload(keyId);
  payload.artifact = {
    kind: "deployable_contract",
    profile: "ckb_executable",
    consumption_mode: "deployment",
    language: "rust",
  };
  payload.registry_entry.artifact = payload.artifact;
  const release = payload.registry_entry.versions[0];
  delete release.cellscript_version;
  delete release.edition;
  delete release.compatibility_profile_hash;
  delete release.dependencies;
  release.artifact_hash = `0x${"31".repeat(32)}`;
  release.abi_hash = `0x${"32".repeat(32)}`;
  release.profile_contract = {
    schema: "cellscript-registry-profile-contract-v1",
    artifact_kind: "deployable_contract",
    profile: "ckb_executable",
    build: {
      target: "riscv64imac-unknown-none-elf",
      toolchain: "rustc 1.97.1",
      profile: "release",
      source_revision: "0123456789abcdef",
      reproducible: false,
    },
    security: { status: "review_required" },
    ckb: {
      vm_version: "2",
      script_role: "type",
      hash_type: "data1",
      dep_type: "code",
      abi_hash: release.abi_hash,
    },
  };
  payload.manifest_hash = ckbBlake2bHex(canonicalJson(release.profile_contract));
  release.deployment_status = "undeployed";
  return payload;
}

describe("generic artifact profile contracts", () => {
  it("requires a typed profile contract for non-CellScript releases", async () => {
    const payload = await ckbExecutablePublishPayload("cap_test");
    delete payload.registry_entry.versions[0].profile_contract;
    expect(() => validatePublishPayload(payload, DEFAULT_REGISTRY_ORIGIN, now)).toThrow(/profile_contract/);
  });

  it("rejects contract hashes that do not bind the immutable ABI identity", async () => {
    const payload = await ckbExecutablePublishPayload("cap_test");
    const contract = payload.registry_entry.versions[0].profile_contract!;
    (contract["ckb"] as Record<string, unknown>)["abi_hash"] = `0x${"99".repeat(32)}`;
    payload.manifest_hash = ckbBlake2bHex(canonicalJson(contract));
    expect(() => validatePublishPayload(payload, DEFAULT_REGISTRY_ORIGIN, now)).toThrow(/abi_hash.*does not match/);
  });

  it("rejects unknown profile contract fields", async () => {
    const payload = await ckbExecutablePublishPayload("cap_test");
    const contract = payload.registry_entry.versions[0].profile_contract!;
    contract["trust_me"] = true;
    payload.manifest_hash = ckbBlake2bHex(canonicalJson(contract));
    expect(() => validatePublishPayload(payload, DEFAULT_REGISTRY_ORIGIN, now)).toThrow(/trust_me is not recognised/);
  });

  it("allows a deployed CKB executable to bind a reproducible build recipe", async () => {
    const payload = await ckbExecutablePublishPayload("cap_test");
    const release = payload.registry_entry.versions[0];
    const contract = release.profile_contract!;
    const recipeHash = `0x${"34".repeat(32)}`;
    (contract["build"] as Record<string, unknown>)["reproducible"] = true;
    contract["reproduction"] = {
      environment: "docker.io/library/rust:1.97.1@sha256:0123456789abcdef",
      command: "cargo build --locked --release",
      recipe_hash: recipeHash,
      expected_artifact_hash: release.artifact_hash,
    };
    release.build_recipe_hash = recipeHash;
    payload.manifest_hash = ckbBlake2bHex(canonicalJson(contract));

    expect(validatePublishPayload(payload, DEFAULT_REGISTRY_ORIGIN, now).artifact.profile).toBe("ckb_executable");
  });
});

function deploymentPayload(keyId: string): DeploymentPayload {
  return {
    protocol: DEPLOYMENT_PROTOCOL,
    action: DEPLOYMENT_ACTION,
    registry_origin: DEFAULT_REGISTRY_ORIGIN,
    namespace: "cellscript",
    name: "demo",
    release: "1.2.3",
    network: "mainnet",
    artifact_hash: `0x${"31".repeat(32)}`,
    data_hash: `0x${"31".repeat(32)}`,
    code_hash: `0x${"31".repeat(32)}`,
    hash_type: "data1",
    dep_type: "code",
    out_point: { tx_hash: `0x${"41".repeat(32)}`, index: 0 },
    capability_key_id: keyId,
    nonce: "0x4444444444444444",
    issued_at: "2026-06-23T12:00:00Z",
    expires_at: "2026-06-23T12:10:00Z",
    cli_version: "cellc 0.23.0",
  };
}

function base64(value: string): string {
  return btoa(value);
}

function utf8(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

function testApp(store = new MemoryRegistryStore(), writer?: SnapshotWriter, deps: Partial<AppDeps> = {}) {
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
    ...deps,
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
  it("matches the canonical CKB Molecule Script hash", () => {
    expect(ckbScriptHash({
      code_hash: `0x${"11".repeat(32)}`,
      hash_type: "type",
      args: "0x1234",
    })).toBe("0x6106e30cbb34d68302798abf8259e5a6e0adbbd73c7f3dfe1c96ada1f6c00cee");
  });

  it("treats edition and compatibility profile as independent registry axes", async () => {
    const first = await publishPayload("profile-axis-test");
    const second = structuredClone(first);
    second.registry_entry.versions[0].compatibility_profile_hash = "12".repeat(32);

    const firstValidated = validatePublishPayload(first, DEFAULT_REGISTRY_ORIGIN, now);
    const secondValidated = validatePublishPayload(second, DEFAULT_REGISTRY_ORIGIN, now);

    expect(firstValidated.registry_entry.versions[0].edition).toBe("2026");
    expect(secondValidated.registry_entry.versions[0].edition).toBe("2026");
    expect(secondValidated.registry_entry.versions[0].compatibility_profile_hash)
      .not.toBe(firstValidated.registry_entry.versions[0].compatibility_profile_hash);
  });

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

    const readyApp = createApp({
      store: new MemoryRegistryStore(),
      snapshotWriter: { async put() {} },
      registryObjectReader: { async get() { return null; } },
      readinessCheck: async () => ({ runtime: "ready" }),
    });
    const ready = await get(readyApp, "/ready", { REGISTRY_ADMIN_TOKEN: "secret" });
    expect(ready.status).toBe(200);
    expect(ready.headers.get("content-security-policy"))
      .toBe("default-src 'none'; base-uri 'none'; frame-ancestors 'none'");
    expect(ready.headers.get("permissions-policy")).toBe("camera=(), geolocation=(), microphone=()");
    expect(ready.headers.get("strict-transport-security")).toBe("max-age=31536000");
    expect(ready.headers.get("x-frame-options")).toBe("DENY");
    expect(ready.headers.get("x-permitted-cross-domain-policies")).toBe("none");
    expect(await ready.json()).toMatchObject({
      status: "ready",
      checks: {
        store: "ready",
        object_store: "configured",
        admin_token: "configured",
        runtime: "ready",
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

  it("accepts a capability authorised by a standard CKB secp256k1 wallet", async () => {
    const { app } = testApp();
    const payload = await ckbAuthPayload();
    const response = await post(app, "/v1/capabilities", {
      payload,
      wallet_signature: ckbWalletSignature(payload),
    });

    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({
      principal_type: "ckb_secp256k1",
      principal_id: payload.principal_id,
      status: "active",
    });
  });

  it("lets a standard CKB wallet claim a namespace and revoke its capability", async () => {
    const { app, store } = testApp();
    const payload = await ckbAuthPayload();
    payload.requested_scopes = ["publish:walletdemo/demo"];
    const capabilityResponse = await post(app, "/v1/capabilities", {
      payload,
      wallet_signature: ckbWalletSignature(payload),
    });
    expect(capabilityResponse.status).toBe(201);
    const capability = await capabilityResponse.json() as any;

    const claimResponse = await post(app, "/v1/namespaces/claim", {
      namespace: "walletdemo",
      payload,
      wallet_signature: ckbWalletSignature(payload),
    });
    expect(claimResponse.status).toBe(201);
    expect(await claimResponse.json()).toMatchObject({
      namespace: "walletdemo",
      status: "active",
    });
    expect(store.namespaces.get("walletdemo")).toMatchObject({
      owner_principal_type: "ckb_secp256k1",
      owner_principal_id: payload.principal_id,
    });

    const revoke: CapabilityRevocationPayload = {
      ...revokePayload(capability.key_id, payload.principal_id),
      principal_type: "ckb_secp256k1",
    };
    const revokeResponse = await post(app, `/v1/capabilities/${capability.key_id}/revoke`, {
      payload: revoke,
      wallet_signature: ckbWalletSignature(revoke),
      reason: "rotated",
    });
    expect(revokeResponse.status).toBe(200);
    expect((await revokeResponse.json() as any).status).toBe("revoked");
    expect(store.capabilities.get(capability.key_id)?.revoked_at).toBeTruthy();
  });

  it("rejects a CKB wallet signature whose public key is not the payload principal", async () => {
    const { app } = testApp();
    const payload = await ckbAuthPayload();
    payload.principal_id = `0x${"44".repeat(32)}`;
    const response = await post(app, "/v1/capabilities", {
      payload,
      wallet_signature: ckbWalletSignature(payload),
    });

    expect(response.status).toBe(401);
    expect((await response.json() as any).error.code).toBe("ckb_principal_mismatch");
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
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
    expect(body).toMatchObject({
      verification_status: "pending",
      deployment_status: "not_applicable",
      availability_status: "active",
    });
    expect(body.direct_url).toBe("https://registry.cellscript.dev/artifacts/cellscript/demo/releases/1.2.3.json");
    expect(snapshots).toHaveLength(2);
    const sourceSnapshot = snapshots.find((snapshot) => snapshot.key.startsWith("source-snapshots/"));
    const staticEntry = snapshots.find((snapshot) => snapshot.key === "artifacts/cellscript/demo/releases/1.2.3.json");
    expect(sourceSnapshot?.key).toContain("source-snapshots/cellscript/demo/1.2.3/");
    expect(staticEntry).toBeTruthy();
    const staticBody = JSON.parse(utf8(staticEntry!.body)) as any;
    expect(staticBody.kind).toBe("cellscript.registry.artifact_release");
    expect(staticBody.schema_version).toBe(1);
    expect(staticBody.coordinate).toBe("cellscript/demo@1.2.3");
    expect(staticBody).toMatchObject({
      verification_status: "pending",
      deployment_status: "not_applicable",
      availability_status: "active",
    });
    expect(staticBody.edition).toBe("2026");
    expect(staticBody.compatibility_profile_hash).toBe("ef".repeat(32));
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.status).toBe("source_published");
    expect(store.capabilities.get(capability.key_id)?.last_used_at).toBeTruthy();
    expect(store.auditEvents.some((event) => event.event_type === "capability.used" && event.capability_key_id === capability.key_id)).toBe(true);
    expect(store.auditEvents.some((event) => event.event_type === "publish.accepted")).toBe(true);
    expect([...store.verificationJobs.values()]).toHaveLength(1);
    expect([...store.verificationJobs.values()][0]?.status).toBe("queued");
  });

  it("leases verification jobs once, dead-letters terminal failures, and resumes static sync without rebuilding", async () => {
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
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("source snapshot"),
        content_type: "application/vnd.cellscript.source-snapshot+json",
        size_bytes: "source snapshot".length,
        source_hash: publish.source_hash,
      },
    });
    expect(publishResponse.status).toBe(202);

    const claimTime = new Date().toISOString();
    const first = await store.claimVerificationJob({ worker_id: "worker-a", lease_seconds: 300, now_iso: claimTime });
    expect(first).toMatchObject({ status: "running", attempt_count: 1, lease_owner: "worker-a" });
    expect(await store.claimVerificationJob({ worker_id: "worker-b", lease_seconds: 300, now_iso: claimTime })).toBeNull();

    const dead = await store.failVerificationJob({
      job_id: first!.id,
      worker_id: "worker-a",
      error_code: "compile_rejected",
      error_message: "package does not compile",
      retryable: false,
      retry_after_seconds: 5,
      request_id: "verification:test:1",
    });
    expect(dead.status).toBe("dead_letter");

    const adminEnv = { REGISTRY_ADMIN_TOKEN: "secret" };
    const adminHeaders = { authorization: "Bearer secret", "x-registry-admin-actor": "release-bot" };
    const queue = await get(app, "/v1/admin/verification-queue", adminEnv, adminHeaders);
    expect(queue.status).toBe(200);
    expect(await queue.json()).toMatchObject({ counts: { dead_letter: 1, running: 0 } });
    const retry = await post(app, `/v1/admin/verification-jobs/${first!.id}/retry`, {}, adminEnv, adminHeaders);
    expect(retry.status).toBe(200);
    expect((await retry.json() as any).job.status).toBe("queued");

    const second = await store.claimVerificationJob({
      worker_id: "worker-b",
      lease_seconds: 300,
      now_iso: new Date(Date.now() + 1_000).toISOString(),
    });
    expect(second).toMatchObject({ status: "running", attempt_count: 1, lease_owner: "worker-b" });
    const evidence = {
      schema: "cellscript-registry-evidence-v1",
      kind: "verified_build",
      producer: "test-verifier",
      generated_at: new Date().toISOString(),
      verification_status: "passed",
      verification_level: "compiled",
      source_hash: publish.source_hash,
      manifest_hash: publish.manifest_hash,
      compatibility_profile_hash: publish.registry_entry.versions[0].compatibility_profile_hash,
      artifact_hash: `0x${"31".repeat(32)}`,
      metadata_hash: `0x${"32".repeat(32)}`,
      compiler_version: "0.23.0",
    };
    const promoted = await store.promoteVerifiedBuildForJob({
      job_id: second!.id,
      worker_id: "worker-b",
      evidence_hash: `sha256:${"11".repeat(32)}`,
      evidence,
      request_id: "verification:test:2",
      admin_actor: "verification-worker:test",
    });
    expect(promoted.job.status).toBe("publishing");
    expect(promoted.version.status).toBe("verified_build");

    const staticRetry = await store.failVerificationJob({
      job_id: second!.id,
      worker_id: "worker-b",
      error_code: "static_sync_failed",
      error_message: "object store unavailable",
      retryable: true,
      retry_after_seconds: 5,
      request_id: "verification:test:2",
    });
    expect(staticRetry).toMatchObject({ status: "retry_wait", attempt_count: 1, evidence_hash: `sha256:${"11".repeat(32)}` });
    const resumed = await store.claimVerificationJob({
      worker_id: "worker-c",
      lease_seconds: 300,
      now_iso: new Date(Date.now() + 10_000).toISOString(),
    });
    expect(resumed).toMatchObject({ status: "publishing", attempt_count: 2, lease_owner: "worker-c" });
    const completed = await store.completeVerificationJob({ job_id: resumed!.id, worker_id: "worker-c" });
    expect(completed.status).toBe("succeeded");
    expect((await store.getVerificationQueueMetrics()).counts.succeeded).toBe(1);
  });

  it("serves package-version JSON from the static registry read path without the write store", async () => {
    const app = createApp({
      registryObjectReader: {
        async get(key) {
          expect(key).toBe("artifacts/cellscript/demo/releases/1.2.3.json");
          return {
            body: JSON.stringify({ schema_version: 1, coordinate: "cellscript/demo@1.2.3", status: "source_published" }),
            contentType: "application/json; charset=utf-8",
            etag: "\"static-entry\"",
          };
        },
      },
    });

    const response = await app.fetch(new Request("https://registry.cellscript.dev/artifacts/cellscript/demo/releases/1.2.3.json"));
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toContain("max-age=60");
    expect(response.headers.get("etag")).toBe("\"static-entry\"");
    expect((await response.json() as any).coordinate).toBe("cellscript/demo@1.2.3");
  });

  it("rejects unknown schemas, incomplete entries, and mismatched nested identities", async () => {
    const { app } = testApp();
    const publish = await publishPayload("cap_11111111111111111111111111111111");
    const sourceSnapshot = {
      content_base64: base64("source snapshot"),
      content_type: "application/vnd.cellscript.source+tar",
      size_bytes: "source snapshot".length,
      source_hash: publish.source_hash,
    };
    const submit = (payload: unknown) =>
      post(app, "/v1/artifacts/cellscript/demo/releases", {
        payload,
        capability_signature: { algorithm: "p256-sha256", signature: "sig" },
        source_snapshot: sourceSnapshot,
      });

    const unknownSchema = await submit({
      ...publish,
      registry_entry: { ...publish.registry_entry, schema_version: 2 },
    });
    expect(unknownSchema.status).toBe(400);
    expect((await unknownSchema.json() as any).error.code).toBe("unsupported_registry_schema");

    for (const [field, expectedCode] of [
      ["dependencies", "invalid_registry_dependencies"],
      ["verification_status", "invalid_initial_artifact_state"],
      ["availability_status", "invalid_initial_artifact_state"],
    ] as const) {
      const incompleteVersion = { ...publish.registry_entry.versions[0] } as Record<string, unknown>;
      delete incompleteVersion[field];
      const incomplete = await submit({
        ...publish,
        registry_entry: { ...publish.registry_entry, versions: [incompleteVersion] },
      });
      expect(incomplete.status).toBe(400);
      expect((await incomplete.json() as any).error.code).toBe(expectedCode);
    }

    const wrongVersion = await submit({
      ...publish,
      registry_entry: {
        ...publish.registry_entry,
        versions: [{ ...publish.registry_entry.versions[0], version: "1.2.4", tag: "v1.2.4" }],
      },
    });
    expect(wrongVersion.status).toBe(400);
    expect((await wrongVersion.json() as any).error.code).toBe("registry_identity_mismatch");
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
    const first = await post(app, "/v1/artifacts/cellscript/demo/releases", body, {}, { "idempotency-key": "publish-key-0001" });
    expect(first.status).toBe(202);
    const firstBody = await first.json() as any;

    const replay = await post(app, "/v1/artifacts/cellscript/demo/releases", body, {}, { "idempotency-key": "publish-key-0001" });
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
    const first = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
      registry_entry: {
        ...publish.registry_entry,
        versions: [{
          ...publish.registry_entry.versions[0],
          version: "1.2.4",
          tag: "v1.2.4",
          source_hash: `0x${"ef".repeat(32)}`,
        }],
      },
    };
    const conflict = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
    const first = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
      registry_entry: {
        ...publish.registry_entry,
        versions: [{
          ...publish.registry_entry.versions[0],
          version: "1.2.4",
          tag: "v1.2.4",
          source_hash: `0x${"ef".repeat(32)}`,
        }],
      },
    };
    const replay = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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

  it("releases publish nonce and idempotency reservation when an object write fails before admission", async () => {
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
          if (failStaticWrites && key.startsWith("artifacts/")) {
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
    const noncesBeforePublish = store.usedNonces.size;
    const response = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
    expect(store.usedNonces.size).toBe(noncesBeforePublish);
    expect(store.capabilities.get(capability.key_id)?.last_used_at).toBeFalsy();
    expect(store.auditEvents.some((event) => event.event_type === "capability.used")).toBe(false);
    expect(store.auditEvents.some((event) => event.event_type === "publish.accepted")).toBe(false);

    failStaticWrites = false;
    const retry = await post(app, "/v1/artifacts/cellscript/demo/releases", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: sourceSnapshot,
    }, {}, { "idempotency-key": idempotencyKey });

    expect(retry.status).toBe(202);
    expect((await retry.json() as any).verification_status).toBe("pending");
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
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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

    const unsupportedPromotion = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/availability",
      { availability_status: "verified_build", reason: "manual claim without evidence" },
      adminEnv,
      adminHeaders,
    );
    expect(unsupportedPromotion.status).toBe(400);
    expect((await unsupportedPromotion.json() as any).error.code).toBe("invalid_availability_status");

    const quarantineResponse = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/availability",
      { availability_status: "quarantined", reason: "manual review" },
      adminEnv,
      adminHeaders,
    );
    expect(quarantineResponse.status).toBe(200);
    expect((await quarantineResponse.json() as any).availability_status).toBe("quarantined");
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.availability_status).toBe("quarantined");
    const staticEntryWrites = snapshots.filter((snapshot) => snapshot.key === "artifacts/cellscript/demo/releases/1.2.3.json");
    expect(staticEntryWrites).toHaveLength(2);
    expect(JSON.parse(utf8(staticEntryWrites.at(-1)!.body)).availability_status).toBe("quarantined");
    expect(store.auditEvents.some((event) => event.event_type === "admin.namespace.status_updated")).toBe(true);
    expect(store.auditEvents.some((event) => event.event_type === "admin.package_version.status_updated")).toBe(true);
  });

  it("lists public packages and requires chained evidence for production promotions", async () => {
    const { app, store, snapshots } = testApp(undefined, undefined, {
      verifyMainnetDeployment: async () => ({ block_hash: `0x${"60".repeat(32)}` }),
      verifyMainnetCommitment: async () => ({
        commitment_schema: "cellscript-registry-commitment-v1",
        chain_verification: "get_live_cell+type_index",
        observed_block_hash: `0x${"61".repeat(32)}`,
      }),
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
    const publish = await ckbExecutablePublishPayload(capability.key_id);
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("artifact bundle"),
        content_type: "application/vnd.cellscript.artifact-bundle+json",
        size_bytes: "artifact bundle".length,
        source_hash: publish.source_hash,
      },
    });
    expect(publishResponse.status).toBe(202);

    const publicIndex = await get(app, "/v1/artifacts?q=demo&limit=10");
    expect(publicIndex.status).toBe(200);
    expect(await publicIndex.json()).toMatchObject({
      schema: "cellscript-registry-artifact-index",
      count: 1,
      artifacts: [{
        coordinate: "cellscript/demo",
        latest_release: "1.2.3",
        verification_status: "pending",
        deployment_status: "undeployed",
        availability_status: "active",
      }],
    });
    const explicitlyUnverified = await get(app, "/v1/artifacts?q=demo&verification=pending&limit=10");
    expect(explicitlyUnverified.status).toBe(200);
    expect(await explicitlyUnverified.json()).toMatchObject({
      schema: "cellscript-registry-artifact-index",
      count: 1,
      artifacts: [{
        coordinate: "cellscript/demo",
        latest_release: "1.2.3",
        verification_status: "pending",
        releases: [{
          immutable_bundle: {
            schema: "cellscript-registry-immutable-bundle",
            url: expect.stringContaining("https://registry.cellscript.dev/source-snapshots/cellscript/demo/1.2.3/"),
            content_type: "application/vnd.cellscript.artifact-bundle+json",
          },
        }],
      }],
    });

    const adminEnv = { REGISTRY_ADMIN_TOKEN: "secret" };
    const adminHeaders = { authorization: "Bearer secret", "x-registry-admin-actor": "release-bot" };
    const commonEvidence = {
      schema: "cellscript-registry-evidence",
      producer: "cellscript-release-gate/0.23.0",
      generated_at: "2026-06-23T12:00:00Z",
      verification_status: "passed",
      source_hash: publish.source_hash,
      manifest_hash: publish.manifest_hash,
    };

    const missingDependency = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/promote",
      {
        kind: "deployed",
        evidence: {
          ...commonEvidence,
          kind: "deployed",
          verified_build_evidence_hash: `sha256:${"11".repeat(32)}`,
          artifact_hash: `0x${"31".repeat(32)}`,
          network: "mainnet",
          code_hash: `0x${"31".repeat(32)}`,
          data_hash: `0x${"31".repeat(32)}`,
          hash_type: "data1",
          dep_type: "code",
          out_point: { tx_hash: `0x${"43".repeat(32)}`, index: 0 },
          deployment_status: "live",
        },
      },
      adminEnv,
      adminHeaders,
    );
    expect(missingDependency.status).toBe(409);
    expect((await missingDependency.json() as any).error.code).toBe("evidence_dependency_missing");

    const verified = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/promote",
      {
        kind: "verified_build",
        evidence: {
          ...commonEvidence,
          kind: "verified_build",
          verification_level: "hash_bound",
          artifact_hash: `0x${"31".repeat(32)}`,
          metadata_hash: `0x${"32".repeat(32)}`,
        },
      },
      adminEnv,
      adminHeaders,
    );
    expect(verified.status).toBe(200);
    const verifiedBody = await verified.json() as any;
    expect(verifiedBody.status).toBe("verified_build");

    const mismatchedDeployment = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/promote",
      {
        kind: "deployed",
        evidence: {
          ...commonEvidence,
          kind: "deployed",
          verified_build_evidence_hash: verifiedBody.evidence.evidence_hash,
          artifact_hash: `0x${"31".repeat(32)}`,
          network: "mainnet",
          code_hash: `0x${"44".repeat(32)}`,
          data_hash: `0x${"44".repeat(32)}`,
          hash_type: "data1",
          dep_type: "code",
          out_point: { tx_hash: `0x${"43".repeat(32)}`, index: 0 },
          deployment_status: "live",
        },
      },
      adminEnv,
      adminHeaders,
    );
    expect(mismatchedDeployment.status).toBe(400);
    expect((await mismatchedDeployment.json() as any).error.code).toBe("deployment_data_hash_mismatch");

    const deployed = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/promote",
      {
        kind: "deployed",
        evidence: {
          ...commonEvidence,
          kind: "deployed",
          verified_build_evidence_hash: verifiedBody.evidence.evidence_hash,
          artifact_hash: `0x${"31".repeat(32)}`,
          network: "mainnet",
          code_hash: `0x${"31".repeat(32)}`,
          data_hash: `0x${"31".repeat(32)}`,
          hash_type: "data1",
          dep_type: "code",
          out_point: { tx_hash: `0x${"43".repeat(32)}`, index: 0 },
          deployment_status: "live",
        },
      },
      adminEnv,
      adminHeaders,
    );
    expect(deployed.status).toBe(200);
    const deployedBody = await deployed.json() as any;
    expect(deployedBody.status).toBe("deployed");

    const attested = await post(
      app,
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/promote",
      {
        kind: "on_chain_attested",
        evidence: {
          ...commonEvidence,
          kind: "on_chain_attested",
          deployed_evidence_hash: deployedBody.evidence.evidence_hash,
          network: "mainnet",
          attestation_tx_hash: `0x${"51".repeat(32)}`,
          attestation_hash: `0x${"52".repeat(32)}`,
          attestor: "cellscript-release-bot",
          attestor_lock_hash: `0x${"53".repeat(32)}`,
          registry_type_hash: `0x${"54".repeat(32)}`,
          attestation_out_point: { tx_hash: `0x${"51".repeat(32)}`, index: 0 },
          observed_at: "2026-06-23T12:00:00Z",
          attestation_status: "confirmed",
        },
      },
      adminEnv,
      adminHeaders,
    );
    expect(attested.status).toBe(200);
    expect((await attested.json() as any).status).toBe("on_chain_attested");

    const acceptedIndex = await get(app, "/v1/artifacts?q=demo&limit=10");
    expect(acceptedIndex.status).toBe(200);
    expect(await acceptedIndex.json()).toMatchObject({
      count: 1,
      artifacts: [{ coordinate: "cellscript/demo", verification_status: "hash_bound", deployment_status: "chain_verified" }],
    });

    const detail = await get(app, "/v1/artifacts/cellscript/demo");
    expect(detail.status).toBe(200);
    expect(await detail.json()).toMatchObject({
      coordinate: "cellscript/demo",
      verification_status: "hash_bound",
      deployment_status: "chain_verified",
      releases: [{
        release: "1.2.3",
        verification_status: "hash_bound",
        deployment_status: "chain_verified",
        immutable_bundle: { schema: "cellscript-registry-immutable-bundle" },
        evidence: [{ kind: "verified_build" }, { kind: "deployed" }, { kind: "on_chain_attested" }],
      }],
    });
    const evidence = await get(app, "/v1/artifacts/cellscript/demo/releases/1.2.3/evidence");
    expect(evidence.status).toBe(200);
    expect((await evidence.json() as any).evidence).toHaveLength(3);
    const staticWrites = snapshots.filter((snapshot) => snapshot.key === "artifacts/cellscript/demo/releases/1.2.3.json");
    expect(staticWrites).toHaveLength(4);
    expect(JSON.parse(utf8(staticWrites.at(-1)!.body)).evidence).toHaveLength(3);
    expect(JSON.parse(utf8(staticWrites.at(-1)!.body)).immutable_bundle.url).toContain("/source-snapshots/cellscript/demo/1.2.3/");
  });

  it("records only capability-signed, chain-verified mainnet deployments for executable artifacts", async () => {
    const store = new MemoryRegistryStore();
    const snapshots: Array<{ key: string; body: Uint8Array }> = [];
    const app = createApp({
      store,
      now: () => now,
      joyidVerifier: { verifySignature: async () => true },
      capabilityVerifier: { verify: async () => true },
      verifyMainnetDeployment: async (payload) => {
        expect(payload.network).toBe("mainnet");
        expect(payload.out_point).toEqual({ tx_hash: `0x${"41".repeat(32)}`, index: 0 });
        return { block_hash: `0x${"51".repeat(32)}` };
      },
      snapshotWriter: {
        async put(key, body) { snapshots.push({ key, body }); },
      },
    });
    const root = authPayload();
    const capability = await (await post(app, "/v1/capabilities", {
      payload: root,
      joyid_signature: joyidSignature(root),
    })).json() as any;
    store.namespaces.set("cellscript", {
      namespace: "cellscript",
      status: "active",
      owner_principal_type: "joyid_ckb",
      owner_principal_id: root.principal_id,
    });
    const publish = await ckbExecutablePublishPayload(capability.key_id);
    const published = await post(app, "/v1/artifacts/cellscript/demo/releases", {
      payload: publish,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
      source_snapshot: {
        content_base64: base64("artifact bundle"),
        content_type: "application/vnd.cellscript.artifact-bundle+json",
        size_bytes: "artifact bundle".length,
        source_hash: publish.source_hash,
      },
    });
    expect(published.status).toBe(202);
    await store.promotePackageVersion({
      namespace: "cellscript",
      name: "demo",
      version: "1.2.3",
      kind: "verified_build",
      evidence_hash: `sha256:${"61".repeat(32)}`,
      evidence: { verification_level: "hash_bound", artifact_hash: `0x${"31".repeat(32)}` },
      request_id: "verification:test",
      admin_actor: "verification-worker:test",
    });

    const deployment = deploymentPayload(capability.key_id);
    const response = await post(app, "/v1/artifacts/cellscript/demo/releases/1.2.3/deployments", {
      payload: deployment,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
    });
    expect(response.status).toBe(201);
    expect(await response.json()).toMatchObject({
      coordinate: "cellscript/demo@1.2.3",
      deployment_status: "chain_verified",
      evidence: {
        kind: "deployed",
        evidence: {
          network: "mainnet",
          deployment_status: "live",
          chain_verification: "get_live_cell",
        },
      },
    });
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.deployment_status).toBe("chain_verified");
    expect(store.auditEvents.some((event) => event.event_type === "deployment.chain_verified")).toBe(true);
    expect(snapshots.filter((item) => item.key === "artifacts/cellscript/demo/releases/1.2.3.json")).toHaveLength(2);
    const commitment = await get(app, "/v1/artifacts/cellscript/demo/releases/1.2.3/commitment");
    expect(commitment.status).toBe(200);
    expect(await commitment.json()).toMatchObject({
      schema: "cellscript-registry-commitment-proof-v1",
      status: "commitment_ready",
      payload: {
        schema: "cellscript-registry-commitment-v1",
        namespace: "cellscript",
        name: "demo",
        release: "1.2.3",
      },
      cell_data: expect.stringMatching(/^0x43535245477631[0-9a-f]{64}$/),
    });
  });

  it("rejects testnet deployment payloads and exposes no retired package routes", async () => {
    const { app } = testApp();
    const deployment = { ...deploymentPayload("cap_11111111111111111111111111111111"), network: "testnet" };
    const rejected = await post(app, "/v1/artifacts/cellscript/demo/releases/1.2.3/deployments", {
      payload: deployment,
      capability_signature: { algorithm: "p256-sha256", signature: "sig" },
    });
    expect(rejected.status).toBe(400);
    expect((await rejected.json() as any).error.code).toBe("unsupported_deployment_network");
    expect((await get(app, "/v1/packages")).status).toBe(404);
    expect((await get(app, "/v1/packages/cellscript/demo")).status).toBe(404);
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
          if (failStaticWrites && key.startsWith("artifacts/")) {
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
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
      "/v1/admin/artifacts/cellscript/demo/releases/1.2.3/availability",
      { availability_status: "quarantined", reason: "manual review" },
      { REGISTRY_ADMIN_TOKEN: "secret" },
      { authorization: "Bearer secret" },
    );

    expect(response.status).toBe(500);
    expect((await response.json() as any).error.code).toBe("internal_error");
    expect(store.packageVersions.get("cellscript/demo@1.2.3")?.availability_status).toBe("active");
    expect(store.auditEvents.some((event) => event.event_type === "admin.package_version.status_updated")).toBe(false);
    const staticEntryWrites = snapshots.filter((snapshot) => snapshot.key === "artifacts/cellscript/demo/releases/1.2.3.json");
    expect(staticEntryWrites).toHaveLength(1);
    expect(JSON.parse(utf8(staticEntryWrites[0]!.body)).availability_status).toBe("active");
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

    const response = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
    const response = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
      path: "/v1/artifacts/cellscript/demo/releases",
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
      protocol: PUBLISH_PROTOCOL,
      action: "publish",
      nonce: "0xaaaaaaaaaaaaaaaa",
      request_id: "old-request",
      expires_at: "2026-06-23T11:59:00Z",
      created_at: "2026-06-23T11:50:00Z",
    });
    store.usedNonces.set("live-nonce", {
      protocol: PUBLISH_PROTOCOL,
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
    const publishResponse = await post(app, "/v1/artifacts/cellscript/demo/releases", {
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
