import {
  ApiError,
  capabilityKeyId,
  canonicalJson,
  type ArtifactDescriptor,
  type ArtifactKind,
  type AvailabilityStatus,
  type CapabilityAuthorisationPayload,
  type DeploymentStatus,
  type PrincipalType,
  type PublishPayload,
  type RegistryEntryStatus,
  type RegistryIndexEntry,
  type VerificationStatus,
} from "./domain";

export type NamespaceStatus = "active" | "review_pending" | "reserved" | "rejected" | "quarantined";

export interface ReservedNamespaceRecord {
  namespace: string;
  match_type: "exact" | "prefix" | "typosquat";
  reason: string;
}

export interface CapabilityRecord {
  key_id: string;
  principal_type: PrincipalType;
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
  artifact: ArtifactDescriptor;
  verification_status: VerificationStatus;
  deployment_status: DeploymentStatus;
  availability_status: AvailabilityStatus;
  source_hash: string;
  manifest_hash: string;
  /** Source-language semantics, not a compiler or wire-ABI version. */
  edition?: "2026";
  /** Complete resolved compatibility identity across independent axes. */
  compatibility_profile_hash?: string;
  capability_key_id: string;
  principal_type: PrincipalType;
  principal_id: string;
  registry_entry: RegistryIndexEntry;
  snapshot_hash: string;
  direct_url: string;
  created_at: string;
}

export interface PackageVersionQuery {
  query?: string;
  namespace?: string;
  name?: string;
  artifact_kind?: ArtifactKind;
  verification_status?: VerificationStatus;
  deployment_status?: DeploymentStatus;
  availability_status?: AvailabilityStatus;
  status?: RegistryEntryStatus;
  statuses?: RegistryEntryStatus[];
  limit: number;
  offset: number;
}

export type PackageEvidenceKind = "verified_build" | "deployed" | "on_chain_attested";

export interface PackageEvidenceRecord {
  namespace: string;
  name: string;
  version: string;
  kind: PackageEvidenceKind;
  evidence_hash: string;
  evidence: Record<string, unknown>;
  request_id: string;
  admin_actor: string;
  created_at: string;
}

export interface PromotePackageVersionInput {
  namespace: string;
  name: string;
  version: string;
  kind: PackageEvidenceKind;
  evidence_hash: string;
  evidence: Record<string, unknown>;
  request_id: string;
  admin_actor: string;
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

export type VerificationJobStatus =
  | "queued"
  | "running"
  | "publishing"
  | "retry_wait"
  | "succeeded"
  | "dead_letter";

export interface VerificationJobRecord {
  id: string;
  namespace: string;
  name: string;
  version: string;
  status: VerificationJobStatus;
  attempt_count: number;
  max_attempts: number;
  available_at: string;
  lease_owner?: string | null;
  lease_expires_at?: string | null;
  evidence_hash?: string | null;
  evidence?: Record<string, unknown> | null;
  last_error_code?: string | null;
  last_error_message?: string | null;
  created_at: string;
  updated_at: string;
  started_at?: string | null;
  completed_at?: string | null;
  source_hash: string;
  manifest_hash: string;
  artifact: ArtifactDescriptor;
  compatibility_profile_hash?: string;
  snapshot_hash: string;
  snapshot_object_key: string;
  snapshot_size_bytes: number;
  snapshot_content_type: string;
}

export interface VerificationQueueMetrics {
  counts: Record<VerificationJobStatus, number>;
  oldest_available_at?: string | null;
  oldest_dead_letter_at?: string | null;
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

export interface PublishAdmissionInput {
  package: {
    namespace: string;
    name: string;
    principal_type: PrincipalType;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  };
  snapshot: SnapshotRecord;
  version: PackageVersionRecord;
  capability_usage: {
    key_id: string;
    principal_type: PrincipalType;
    principal_id: string;
    request_id: string;
    action: string;
    namespace?: string;
    name?: string;
    version?: string;
  };
  audit_event: AuditEventInput;
  idempotency?: {
    key: string;
    request_hash: string;
    response_status: number;
    response_body: Record<string, unknown>;
  };
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
  owner_principal_type: PrincipalType;
  owner_principal_id: string;
}

export interface RegistryStore {
  healthCheck(): Promise<void>;
  recordCapability(input: {
    payload: CapabilityAuthorisationPayload;
    principal_signature: unknown;
    request_id: string;
  }): Promise<CapabilityRecord>;
  getCapability(keyId: string): Promise<CapabilityRecord | null>;
  revokeCapability(input: {
    key_id: string;
    principal_type: PrincipalType;
    principal_id: string;
    request_id: string;
    reason?: string;
  }): Promise<CapabilityRecord>;
  getNamespace(namespace: string): Promise<NamespaceRecord | null>;
  claimNamespace(input: {
    namespace: string;
    principal_type: PrincipalType;
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
    principal_type: PrincipalType;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  }): Promise<void>;
  recordSnapshot(input: SnapshotRecord): Promise<void>;
  getSnapshot(snapshotHash: string): Promise<SnapshotRecord | null>;
  getSnapshots(snapshotHashes: string[]): Promise<Map<string, SnapshotRecord>>;
  getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null>;
  listPackageVersions(input: PackageVersionQuery): Promise<PackageVersionRecord[]>;
  recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord>;
  admitPackageVersion(input: PublishAdmissionInput): Promise<PackageVersionRecord>;
  listPackageEvidence(namespace: string, name: string, version: string): Promise<PackageEvidenceRecord[]>;
  listPackageEvidenceForPackage(namespace: string, name: string): Promise<PackageEvidenceRecord[]>;
  promotePackageVersion(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }>;
  recordChainVerifiedDeployment(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }>;
  recordCapabilityUsage(input: {
    key_id: string;
    principal_type: PrincipalType;
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
    status: AvailabilityStatus;
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
  releaseNonce(input: {
    nonce_key: string;
    request_id: string;
  }): Promise<void>;
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
  claimVerificationJob(input: {
    worker_id: string;
    lease_seconds: number;
    now_iso: string;
  }): Promise<VerificationJobRecord | null>;
  promoteVerifiedBuildForJob(input: {
    job_id: string;
    worker_id: string;
    evidence_hash: string;
    evidence: Record<string, unknown>;
    request_id: string;
    admin_actor: string;
  }): Promise<{
    job: VerificationJobRecord;
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }>;
  completeVerificationJob(input: {
    job_id: string;
    worker_id: string;
  }): Promise<VerificationJobRecord>;
  failVerificationJob(input: {
    job_id: string;
    worker_id: string;
    error_code: string;
    error_message: string;
    retryable: boolean;
    retry_after_seconds: number;
    request_id: string;
  }): Promise<VerificationJobRecord>;
  retryVerificationJob(input: {
    job_id: string;
    request_id: string;
    admin_actor: string;
  }): Promise<VerificationJobRecord>;
  getVerificationQueueMetrics(): Promise<VerificationQueueMetrics>;
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
  packageEvidence = new Map<string, PackageEvidenceRecord>();
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
  verificationJobs = new Map<string, VerificationJobRecord>();

  async healthCheck(): Promise<void> {}

  async recordCapability(input: {
    payload: CapabilityAuthorisationPayload;
    principal_signature: unknown;
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
      data: { scopes: record.scopes, payload_hash: await hashForMemory(input.payload), principal_signature_present: !!input.principal_signature },
    });
    return record;
  }

  async getCapability(keyId: string): Promise<CapabilityRecord | null> {
    return this.capabilities.get(keyId) ?? null;
  }

  async revokeCapability(input: {
    key_id: string;
    principal_type: PrincipalType;
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
    principal_type: PrincipalType;
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
    principal_type: PrincipalType;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  }): Promise<void> {
    if (!this.namespaces.has(input.namespace)) {
      this.namespaces.set(input.namespace, {
        namespace: input.namespace,
        status: "active",
        owner_principal_type: input.principal_type,
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

  async getSnapshot(snapshotHash: string): Promise<SnapshotRecord | null> {
    return this.snapshots.get(snapshotHash) ?? null;
  }

  async getSnapshots(snapshotHashes: string[]): Promise<Map<string, SnapshotRecord>> {
    const records = new Map<string, SnapshotRecord>();
    for (const hash of new Set(snapshotHashes)) {
      const snapshot = this.snapshots.get(hash);
      if (snapshot) records.set(hash, snapshot);
    }
    return records;
  }

  async getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null> {
    return this.packageVersions.get(`${namespace}/${name}@${version}`) ?? null;
  }

  async listPackageVersions(input: PackageVersionQuery): Promise<PackageVersionRecord[]> {
    const query = input.query?.toLowerCase();
    return [...this.packageVersions.values()]
      .filter((record) => !input.namespace || record.namespace === input.namespace)
      .filter((record) => !input.name || record.name === input.name)
      .filter((record) => !input.artifact_kind || record.artifact.kind === input.artifact_kind)
      .filter((record) => !input.verification_status || record.verification_status === input.verification_status)
      .filter((record) => !input.deployment_status || record.deployment_status === input.deployment_status)
      .filter((record) => !input.availability_status || record.availability_status === input.availability_status)
      .filter((record) => !input.status || record.status === input.status)
      .filter((record) => !input.statuses || input.statuses.includes(record.status))
      .filter((record) => {
        if (!query) return true;
        return `${record.namespace}/${record.name}@${record.version} ${JSON.stringify(record.registry_entry)}`
          .toLowerCase()
          .includes(query);
      })
      .sort((left, right) => right.created_at.localeCompare(left.created_at))
      .slice(input.offset, input.offset + input.limit);
  }

  async recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord> {
    const key = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(key);
    if (existing) {
      throw new ApiError(409, "artifact_release_exists", "artifact release already exists and cannot be overwritten");
    }
    this.packageVersions.set(key, input);
    return input;
  }

  async admitPackageVersion(input: PublishAdmissionInput): Promise<PackageVersionRecord> {
    const versionKey = `${input.version.namespace}/${input.version.name}@${input.version.version}`;
    if (this.packageVersions.has(versionKey)) {
      throw new ApiError(409, "artifact_release_exists", "artifact release already exists and cannot be overwritten");
    }
    if (input.idempotency) {
      const reservation = this.idempotencyKeys.get(input.idempotency.key);
      if (reservation?.status !== "processing" || reservation.request_hash !== input.idempotency.request_hash) {
        throw new ApiError(409, "idempotency_key_conflict", "idempotency key is reserved for another request");
      }
    }

    await this.ensurePackage(input.package);
    await this.recordSnapshot(input.snapshot);
    await this.recordPackageVersion(input.version);
    this.enqueueVerificationJob(input.version, input.snapshot);
    await this.recordCapabilityUsage(input.capability_usage);
    await this.appendAuditEvent(input.audit_event);
    if (input.idempotency) {
      await this.completeIdempotencyKey(input.idempotency);
    }
    return input.version;
  }

  async listPackageEvidence(namespace: string, name: string, version: string): Promise<PackageEvidenceRecord[]> {
    const prefix = `${namespace}/${name}@${version}:`;
    return [...this.packageEvidence.entries()]
      .filter(([key]) => key.startsWith(prefix))
      .map(([, record]) => record)
      .sort((left, right) => left.created_at.localeCompare(right.created_at));
  }

  async listPackageEvidenceForPackage(namespace: string, name: string): Promise<PackageEvidenceRecord[]> {
    const prefix = `${namespace}/${name}@`;
    return [...this.packageEvidence.entries()]
      .filter(([key]) => key.startsWith(prefix))
      .map(([, record]) => record)
      .sort((left, right) => left.created_at.localeCompare(right.created_at));
  }

  async promotePackageVersion(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }> {
    const versionKey = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(versionKey);
    if (!existing) {
      throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
    }
    assertPromotionTransition(existing.status, input.kind);
    const evidenceKey = `${versionKey}:${input.kind}:${input.evidence_hash}`;
    const prior = this.packageEvidence.get(evidenceKey);
    const evidence: PackageEvidenceRecord = prior ?? {
      namespace: input.namespace,
      name: input.name,
      version: input.version,
      kind: input.kind,
      evidence_hash: input.evidence_hash,
      evidence: input.evidence,
      request_id: input.request_id,
      admin_actor: input.admin_actor,
      created_at: nowIso(),
    };
    this.packageEvidence.set(evidenceKey, evidence);
    const versionRecord: PackageVersionRecord = {
      ...existing,
      status: input.kind,
      verification_status: verificationStatusForAcceptedEvidence(existing.verification_status, input.kind, input.evidence),
      deployment_status: input.kind === "on_chain_attested"
        ? "chain_verified"
        : input.kind === "deployed"
          ? "deployed"
          : existing.deployment_status,
    };
    this.packageVersions.set(versionKey, versionRecord);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: `evidence.${input.kind}.accepted`,
      principal_type: existing.principal_type,
      principal_id: existing.principal_id,
      capability_key_id: existing.capability_key_id,
      namespace: input.namespace,
      name: input.name,
      version: input.version,
      data: { admin_actor: input.admin_actor, evidence_hash: input.evidence_hash },
    });
    return { version: versionRecord, evidence };
  }

  async recordChainVerifiedDeployment(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }> {
    if (input.kind !== "deployed") {
      throw new ApiError(500, "invalid_deployment_evidence_kind", "chain-verified deployment evidence must use kind deployed");
    }
    const versionKey = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(versionKey);
    if (!existing) {
      throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
    }
    if (existing.deployment_status === "not_applicable") {
      throw new ApiError(409, "deployment_not_applicable", "this artifact profile cannot have a CKB deployment");
    }
    if (!(existing.verification_status === "verified" || existing.verification_status === "hash_bound" || existing.verification_status === "evidence_required")) {
      throw new ApiError(409, "artifact_not_verified", "artifact verification must finish before recording a deployment");
    }
    const evidenceKey = `${versionKey}:${input.kind}:${input.evidence_hash}`;
    const evidence: PackageEvidenceRecord = this.packageEvidence.get(evidenceKey) ?? {
      namespace: input.namespace,
      name: input.name,
      version: input.version,
      kind: input.kind,
      evidence_hash: input.evidence_hash,
      evidence: input.evidence,
      request_id: input.request_id,
      admin_actor: input.admin_actor,
      created_at: nowIso(),
    };
    this.packageEvidence.set(evidenceKey, evidence);
    const versionRecord: PackageVersionRecord = {
      ...existing,
      status: "deployed",
      deployment_status: "chain_verified",
    };
    this.packageVersions.set(versionKey, versionRecord);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "deployment.chain_verified",
      principal_type: existing.principal_type,
      principal_id: existing.principal_id,
      capability_key_id: existing.capability_key_id,
      namespace: input.namespace,
      name: input.name,
      version: input.version,
      data: { actor: input.admin_actor, evidence_hash: input.evidence_hash },
    });
    return { version: versionRecord, evidence };
  }

  async recordCapabilityUsage(input: {
    key_id: string;
    principal_type: PrincipalType;
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
    status: AvailabilityStatus;
    reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<PackageVersionRecord> {
    const key = `${input.namespace}/${input.name}@${input.version}`;
    const existing = this.packageVersions.get(key);
    if (!existing) {
      throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
    }
    const restoredStatus: RegistryEntryStatus = existing.deployment_status === "chain_verified"
      ? "on_chain_attested"
      : existing.deployment_status === "deployed"
        ? "deployed"
        : existing.verification_status === "verified" || existing.verification_status === "hash_bound" || existing.verification_status === "evidence_required"
          ? "verified_build"
          : "source_published";
    const updated: PackageVersionRecord = {
      ...existing,
      status: input.status === "active" ? restoredStatus : input.status,
      availability_status: input.status,
    };
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

  async releaseNonce(input: { nonce_key: string; request_id: string }): Promise<void> {
    const existing = this.usedNonces.get(input.nonce_key);
    if (existing?.request_id === input.request_id) {
      this.usedNonces.delete(input.nonce_key);
    }
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

  async claimVerificationJob(input: {
    worker_id: string;
    lease_seconds: number;
    now_iso: string;
  }): Promise<VerificationJobRecord | null> {
    const now = Date.parse(input.now_iso);
    const candidate = [...this.verificationJobs.values()]
      .filter((job) => {
        if ((job.status === "queued" || job.status === "retry_wait") && Date.parse(job.available_at) <= now) return true;
        if ((job.status === "running" || job.status === "publishing") && job.lease_expires_at) {
          return Date.parse(job.lease_expires_at) <= now;
        }
        return false;
      })
      .sort((left, right) => left.available_at.localeCompare(right.available_at) || left.created_at.localeCompare(right.created_at))[0];
    if (!candidate) return null;

    const hasEvidence = !!candidate.evidence_hash && !!candidate.evidence;
    const claimed: VerificationJobRecord = {
      ...candidate,
      status: hasEvidence ? "publishing" : "running",
      attempt_count: candidate.attempt_count + 1,
      lease_owner: input.worker_id,
      lease_expires_at: new Date(now + input.lease_seconds * 1_000).toISOString(),
      started_at: candidate.started_at ?? input.now_iso,
      updated_at: input.now_iso,
    };
    this.verificationJobs.set(claimed.id, claimed);
    return claimed;
  }

  async promoteVerifiedBuildForJob(input: {
    job_id: string;
    worker_id: string;
    evidence_hash: string;
    evidence: Record<string, unknown>;
    request_id: string;
    admin_actor: string;
  }): Promise<{ job: VerificationJobRecord; version: PackageVersionRecord; evidence: PackageEvidenceRecord }> {
    const job = this.requireOwnedVerificationJob(input.job_id, input.worker_id, "running");
    const promoted = await this.promotePackageVersion({
      namespace: job.namespace,
      name: job.name,
      version: job.version,
      kind: "verified_build",
      evidence_hash: input.evidence_hash,
      evidence: input.evidence,
      request_id: input.request_id,
      admin_actor: input.admin_actor,
    });
    const updated: VerificationJobRecord = {
      ...job,
      status: "publishing",
      evidence_hash: input.evidence_hash,
      evidence: input.evidence,
      updated_at: nowIso(),
    };
    this.verificationJobs.set(job.id, updated);
    return { job: updated, ...promoted };
  }

  async completeVerificationJob(input: { job_id: string; worker_id: string }): Promise<VerificationJobRecord> {
    const job = this.requireOwnedVerificationJob(input.job_id, input.worker_id, "publishing");
    const completedAt = nowIso();
    const completed: VerificationJobRecord = {
      ...job,
      status: "succeeded",
      lease_owner: null,
      lease_expires_at: null,
      completed_at: completedAt,
      updated_at: completedAt,
      last_error_code: null,
      last_error_message: null,
    };
    this.verificationJobs.set(job.id, completed);
    await this.appendAuditEvent({
      request_id: `verification:${job.id}`,
      event_type: "verification.succeeded",
      namespace: job.namespace,
      name: job.name,
      version: job.version,
      data: { job_id: job.id, attempt_count: job.attempt_count, evidence_hash: job.evidence_hash },
    });
    return completed;
  }

  async failVerificationJob(input: {
    job_id: string;
    worker_id: string;
    error_code: string;
    error_message: string;
    retryable: boolean;
    retry_after_seconds: number;
    request_id: string;
  }): Promise<VerificationJobRecord> {
    const job = this.requireOwnedVerificationJob(input.job_id, input.worker_id);
    const retry = input.retryable && job.attempt_count < job.max_attempts;
    const now = new Date();
    const failed: VerificationJobRecord = {
      ...job,
      status: retry ? "retry_wait" : "dead_letter",
      available_at: new Date(now.getTime() + (retry ? input.retry_after_seconds : 0) * 1_000).toISOString(),
      lease_owner: null,
      lease_expires_at: null,
      last_error_code: input.error_code,
      last_error_message: input.error_message,
      updated_at: now.toISOString(),
    };
    this.verificationJobs.set(job.id, failed);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: retry ? "verification.retry_scheduled" : "verification.dead_lettered",
      namespace: job.namespace,
      name: job.name,
      version: job.version,
      data: {
        job_id: job.id,
        attempt_count: job.attempt_count,
        error_code: input.error_code,
        retry_after_seconds: retry ? input.retry_after_seconds : null,
      },
    });
    return failed;
  }

  async retryVerificationJob(input: {
    job_id: string;
    request_id: string;
    admin_actor: string;
  }): Promise<VerificationJobRecord> {
    const job = this.verificationJobs.get(input.job_id);
    if (!job) throw new ApiError(404, "verification_job_not_found", "verification job was not found");
    if (job.status !== "dead_letter") {
      throw new ApiError(409, "verification_job_not_dead_letter", "only dead-letter verification jobs can be retried manually");
    }
    const now = nowIso();
    const retried: VerificationJobRecord = {
      ...job,
      status: "queued",
      attempt_count: 0,
      available_at: now,
      lease_owner: null,
      lease_expires_at: null,
      last_error_code: null,
      last_error_message: null,
      updated_at: now,
    };
    this.verificationJobs.set(job.id, retried);
    await this.appendAuditEvent({
      request_id: input.request_id,
      event_type: "verification.requeued",
      namespace: job.namespace,
      name: job.name,
      version: job.version,
      data: { job_id: job.id, admin_actor: input.admin_actor },
    });
    return retried;
  }

  async getVerificationQueueMetrics(): Promise<VerificationQueueMetrics> {
    const counts: Record<VerificationJobStatus, number> = {
      queued: 0,
      running: 0,
      publishing: 0,
      retry_wait: 0,
      succeeded: 0,
      dead_letter: 0,
    };
    let oldestAvailable: string | undefined;
    let oldestDeadLetter: string | undefined;
    for (const job of this.verificationJobs.values()) {
      counts[job.status] += 1;
      if ((job.status === "queued" || job.status === "retry_wait") && (!oldestAvailable || job.available_at < oldestAvailable)) {
        oldestAvailable = job.available_at;
      }
      if (job.status === "dead_letter" && (!oldestDeadLetter || job.updated_at < oldestDeadLetter)) {
        oldestDeadLetter = job.updated_at;
      }
    }
    return {
      counts,
      oldest_available_at: oldestAvailable ?? null,
      oldest_dead_letter_at: oldestDeadLetter ?? null,
    };
  }

  private enqueueVerificationJob(version: PackageVersionRecord, snapshot: SnapshotRecord): void {
    const existing = [...this.verificationJobs.values()].find(
      (job) => job.namespace === version.namespace && job.name === version.name && job.version === version.version,
    );
    if (existing) return;
    const createdAt = nowIso();
    const job: VerificationJobRecord = {
      id: crypto.randomUUID(),
      namespace: version.namespace,
      name: version.name,
      version: version.version,
      status: "queued",
      attempt_count: 0,
      max_attempts: 3,
      available_at: createdAt,
      created_at: createdAt,
      updated_at: createdAt,
      source_hash: version.source_hash,
      manifest_hash: version.manifest_hash,
      artifact: version.artifact,
      ...(version.compatibility_profile_hash ? { compatibility_profile_hash: version.compatibility_profile_hash } : {}),
      snapshot_hash: snapshot.snapshot_hash,
      snapshot_object_key: snapshot.r2_key,
      snapshot_size_bytes: snapshot.size_bytes,
      snapshot_content_type: snapshot.content_type,
    };
    this.verificationJobs.set(job.id, job);
  }

  private requireOwnedVerificationJob(
    jobId: string,
    workerId: string,
    requiredStatus?: "running" | "publishing",
  ): VerificationJobRecord {
    const job = this.verificationJobs.get(jobId);
    if (!job) throw new ApiError(404, "verification_job_not_found", "verification job was not found");
    if (job.lease_owner !== workerId || !job.lease_expires_at || Date.parse(job.lease_expires_at) <= Date.now()) {
      throw new ApiError(409, "verification_job_lease_lost", "verification job lease is no longer owned by this worker");
    }
    if (requiredStatus ? job.status !== requiredStatus : job.status !== "running" && job.status !== "publishing") {
      throw new ApiError(409, "verification_job_state_conflict", "verification job is not in an active worker state");
    }
    return job;
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

export function assertPromotionTransition(current: RegistryEntryStatus, next: PackageEvidenceKind): void {
  const allowed: Record<PackageEvidenceKind, RegistryEntryStatus[]> = {
    verified_build: ["source_published", "indexed_pending", "verified_build"],
    deployed: ["verified_build", "deployed"],
    on_chain_attested: ["deployed", "on_chain_attested"],
  };
  if (!allowed[next].includes(current)) {
    throw new ApiError(409, "invalid_evidence_transition", `cannot promote package version from '${current}' to '${next}'`);
  }
}

function verificationStatusForAcceptedEvidence(
  current: VerificationStatus,
  kind: PackageEvidenceKind,
  evidence: Record<string, unknown>,
): VerificationStatus {
  if (kind !== "verified_build") return current;
  switch (evidence["verification_level"]) {
    case "compiled":
      return "verified";
    case "hash_bound":
      return "hash_bound";
    case "evidence_required":
      return "evidence_required";
    default:
      throw new ApiError(500, "invalid_verification_level", "accepted build evidence has no recognised verification level");
  }
}

async function hashForMemory(value: unknown): Promise<string> {
  const { sha256Hex } = await import("./domain");
  return sha256Hex(canonicalJson(value));
}
