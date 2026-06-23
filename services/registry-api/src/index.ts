import { verifySignature, type SignChallengeResponseData } from "@joyid/ckb";
import {
  ACCEPTED_PRINCIPAL_TYPE,
  ApiError,
  DEFAULT_REGISTRY_ORIGIN,
  DEFAULT_STATIC_REGISTRY_ORIGIN,
  WebCryptoP256Verifier,
  base64ToBytes,
  canonicalJson,
  capabilityKeyId,
  scopeAllowsPublish,
  sha256Hex,
  validateCapabilityPayload,
  validateCapabilityRevocationPayload,
  validatePackageIdent,
  validatePublishPayload,
  validateSnapshot,
  validateVersion,
  verifyJoyidAuthorisationPayload,
  verifyJoyidPayloadSignature,
  type CapabilitySignature,
  type CapabilitySignatureVerifier,
  type JoyidVerifier,
  type SourceSnapshotInput,
} from "./domain";
import { MemoryRegistryStore, type IdempotencyRecord, type RegistryStore, type SnapshotRecord } from "./store";
import { SqlRegistryStore, type HyperdriveLike } from "./sql-store";

export interface Env {
  HYPERDRIVE?: HyperdriveLike;
  REGISTRY_OBJECTS?: R2Bucket;
  SOURCE_SNAPSHOTS?: R2Bucket;
  REGISTRY_ORIGIN?: string;
  STATIC_REGISTRY_ORIGIN?: string;
  MAX_JSON_BODY_BYTES?: string;
  MAX_SNAPSHOT_BYTES?: string;
  REGISTRY_ADMIN_TOKEN?: string;
  ENVIRONMENT?: string;
  CLEANUP_QUOTA_EVENT_RETENTION_HOURS?: string;
  NAMESPACE_CLAIM_COOLDOWN_SECONDS?: string;
}

export interface SnapshotWriter {
  put(key: string, body: Uint8Array, options: { contentType: string; metadata: Record<string, string> }): Promise<void>;
}

export interface RegistryObjectRead {
  body: BodyInit;
  contentType?: string;
  etag?: string;
}

export interface RegistryObjectReader {
  get(key: string): Promise<RegistryObjectRead | null>;
}

export interface AppDeps {
  store?: RegistryStore;
  joyidVerifier?: JoyidVerifier;
  capabilityVerifier?: CapabilitySignatureVerifier;
  snapshotWriter?: SnapshotWriter;
  registryObjectReader?: RegistryObjectReader;
  now?: () => Date;
}

const DEFAULT_MAX_JSON_BODY_BYTES = 6 * 1024 * 1024;
const DEFAULT_MAX_SNAPSHOT_BYTES = 5 * 1024 * 1024;
const DEFAULT_QUOTA_EVENT_RETENTION_HOURS = 48;
const DEFAULT_NAMESPACE_CLAIM_COOLDOWN_SECONDS = 60 * 60;

export function createApp(deps: AppDeps = {}) {
  return {
    async fetch(request: Request, env: Env = {}, ctx?: ExecutionContext): Promise<Response> {
      const requestId = request.headers.get("cf-ray") ?? crypto.randomUUID();
      try {
        return await routeRequest(request, env, requestId, deps, ctx);
      } catch (error) {
        await appendFailureAuditEvent(request, env, requestId, deps, error);
        return errorResponse(error, requestId);
      }
    },
    async scheduled(_controller: ScheduledController, env: Env = {}, _ctx?: ExecutionContext): Promise<void> {
      await runScheduledMaintenance(env, deps);
    },
  };
}

async function runScheduledMaintenance(env: Env, deps: AppDeps): Promise<void> {
  const store = deps.store ?? getProductionStore(env);
  const now = deps.now?.() ?? new Date();
  const requestId = `scheduled:${now.toISOString()}`;
  const quotaCutoff = new Date(now.getTime() - quotaEventRetentionHours(env) * 60 * 60 * 1000).toISOString();
  const result = await store.cleanupExpiredState({
    now_iso: now.toISOString(),
    quota_events_before_iso: quotaCutoff,
  });
  await store.appendAuditEvent({
    request_id: requestId,
    event_type: "maintenance.cleanup",
    data: {
      quota_events_before_iso: quotaCutoff,
      ...result,
    },
  });
}

async function routeRequest(
  request: Request,
  env: Env,
  requestId: string,
  deps: AppDeps,
  _ctx?: ExecutionContext,
): Promise<Response> {
  const url = new URL(request.url);
  const headers = corsHeaders(requestId);
  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers });
  }
  if (request.method === "GET" && url.pathname === "/health") {
    return json({ status: "ok", request_id: requestId }, 200, headers);
  }
  if (request.method === "GET" && url.pathname === "/ready") {
    return handleReadiness(env, deps, requestId, headers);
  }
  const staticPackageVersionMatch = url.pathname.match(/^\/packages\/([^/]+)\/([^/]+)\/versions\/([^/]+)[.]json$/);
  if (request.method === "GET" && staticPackageVersionMatch) {
    return handleStaticPackageVersionRead(
      env,
      deps,
      requestId,
      decodeURIComponent(staticPackageVersionMatch[1] ?? ""),
      decodeURIComponent(staticPackageVersionMatch[2] ?? ""),
      decodeURIComponent(staticPackageVersionMatch[3] ?? ""),
    );
  }

  const store = deps.store ?? getProductionStore(env);
  const now = deps.now?.() ?? new Date();
  const registryOrigin = env.REGISTRY_ORIGIN ?? DEFAULT_REGISTRY_ORIGIN;
  const staticOrigin = env.STATIC_REGISTRY_ORIGIN ?? DEFAULT_STATIC_REGISTRY_ORIGIN;

  if (request.method === "POST" && url.pathname === "/v1/capabilities") {
    return handleCreateCapability(request, env, store, requestId, registryOrigin, now, deps, headers);
  }

  if (request.method === "POST" && url.pathname === "/v1/admin/reserved-namespaces") {
    return handleAdminReservedNamespace(request, env, store, requestId, headers);
  }

  if (request.method === "GET" && url.pathname === "/v1/admin/audit-events") {
    return handleAdminAuditEvents(request, env, store, requestId, headers);
  }

  const adminNamespaceStatusMatch = url.pathname.match(/^\/v1\/admin\/namespaces\/([^/]+)\/status$/);
  if (request.method === "POST" && adminNamespaceStatusMatch) {
    return handleAdminNamespaceStatus(request, env, store, requestId, headers, decodeURIComponent(adminNamespaceStatusMatch[1] ?? ""));
  }

  const adminVersionStatusMatch = url.pathname.match(/^\/v1\/admin\/packages\/([^/]+)\/([^/]+)\/versions\/([^/]+)\/status$/);
  if (request.method === "POST" && adminVersionStatusMatch) {
    return handleAdminPackageVersionStatus(
      request,
      env,
      store,
      requestId,
      staticOrigin,
      deps,
      headers,
      decodeURIComponent(adminVersionStatusMatch[1] ?? ""),
      decodeURIComponent(adminVersionStatusMatch[2] ?? ""),
      decodeURIComponent(adminVersionStatusMatch[3] ?? ""),
    );
  }

  const revokeMatch = url.pathname.match(/^\/v1\/capabilities\/([^/]+)\/revoke$/);
  if (request.method === "POST" && revokeMatch) {
    return handleRevokeCapability(
      request,
      env,
      store,
      requestId,
      registryOrigin,
      now,
      deps,
      headers,
      decodeURIComponent(revokeMatch[1] ?? ""),
    );
  }

  if (request.method === "POST" && url.pathname === "/v1/namespaces/claim") {
    return handleClaimNamespace(request, env, store, requestId, registryOrigin, now, deps, headers);
  }

  const publishMatch = url.pathname.match(/^\/v1\/packages\/([^/]+)\/([^/]+)\/versions$/);
  if (request.method === "POST" && publishMatch) {
    return handlePublishVersion(
      request,
      env,
      store,
      requestId,
      registryOrigin,
      staticOrigin,
      now,
      deps,
      headers,
      decodeURIComponent(publishMatch[1] ?? ""),
      decodeURIComponent(publishMatch[2] ?? ""),
    );
  }

  throw new ApiError(404, "not_found", "route not found");
}

async function handleStaticPackageVersionRead(
  env: Env,
  deps: AppDeps,
  requestId: string,
  namespaceFromPath: string,
  nameFromPath: string,
  versionFromPath: string,
): Promise<Response> {
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const version = validateVersion(versionFromPath);
  const key = staticPackageVersionKey(namespace, name, version);
  const reader = deps.registryObjectReader ?? r2RegistryObjectReader(env);
  const object = await reader.get(key);
  if (!object) {
    throw new ApiError(404, "registry_object_not_found", "package version registry object was not found");
  }
  const headers = corsHeaders(requestId);
  headers.set("content-type", object.contentType ?? "application/json; charset=utf-8");
  headers.set("cache-control", "public, max-age=60, stale-while-revalidate=300");
  if (object.etag) {
    headers.set("etag", object.etag);
  }
  return new Response(object.body, { status: 200, headers });
}

function handleReadiness(env: Env, deps: AppDeps, requestId: string, headers: Headers): Response {
  const storeConfigured = !!deps.store || !!env.HYPERDRIVE;
  const objectStoreConfigured =
    (!!deps.snapshotWriter && !!deps.registryObjectReader)
    || !!env.REGISTRY_OBJECTS
    || !!env.SOURCE_SNAPSHOTS;
  const adminConfigured = typeof env.REGISTRY_ADMIN_TOKEN === "string" && env.REGISTRY_ADMIN_TOKEN.trim() !== "";
  const ready = storeConfigured && objectStoreConfigured && adminConfigured;
  return json(
    {
      status: ready ? "ready" : "not_ready",
      request_id: requestId,
      checks: {
        store: storeConfigured ? "configured" : "missing_hyperdrive",
        object_store: objectStoreConfigured ? "configured" : "missing_r2",
        admin_token: adminConfigured ? "configured" : "missing_secret",
      },
    },
    ready ? 200 : 503,
    headers,
  );
}

async function handleAdminReservedNamespace(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  headers: Headers,
): Promise<Response> {
  const adminActor = requireAdminActor(request, env);
  const body = await readJson(request, maxJsonBytes(env));
  const namespace = validatePackageIdent(String(body["namespace"] ?? ""), "namespace");
  const matchType = requireOneOf(String(body["match_type"] ?? "exact"), ["exact", "prefix", "typosquat"], "invalid_reserved_match_type");
  const reason = requireNonEmptyAdminString(body["reason"], "reason");
  const record = await store.upsertReservedNamespace({
    namespace,
    match_type: matchType,
    reason,
    request_id: requestId,
    admin_actor: adminActor,
  });
  return json({ request_id: requestId, ...record }, 200, headers);
}

async function handleAdminAuditEvents(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  headers: Headers,
): Promise<Response> {
  requireAdminActor(request, env);
  const params = new URL(request.url).searchParams;
  const eventType = optionalAuditParam(params, "event_type");
  const principalType = optionalAuditParam(params, "principal_type");
  const principalId = optionalAuditParam(params, "principal_id");
  const namespaceRaw = optionalAuditParam(params, "namespace");
  const nameRaw = optionalAuditParam(params, "name");
  const versionRaw = optionalAuditParam(params, "version");
  const beforeRaw = optionalAuditParam(params, "before");
  const limit = auditLimit(params);
  if (principalType && principalType !== ACCEPTED_PRINCIPAL_TYPE) {
    throw new ApiError(400, "invalid_audit_filter", "principal_type filter must be joyid_ckb");
  }
  const before = beforeRaw ? parseAuditBefore(beforeRaw) : undefined;
  const namespace = namespaceRaw ? validatePackageIdent(namespaceRaw, "namespace") : undefined;
  const name = nameRaw ? validatePackageIdent(nameRaw, "name") : undefined;
  const version = versionRaw ? validateVersion(versionRaw) : undefined;
  const events = await store.listAuditEvents({
    ...(eventType ? { event_type: eventType } : {}),
    ...(principalType ? { principal_type: principalType } : {}),
    ...(principalId ? { principal_id: principalId } : {}),
    ...(namespace ? { namespace } : {}),
    ...(name ? { name } : {}),
    ...(version ? { version } : {}),
    ...(before ? { before } : {}),
    limit,
  });
  const nextBefore = events.length === limit ? events[events.length - 1]?.created_at : undefined;
  return json(
    {
      request_id: requestId,
      events,
      ...(nextBefore ? { next_before: nextBefore } : {}),
    },
    200,
    headers,
  );
}

async function handleAdminNamespaceStatus(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  headers: Headers,
  namespaceFromPath: string,
): Promise<Response> {
  const adminActor = requireAdminActor(request, env);
  const body = await readJson(request, maxJsonBytes(env));
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const status = requireOneOf(
    String(body["status"] ?? ""),
    ["active", "review_pending", "reserved", "rejected", "quarantined"],
    "invalid_namespace_status",
  );
  const reviewReason = typeof body["review_reason"] === "string" && body["review_reason"].trim() !== "" ? body["review_reason"].trim() : undefined;
  const record = await store.updateNamespaceStatus({
    namespace,
    status,
    ...(reviewReason ? { review_reason: reviewReason } : {}),
    request_id: requestId,
    admin_actor: adminActor,
  });
  return json({ request_id: requestId, ...record }, 200, headers);
}

async function handleAdminPackageVersionStatus(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  staticOrigin: string,
  deps: AppDeps,
  headers: Headers,
  namespaceFromPath: string,
  nameFromPath: string,
  versionFromPath: string,
): Promise<Response> {
  const adminActor = requireAdminActor(request, env);
  const body = await readJson(request, maxJsonBytes(env));
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const version = validateVersion(versionFromPath);
  const status = requireOneOf(
    String(body["status"] ?? ""),
    ["source_published", "indexed_pending", "verified_build", "deployed", "deprecated", "yanked", "quarantined"],
    "invalid_package_version_status",
  );
  const reason = typeof body["reason"] === "string" && body["reason"].trim() !== "" ? body["reason"].trim() : undefined;
  const directUrl = staticPackageVersionUrl(staticOrigin, namespace, name, version);
  if (isSuppressivePackageVersionStatus(status)) {
    const existing = await store.getPackageVersion(namespace, name, version);
    if (!existing) {
      throw new ApiError(404, "package_version_not_found", "package version is not known to the registry");
    }
    await writeStaticRegistryVersionObject(env, deps, { ...existing, status, direct_url: directUrl });
  }
  const record = await store.updatePackageVersionStatus({
    namespace,
    name,
    version,
    status,
    ...(reason ? { reason } : {}),
    request_id: requestId,
    admin_actor: adminActor,
  });
  if (!isSuppressivePackageVersionStatus(status)) {
    await writeStaticRegistryVersionObject(env, deps, { ...record, direct_url: directUrl });
  }
  return json({ request_id: requestId, ...record }, 200, headers);
}

function isSuppressivePackageVersionStatus(status: string): boolean {
  return status === "deprecated" || status === "yanked" || status === "quarantined";
}

function getProductionStore(env: Env): RegistryStore {
  if (!env.HYPERDRIVE) {
    throw new ApiError(503, "registry_store_unconfigured", "HYPERDRIVE binding is required for production registry writes");
  }
  return new SqlRegistryStore(env.HYPERDRIVE);
}

function optionalStore(env: Env, deps: AppDeps): RegistryStore | undefined {
  if (deps.store) {
    return deps.store;
  }
  return env.HYPERDRIVE ? new SqlRegistryStore(env.HYPERDRIVE) : undefined;
}

async function handleCreateCapability(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  registryOrigin: string,
  now: Date,
  deps: AppDeps,
  headers: Headers,
): Promise<Response> {
  await throttleRequestSource(store, request, requestId, "capability_create", 120, 60, now);
  const body = await readJson(request, maxJsonBytes(env));
  const payload = validateCapabilityPayload(body["payload"], registryOrigin, now);
  const signature = requireJoyidSignature(body["joyid_signature"]);
  await verifyJoyidAuthorisationPayload(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier(),);
  await throttle(store, requestId, `principal:${payload.principal_type}:${payload.principal_id}`, "capability", 8, 60 * 60, now);
  await consumeSignedNonce(store, requestId, {
    protocol: payload.protocol,
    action: `${payload.action}:capability_create`,
    nonce: payload.nonce,
    expires_at: payload.expires_at,
    principal_type: payload.principal_type,
    principal_id: payload.principal_id,
  });
  const capability = await store.recordCapability({ payload, joyid_signature: signature, request_id: requestId });
  return json(
    {
      request_id: requestId,
      key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      scopes: capability.scopes,
      expires_at: capability.expires_at,
      status: "active",
    },
    201,
    headers,
  );
}

async function handleClaimNamespace(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  registryOrigin: string,
  now: Date,
  deps: AppDeps,
  headers: Headers,
): Promise<Response> {
  await throttleRequestSource(store, request, requestId, "namespace_claim", 40, 60 * 60, now);
  const body = await readJson(request, maxJsonBytes(env));
  const namespace = validatePackageIdent(String(body["namespace"] ?? ""), "namespace");
  const payload = validateCapabilityPayload(body["payload"], registryOrigin, now);
  const signature = requireJoyidSignature(body["joyid_signature"]);
  if (!payload.requested_scopes.some((scope) => scope.startsWith(`publish:${namespace}/`))) {
    throw new ApiError(403, "namespace_scope_missing", "namespace claim requires a publish scope for that namespace");
  }
  await verifyJoyidAuthorisationPayload(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier());
  await throttle(store, requestId, `principal:${payload.principal_type}:${payload.principal_id}`, "namespace_claim", 12, 24 * 60 * 60, now);
  const existing = await store.getNamespace(namespace);
  if (
    existing
    && (existing.owner_principal_type !== payload.principal_type || existing.owner_principal_id !== payload.principal_id)
  ) {
    throw new ApiError(409, "namespace_already_claimed", "namespace is already claimed by another principal");
  }
  if (existing) {
    return json(
      {
        request_id: requestId,
        namespace: existing.namespace,
        status: existing.status,
        ...(existing.review_reason ? { review_reason: existing.review_reason } : {}),
      },
      existing.status === "active" ? 201 : 202,
      headers,
    );
  }
  await enforceNamespaceClaimCooldown(store, requestId, payload.principal_type, payload.principal_id, now, namespaceClaimCooldownSeconds(env));
  const claim = await store.claimNamespace({
    namespace,
    principal_type: ACCEPTED_PRINCIPAL_TYPE,
    principal_id: payload.principal_id,
    request_id: requestId,
  });
  return json({ request_id: requestId, ...claim }, claim.status === "active" ? 201 : 202, headers);
}

async function handleRevokeCapability(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  registryOrigin: string,
  now: Date,
  deps: AppDeps,
  headers: Headers,
  keyIdFromPath: string,
): Promise<Response> {
  await throttleRequestSource(store, request, requestId, "capability_revoke", 60, 60 * 60, now);
  const body = await readJson(request, maxJsonBytes(env));
  const payload = validateCapabilityRevocationPayload(body["payload"], registryOrigin, now);
  if (payload.capability_key_id !== keyIdFromPath) {
    throw new ApiError(400, "route_payload_mismatch", "capability route and revocation payload do not match");
  }
  const capability = await store.getCapability(payload.capability_key_id);
  if (!capability) {
    throw new ApiError(404, "capability_not_found", "capability key is not known to the registry");
  }
  if (capability.principal_type !== payload.principal_type || capability.principal_id !== payload.principal_id) {
    throw new ApiError(403, "capability_owner_mismatch", "JoyID principal does not own this capability");
  }
  const signature = requireJoyidSignature(body["joyid_signature"]);
  await verifyJoyidPayloadSignature(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier());
  await throttle(store, requestId, `principal:${payload.principal_type}:${payload.principal_id}`, "capability_revoke", 8, 60 * 60, now);
  await consumeSignedNonce(store, requestId, {
    protocol: payload.protocol,
    action: payload.action,
    nonce: payload.nonce,
    expires_at: payload.expires_at,
    principal_type: payload.principal_type,
    principal_id: payload.principal_id,
    capability_key_id: capability.key_id,
  });
  const reason = typeof body["reason"] === "string" ? body["reason"] : undefined;
  const revoked = await store.revokeCapability({
    key_id: capability.key_id,
    principal_type: payload.principal_type,
    principal_id: payload.principal_id,
    request_id: requestId,
    ...(reason ? { reason } : {}),
  });
  return json(
    {
      request_id: requestId,
      key_id: revoked.key_id,
      principal_type: revoked.principal_type,
      principal_id: revoked.principal_id,
      revoked_at: revoked.revoked_at,
      status: "revoked",
    },
    200,
    headers,
  );
}

async function handlePublishVersion(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  registryOrigin: string,
  staticOrigin: string,
  now: Date,
  deps: AppDeps,
  headers: Headers,
  namespaceFromPath: string,
  nameFromPath: string,
): Promise<Response> {
  await throttleRequestSource(store, request, requestId, "publish", 80, 60 * 60, now);
  const body = await readJson(request, maxJsonBytes(env));
  const payload = validatePublishPayload(body["payload"], registryOrigin, now);
  if (payload.namespace !== validatePackageIdent(namespaceFromPath, "namespace") || payload.name !== validatePackageIdent(nameFromPath, "name")) {
    throw new ApiError(400, "route_payload_mismatch", "package route and publish payload do not match");
  }
  const signature = requireCapabilitySignature(body["capability_signature"]);
  const snapshot = validateSnapshot(body["source_snapshot"], payload, maxSnapshotBytes(env));
  const requestHash = await publishRequestHash(payload, signature, snapshot);
  const idempotencyKey = requestIdempotencyKey(request, "publish");
  if (idempotencyKey) {
    const replay = await idempotencyReplayResponse(store, idempotencyKey, requestHash, headers);
    if (replay) {
      return replay;
    }
  }
  const capability = await store.getCapability(payload.capability_key_id);
  if (!capability) {
    throw new ApiError(401, "capability_not_found", "capability key is not known to the registry");
  }
  if (capability.revoked_at) {
    throw new ApiError(401, "capability_revoked", "capability key is revoked");
  }
  if (new Date(capability.expires_at).getTime() <= now.getTime()) {
    throw new ApiError(401, "capability_expired", "capability key has expired");
  }
  if (!scopeAllowsPublish(capability.scopes, payload.namespace, payload.name)) {
    throw new ApiError(403, "capability_scope_denied", "capability scope does not allow this package publish");
  }
  const namespace = await store.getNamespace(payload.namespace);
  if (!namespace) {
    throw new ApiError(409, "namespace_not_claimed", "namespace must be claimed before publishing");
  }
  if (namespace.status !== "active") {
    throw new ApiError(409, "namespace_not_active", "namespace is not active");
  }
  if (namespace.owner_principal_id !== capability.principal_id || namespace.owner_principal_type !== capability.principal_type) {
    throw new ApiError(403, "namespace_owner_mismatch", "capability principal does not own this namespace");
  }

  const canonicalPayload = canonicalJson(payload);
  const verifier = deps.capabilityVerifier ?? new WebCryptoP256Verifier();
  if (!(await verifier.verify(canonicalPayload, capability.capability_pubkey, signature))) {
    throw new ApiError(401, "capability_signature_invalid", "capability signature verification failed");
  }
  await throttle(store, requestId, `capability:${capability.key_id}`, "publish", 60, 60 * 60, now);
  await throttle(store, requestId, `package:${payload.namespace}/${payload.name}`, "publish", 12, 60 * 60, now);
  if (await store.getPackageVersion(payload.namespace, payload.name, payload.version)) {
    throw new ApiError(409, "package_version_exists", "package version already exists and cannot be overwritten");
  }
  let idempotencyReserved = false;
  if (idempotencyKey) {
    const reservation = await store.reserveIdempotencyKey({
      key: idempotencyKey,
      request_hash: requestHash,
      request_id: requestId,
      expires_at: payload.expires_at,
    });
    if (reservation.state === "conflict") {
      throw new ApiError(409, "idempotency_key_conflict", "Idempotency-Key was already used for a different request");
    }
    if (reservation.state === "in_progress") {
      throw new ApiError(409, "idempotency_request_in_progress", "matching idempotent request is still being processed");
    }
    if (reservation.state === "completed") {
      return idempotencyResponse(reservation.record, headers);
    }
    idempotencyReserved = true;
  }

  try {
    await consumeSignedNonce(store, requestId, {
      protocol: payload.protocol,
      action: payload.action,
      nonce: payload.nonce,
      expires_at: payload.expires_at,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      capability_key_id: capability.key_id,
    });

    const snapshotRecord = await writeSnapshot(env, deps, payload.namespace, payload.name, payload.version, snapshot);
    const sourceRepo = typeof payload.registry_entry["repository"] === "string" ? payload.registry_entry["repository"] : undefined;
    await store.ensurePackage({
      namespace: payload.namespace,
      name: payload.name,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      ...(sourceRepo ? { source_repo: sourceRepo } : {}),
      request_id: requestId,
    });
    const directUrl = staticPackageVersionUrl(staticOrigin, payload.namespace, payload.name, payload.version);
    const versionInput = {
      namespace: payload.namespace,
      name: payload.name,
      version: payload.version,
      status: "source_published",
      source_hash: payload.source_hash,
      capability_key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      registry_entry: payload.registry_entry,
      snapshot_hash: snapshotRecord.snapshot_hash,
      direct_url: directUrl,
      created_at: now.toISOString(),
    } as const;
    const version = payload.manifest_hash ? { ...versionInput, manifest_hash: payload.manifest_hash } : versionInput;
    await writeStaticRegistryVersionObject(env, deps, version);
    await store.recordSnapshot(snapshotRecord);
    const recordedVersion = await store.recordPackageVersion(version);
    await store.recordCapabilityUsage({
      key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      request_id: requestId,
      action: "publish",
      namespace: payload.namespace,
      name: payload.name,
      version: payload.version,
    });
    const ipHash = await requestIpHash(request);
    const userAgent = request.headers.get("user-agent") ?? undefined;
    await store.appendAuditEvent({
      request_id: requestId,
      event_type: "publish.accepted",
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      capability_key_id: capability.key_id,
      namespace: payload.namespace,
      name: payload.name,
      version: payload.version,
      ...(ipHash ? { ip_hash: ipHash } : {}),
      ...(userAgent ? { user_agent: userAgent } : {}),
      data: { status: recordedVersion.status, snapshot_hash: snapshotRecord.snapshot_hash, direct_url: directUrl },
    });
    const responseBody = {
      request_id: requestId,
      status: recordedVersion.status,
      direct_url: directUrl,
      snapshot_hash: snapshotRecord.snapshot_hash,
      verification: "queued",
    };
    if (idempotencyKey) {
      await store.completeIdempotencyKey({
        key: idempotencyKey,
        request_hash: requestHash,
        response_status: 202,
        response_body: responseBody,
      });
    }
    return json(responseBody, 202, headers);
  } catch (error) {
    if (idempotencyKey && idempotencyReserved) {
      await store.releaseProcessingIdempotencyKey({ key: idempotencyKey, request_hash: requestHash });
    }
    throw error;
  }
}

async function publishRequestHash(payload: unknown, signature: CapabilitySignature, snapshot: SourceSnapshotInput): Promise<string> {
  return sha256Hex(canonicalJson({
    route: "publish_package_version",
    payload,
    capability_signature: signature,
    source_snapshot: snapshot,
  }));
}

function requestIdempotencyKey(request: Request, scope: string): string | undefined {
  const raw = request.headers.get("idempotency-key")?.trim();
  if (!raw) {
    return undefined;
  }
  if (raw.length < 16 || raw.length > 160 || !/^[A-Za-z0-9._:-]+$/.test(raw)) {
    throw new ApiError(400, "invalid_idempotency_key", "Idempotency-Key must be 16..160 visible token characters");
  }
  return `${scope}:${raw}`;
}

async function idempotencyReplayResponse(
  store: RegistryStore,
  idempotencyKey: string,
  requestHash: string,
  headers: Headers,
): Promise<Response | undefined> {
  const record = await store.getIdempotencyKey(idempotencyKey);
  if (!record) {
    return undefined;
  }
  if (record.request_hash !== requestHash) {
    throw new ApiError(409, "idempotency_key_conflict", "Idempotency-Key was already used for a different request");
  }
  if (record.status !== "completed") {
    throw new ApiError(409, "idempotency_request_in_progress", "matching idempotent request is still being processed");
  }
  return idempotencyResponse(record, headers);
}

function idempotencyResponse(record: IdempotencyRecord, headers: Headers): Response {
  if (record.response_status === undefined || !record.response_body) {
    throw new ApiError(500, "idempotency_response_incomplete", "stored idempotency response is incomplete");
  }
  const replayHeaders = new Headers(headers);
  replayHeaders.set("x-idempotency-status", "replayed");
  return json(record.response_body, record.response_status, replayHeaders);
}

async function consumeSignedNonce(
  store: RegistryStore,
  requestId: string,
  input: {
    protocol: string;
    action: string;
    nonce: string;
    expires_at: string;
    principal_type?: string;
    principal_id?: string;
    capability_key_id?: string;
  },
): Promise<void> {
  const nonceKey = `nonce_${await sha256Hex(canonicalJson({
    protocol: input.protocol,
    action: input.action,
    nonce: input.nonce,
    principal_type: input.principal_type ?? null,
    principal_id: input.principal_id ?? null,
    capability_key_id: input.capability_key_id ?? null,
  }))}`;
  const accepted = await store.consumeNonce({
    nonce_key: nonceKey,
    protocol: input.protocol,
    action: input.action,
    nonce: input.nonce,
    request_id: requestId,
    expires_at: input.expires_at,
    ...(input.principal_type ? { principal_type: input.principal_type } : {}),
    ...(input.principal_id ? { principal_id: input.principal_id } : {}),
    ...(input.capability_key_id ? { capability_key_id: input.capability_key_id } : {}),
  });
  if (!accepted) {
    await store.appendAuditEvent({
      request_id: requestId,
      event_type: "nonce.replay_blocked",
      ...(input.principal_type ? { principal_type: input.principal_type } : {}),
      ...(input.principal_id ? { principal_id: input.principal_id } : {}),
      ...(input.capability_key_id ? { capability_key_id: input.capability_key_id } : {}),
      data: {
        protocol: input.protocol,
        action: input.action,
        nonce_key: nonceKey,
      },
    });
    throw new ApiError(409, "nonce_replay", "signed nonce has already been used");
  }
}

async function writeStaticRegistryVersionObject(
  env: Env,
  deps: AppDeps,
  version: SnapshotPackageVersionRecord,
): Promise<void> {
  const key = staticPackageVersionKey(version.namespace, version.name, version.version);
  const body = new TextEncoder().encode(`${JSON.stringify(staticRegistryVersionPayload(version), null, 2)}\n`);
  const writer = deps.snapshotWriter ?? r2SnapshotWriter(env);
  await writer.put(key, body, {
    contentType: "application/json; charset=utf-8",
    metadata: {
      namespace: version.namespace,
      name: version.name,
      version: version.version,
      status: version.status,
      source_hash: version.source_hash,
      snapshot_hash: version.snapshot_hash,
    },
  });
}

type SnapshotPackageVersionRecord = Awaited<ReturnType<RegistryStore["recordPackageVersion"]>>;

function staticRegistryVersionPayload(version: SnapshotPackageVersionRecord): Record<string, unknown> {
  return {
    schema_version: 1,
    kind: "cellscript.registry.package_version",
    coordinate: `${version.namespace}/${version.name}@${version.version}`,
    namespace: version.namespace,
    name: version.name,
    version: version.version,
    status: version.status,
    source_hash: version.source_hash,
    ...(version.manifest_hash ? { manifest_hash: version.manifest_hash } : {}),
    capability_key_id: version.capability_key_id,
    principal_type: version.principal_type,
    principal_id: version.principal_id,
    registry_entry: version.registry_entry,
    snapshot_hash: version.snapshot_hash,
    direct_url: version.direct_url,
    created_at: version.created_at,
  };
}

function staticPackageVersionKey(namespace: string, name: string, version: string): string {
  return `packages/${namespace}/${name}/versions/${version}.json`;
}

function staticPackageVersionUrl(staticOrigin: string, namespace: string, name: string, version: string): string {
  return `${staticOrigin.replace(/\/+$/, "")}/packages/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}/versions/${encodeURIComponent(version)}.json`;
}

async function writeSnapshot(
  env: Env,
  deps: AppDeps,
  namespace: string,
  name: string,
  version: string,
  snapshot: SourceSnapshotInput,
): Promise<SnapshotRecord> {
  const bytes = base64ToBytes(snapshot.content_base64);
  if (bytes.byteLength !== snapshot.size_bytes) {
    throw new ApiError(400, "snapshot_size_mismatch", "snapshot size_bytes does not match decoded content");
  }
  const snapshotHash = `sha256:${await sha256Hex(bytes)}`;
  const extension = snapshotExtension(snapshot.content_type);
  const r2Key = `source-snapshots/${namespace}/${name}/${version}/${snapshotHash.slice("sha256:".length)}.${extension}`;
  const writer = deps.snapshotWriter ?? r2SnapshotWriter(env);
  await writer.put(r2Key, bytes, {
    contentType: snapshot.content_type,
    metadata: { source_hash: snapshot.source_hash, snapshot_hash: snapshotHash },
  });
  return {
    snapshot_hash: snapshotHash,
    r2_key: r2Key,
    source_hash: snapshot.source_hash,
    size_bytes: snapshot.size_bytes,
    content_type: snapshot.content_type,
  };
}

function snapshotExtension(contentType: string): "json" | "tar" | "tar.gz" | "bin" {
  if (contentType.includes("json")) {
    return "json";
  }
  if (contentType.includes("gzip")) {
    return "tar.gz";
  }
  if (contentType.includes("tar")) {
    return "tar";
  }
  return "bin";
}

function r2SnapshotWriter(env: Env): SnapshotWriter {
  const bucket = env.REGISTRY_OBJECTS ?? env.SOURCE_SNAPSHOTS;
  if (!bucket) {
    throw new ApiError(503, "registry_object_store_unconfigured", "REGISTRY_OBJECTS R2 binding is required for publish");
  }
  return {
    async put(key, body, options) {
      await bucket.put(key, body, {
        httpMetadata: { contentType: options.contentType },
        customMetadata: options.metadata,
      });
    },
  };
}

function r2RegistryObjectReader(env: Env): RegistryObjectReader {
  const bucket = env.REGISTRY_OBJECTS ?? env.SOURCE_SNAPSHOTS;
  if (!bucket) {
    throw new ApiError(503, "registry_object_store_unconfigured", "REGISTRY_OBJECTS R2 binding is required for registry reads");
  }
  return {
    async get(key) {
      const object = await bucket.get(key);
      if (!object) {
        return null;
      }
      const read: RegistryObjectRead = {
        body: object.body,
        etag: object.httpEtag,
      };
      if (object.httpMetadata?.contentType) {
        read.contentType = object.httpMetadata.contentType;
      }
      return read;
    },
  };
}

function productionJoyidVerifier(): JoyidVerifier {
  return {
    verifySignature(signature: SignChallengeResponseData) {
      return verifySignature(signature);
    },
  };
}

async function throttle(
  store: RegistryStore,
  requestId: string,
  quotaKey: string,
  bucket: string,
  limit: number,
  windowSeconds: number,
  now: Date,
): Promise<void> {
  const since = new Date(now.getTime() - windowSeconds * 1000).toISOString();
  const count = await store.countRecentQuotaEvents(quotaKey, bucket, since);
  if (count >= limit) {
    await store.appendAuditEvent({
      request_id: requestId,
      event_type: "rate_limit.blocked",
      data: { quota_key: quotaKey, bucket, limit, window_seconds: windowSeconds },
    });
    throw new ApiError(429, "rate_limited", "rate limit exceeded");
  }
  await store.recordQuotaEvent(quotaKey, bucket);
}

async function enforceNamespaceClaimCooldown(
  store: RegistryStore,
  requestId: string,
  principalType: string,
  principalId: string,
  now: Date,
  cooldownSeconds: number,
): Promise<void> {
  if (cooldownSeconds <= 0) {
    return;
  }
  const quotaKey = `principal:${principalType}:${principalId}`;
  const bucket = "namespace_claim_cooldown";
  const since = new Date(now.getTime() - cooldownSeconds * 1000).toISOString();
  const count = await store.countRecentQuotaEvents(quotaKey, bucket, since);
  if (count >= 1) {
    await store.appendAuditEvent({
      request_id: requestId,
      event_type: "namespace_claim.cooldown_blocked",
      principal_type: principalType,
      principal_id: principalId,
      data: { cooldown_seconds: cooldownSeconds },
    });
    throw new ApiError(429, "namespace_claim_cooldown", "namespace claim cooldown is active");
  }
  await store.recordQuotaEvent(quotaKey, bucket);
}

async function appendFailureAuditEvent(
  request: Request,
  env: Env,
  requestId: string,
  deps: AppDeps,
  error: unknown,
): Promise<void> {
  const status = error instanceof ApiError ? error.status : 500;
  const code = error instanceof ApiError ? error.code : "internal_error";
  const eventType = status === 401 || status === 403 ? "auth.failure" : "request.failed";
  const store = optionalStore(env, deps);
  if (!store) {
    return;
  }
  try {
    const url = new URL(request.url);
    const ipHash = await requestIpHash(request);
    const userAgent = request.headers.get("user-agent") ?? undefined;
    await store.appendAuditEvent({
      request_id: requestId,
      event_type: eventType,
      ...(ipHash ? { ip_hash: ipHash } : {}),
      ...(userAgent ? { user_agent: userAgent } : {}),
      data: {
        method: request.method,
        path: url.pathname,
        status,
        code,
      },
    });
  } catch {
    // Failure audit is best effort and must not replace the original response.
  }
}

async function throttleRequestSource(
  store: RegistryStore,
  request: Request,
  requestId: string,
  bucket: string,
  ipLimit: number,
  windowSeconds: number,
  now: Date,
): Promise<void> {
  const ipHash = await requestIpHash(request);
  if (ipHash) {
    await throttle(store, requestId, `ip:${ipHash}`, bucket, ipLimit, windowSeconds, now);
  }
  const asn = requestAsn(request);
  if (asn) {
    await throttle(store, requestId, `asn:${asn}`, bucket, ipLimit * 20, windowSeconds, now);
  }
}

async function readJson(request: Request, maxBytes: number): Promise<Record<string, unknown>> {
  const contentLength = Number(request.headers.get("content-length") ?? "0");
  if (contentLength > maxBytes) {
    throw new ApiError(413, "body_too_large", `JSON body exceeds ${maxBytes} bytes`);
  }
  const text = await request.text();
  if (new TextEncoder().encode(text).byteLength > maxBytes) {
    throw new ApiError(413, "body_too_large", `JSON body exceeds ${maxBytes} bytes`);
  }
  try {
    const parsed = JSON.parse(text) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new ApiError(400, "invalid_json", "request body must be a JSON object");
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    if (error instanceof ApiError) {
      throw error;
    }
    throw new ApiError(400, "invalid_json", "request body is not valid JSON");
  }
}

function requireJoyidSignature(value: unknown): SignChallengeResponseData {
  if (!value || typeof value !== "object") {
    throw new ApiError(400, "missing_joyid_signature", "joyid_signature is required");
  }
  return value as SignChallengeResponseData;
}

function requireCapabilitySignature(value: unknown): CapabilitySignature {
  if (!value || typeof value !== "object") {
    throw new ApiError(400, "missing_capability_signature", "capability_signature is required");
  }
  const algorithm = (value as Record<string, unknown>)["algorithm"];
  const signature = (value as Record<string, unknown>)["signature"];
  if (algorithm !== "p256-sha256" || typeof signature !== "string" || signature.trim() === "") {
    throw new ApiError(400, "invalid_capability_signature", "capability_signature must use p256-sha256");
  }
  return { algorithm, signature };
}

function requireAdminActor(request: Request, env: Env): string {
  const expected = env.REGISTRY_ADMIN_TOKEN;
  if (!expected || expected.trim() === "") {
    throw new ApiError(503, "admin_unconfigured", "REGISTRY_ADMIN_TOKEN must be configured for admin operations");
  }
  const auth = request.headers.get("authorization") ?? "";
  const bearer = auth.match(/^Bearer\s+(.+)$/i)?.[1]?.trim();
  const supplied = bearer || request.headers.get("x-registry-admin-token")?.trim();
  if (supplied !== expected) {
    throw new ApiError(401, "admin_unauthorized", "admin token is missing or invalid");
  }
  const actor = request.headers.get("x-registry-admin-actor")?.trim();
  return actor && actor.length <= 128 ? actor : "registry-admin";
}

function requireNonEmptyAdminString(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new ApiError(400, "invalid_admin_field", `${field} is required`);
  }
  return value.trim();
}

function requireOneOf<const T extends readonly string[]>(value: string, allowed: T, code: string): T[number] {
  if (!allowed.includes(value)) {
    throw new ApiError(400, code, `value must be one of: ${allowed.join(", ")}`);
  }
  return value as T[number];
}

function optionalAuditParam(params: URLSearchParams, key: string): string | undefined {
  const value = params.get(key)?.trim();
  if (!value) {
    return undefined;
  }
  if (value.length > 256) {
    throw new ApiError(400, "invalid_audit_filter", `${key} filter is too long`);
  }
  return value;
}

function auditLimit(params: URLSearchParams): number {
  const raw = params.get("limit")?.trim();
  if (!raw) {
    return 50;
  }
  const value = Number(raw);
  if (!Number.isInteger(value) || value < 1 || value > 200) {
    throw new ApiError(400, "invalid_audit_limit", "audit limit must be an integer from 1 to 200");
  }
  return value;
}

function parseAuditBefore(value: string): string {
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) {
    throw new ApiError(400, "invalid_audit_before", "before must be an ISO timestamp");
  }
  return date.toISOString();
}

function maxJsonBytes(env: Env): number {
  return Number(env.MAX_JSON_BODY_BYTES ?? DEFAULT_MAX_JSON_BODY_BYTES);
}

function maxSnapshotBytes(env: Env): number {
  return Number(env.MAX_SNAPSHOT_BYTES ?? DEFAULT_MAX_SNAPSHOT_BYTES);
}

function quotaEventRetentionHours(env: Env): number {
  const value = Number(env.CLEANUP_QUOTA_EVENT_RETENTION_HOURS ?? DEFAULT_QUOTA_EVENT_RETENTION_HOURS);
  return Number.isFinite(value) && value >= 1 ? value : DEFAULT_QUOTA_EVENT_RETENTION_HOURS;
}

function namespaceClaimCooldownSeconds(env: Env): number {
  const value = Number(env.NAMESPACE_CLAIM_COOLDOWN_SECONDS ?? DEFAULT_NAMESPACE_CLAIM_COOLDOWN_SECONDS);
  return Number.isFinite(value) && value >= 0 ? value : DEFAULT_NAMESPACE_CLAIM_COOLDOWN_SECONDS;
}

async function requestIpHash(request: Request): Promise<string | undefined> {
  const ip = request.headers.get("cf-connecting-ip") ?? request.headers.get("x-forwarded-for");
  return ip ? `sha256:${await sha256Hex(ip)}` : undefined;
}

function requestAsn(request: Request): string | undefined {
  const cf = (request as Request & { cf?: { asn?: number | string } }).cf;
  const asn = cf?.asn ?? request.headers.get("cf-asn");
  return asn === undefined || asn === null || `${asn}`.trim() === "" ? undefined : `${asn}`.trim();
}

function corsHeaders(requestId: string): Headers {
  return new Headers({
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "GET,POST,OPTIONS",
    "access-control-allow-headers": "content-type,authorization,idempotency-key",
    "access-control-expose-headers": "x-request-id,x-idempotency-status",
    "x-request-id": requestId,
  });
}

function json(value: unknown, status: number, headers: Headers): Response {
  const out = new Headers(headers);
  out.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify(value, null, 2), { status, headers: out });
}

function errorResponse(error: unknown, requestId: string): Response {
  const headers = corsHeaders(requestId);
  const status = error instanceof ApiError ? error.status : 500;
  const code = error instanceof ApiError ? error.code : "internal_error";
  const message = error instanceof Error ? error.message : "internal error";
  return json({ request_id: requestId, error: { code, message } }, status, headers);
}

export default createApp();

export { MemoryRegistryStore };
