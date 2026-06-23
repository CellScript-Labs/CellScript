import { Client } from "pg";
import type {
  AuditEventInput,
  AuditEventRecord,
  CapabilityRecord,
  IdempotencyRecord,
  IdempotencyReservation,
  ListAuditEventsInput,
  MaintenanceResult,
  NamespaceClaimResult,
  NamespaceRecord,
  NamespaceStatus,
  PackageVersionRecord,
  ReservedNamespaceRecord,
  RegistryStore,
  SnapshotRecord,
} from "./store";
import { ApiError, capabilityKeyId, canonicalJson, sha256Hex, type CapabilityAuthorisationPayload, type RegistryEntryStatus } from "./domain";

export interface HyperdriveLike {
  connectionString: string;
}

export class SqlRegistryStore implements RegistryStore {
  constructor(private readonly hyperdrive: HyperdriveLike) {}

  private async withClient<T>(fn: (client: Client) => Promise<T>): Promise<T> {
    const client = new Client({ connectionString: this.hyperdrive.connectionString });
    await client.connect();
    try {
      return await fn(client);
    } finally {
      await client.end();
    }
  }

  async recordCapability(input: {
    payload: CapabilityAuthorisationPayload;
    joyid_signature: unknown;
    request_id: string;
  }): Promise<CapabilityRecord> {
    const keyId = await capabilityKeyId(input.payload.capability_pubkey);
    const payloadHash = await sha256Hex(canonicalJson(input.payload));
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        await client.query(
          `insert into principals(principal_type, principal_id)
           values ($1, $2)
           on conflict (principal_type, principal_id)
           do update set updated_at = now()`,
          [input.payload.principal_type, input.payload.principal_id],
        );
        const capabilityInsert = await client.query(
          `insert into capabilities(
             key_id, principal_type, principal_id, capability_pubkey, scopes,
             expires_at, authorisation_payload, joyid_signature
           )
           values ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb)
           on conflict (key_id)
           do update set scopes = excluded.scopes,
                         expires_at = excluded.expires_at,
                         authorisation_payload = excluded.authorisation_payload,
                         joyid_signature = excluded.joyid_signature
           where capabilities.revoked_at is null
           returning key_id`,
          [
            keyId,
            input.payload.principal_type,
            input.payload.principal_id,
            input.payload.capability_pubkey,
            input.payload.requested_scopes,
            input.payload.capability_expires_at,
            JSON.stringify(input.payload),
            JSON.stringify(input.joyid_signature),
          ],
        );
        if (capabilityInsert.rowCount !== 1) {
          throw new ApiError(409, "capability_key_revoked", "revoked capability keys cannot be reactivated");
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id, data
           )
           values ($1, 'capability.created', $2, $3, $4, $5::jsonb)`,
          [
            input.request_id,
            input.payload.principal_type,
            input.payload.principal_id,
            keyId,
            JSON.stringify({ scopes: input.payload.requested_scopes, payload_hash: payloadHash }),
          ],
        );
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    const record = await this.getCapability(keyId);
    if (!record) {
      throw new Error("capability insert did not return a readable record");
    }
    return record;
  }

  async getCapability(keyId: string): Promise<CapabilityRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select key_id, principal_type, principal_id, capability_pubkey, scopes,
                expires_at, revoked_at, created_at, last_used_at
         from capabilities
         where key_id = $1`,
        [keyId],
      );
      const row = result.rows[0];
      if (!row) {
        return null;
      }
      return {
        key_id: row.key_id,
        principal_type: row.principal_type,
        principal_id: row.principal_id,
        capability_pubkey: row.capability_pubkey,
        scopes: row.scopes,
        expires_at: new Date(row.expires_at).toISOString(),
        revoked_at: row.revoked_at ? new Date(row.revoked_at).toISOString() : null,
        created_at: new Date(row.created_at).toISOString(),
        last_used_at: row.last_used_at ? new Date(row.last_used_at).toISOString() : null,
      };
    });
  }

  async revokeCapability(input: {
    key_id: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
    reason?: string;
  }): Promise<CapabilityRecord> {
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update capabilities
           set revoked_at = coalesce(revoked_at, now())
           where key_id = $1
           returning key_id`,
          [input.key_id],
        );
        if (updated.rowCount !== 1) {
          throw new Error(`capability '${input.key_id}' not found`);
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id, data
           )
           values ($1, 'capability.revoked', $2, $3, $4, $5::jsonb)`,
          [
            input.request_id,
            input.principal_type,
            input.principal_id,
            input.key_id,
            JSON.stringify({ reason: input.reason ?? null }),
          ],
        );
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    const record = await this.getCapability(input.key_id);
    if (!record) {
      throw new Error("capability revoke did not return a readable record");
    }
    return record;
  }

  async getNamespace(namespace: string): Promise<NamespaceRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select namespace, owner_principal_type, owner_principal_id, status, review_reason
         from namespaces
         where namespace = $1`,
        [namespace],
      );
      const row = result.rows[0];
      if (!row) {
        return null;
      }
      return {
        namespace: row.namespace,
        owner_principal_type: row.owner_principal_type,
        owner_principal_id: row.owner_principal_id,
        status: row.status,
        ...(row.review_reason ? { review_reason: row.review_reason } : {}),
      };
    });
  }

  async claimNamespace(input: {
    namespace: string;
    principal_type: "joyid_ckb";
    principal_id: string;
    request_id: string;
  }): Promise<NamespaceClaimResult> {
    const reserved = await this.withClient(async (client) => {
      const result = await client.query(
        `select reason from reserved_namespaces
         where (match_type in ('exact', 'typosquat') and namespace = $1)
            or (match_type = 'prefix' and $1 like namespace || '%')
         limit 1`,
        [input.namespace],
      );
      return result.rows[0]?.reason as string | undefined;
    });
    const reviewReason = reserved ?? (input.namespace.length <= 3 ? "short_namespace_review" : undefined);
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        await client.query(
          `insert into principals(principal_type, principal_id)
           values ($1, $2)
           on conflict (principal_type, principal_id)
           do update set updated_at = now()`,
          [input.principal_type, input.principal_id],
        );
        await client.query(
          `insert into namespaces(
             namespace, owner_principal_type, owner_principal_id, status, review_reason, audit_request_id
           )
           values ($1, $2, $3, $4, $5, $6)
           on conflict (namespace) do nothing`,
          [
            input.namespace,
            input.principal_type,
            input.principal_id,
            reviewReason ? "review_pending" : "active",
            reviewReason ?? null,
            input.request_id,
          ],
        );
        await client.query(
          `insert into audit_events(request_id, event_type, principal_type, principal_id, namespace, data)
           values ($1, 'namespace.claimed', $2, $3, $4, $5::jsonb)`,
          [
            input.request_id,
            input.principal_type,
            input.principal_id,
            input.namespace,
            JSON.stringify({ review_reason: reviewReason ?? null }),
          ],
        );
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    const namespace = await this.getNamespace(input.namespace);
    if (!namespace) {
      throw new Error("namespace claim did not return a readable record");
    }
    return {
      namespace: namespace.namespace,
      status: namespace.status === "active" ? "active" : "review_pending",
      ...(namespace.review_reason ? { review_reason: namespace.review_reason } : {}),
    };
  }

  async upsertReservedNamespace(input: ReservedNamespaceRecord & {
    request_id: string;
    admin_actor: string;
  }): Promise<ReservedNamespaceRecord> {
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        await client.query(
          `insert into reserved_namespaces(namespace, match_type, reason)
           values ($1, $2, $3)
           on conflict (namespace)
           do update set match_type = excluded.match_type,
                         reason = excluded.reason`,
          [input.namespace, input.match_type, input.reason],
        );
        await client.query(
          `insert into audit_events(request_id, event_type, namespace, data)
           values ($1, 'admin.reserved_namespace.upserted', $2, $3::jsonb)`,
          [
            input.request_id,
            input.namespace,
            JSON.stringify({ admin_actor: input.admin_actor, match_type: input.match_type, reason: input.reason }),
          ],
        );
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    return {
      namespace: input.namespace,
      match_type: input.match_type,
      reason: input.reason,
    };
  }

  async updateNamespaceStatus(input: {
    namespace: string;
    status: NamespaceStatus;
    review_reason?: string;
    request_id: string;
    admin_actor: string;
  }): Promise<NamespaceRecord> {
    const record = await this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update namespaces
           set status = $2,
               review_reason = $3
           where namespace = $1
           returning namespace, owner_principal_type, owner_principal_id, status, review_reason`,
          [input.namespace, input.status, input.review_reason ?? null],
        );
        const row = updated.rows[0];
        if (!row) {
          throw new ApiError(404, "namespace_not_found", "namespace is not known to the registry");
        }
        await client.query(
          `insert into audit_events(request_id, event_type, principal_type, principal_id, namespace, data)
           values ($1, 'admin.namespace.status_updated', $2, $3, $4, $5::jsonb)`,
          [
            input.request_id,
            row.owner_principal_type,
            row.owner_principal_id,
            input.namespace,
            JSON.stringify({ admin_actor: input.admin_actor, status: input.status, review_reason: input.review_reason ?? null }),
          ],
        );
        await client.query("commit");
        return row;
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    return {
      namespace: record.namespace,
      owner_principal_type: record.owner_principal_type,
      owner_principal_id: record.owner_principal_id,
      status: record.status,
      ...(record.review_reason ? { review_reason: record.review_reason } : {}),
    };
  }

  async ensurePackage(input: {
    namespace: string;
    name: string;
    principal_type: string;
    principal_id: string;
    source_repo?: string;
    request_id: string;
  }): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `insert into packages(namespace, name, source_repo)
         values ($1, $2, $3)
         on conflict (namespace, name)
         do update set source_repo = coalesce(excluded.source_repo, packages.source_repo),
                       updated_at = now()`,
        [input.namespace, input.name, input.source_repo ?? null],
      );
    });
  }

  async recordSnapshot(input: SnapshotRecord): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `insert into source_snapshots(snapshot_hash, r2_key, source_hash, size_bytes, content_type)
         values ($1, $2, $3, $4, $5)
         on conflict (snapshot_hash) do nothing`,
        [input.snapshot_hash, input.r2_key, input.source_hash, input.size_bytes, input.content_type],
      );
    });
  }

  async getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select namespace, name, version, status, source_hash, manifest_hash,
                capability_key_id, principal_type, principal_id, registry_entry,
                snapshot_hash, direct_url, created_at
         from package_versions
         where namespace = $1 and name = $2 and version = $3`,
        [namespace, name, version],
      );
      const row = result.rows[0];
      return row ? packageVersionFromRow(row) : null;
    });
  }

  async recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord> {
    await this.withClient(async (client) => {
      const result = await client.query(
        `insert into package_versions(
           namespace, name, version, status, source_hash, manifest_hash,
           capability_key_id, principal_type, principal_id, registry_entry,
           snapshot_hash, direct_url
         )
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb, $11, $12)
         on conflict (namespace, name, version) do nothing
         returning namespace`,
        [
          input.namespace,
          input.name,
          input.version,
          input.status,
          input.source_hash,
          input.manifest_hash ?? null,
          input.capability_key_id,
          input.principal_type,
          input.principal_id,
          JSON.stringify(input.registry_entry),
          input.snapshot_hash,
          input.direct_url,
        ],
      );
      if (result.rowCount !== 1) {
        throw new ApiError(409, "package_version_exists", "package version already exists and cannot be overwritten");
      }
    });
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
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        await client.query("update capabilities set last_used_at = now() where key_id = $1", [input.key_id]);
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           )
           values ($1, 'capability.used', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            input.principal_type,
            input.principal_id,
            input.key_id,
            input.namespace ?? null,
            input.name ?? null,
            input.version ?? null,
            JSON.stringify({ action: input.action }),
          ],
        );
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
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
    const row = await this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update package_versions
           set status = $4,
               yanked_at = case when $4 = 'yanked' then coalesce(yanked_at, now()) else yanked_at end,
               yanked_reason = case when $4 = 'yanked' then $5 else yanked_reason end,
               quarantined_at = case when $4 = 'quarantined' then coalesce(quarantined_at, now()) else quarantined_at end,
               quarantine_reason = case when $4 = 'quarantined' then $5 else quarantine_reason end,
               indexed_at = case when $4 in ('indexed_pending', 'verified_build') then coalesce(indexed_at, now()) else indexed_at end,
               verified_at = case when $4 = 'verified_build' then coalesce(verified_at, now()) else verified_at end
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, source_hash, manifest_hash,
                     capability_key_id, principal_type, principal_id, registry_entry,
                     snapshot_hash, direct_url, created_at`,
          [input.namespace, input.name, input.version, input.status, input.reason ?? null],
        );
        const record = updated.rows[0];
        if (!record) {
          throw new ApiError(404, "package_version_not_found", "package version is not known to the registry");
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           )
           values ($1, 'admin.package_version.status_updated', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            record.principal_type,
            record.principal_id,
            record.capability_key_id,
            input.namespace,
            input.name,
            input.version,
            JSON.stringify({ admin_actor: input.admin_actor, status: input.status, reason: input.reason ?? null }),
          ],
        );
        await client.query("commit");
        return record;
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    return packageVersionFromRow(row);
  }

  async appendAuditEvent(event: AuditEventInput): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `insert into audit_events(
           request_id, event_type, principal_type, principal_id, capability_key_id,
           namespace, name, version, ip_hash, user_agent, data
         )
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb)`,
        [
          event.request_id,
          event.event_type,
          event.principal_type ?? null,
          event.principal_id ?? null,
          event.capability_key_id ?? null,
          event.namespace ?? null,
          event.name ?? null,
          event.version ?? null,
          event.ip_hash ?? null,
          event.user_agent ?? null,
          JSON.stringify(event.data ?? {}),
        ],
      );
    });
  }

  async listAuditEvents(input: ListAuditEventsInput): Promise<AuditEventRecord[]> {
    return this.withClient(async (client) => {
      const predicates: string[] = [];
      const values: unknown[] = [];
      const addPredicate = (sql: string, value: unknown) => {
        values.push(value);
        predicates.push(sql.replace("?", `$${values.length}`));
      };
      if (input.event_type) addPredicate("event_type = ?", input.event_type);
      if (input.principal_type) addPredicate("principal_type = ?", input.principal_type);
      if (input.principal_id) addPredicate("principal_id = ?", input.principal_id);
      if (input.namespace) addPredicate("namespace = ?", input.namespace);
      if (input.name) addPredicate("name = ?", input.name);
      if (input.version) addPredicate("version = ?", input.version);
      if (input.before) addPredicate("created_at < ?", input.before);
      values.push(input.limit);
      const limitParam = `$${values.length}`;
      const result = await client.query(
        `select id::text, request_id, event_type, principal_type, principal_id,
                capability_key_id, namespace, name, version, ip_hash, user_agent,
                data, created_at
         from audit_events
         ${predicates.length ? `where ${predicates.join(" and ")}` : ""}
         order by created_at desc
         limit ${limitParam}`,
        values,
      );
      return result.rows.map(auditEventFromRow);
    });
  }

  async countRecentQuotaEvents(quotaKey: string, bucket: string, sinceIso: string): Promise<number> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select count(*)::int as count
         from quota_events
         where quota_key = $1 and bucket = $2 and created_at >= $3`,
        [quotaKey, bucket, sinceIso],
      );
      return Number(result.rows[0]?.count ?? 0);
    });
  }

  async recordQuotaEvent(quotaKey: string, bucket: string): Promise<void> {
    await this.withClient(async (client) => {
      await client.query("insert into quota_events(quota_key, bucket) values ($1, $2)", [quotaKey, bucket]);
    });
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
    return this.withClient(async (client) => {
      const result = await client.query(
        `insert into used_nonces(
           nonce_key, protocol, action, nonce, request_id, expires_at,
           principal_type, principal_id, capability_key_id
         )
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         on conflict (nonce_key) do nothing`,
        [
          input.nonce_key,
          input.protocol,
          input.action,
          input.nonce,
          input.request_id,
          input.expires_at,
          input.principal_type ?? null,
          input.principal_id ?? null,
          input.capability_key_id ?? null,
        ],
      );
      return result.rowCount === 1;
    });
  }

  async reserveIdempotencyKey(input: {
    key: string;
    request_hash: string;
    request_id: string;
    expires_at: string;
  }): Promise<IdempotencyReservation> {
    return this.withClient(async (client) => {
      const inserted = await client.query(
        `insert into idempotency_keys(key, request_hash, request_id, status, expires_at)
         values ($1, $2, $3, 'processing', $4)
         on conflict (key) do nothing
         returning key, request_hash, request_id, status, response_status, response,
                   expires_at, created_at, completed_at`,
        [input.key, input.request_hash, input.request_id, input.expires_at],
      );
      const insertedRow = inserted.rows[0];
      if (insertedRow) {
        return { state: "reserved", record: idempotencyRecordFromRow(insertedRow) };
      }

      const existing = await client.query(
        `select key, request_hash, request_id, status, response_status, response,
                expires_at, created_at, completed_at
         from idempotency_keys
         where key = $1`,
        [input.key],
      );
      const record = idempotencyRecordFromRow(existing.rows[0]);
      if (record.request_hash !== input.request_hash) {
        return { state: "conflict", record };
      }
      if (record.status === "completed") {
        return { state: "completed", record };
      }
      return { state: "in_progress", record };
    });
  }

  async getIdempotencyKey(key: string): Promise<IdempotencyRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select key, request_hash, request_id, status, response_status, response,
                expires_at, created_at, completed_at
         from idempotency_keys
         where key = $1`,
        [key],
      );
      const row = result.rows[0];
      return row ? idempotencyRecordFromRow(row) : null;
    });
  }

  async completeIdempotencyKey(input: {
    key: string;
    request_hash: string;
    response_status: number;
    response_body: Record<string, unknown>;
  }): Promise<IdempotencyRecord> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `update idempotency_keys
         set status = 'completed',
             response_status = $3,
             response = $4::jsonb,
             completed_at = now()
         where key = $1 and request_hash = $2
         returning key, request_hash, request_id, status, response_status, response,
                   expires_at, created_at, completed_at`,
        [input.key, input.request_hash, input.response_status, JSON.stringify(input.response_body)],
      );
      const row = result.rows[0];
      if (!row) {
        throw new ApiError(409, "idempotency_key_conflict", "idempotency key is reserved for another request");
      }
      return idempotencyRecordFromRow(row);
    });
  }

  async releaseProcessingIdempotencyKey(input: {
    key: string;
    request_hash: string;
  }): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `delete from idempotency_keys
         where key = $1 and request_hash = $2 and status = 'processing'`,
        [input.key, input.request_hash],
      );
    });
  }

  async cleanupExpiredState(input: {
    now_iso: string;
    quota_events_before_iso: string;
  }): Promise<MaintenanceResult> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const usedNonces = await client.query("delete from used_nonces where expires_at < $1", [input.now_iso]);
        const idempotencyKeys = await client.query("delete from idempotency_keys where expires_at < $1", [input.now_iso]);
        const quotaEvents = await client.query("delete from quota_events where created_at < $1", [input.quota_events_before_iso]);
        await client.query("commit");
        return {
          used_nonces_deleted: usedNonces.rowCount ?? 0,
          idempotency_keys_deleted: idempotencyKeys.rowCount ?? 0,
          quota_events_deleted: quotaEvents.rowCount ?? 0,
        };
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }
}

function packageVersionFromRow(row: any): PackageVersionRecord {
  return {
    namespace: row.namespace,
    name: row.name,
    version: row.version,
    status: row.status,
    source_hash: row.source_hash,
    ...(row.manifest_hash ? { manifest_hash: row.manifest_hash } : {}),
    capability_key_id: row.capability_key_id,
    principal_type: row.principal_type,
    principal_id: row.principal_id,
    registry_entry: row.registry_entry,
    snapshot_hash: row.snapshot_hash,
    direct_url: row.direct_url,
    created_at: new Date(row.created_at).toISOString(),
  };
}

function auditEventFromRow(row: any): AuditEventRecord {
  return {
    id: row.id,
    request_id: row.request_id,
    event_type: row.event_type,
    ...(row.principal_type ? { principal_type: row.principal_type } : {}),
    ...(row.principal_id ? { principal_id: row.principal_id } : {}),
    ...(row.capability_key_id ? { capability_key_id: row.capability_key_id } : {}),
    ...(row.namespace ? { namespace: row.namespace } : {}),
    ...(row.name ? { name: row.name } : {}),
    ...(row.version ? { version: row.version } : {}),
    ...(row.ip_hash ? { ip_hash: row.ip_hash } : {}),
    ...(row.user_agent ? { user_agent: row.user_agent } : {}),
    data: row.data && typeof row.data === "object" && !Array.isArray(row.data) ? row.data : {},
    created_at: new Date(row.created_at).toISOString(),
  };
}

function idempotencyRecordFromRow(row: any): IdempotencyRecord {
  if (!row) {
    throw new ApiError(404, "idempotency_key_not_found", "idempotency key was not found");
  }
  const record: IdempotencyRecord = {
    key: row.key,
    request_hash: row.request_hash,
    request_id: row.request_id,
    status: row.status,
    expires_at: new Date(row.expires_at).toISOString(),
    created_at: new Date(row.created_at).toISOString(),
    completed_at: row.completed_at ? new Date(row.completed_at).toISOString() : null,
  };
  if (typeof row.response_status === "number") {
    record.response_status = row.response_status;
  }
  if (row.response && typeof row.response === "object" && !Array.isArray(row.response)) {
    record.response_body = row.response as Record<string, unknown>;
  }
  return record;
}
