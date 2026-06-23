import {
  ApiError,
  capabilityKeyId,
  canonicalJson,
  type CapabilityAuthorisationPayload,
  type PublishPayload,
  type RegistryEntryStatus,
} from "./domain";

export type NamespaceStatus = "active" | "review_pending" | "reserved" | "rejected" | "quarantined";

export interface ReservedNamespaceRecord {
  namespace: string;
  match_type: "exact" | "prefix" | "typosquat";
  reason: string;
}

export interface CapabilityRecord {
  key_id: string;
  principal_type: "joyid_ckb";
  principal_id: string;
  capability_pubkey: string;
  scopes: string[];
  expires_at: string;
  revoked_at?: string | null;
  created_at: string;
  last_used_at?: string | null;
}

export interface SnapshotRecord {
  snapshot_hash: string;
  r2_key: string;
  source_hash: string;
  size_bytes: number;
  content_type: string;
}

export interface PackageVersionRecord {
  namespace: string;
  name: string;
  version: string;
  status: RegistryEntryStatus;
  source_hash: string;
  manifest_hash?: string;
  capability_key_id: string;
  principal_type: string;
  principal_id: string;
  registry_entry: Record<string, unknown>;
  snapshot_hash: string;
  direct_url: string;
  created_at: string;
}

export interface IdempotencyRecord {
  key: string;
  request_hash: string;
  request_id: string;
  status: "processing" | "completed";
  response_status?: number;
  response_body?: Record<string, unknown>;
  expires_at: string;
  created_at: string;
  completed_at?: string | null;
}

export interface MaintenanceResult {
  used_nonces_deleted: number;
  idempotency_keys_deleted: number;
  quota_events_deleted: number;
}

export type IdempotencyReservation =
  | { state: "reserved"; record: IdempotencyRecord }
  | { state: "in_progress"; record: IdempotencyRecord }
  | { state: "completed"; record: IdempotencyRecord }
  | { state: "conflict"; record: IdempotencyRecord };

export interface AuditEventInput {
  request_id: string;
  event_type: string;
  principal_type?: string;
  principal_id?: string;
  capability_key_id?: string;
  namespace?: string;
  name?: string;
  version?: string;
  ip_hash?: string;
  user_agent?: string;
  data?: Record<string, unknown>;
}

export interface AuditEventRecord extends AuditEventInput {
  id: string;
  created_at: string;
}

export interface ListAuditEventsInput {
  event_type?: string;
  principal_type?: string;
  principal_id?: string;
  namespace?: string;
  name?: string;
  version?: string;
  before?: string;
  limit: number;
}

export interface NamespaceClaimResult {
  namespace: string;
  status: "active" | "review_pending";
  review_reason?: string;
}

export interface NamespaceRecord {
  namespace: string;
  status: NamespaceStatus;
  review_reason?: string;
  owner_principal_type: "joyid_ckb";
  owner_principal_id: string;
}

export interface RegistryStore {
  recordCapability(input: {
    payload: CapabilityAuthorisationPayload;
    joyid_signature: unknown;
    request_id: string;
  }): Promise<CapabilityRecord>;
  getCapability(keyId: string): Promise<CapabilityRecord | null>;
  revokeCapability(input: {
    key_id: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
    reason?: string;
  }): Promise<CapabilityRecord>;
  getNamespace(namespace: string): Promise<NamespaceRecord | null>;
  claimNamespace(input: {
    namespace: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
  }): Promise<NamespaceClaimResult>;
  upsertReservedNamespace(input: ReservedNamespaceRecord & {
    request_id: string;
    admin_actor: string;
  }): Promise<ReservedNamespaceRecord>;
  updateNamespaceStatus(input: {
    namespace: string;
    status: NamespaceStatus;
    review_reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<NamespaceRecord>;
  ensurePackage(input: {
    namespace: string;
    name: string;
    principal_type: string;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  }): Promise<void>;
  recordSnapshot(input: SnapshotRecord): Promise<void>;
  getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null>;
  recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord>;
  recordCapabilityUsage(input: {
    key_id: string;
    principal_type: string;
    principal_id: string;
    request_id: string;
    action: string;
    namespace?: string;
    name?: string;
    version?: string;
  }): Promise<void>;
  updatePackageVersionStatus(input: {
    namespace: string;
    name: string;
    version: string;
    status: RegistryEntryStatus;
    reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<PackageVersionRecord>;
  appendAuditEvent(event: AuditEventInput): Promise<void>;
  listAuditEvents(input: ListAuditEventsInput): Promise<AuditEventRecord[]>;
  countRecentQuotaEvents(quotaKey: string, bucket: string, sinceIso: string): Promise<number>;
  recordQuotaEvent(quotaKey: string, bucket: string): Promise<void>;
  consumeNonce(input: {
    nonce_key: string;
    protocol: string;
    action: string;
    nonce: string;
    request_id: string;
    expires_at: string;
    principal_type?: string;
    principal_id?: string;
    capability_key_id?: string;
  }): Promise<boolean>;
  reserveIdempotencyKey(input: {
    key: string;
    request_hash: string;
    request_id: string;
    expires_at: string;
  }): Promise<IdempotencyReservation>;
  getIdempotencyKey(key: string): Promise<IdempotencyRecord | null>;
  completeIdempotencyKey(input: {
    key: string;
    request_hash: string;
    response_status: number;
    response_body: Record<string, unknown>;
  }): Promise<IdempotencyRecord>;
  releaseProcessingIdempotencyKey(input: {
    key: string;
    request_hash: string;
  }): Promise<void>;
  cleanupExpiredState(input: {
    now_iso: string;
    quota_events_before_iso: string;
  }): Promise<MaintenanceResult>;
}

const DEFAULT_RESERVED_NAMESPACES: ReservedNamespaceRecord[] = [
  { namespace: "admin", match_type: "exact", reason: "core registry administration namespace" },
  { namespace: "api", match_type: "exact", reason: "production API hostname namespace" },
  { namespace: "cellscript", match_type: "exact", reason: "core CellScript ecosystem namespace" },
  { namespace: "ckb", match_type: "exact", reason: "core CKB ecosystem namespace" },
  { namespace: "joyid", match_type: "exact", reason: "wallet identity provider namespace" },
  { namespace: "nervos", match_type: "exact", reason: "core Nervos ecosystem namespace" },
  { namespace: "official", match_type: "exact", reason: "reserved for official package labels" },
  { namespace: "registry", match_type: "exact", reason: "core registry service namespace" },
  { namespace: "security", match_type: "exact", reason: "reserved for security advisory workflows" },
  { namespace: "support", match_type: "exact", reason: "reserved for support workflows" },
  { namespace: "www", match_type: "exact", reason: "production website hostname namespace" },
];

function nowIso(): string {
  return new Date().toISOString();
}

export class MemoryRegistryStore implements RegistryStore {
  capabilities = new Map<string, CapabilityRecord>();
  namespaces = new Map<string, NamespaceRecord>();
  packageVersions = new Map<string, PackageVersionRecord>();
  snapshots = new Map<string, SnapshotRecord>();
  reservedNamespaces = new Map<string, ReservedNamespaceRecord>(DEFAULT_RESERVED_NAMESPACES.map((record) => [record.namespace, record]));
  auditEvents: AuditEventRecord[] = [];
  quotaEvents: Array<{ quotaKey: string; bucket: string; at: string }> = [];
  usedNonces = new Map<string, {
    protocol: string;
    action: string;
    nonce: string;
    request_id: string;
    expires_at: string;
    principal_type?: string;
    principal_id?: string;
    capability_key_id?: string;
    created_at: string;
  }>();
  idempotencyKeys = new Map<string, IdempotencyRecord>();

  async recordCapability(input: {
    payload: CapabilityAuthorisationPayload;
    joyid_signature: unknown;
    request_id: string;
  }): Promise<CapabilityRecord> {
    const key_id = await capabilityKeyId(input.payload.capability_pubkey);
    const existing = this.capabilities.get(key_id);
    if (existing?.revoked_at) {
      throw new ApiError(409, "capability_key_revoked", "revoked capability keys cannot be reactivated");
    }
    const record: CapabilityRecord = {
      key_id,
      principal_type: input.payload.principal_type,
      principal_id: input.payload.principal_id,
      capability_pubkey: input.payload.capability_pubkey,
      scopes: [...input.payload.requested_scopes],
      expires_at: input.payload.capability_expires_at,
      revoked_at: null,
      created_at: nowIso(),
    };
    this.capabilities.set(key_id, record);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "capability.created",
      principal_type: record.principal_type,
      principal_id: record.principal_id,
      capability_key_id: key_id,
      data: { scopes: record.scopes, payload_hash: await hashForMemory(input.payload), joyid_signature_present: !!input.joyid_signature },
    });
    return record;
  }

  async getCapability(keyId: string): Promise<CapabilityRecord | null> {
    return this.capabilities.get(keyId) ?? null;
  }

  async revokeCapability(input: {
    key_id: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
    reason?: string;
  }): Promise<CapabilityRecord> {
    const existing = this.capabilities.get(input.key_id);
    if (!existing) {
      throw new Error(`capability '${input.key_id}' not found`);
    }
    const revoked_at = nowIso();
    const record = { ...existing, revoked_at };
    this.capabilities.set(input.key_id, record);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "capability.revoked",
      principal_type: input.principal_type,
      principal_id: input.principal_id,
      capability_key_id: input.key_id,
      data: { reason: input.reason ?? null },
    });
    return record;
  }

  async getNamespace(namespace: string): Promise<NamespaceRecord | null> {
    return this.namespaces.get(namespace) ?? null;
  }

  async claimNamespace(input: {
    namespace: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
  }): Promise<NamespaceClaimResult> {
    const existing = this.namespaces.get(input.namespace);
    if (existing) {
      const status = existing.status === "active" ? "active" : "review_pending";
      return existing.review_reason
        ? { namespace: existing.namespace, status, review_reason: existing.review_reason }
        : { namespace: existing.namespace, status };
    }
    const reserved = this.reservedNamespaceFor(input.namespace);
    const review_reason = reserved?.reason ?? (input.namespace.length <= 3 ? "short_namespace_review" : undefined);
    const result: NamespaceClaimResult = {
      namespace: input.namespace,
      status: review_reason ? "review_pending" : "active",
      ...(review_reason ? { review_reason } : {}),
    };
    this.namespaces.set(input.namespace, {
      ...result,
      owner_principal_type: input.principal_type,
      owner_principal_id: input.principal_id,
    });
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "namespace.claimed",
      principal_type: input.principal_type,
      principal_id: input.principal_id,
      namespace: input.namespace,
      data: { status: result.status, review_reason },
    });
    return result;
  }

  async upsertReservedNamespace(input: ReservedNamespaceRecord & {
    request_id: string;
    admin_actor: string;
  }): Promise<ReservedNamespaceRecord> {
    const record: ReservedNamespaceRecord = {
      namespace: input.namespace,
      match_type: input.match_type,
      reason: input.reason,
    };
    this.reservedNamespaces.set(input.namespace, record);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "admin.reserved_namespace.upserted",
      namespace: input.namespace,
      data: { admin_actor: input.admin_actor, match_type: input.match_type, reason: input.reason },
    });
    return record;
  }

  async updateNamespaceStatus(input: {
    namespace: string;
    status: NamespaceStatus;
    review_reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<NamespaceRecord> {
    const existing = this.namespaces.get(input.namespace);
    if (!existing) {
      throw new ApiError(404, "namespace_not_found", "namespace is not known to the registry");
    }
    const updated: NamespaceRecord = {
      ...existing,
      status: input.status,
      ...(input.review_reason ? { review_reason: input.review_reason } : {}),
    };
    if (!input.review_reason) {
      delete updated.review_reason;
    }
    this.namespaces.set(input.namespace, updated);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "admin.namespace.status_updated",
      principal_type: updated.owner_principal_type,
      principal_id: updated.owner_principal_id,
      namespace: input.namespace,
      data: { admin_actor: input.admin_actor, status: input.status, review_reason: input.review_reason ?? null },
    });
    return updated;
  }

  async ensurePackage(input: {
    namespace: string;
    name: string;
    principal_type: string;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  }): Promise<void> {
    if (!this.namespaces.has(input.namespace)) {
      this.namespaces.set(input.namespace, {
        namespace: input.namespace,
        status: "active",
        owner_principal_type: input.principal_type as "joyid_ckb",
        owner_principal_id: input.principal_id,
      });
    }
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "package.ensure",
      principal_type: input.principal_type,
      principal_id: input.principal_id,
      namespace: input.namespace,
      name: input.name,
      data: { source_repo: input.source_repo },
    });
  }

  async recordSnapshot(input: SnapshotRecord): Promise<void> {
    this.snapshots.set(input.snapshot_hash, input);
  }

  async getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null> {
    return this.packageVersions.get(`${namespace}/${name}@${version}`) ?? null;
  }

  async recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord> {
    const key = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(key);
    if (existing) {
      throw new ApiError(409, "package_version_exists", "package version already exists and cannot be overwritten");
    }
    this.packageVersions.set(key, input);
    return input;
  }

  async recordCapabilityUsage(input: {
    key_id: string;
    principal_type: string;
    principal_id: string;
    request_id: string;
    action: string;
    namespace?: string;
    name?: string;
    version?: string;
  }): Promise<void> {
    const existing = this.capabilities.get(input.key_id);
    if (existing) {
      this.capabilities.set(input.key_id, { ...existing, last_used_at: nowIso() });
    }
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "capability.used",
      principal_type: input.principal_type,
      principal_id: input.principal_id,
      capability_key_id: input.key_id,
      ...(input.namespace ? { namespace: input.namespace } : {}),
      ...(input.name ? { name: input.name } : {}),
      ...(input.version ? { version: input.version } : {}),
      data: { action: input.action },
    });
  }

  async updatePackageVersionStatus(input: {
    namespace: string;
    name: string;
    version: string;
    status: RegistryEntryStatus;
    reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<PackageVersionRecord> {
    const key = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(key);
    if (!existing) {
      throw new ApiError(404, "package_version_not_found", "package version is not known to the registry");
    }
    const updated = { ...existing, status: input.status };
    this.packageVersions.set(key, updated);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "admin.package_version.status_updated",
      principal_type: existing.principal_type,
      principal_id: existing.principal_id,
      capability_key_id: existing.capability_key_id,
      namespace: input.namespace,
      name: input.name,
      version: input.version,
      data: { admin_actor: input.admin_actor, status: input.status, reason: input.reason ?? null },
    });
    return updated;
  }

  async appendAuditEvent(event: AuditEventInput): Promise<void> {
    this.auditEvents.push({
      id: `memory-audit-${this.auditEvents.length + 1}`,
      created_at: nowIso(),
      ...event,
    });
  }

  async listAuditEvents(input: ListAuditEventsInput): Promise<AuditEventRecord[]> {
    const before = input.before ? Date.parse(input.before) : Number.POSITIVE_INFINITY;
    return this.auditEvents
      .filter((event) => Date.parse(event.created_at) < before)
      .filter((event) => !input.event_type || event.event_type === input.event_type)
      .filter((event) => !input.principal_type || event.principal_type === input.principal_type)
      .filter((event) => !input.principal_id || event.principal_id === input.principal_id)
      .filter((event) => !input.namespace || event.namespace === input.namespace)
      .filter((event) => !input.name || event.name === input.name)
      .filter((event) => !input.version || event.version === input.version)
      .slice()
      .reverse()
      .slice(0, input.limit);
  }

  async countRecentQuotaEvents(quotaKey: string, bucket: string, sinceIso: string): Promise<number> {
    const since = Date.parse(sinceIso);
    return this.quotaEvents.filter((event) => event.quotaKey === quotaKey && event.bucket === bucket && Date.parse(event.at) >= since).length;
  }

  async recordQuotaEvent(quotaKey: string, bucket: string): Promise<void> {
    this.quotaEvents.push({ quotaKey, bucket, at: nowIso() });
  }

  async consumeNonce(input: {
    nonce_key: string;
    protocol: string;
    action: string;
    nonce: string;
    request_id: string;
    expires_at: string;
    principal_type?: string;
    principal_id?: string;
    capability_key_id?: string;
  }): Promise<boolean> {
    if (this.usedNonces.has(input.nonce_key)) {
      return false;
    }
    const record = {
      protocol: input.protocol,
      action: input.action,
      nonce: input.nonce,
      request_id: input.request_id,
      expires_at: input.expires_at,
      created_at: nowIso(),
      ...(input.principal_type ? { principal_type: input.principal_type } : {}),
      ...(input.principal_id ? { principal_id: input.principal_id } : {}),
      ...(input.capability_key_id ? { capability_key_id: input.capability_key_id } : {}),
    };
    this.usedNonces.set(input.nonce_key, record);
    return true;
  }

  async reserveIdempotencyKey(input: {
    key: string;
    request_hash: string;
    request_id: string;
    expires_at: string;
  }): Promise<IdempotencyReservation> {
    const existing = this.idempotencyKeys.get(input.key);
    if (existing) {
      if (existing.request_hash !== input.request_hash) {
        return { state: "conflict", record: existing };
      }
      if (existing.status === "completed") {
        return { state: "completed", record: existing };
      }
      return { state: "in_progress", record: existing };
    }
    const record: IdempotencyRecord = {
      key: input.key,
      request_hash: input.request_hash,
      request_id: input.request_id,
      status: "processing",
      expires_at: input.expires_at,
      created_at: nowIso(),
      completed_at: null,
    };
    this.idempotencyKeys.set(input.key, record);
    return { state: "reserved", record };
  }

  async getIdempotencyKey(key: string): Promise<IdempotencyRecord | null> {
    return this.idempotencyKeys.get(key) ?? null;
  }

  async completeIdempotencyKey(input: {
    key: string;
    request_hash: string;
    response_status: number;
    response_body: Record<string, unknown>;
  }): Promise<IdempotencyRecord> {
    const existing = this.idempotencyKeys.get(input.key);
    if (!existing || existing.request_hash !== input.request_hash) {
      throw new ApiError(409, "idempotency_key_conflict", "idempotency key is reserved for another request");
    }
    const completed: IdempotencyRecord = {
      ...existing,
      status: "completed",
      response_status: input.response_status,
      response_body: input.response_body,
      completed_at: nowIso(),
    };
    this.idempotencyKeys.set(input.key, completed);
    return completed;
  }

  async releaseProcessingIdempotencyKey(input: {
    key: string;
    request_hash: string;
  }): Promise<void> {
    const existing = this.idempotencyKeys.get(input.key);
    if (existing?.status === "processing" && existing.request_hash === input.request_hash) {
      this.idempotencyKeys.delete(input.key);
    }
  }

  async cleanupExpiredState(input: {
    now_iso: string;
    quota_events_before_iso: string;
  }): Promise<MaintenanceResult> {
    const now = Date.parse(input.now_iso);
    const quotaCutoff = Date.parse(input.quota_events_before_iso);
    let usedNoncesDeleted = 0;
    let idempotencyKeysDeleted = 0;

    for (const [key, record] of this.usedNonces.entries()) {
      if (Date.parse(record.expires_at) < now) {
        this.usedNonces.delete(key);
        usedNoncesDeleted += 1;
      }
    }
    for (const [key, record] of this.idempotencyKeys.entries()) {
      if (Date.parse(record.expires_at) < now) {
        this.idempotencyKeys.delete(key);
        idempotencyKeysDeleted += 1;
      }
    }
    const quotaBefore = this.quotaEvents.length;
    this.quotaEvents = this.quotaEvents.filter((event) => Date.parse(event.at) >= quotaCutoff);

    return {
      used_nonces_deleted: usedNoncesDeleted,
      idempotency_keys_deleted: idempotencyKeysDeleted,
      quota_events_deleted: quotaBefore - this.quotaEvents.length,
    };
  }

  private reservedNamespaceFor(namespace: string): ReservedNamespaceRecord | undefined {
    for (const record of this.reservedNamespaces.values()) {
      if (record.match_type === "prefix" && namespace.startsWith(record.namespace)) {
        return record;
      }
      if ((record.match_type === "exact" || record.match_type === "typosquat") && namespace === record.namespace) {
        return record;
      }
    }
    return undefined;
  }
}

async function hashForMemory(value: unknown): Promise<string> {
  const { sha256Hex } = await import("./domain");
  return sha256Hex(canonicalJson(value));
}
