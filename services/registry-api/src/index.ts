import { verifySignature, type SignChallengeResponseData } from "@joyid/ckb";
import {
  ARTIFACT_KINDS,
  CKB_SECP256K1_PRINCIPAL_TYPE,
  JOYID_PRINCIPAL_TYPE,
  ApiError,
  DEFAULT_REGISTRY_ORIGIN,
  DEFAULT_STATIC_REGISTRY_ORIGIN,
  REGISTRY_SCHEMA_VERSION,
  WebCryptoP256Verifier,
  assertPlainObject,
  base64ToBytes,
  canonicalJson,
  capabilityKeyId,
  ckbScriptHash,
  initialArtifactStates,
  isPrincipalType,
  scopeAllowsPublish,
  sha256Hex,
  sameCkbHash,
  validateCapabilityPayload,
  validateCapabilityRevocationPayload,
  validateDeploymentPayload,
  validatePackageIdent,
  validatePublishPayload,
  validateSnapshot,
  validateVersion,
  verifyPrincipalAuthorisationPayload,
  verifyPrincipalPayloadSignature,
  type CapabilitySignature,
  type CapabilitySignatureVerifier,
  type CkbSecp256k1Signature,
  type JoyidVerifier,
  type PrincipalSignature,
  type PrincipalType,
  type ArtifactKind,
  type AvailabilityStatus,
  type DeploymentStatus,
  type DeploymentPayload,
  type SourceSnapshotInput,
  type VerificationStatus,
} from "./domain";
import {
  MemoryRegistryStore,
  type IdempotencyRecord,
  type PackageEvidenceKind,
  type PackageEvidenceRecord,
  type PackageVersionRecord,
  type RegistryStore,
  type SnapshotRecord,
} from "./store";
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
  CKB_MAINNET_RPC_URL?: string;
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
  readinessCheck?: () => Promise<Record<string, string>>;
  verifyMainnetDeployment?: (payload: DeploymentPayload) => Promise<{ block_hash?: string | null }>;
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
  const staticPackageVersionMatch = url.pathname.match(/^\/artifacts\/([^/]+)\/([^/]+)\/releases\/([^/]+)[.]json$/);
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

  if (request.method === "GET" && url.pathname === "/v1/artifacts") {
    return handleListPackages(request, store, requestId, staticOrigin, headers);
  }

  const publicEvidenceMatch = url.pathname.match(/^\/v1\/artifacts\/([^/]+)\/([^/]+)\/releases\/([^/]+)\/evidence$/);
  if (request.method === "GET" && publicEvidenceMatch) {
    return handlePublicPackageEvidence(
      store,
      requestId,
      headers,
      decodeURIComponent(publicEvidenceMatch[1] ?? ""),
      decodeURIComponent(publicEvidenceMatch[2] ?? ""),
      decodeURIComponent(publicEvidenceMatch[3] ?? ""),
    );
  }

  const deploymentMatch = url.pathname.match(/^\/v1\/artifacts\/([^/]+)\/([^/]+)\/releases\/([^/]+)\/deployments$/);
  if (request.method === "POST" && deploymentMatch) {
    return handleRecordDeployment(
      request,
      env,
      store,
      requestId,
      registryOrigin,
      staticOrigin,
      now,
      deps,
      headers,
      decodeURIComponent(deploymentMatch[1] ?? ""),
      decodeURIComponent(deploymentMatch[2] ?? ""),
      decodeURIComponent(deploymentMatch[3] ?? ""),
    );
  }

  const publicPackageMatch = url.pathname.match(/^\/v1\/artifacts\/([^/]+)\/([^/]+)$/);
  if (request.method === "GET" && publicPackageMatch) {
    return handlePublicPackageDetail(
      store,
      requestId,
      staticOrigin,
      headers,
      decodeURIComponent(publicPackageMatch[1] ?? ""),
      decodeURIComponent(publicPackageMatch[2] ?? ""),
    );
  }

  if (request.method === "POST" && url.pathname === "/v1/capabilities") {
    return handleCreateCapability(request, env, store, requestId, registryOrigin, now, deps, headers);
  }

  if (request.method === "POST" && url.pathname === "/v1/admin/reserved-namespaces") {
    return handleAdminReservedNamespace(request, env, store, requestId, headers);
  }

  if (request.method === "GET" && url.pathname === "/v1/admin/audit-events") {
    return handleAdminAuditEvents(request, env, store, requestId, headers);
  }

  if (request.method === "GET" && url.pathname === "/v1/admin/verification-queue") {
    return handleAdminVerificationQueue(request, env, store, requestId, headers);
  }

  const adminVerificationRetryMatch = url.pathname.match(/^\/v1\/admin\/verification-jobs\/([^/]+)\/retry$/);
  if (request.method === "POST" && adminVerificationRetryMatch) {
    return handleAdminVerificationRetry(
      request,
      env,
      store,
      requestId,
      headers,
      decodeURIComponent(adminVerificationRetryMatch[1] ?? ""),
    );
  }

  const adminNamespaceStatusMatch = url.pathname.match(/^\/v1\/admin\/namespaces\/([^/]+)\/status$/);
  if (request.method === "POST" && adminNamespaceStatusMatch) {
    return handleAdminNamespaceStatus(request, env, store, requestId, headers, decodeURIComponent(adminNamespaceStatusMatch[1] ?? ""));
  }

  const adminVersionStatusMatch = url.pathname.match(/^\/v1\/admin\/artifacts\/([^/]+)\/([^/]+)\/releases\/([^/]+)\/availability$/);
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

  const adminPromotionMatch = url.pathname.match(/^\/v1\/admin\/artifacts\/([^/]+)\/([^/]+)\/releases\/([^/]+)\/promote$/);
  if (request.method === "POST" && adminPromotionMatch) {
    return handleAdminPackageVersionPromotion(
      request,
      env,
      store,
      requestId,
      staticOrigin,
      deps,
      headers,
      decodeURIComponent(adminPromotionMatch[1] ?? ""),
      decodeURIComponent(adminPromotionMatch[2] ?? ""),
      decodeURIComponent(adminPromotionMatch[3] ?? ""),
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

  const publishMatch = url.pathname.match(/^\/v1\/artifacts\/([^/]+)\/([^/]+)\/releases$/);
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
    throw new ApiError(404, "registry_object_not_found", "artifact release registry object was not found");
  }
  const headers = corsHeaders(requestId);
  headers.set("content-type", object.contentType ?? "application/json; charset=utf-8");
  headers.set("cache-control", "public, max-age=60, stale-while-revalidate=300");
  if (object.etag) {
    headers.set("etag", object.etag);
  }
  return new Response(object.body, { status: 200, headers });
}

async function handleListPackages(
  request: Request,
  store: RegistryStore,
  requestId: string,
  staticOrigin: string,
  headers: Headers,
): Promise<Response> {
  const params = new URL(request.url).searchParams;
  const query = optionalPublicQuery(params, "q");
  const namespaceRaw = optionalPublicQuery(params, "namespace");
  const kindRaw = optionalPublicQuery(params, "kind");
  const verificationRaw = optionalPublicQuery(params, "verification");
  const deploymentRaw = optionalPublicQuery(params, "deployment");
  const availabilityRaw = optionalPublicQuery(params, "availability");
  const namespace = namespaceRaw ? validatePackageIdent(namespaceRaw, "namespace") : undefined;
  const artifactKind = kindRaw ? requireOneOf(kindRaw, ARTIFACT_KINDS, "invalid_artifact_kind") as ArtifactKind : undefined;
  const verificationStatus = verificationRaw
    ? requireOneOf(verificationRaw, ["pending", "verified", "evidence_required", "rejected"] as const, "invalid_verification_status") as VerificationStatus
    : undefined;
  const deploymentStatus = deploymentRaw
    ? requireOneOf(deploymentRaw, ["not_applicable", "undeployed", "deployed", "chain_verified"] as const, "invalid_deployment_status") as DeploymentStatus
    : undefined;
  const availabilityStatus = availabilityRaw
    ? requireOneOf(availabilityRaw, ["active", "deprecated", "yanked", "quarantined"] as const, "invalid_availability_status") as AvailabilityStatus
    : "active";
  const limit = publicListInteger(params, "limit", 50, 1, 100);
  const offset = publicListInteger(params, "offset", 0, 0, 10_000);
  const records = await store.listPackageVersions({
    ...(query ? { query } : {}),
    ...(namespace ? { namespace } : {}),
    ...(artifactKind ? { artifact_kind: artifactKind } : {}),
    ...(verificationStatus ? { verification_status: verificationStatus } : {}),
    ...(deploymentStatus ? { deployment_status: deploymentStatus } : {}),
    ...(availabilityStatus ? { availability_status: availabilityStatus } : {}),
    limit: Math.min(limit * 4, 400),
    offset,
  });
  const visible = records.filter((record) => record.availability_status !== "quarantined");
  const grouped = new Map<string, PackageVersionRecord[]>();
  for (const record of visible) {
    const key = `${record.namespace}/${record.name}`;
    const versions = grouped.get(key) ?? [];
    versions.push(record);
    grouped.set(key, versions);
  }
  const snapshots = await requireSnapshots(store, visible);
  const packages = [...grouped.entries()].slice(0, limit).map(([coordinate, versions]) => {
    const latest = versions[0]!;
    const entry = latest.registry_entry as Record<string, unknown>;
    return {
      coordinate,
      namespace: latest.namespace,
      name: latest.name,
      latest_release: latest.version,
      artifact: latest.artifact,
      verification_status: latest.verification_status,
      deployment_status: latest.deployment_status,
      availability_status: latest.availability_status,
      description: typeof entry["description"] === "string" ? entry["description"] : null,
      repository: typeof entry["repository"] === "string" ? entry["repository"] : null,
      keywords: Array.isArray(entry["keywords"]) ? entry["keywords"] : [],
      categories: Array.isArray(entry["categories"]) ? entry["categories"] : [],
      releases: versions.map((version) => staticRegistryVersionPayload(version, snapshotForVersion(snapshots, version), staticOrigin)),
      updated_at: latest.created_at,
    };
  });
  return json(
    {
      schema: "cellscript-registry-artifact-index",
      request_id: requestId,
      artifacts: packages,
      count: packages.length,
      offset,
      limit,
      ...(records.length >= Math.min(limit * 4, 400) ? { next_offset: offset + records.length } : {}),
    },
    200,
    headers,
  );
}

async function handlePublicPackageDetail(
  store: RegistryStore,
  requestId: string,
  staticOrigin: string,
  headers: Headers,
  namespaceFromPath: string,
  nameFromPath: string,
): Promise<Response> {
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const versions = await store.listPackageVersions({ namespace, name, limit: 200, offset: 0 });
  const visible = versions.filter((version) => version.availability_status !== "quarantined");
  if (visible.length === 0) {
    throw new ApiError(404, "artifact_not_found", "artifact is not known to the public registry");
  }
  const snapshots = await requireSnapshots(store, visible);
  const evidenceByVersion = new Map<string, PackageEvidenceRecord[]>();
  for (const evidence of await store.listPackageEvidenceForPackage(namespace, name)) {
    const records = evidenceByVersion.get(evidence.version) ?? [];
    records.push(evidence);
    evidenceByVersion.set(evidence.version, records);
  }
  const payloads = visible.map((version) => staticRegistryVersionPayload(
    version,
    snapshotForVersion(snapshots, version),
    staticOrigin,
    evidenceByVersion.get(version.version) ?? [],
  ));
  const latest = visible[0]!;
  const entry = latest.registry_entry as Record<string, unknown>;
  return json(
    {
      schema: "cellscript-registry-artifact",
      request_id: requestId,
      coordinate: `${namespace}/${name}`,
      namespace,
      name,
      description: typeof entry["description"] === "string" ? entry["description"] : null,
      repository: typeof entry["repository"] === "string" ? entry["repository"] : null,
      homepage: typeof entry["homepage"] === "string" ? entry["homepage"] : null,
      documentation: typeof entry["documentation"] === "string" ? entry["documentation"] : null,
      keywords: Array.isArray(entry["keywords"]) ? entry["keywords"] : [],
      categories: Array.isArray(entry["categories"]) ? entry["categories"] : [],
      latest_release: latest.version,
      artifact: latest.artifact,
      verification_status: latest.verification_status,
      deployment_status: latest.deployment_status,
      availability_status: latest.availability_status,
      releases: payloads,
    },
    200,
    headers,
  );
}

async function handlePublicPackageEvidence(
  store: RegistryStore,
  requestId: string,
  headers: Headers,
  namespaceFromPath: string,
  nameFromPath: string,
  versionFromPath: string,
): Promise<Response> {
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const version = validateVersion(versionFromPath);
  const record = await store.getPackageVersion(namespace, name, version);
  if (!record || record.availability_status === "quarantined") {
    throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the public registry");
  }
  const evidence = await store.listPackageEvidence(namespace, name, version);
  return json({ schema: "cellscript-registry-evidence-list", request_id: requestId, namespace, name, release: version, evidence }, 200, headers);
}

async function handleRecordDeployment(
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
  releaseFromPath: string,
): Promise<Response> {
  await throttleRequestSource(store, request, requestId, "deployment", 40, 60 * 60, now);
  const body = await readJson(request, Math.min(maxJsonBytes(env), 512 * 1024));
  const payload = validateDeploymentPayload(body["payload"], registryOrigin, now);
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const release = validateVersion(releaseFromPath);
  if (payload.namespace !== namespace || payload.name !== name || payload.release !== release) {
    throw new ApiError(400, "route_payload_mismatch", "artifact route and deployment payload do not match");
  }
  const version = await store.getPackageVersion(namespace, name, release);
  if (!version) {
    throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
  }
  if (version.artifact.profile !== "ckb_executable" || version.deployment_status === "not_applicable") {
    throw new ApiError(409, "deployment_not_applicable", "this artifact profile cannot have a CKB deployment");
  }
  const signedRelease = version.registry_entry.versions.find((entry) => entry.version === release);
  if (!signedRelease?.artifact_hash || !sameCkbHash(signedRelease.artifact_hash, payload.artifact_hash)) {
    throw new ApiError(400, "deployment_artifact_mismatch", "deployment artifact_hash does not match the published release");
  }
  const capability = await store.getCapability(payload.capability_key_id);
  if (!capability || capability.revoked_at || new Date(capability.expires_at).getTime() <= now.getTime()) {
    throw new ApiError(401, "capability_inactive", "deployment capability is missing, revoked, or expired");
  }
  if (!scopeAllowsPublish(capability.scopes, namespace, name)) {
    throw new ApiError(403, "capability_scope_denied", "capability scope does not allow this artifact deployment");
  }
  const namespaceRecord = await store.getNamespace(namespace);
  if (
    !namespaceRecord
    || namespaceRecord.status !== "active"
    || namespaceRecord.owner_principal_type !== capability.principal_type
    || namespaceRecord.owner_principal_id !== capability.principal_id
  ) {
    throw new ApiError(403, "namespace_owner_mismatch", "capability principal does not own the active namespace");
  }
  const signature = requireCapabilitySignature(body["capability_signature"]);
  const verifier = deps.capabilityVerifier ?? new WebCryptoP256Verifier();
  if (!(await verifier.verify(canonicalJson(payload), capability.capability_pubkey, signature))) {
    throw new ApiError(401, "capability_signature_invalid", "capability signature verification failed");
  }

  const nonceKey = await consumeSignedNonce(store, requestId, {
    protocol: payload.protocol,
    action: payload.action,
    nonce: payload.nonce,
    expires_at: payload.expires_at,
    principal_type: capability.principal_type,
    principal_id: capability.principal_id,
    capability_key_id: capability.key_id,
  });
  try {
    const chain = deps.verifyMainnetDeployment
      ? await deps.verifyMainnetDeployment(payload)
      : await verifyMainnetDeployment(env, payload);
    const previousEvidence = await store.listPackageEvidence(namespace, name, release);
    const buildEvidence = previousEvidence.filter((item) => item.kind === "verified_build").at(-1);
    if (!buildEvidence) {
      throw new ApiError(409, "evidence_dependency_missing", "build verification evidence must exist before deployment");
    }
    const evidence = {
      schema: "cellscript-registry-evidence",
      kind: "deployed",
      producer: `publisher:${capability.principal_type}`,
      generated_at: now.toISOString(),
      verification_status: "passed",
      source_hash: version.source_hash,
      manifest_hash: version.manifest_hash,
      verified_build_evidence_hash: buildEvidence.evidence_hash,
      network: "mainnet",
      artifact_hash: payload.artifact_hash,
      data_hash: payload.data_hash,
      code_hash: payload.code_hash,
      hash_type: payload.hash_type,
      dep_type: payload.dep_type,
      out_point: payload.out_point,
      deployment_status: "live",
      chain_verification: "get_live_cell",
      ...(chain.block_hash ? { block_hash: chain.block_hash } : {}),
    };
    const evidenceHash = `sha256:${await sha256Hex(canonicalJson(evidence))}`;
    const predictedEvidence: PackageEvidenceRecord = {
      namespace,
      name,
      version: release,
      kind: "deployed",
      evidence_hash: evidenceHash,
      evidence,
      request_id: requestId,
      admin_actor: `publisher:${capability.principal_id}`,
      created_at: now.toISOString(),
    };
    const snapshot = await requireSnapshot(store, version);
    await writeStaticRegistryVersionObject(
      env,
      deps,
      { ...version, status: "deployed", deployment_status: "chain_verified" },
      snapshot,
      staticOrigin,
      [...previousEvidence, predictedEvidence],
    );
    const recorded = await store.recordChainVerifiedDeployment({
      namespace,
      name,
      version: release,
      kind: "deployed",
      evidence_hash: evidenceHash,
      evidence,
      request_id: requestId,
      admin_actor: `publisher:${capability.principal_id}`,
    });
    await store.recordCapabilityUsage({
      key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      request_id: requestId,
      action: "record_deployment",
      namespace,
      name,
      version: release,
    });
    return json({
      request_id: requestId,
      coordinate: `${namespace}/${name}@${release}`,
      deployment_status: recorded.version.deployment_status,
      evidence: recorded.evidence,
    }, 201, headers);
  } catch (error) {
    await store.releaseNonce({ nonce_key: nonceKey, request_id: requestId });
    throw error;
  }
}

async function verifyMainnetDeployment(env: Env, payload: DeploymentPayload): Promise<{ block_hash?: string | null }> {
  const rpcUrl = env.CKB_MAINNET_RPC_URL?.trim() || "https://mainnet.ckb.dev/rpc";
  let response: Response;
  try {
    response = await fetch(rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({
        id: 1,
        jsonrpc: "2.0",
        method: "get_live_cell",
        params: [{ tx_hash: payload.out_point.tx_hash, index: `0x${payload.out_point.index.toString(16)}` }, true, false],
      }),
    });
  } catch (error) {
    throw new ApiError(503, "ckb_rpc_unavailable", `mainnet CKB RPC request failed: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!response.ok) {
    throw new ApiError(503, "ckb_rpc_unavailable", `mainnet CKB RPC returned HTTP ${response.status}`);
  }
  const rpc = assertPlainObject(await response.json(), "invalid_ckb_rpc_response");
  if (rpc["error"]) {
    throw new ApiError(503, "ckb_rpc_error", "mainnet CKB RPC rejected get_live_cell");
  }
  const result = assertPlainObject(rpc["result"], "invalid_ckb_rpc_response");
  if (result["status"] !== "live") {
    throw new ApiError(409, "deployment_cell_not_live", "deployment OutPoint is not a live mainnet Cell");
  }
  const cell = assertPlainObject(result["cell"], "invalid_ckb_rpc_response");
  const data = assertPlainObject(cell["data"], "invalid_ckb_rpc_response");
  if (typeof data["hash"] !== "string" || !sameCkbHash(data["hash"], payload.data_hash)) {
    throw new ApiError(409, "deployment_data_hash_mismatch", "live Cell data hash does not match the published executable");
  }
  if (payload.hash_type === "type") {
    const output = assertPlainObject(cell["output"], "invalid_ckb_rpc_response");
    if (!output["type"] || !sameCkbHash(ckbScriptHash(output["type"]), payload.code_hash)) {
      throw new ApiError(409, "deployment_code_hash_mismatch", "live Cell type script hash does not match code_hash");
    }
  } else if (!sameCkbHash(payload.code_hash, payload.data_hash)) {
    throw new ApiError(400, "deployment_code_hash_mismatch", "data hash deployments must use the executable data hash as code_hash");
  }
  return { block_hash: typeof result["block_hash"] === "string" ? result["block_hash"] : null };
}

async function handleReadiness(env: Env, deps: AppDeps, requestId: string, headers: Headers): Promise<Response> {
  const storeConfigured = !!deps.store || !!env.HYPERDRIVE;
  const objectStoreConfigured =
    (!!deps.snapshotWriter && !!deps.registryObjectReader)
    || !!env.REGISTRY_OBJECTS
    || !!env.SOURCE_SNAPSHOTS;
  const adminConfigured = typeof env.REGISTRY_ADMIN_TOKEN === "string" && env.REGISTRY_ADMIN_TOKEN.trim() !== "";
  const checks: Record<string, string> = {
    store: storeConfigured ? "configured" : "missing_hyperdrive",
    object_store: objectStoreConfigured ? "configured" : "missing_r2",
    admin_token: adminConfigured ? "configured" : "missing_secret",
  };
  let dependenciesHealthy = true;
  const store = optionalStore(env, deps);
  if (store) {
    try {
      await store.healthCheck();
      checks["store"] = "ready";
    } catch {
      checks["store"] = "unreachable";
      dependenciesHealthy = false;
    }
  }
  if (deps.readinessCheck) {
    try {
      Object.assign(checks, await deps.readinessCheck());
    } catch {
      checks["runtime"] = "unready";
      dependenciesHealthy = false;
    }
  }
  const ready = storeConfigured && objectStoreConfigured && adminConfigured && dependenciesHealthy;
  return json(
    {
      status: ready ? "ready" : "not_ready",
      request_id: requestId,
      checks,
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
  if (principalType && !isPrincipalType(principalType)) {
    throw new ApiError(400, "invalid_audit_filter", "principal_type filter is unsupported");
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

async function handleAdminVerificationQueue(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  headers: Headers,
): Promise<Response> {
  requireAdminActor(request, env);
  const metrics = await store.getVerificationQueueMetrics();
  return json(
    {
      schema: "cellscript-registry-verification-queue-v1",
      request_id: requestId,
      ...metrics,
    },
    200,
    headers,
  );
}

async function handleAdminVerificationRetry(
  request: Request,
  env: Env,
  store: RegistryStore,
  requestId: string,
  headers: Headers,
  jobIdFromPath: string,
): Promise<Response> {
  const adminActor = requireAdminActor(request, env);
  const jobId = requireUuid(jobIdFromPath, "verification_job_id");
  const job = await store.retryVerificationJob({ job_id: jobId, request_id: requestId, admin_actor: adminActor });
  return json({ request_id: requestId, job }, 200, headers);
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
    String(body["availability_status"] ?? ""),
    ["active", "deprecated", "yanked", "quarantined"],
    "invalid_availability_status",
  );
  const reason = typeof body["reason"] === "string" && body["reason"].trim() !== "" ? body["reason"].trim() : undefined;
  const directUrl = staticPackageVersionUrl(staticOrigin, namespace, name, version);
  const existing = await store.getPackageVersion(namespace, name, version);
  if (!existing) {
    throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
  }
  const snapshot = await requireSnapshot(store, existing);
  const evidence = await store.listPackageEvidence(namespace, name, version);
  if (isSuppressivePackageVersionStatus(status)) {
    await writeStaticRegistryVersionObject(
      env,
      deps,
      { ...existing, status: status === "active" ? existing.status : status, availability_status: status, direct_url: directUrl },
      snapshot,
      staticOrigin,
      evidence,
    );
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
    await writeStaticRegistryVersionObject(env, deps, { ...record, direct_url: directUrl }, snapshot, staticOrigin, evidence);
  }
  return json({ request_id: requestId, ...record }, 200, headers);
}

async function handleAdminPackageVersionPromotion(
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
  const namespace = validatePackageIdent(namespaceFromPath, "namespace");
  const name = validatePackageIdent(nameFromPath, "name");
  const version = validateVersion(versionFromPath);
  const body = await readJson(request, Math.min(maxJsonBytes(env), 512 * 1024));
  const kind = requireOneOf(
    String(body["kind"] ?? ""),
    ["verified_build", "deployed", "on_chain_attested"],
    "invalid_evidence_kind",
  ) as PackageEvidenceKind;
  const existing = await store.getPackageVersion(namespace, name, version);
  if (!existing) {
    throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
  }
  const previousEvidence = await store.listPackageEvidence(namespace, name, version);
  const evidence = validatePromotionEvidence(body["evidence"], kind, existing, previousEvidence);
  const evidenceHash = `sha256:${await sha256Hex(canonicalJson(evidence))}`;
  const promoted = await store.promotePackageVersion({
    namespace,
    name,
    version,
    kind,
    evidence_hash: evidenceHash,
    evidence,
    request_id: requestId,
    admin_actor: adminActor,
  });
  const allEvidence = await store.listPackageEvidence(namespace, name, version);
  const snapshot = await requireSnapshot(store, promoted.version);
  await writeStaticRegistryVersionObject(
    env,
    deps,
    { ...promoted.version, direct_url: staticPackageVersionUrl(staticOrigin, namespace, name, version) },
    snapshot,
    staticOrigin,
    allEvidence,
  );
  return json(
    {
      request_id: requestId,
      namespace,
      name,
      version,
      status: promoted.version.status,
      evidence: promoted.evidence,
    },
    200,
    headers,
  );
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
  const signature = requirePrincipalSignature(body, payload.principal_type);
  await verifyPrincipalAuthorisationPayload(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier());
  await throttle(store, requestId, `principal:${payload.principal_type}:${payload.principal_id}`, "capability", 8, 60 * 60, now);
  const nonceKey = await consumeSignedNonce(store, requestId, {
    protocol: payload.protocol,
    action: `${payload.action}:capability_create`,
    nonce: payload.nonce,
    expires_at: payload.expires_at,
    principal_type: payload.principal_type,
    principal_id: payload.principal_id,
  });
  let capability;
  try {
    capability = await store.recordCapability({ payload, principal_signature: signature, request_id: requestId });
  } catch (error) {
    await store.releaseNonce({ nonce_key: nonceKey, request_id: requestId });
    throw error;
  }
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
  const signature = requirePrincipalSignature(body, payload.principal_type);
  if (!payload.requested_scopes.some((scope) => scope.startsWith(`publish:${namespace}/`))) {
    throw new ApiError(403, "namespace_scope_missing", "namespace claim requires a publish scope for that namespace");
  }
  await verifyPrincipalAuthorisationPayload(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier());
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
    principal_type: payload.principal_type,
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
    throw new ApiError(403, "capability_owner_mismatch", "wallet principal does not own this capability");
  }
  const signature = requirePrincipalSignature(body, payload.principal_type);
  await verifyPrincipalPayloadSignature(payload, signature, deps.joyidVerifier ?? productionJoyidVerifier());
  await throttle(store, requestId, `principal:${payload.principal_type}:${payload.principal_id}`, "capability_revoke", 8, 60 * 60, now);
  const nonceKey = await consumeSignedNonce(store, requestId, {
    protocol: payload.protocol,
    action: payload.action,
    nonce: payload.nonce,
    expires_at: payload.expires_at,
    principal_type: payload.principal_type,
    principal_id: payload.principal_id,
    capability_key_id: capability.key_id,
  });
  const reason = typeof body["reason"] === "string" ? body["reason"] : undefined;
  let revoked;
  try {
    revoked = await store.revokeCapability({
      key_id: capability.key_id,
      principal_type: payload.principal_type,
      principal_id: payload.principal_id,
      request_id: requestId,
      ...(reason ? { reason } : {}),
    });
  } catch (error) {
    await store.releaseNonce({ nonce_key: nonceKey, request_id: requestId });
    throw error;
  }
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
    throw new ApiError(403, "capability_scope_denied", "capability scope does not allow this artifact publish");
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
  await throttle(store, requestId, `artifact:${payload.namespace}/${payload.name}`, "publish", 12, 60 * 60, now);
  if (await store.getPackageVersion(payload.namespace, payload.name, payload.version)) {
    throw new ApiError(409, "artifact_release_exists", "artifact release already exists and cannot be overwritten");
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

  let consumedNonceKey: string | undefined;
  try {
    consumedNonceKey = await consumeSignedNonce(store, requestId, {
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
    const packageInput = {
      namespace: payload.namespace,
      name: payload.name,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      ...(sourceRepo ? { source_repo: sourceRepo } : {}),
      request_id: requestId,
    };
    const directUrl = staticPackageVersionUrl(staticOrigin, payload.namespace, payload.name, payload.version);
    const publishedRegistryVersion = payload.registry_entry.versions[0];
    const states = initialArtifactStates(payload.artifact);
    const versionInput = {
      namespace: payload.namespace,
      name: payload.name,
      version: payload.version,
      status: "source_published",
      artifact: payload.artifact,
      ...states,
      source_hash: payload.source_hash,
      manifest_hash: payload.manifest_hash,
      ...(publishedRegistryVersion.edition ? { edition: publishedRegistryVersion.edition } : {}),
      ...(publishedRegistryVersion.compatibility_profile_hash
        ? { compatibility_profile_hash: publishedRegistryVersion.compatibility_profile_hash }
        : {}),
      capability_key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      registry_entry: payload.registry_entry,
      snapshot_hash: snapshotRecord.snapshot_hash,
      direct_url: directUrl,
      created_at: now.toISOString(),
    } as const;
    await writeStaticRegistryVersionObject(env, deps, versionInput, snapshotRecord, staticOrigin);
    const capabilityUsage = {
      key_id: capability.key_id,
      principal_type: capability.principal_type,
      principal_id: capability.principal_id,
      request_id: requestId,
      action: "publish",
      namespace: payload.namespace,
      name: payload.name,
      version: payload.version,
    };
    const ipHash = await requestIpHash(request);
    const userAgent = request.headers.get("user-agent") ?? undefined;
    const auditEvent = {
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
      data: { artifact: payload.artifact, ...states, snapshot_hash: snapshotRecord.snapshot_hash, direct_url: directUrl },
    };
    const responseBody = {
      request_id: requestId,
      artifact: payload.artifact,
      ...states,
      direct_url: directUrl,
      snapshot_hash: snapshotRecord.snapshot_hash,
      verification: "queued",
    };
    await store.admitPackageVersion({
      package: packageInput,
      snapshot: snapshotRecord,
      version: versionInput,
      capability_usage: capabilityUsage,
      audit_event: auditEvent,
      ...(idempotencyKey
        ? {
            idempotency: {
              key: idempotencyKey,
              request_hash: requestHash,
              response_status: 202,
              response_body: responseBody,
            },
          }
        : {}),
    });
    return json(responseBody, 202, headers);
  } catch (error) {
    if (consumedNonceKey) {
      await store.releaseNonce({ nonce_key: consumedNonceKey, request_id: requestId });
    }
    if (idempotencyKey && idempotencyReserved) {
      await store.releaseProcessingIdempotencyKey({ key: idempotencyKey, request_hash: requestHash });
    }
    throw error;
  }
}

async function publishRequestHash(payload: unknown, signature: CapabilitySignature, snapshot: SourceSnapshotInput): Promise<string> {
  return sha256Hex(canonicalJson({
    route: "publish_artifact_release",
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
): Promise<string> {
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
  return nonceKey;
}

async function writeStaticRegistryVersionObject(
  env: Env,
  deps: AppDeps,
  version: SnapshotPackageVersionRecord,
  snapshot: SnapshotRecord,
  staticOrigin: string,
  evidence: PackageEvidenceRecord[] = [],
): Promise<void> {
  const key = staticPackageVersionKey(version.namespace, version.name, version.version);
  const body = new TextEncoder().encode(`${JSON.stringify(staticRegistryVersionPayload(version, snapshot, staticOrigin, evidence), null, 2)}\n`);
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

export async function syncStaticRegistryVersionObject(
  env: Env,
  deps: Pick<AppDeps, "snapshotWriter">,
  store: RegistryStore,
  version: PackageVersionRecord,
  staticOrigin: string,
): Promise<void> {
  const snapshot = await requireSnapshot(store, version);
  const evidence = await store.listPackageEvidence(version.namespace, version.name, version.version);
  await writeStaticRegistryVersionObject(
    env,
    deps,
    { ...version, direct_url: staticPackageVersionUrl(staticOrigin, version.namespace, version.name, version.version) },
    snapshot,
    staticOrigin,
    evidence,
  );
}

type SnapshotPackageVersionRecord = Awaited<ReturnType<RegistryStore["recordPackageVersion"]>>;

function staticRegistryVersionPayload(
  version: SnapshotPackageVersionRecord,
  snapshot: SnapshotRecord,
  staticOrigin: string,
  evidence: PackageEvidenceRecord[] = [],
): Record<string, unknown> {
  return {
    schema_version: REGISTRY_SCHEMA_VERSION,
    kind: "cellscript.registry.artifact_release",
    coordinate: `${version.namespace}/${version.name}@${version.version}`,
    namespace: version.namespace,
    name: version.name,
    release: version.version,
    artifact: version.artifact,
    verification_status: version.verification_status,
    deployment_status: version.deployment_status,
    availability_status: version.availability_status,
    source_hash: version.source_hash,
    manifest_hash: version.manifest_hash,
    ...(version.edition ? { edition: version.edition } : {}),
    ...(version.compatibility_profile_hash ? { compatibility_profile_hash: version.compatibility_profile_hash } : {}),
    capability_key_id: version.capability_key_id,
    principal_type: version.principal_type,
    principal_id: version.principal_id,
    registry_entry: version.registry_entry,
    snapshot_hash: version.snapshot_hash,
    immutable_bundle: sourceSnapshotPayload(snapshot, staticOrigin),
    direct_url: version.direct_url,
    created_at: version.created_at,
    evidence,
  };
}

async function requireSnapshot(store: RegistryStore, version: SnapshotPackageVersionRecord): Promise<SnapshotRecord> {
  const snapshot = await store.getSnapshot(version.snapshot_hash);
  if (!snapshot || snapshot.source_hash !== version.source_hash) {
    throw new ApiError(503, "source_snapshot_unavailable", "package source snapshot metadata is unavailable or inconsistent");
  }
  return snapshot;
}

async function requireSnapshots(
  store: RegistryStore,
  versions: SnapshotPackageVersionRecord[],
): Promise<Map<string, SnapshotRecord>> {
  const snapshots = await store.getSnapshots(versions.map((version) => version.snapshot_hash));
  for (const version of versions) snapshotForVersion(snapshots, version);
  return snapshots;
}

function snapshotForVersion(
  snapshots: Map<string, SnapshotRecord>,
  version: SnapshotPackageVersionRecord,
): SnapshotRecord {
  const snapshot = snapshots.get(version.snapshot_hash);
  if (!snapshot || snapshot.source_hash !== version.source_hash) {
    throw new ApiError(503, "source_snapshot_unavailable", "package source snapshot metadata is unavailable or inconsistent");
  }
  return snapshot;
}

function sourceSnapshotPayload(snapshot: SnapshotRecord, staticOrigin: string): Record<string, unknown> {
  return {
    schema: "cellscript-registry-immutable-bundle",
    url: `${staticOrigin.replace(/\/+$/, "")}/${snapshot.r2_key}`,
    snapshot_hash: snapshot.snapshot_hash,
    source_hash: snapshot.source_hash,
    size_bytes: snapshot.size_bytes,
    content_type: snapshot.content_type,
  };
}

function staticPackageVersionKey(namespace: string, name: string, version: string): string {
  return `artifacts/${namespace}/${name}/releases/${version}.json`;
}

function staticPackageVersionUrl(staticOrigin: string, namespace: string, name: string, version: string): string {
  return `${staticOrigin.replace(/\/+$/, "")}/artifacts/${encodeURIComponent(namespace)}/${encodeURIComponent(name)}/releases/${encodeURIComponent(version)}.json`;
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

function requirePrincipalSignature(body: Record<string, unknown>, principalType: PrincipalType): PrincipalSignature {
  const value = body["wallet_signature"] ?? body["joyid_signature"];
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ApiError(400, "missing_wallet_signature", "wallet_signature is required");
  }
  if (principalType === JOYID_PRINCIPAL_TYPE) {
    return value as SignChallengeResponseData;
  }
  if (principalType !== CKB_SECP256K1_PRINCIPAL_TYPE) {
    throw new ApiError(400, "unsupported_principal_type", "wallet principal type is unsupported");
  }
  const signature = value as Record<string, unknown>;
  if (
    signature["scheme"] !== CKB_SECP256K1_PRINCIPAL_TYPE
    || typeof signature["challenge"] !== "string"
    || typeof signature["signature"] !== "string"
    || typeof signature["public_key"] !== "string"
  ) {
    throw new ApiError(
      400,
      "invalid_wallet_signature",
      "ckb_secp256k1 wallet_signature must include scheme, challenge, signature, and public_key",
    );
  }
  return signature as unknown as CkbSecp256k1Signature;
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

function requireUuid(value: string, field: string): string {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value)) {
    throw new ApiError(400, `invalid_${field}`, `${field} must be a UUID`);
  }
  return value.toLowerCase();
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

function optionalPublicQuery(params: URLSearchParams, name: string): string | undefined {
  const value = params.get(name)?.trim();
  if (!value) return undefined;
  if (value.length > 160 || /[\u0000-\u001f\u007f]/.test(value)) {
    throw new ApiError(400, "invalid_public_query", `${name} query parameter is invalid`);
  }
  return value;
}

function publicListInteger(params: URLSearchParams, name: string, fallback: number, minimum: number, maximum: number): number {
  const value = params.get(name);
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new ApiError(400, "invalid_public_query", `${name} must be an integer between ${minimum} and ${maximum}`);
  }
  return parsed;
}

export function validatePromotionEvidence(
  value: unknown,
  kind: PackageEvidenceKind,
  version: PackageVersionRecord,
  previous: PackageEvidenceRecord[],
): Record<string, unknown> {
  const evidence = assertPlainObject(value, "invalid_promotion_evidence");
  if (evidence["schema"] !== "cellscript-registry-evidence") {
    throw new ApiError(400, "invalid_evidence_schema", "evidence.schema must be cellscript-registry-evidence");
  }
  if (evidence["kind"] !== kind) {
    throw new ApiError(400, "evidence_kind_mismatch", "evidence.kind must match the requested promotion kind");
  }
  requireEvidenceString(evidence, "producer", 1, 200);
  requireEvidenceTimestamp(evidence, "generated_at");
  if (evidence["verification_status"] !== "passed") {
    throw new ApiError(400, "evidence_not_passed", "evidence.verification_status must be passed");
  }
  requireMatchingEvidenceHash(evidence, "source_hash", version.source_hash);
  requireMatchingEvidenceHash(evidence, "manifest_hash", version.manifest_hash);
  if (version.compatibility_profile_hash) {
    requireMatchingEvidenceHash(evidence, "compatibility_profile_hash", version.compatibility_profile_hash);
  }

  if (kind === "verified_build") {
    const level = requireEvidenceString(evidence, "verification_level", 1, 80);
    if (!(["compiled", "hash_bound", "evidence_required"] as const).includes(level as any)) {
      throw new ApiError(400, "invalid_verification_level", "verification_level is not recognised");
    }
    if (version.artifact.profile !== "copy_material") requireEvidenceHash(evidence, "artifact_hash");
    requireEvidenceHash(evidence, "metadata_hash");
    if (version.artifact.profile === "cellscript_source") requireEvidenceString(evidence, "compiler_version", 1, 80);
  } else if (kind === "deployed") {
    const verified = latestEvidence(previous, "verified_build");
    requireEvidenceReference(evidence, "verified_build_evidence_hash", verified);
    const artifactHash = requireEvidenceHash(evidence, "artifact_hash");
    const verifiedArtifact = requireEvidenceHash(verified.evidence, "artifact_hash");
    if (!sameHash(artifactHash, verifiedArtifact)) {
      throw new ApiError(400, "deployment_artifact_mismatch", "deployed artifact_hash must match verified-build evidence");
    }
    if (requireEvidenceString(evidence, "network", 1, 80) !== "mainnet") {
      throw new ApiError(400, "unsupported_deployment_network", "Registry deployment evidence is mainnet-only");
    }
    requireEvidenceHash(evidence, "code_hash");
    requireEvidenceHash(evidence, "data_hash");
    const outPoint = assertPlainObject(evidence["out_point"], "invalid_deployment_out_point");
    requireEvidenceHash(outPoint, "tx_hash");
    const index = outPoint["index"];
    if (!Number.isSafeInteger(index) || Number(index) < 0 || Number(index) > 0xffff_ffff) {
      throw new ApiError(400, "invalid_deployment_out_point", "evidence.out_point.index must be a non-negative u32 integer");
    }
    if (evidence["deployment_status"] !== "live") {
      throw new ApiError(400, "deployment_not_live", "evidence.deployment_status must be live");
    }
  } else {
    const deployed = latestEvidence(previous, "deployed");
    requireEvidenceReference(evidence, "deployed_evidence_hash", deployed);
    requireEvidenceString(evidence, "network", 1, 80);
    requireEvidenceHash(evidence, "attestation_tx_hash");
    requireEvidenceHash(evidence, "attestation_hash");
    requireEvidenceString(evidence, "attestor", 1, 200);
    requireEvidenceTimestamp(evidence, "observed_at");
    if (evidence["attestation_status"] !== "confirmed") {
      throw new ApiError(400, "attestation_not_confirmed", "evidence.attestation_status must be confirmed");
    }
  }
  return evidence;
}

function latestEvidence(records: PackageEvidenceRecord[], kind: PackageEvidenceKind): PackageEvidenceRecord {
  const record = records.filter((item) => item.kind === kind).at(-1);
  if (!record) {
    throw new ApiError(409, "evidence_dependency_missing", `${kind} evidence must exist before this promotion`);
  }
  return record;
}

function requireEvidenceReference(evidence: Record<string, unknown>, key: string, expected: PackageEvidenceRecord): void {
  const value = requireEvidenceString(evidence, key, 71, 71);
  if (value !== expected.evidence_hash) {
    throw new ApiError(400, "evidence_reference_mismatch", `${key} does not reference the accepted ${expected.kind} evidence`);
  }
}

function requireMatchingEvidenceHash(evidence: Record<string, unknown>, key: string, expected: string): void {
  const value = requireEvidenceHash(evidence, key);
  if (!sameHash(value, expected)) {
    throw new ApiError(400, "evidence_identity_mismatch", `evidence.${key} does not match the published package identity`);
  }
}

function requireEvidenceHash(evidence: Record<string, unknown>, key: string): string {
  const value = requireEvidenceString(evidence, key, 64, 66);
  if (!/^(?:0x)?[0-9a-fA-F]{64}$/.test(value)) {
    throw new ApiError(400, "invalid_evidence_hash", `evidence.${key} must be a 32-byte hex hash`);
  }
  return value;
}

function sameHash(left: string, right: string): boolean {
  return left.replace(/^0x/i, "").toLowerCase() === right.replace(/^0x/i, "").toLowerCase();
}

function requireEvidenceString(
  evidence: Record<string, unknown>,
  key: string,
  minimumLength: number,
  maximumLength: number,
): string {
  const value = evidence[key];
  if (typeof value !== "string" || value.trim() !== value || value.length < minimumLength || value.length > maximumLength) {
    throw new ApiError(400, "invalid_evidence_field", `evidence.${key} is invalid`);
  }
  return value;
}

function requireEvidenceTimestamp(evidence: Record<string, unknown>, key: string): string {
  const value = requireEvidenceString(evidence, key, 20, 40);
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp) || timestamp > Date.now() + 5 * 60 * 1000) {
    throw new ApiError(400, "invalid_evidence_timestamp", `evidence.${key} must be a non-future ISO timestamp`);
  }
  return new Date(timestamp).toISOString();
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
    "access-control-allow-headers": "content-type,authorization,idempotency-key,x-registry-admin-token,x-registry-admin-actor",
    "access-control-expose-headers": "x-request-id,x-idempotency-status",
    "cache-control": "no-store",
    "content-security-policy": "default-src 'none'; base-uri 'none'; frame-ancestors 'none'",
    "permissions-policy": "camera=(), geolocation=(), microphone=()",
    "referrer-policy": "no-referrer",
    "strict-transport-security": "max-age=31536000",
    "x-content-type-options": "nosniff",
    "x-frame-options": "DENY",
    "x-permitted-cross-domain-policies": "none",
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
