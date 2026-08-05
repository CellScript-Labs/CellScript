import { Client } from "pg";
import {
  assertPromotionTransition,
  deriveRegistryEntryStatus,
  packageVersionRequiresReproduction,
  type AuditEventInput,
  type AuditEventRecord,
  type AuthorisationSessionCompletionInput,
  type AuthorisationSessionCompletionResult,
  type AuthorisationSessionRecord,
  type CapabilityRecord,
  type IdempotencyRecord,
  type IdempotencyReservation,
  type ListAuditEventsInput,
  type MaintenanceResult,
  type NamespaceClaimResult,
  type NamespaceRecord,
  type NamespaceStatus,
  type PackageEvidenceRecord,
  type PackageVersionRecord,
  type PackageVersionQuery,
  type PromotePackageVersionInput,
  type PublishAdmissionInput,
  type ReservedNamespaceRecord,
  type RegistryStore,
  type SnapshotRecord,
  type VerificationJobRecord,
  type VerificationJobStatus,
  type VerificationQueueMetrics,
} from "./store";
import {
  ApiError,
  capabilityKeyId,
  canonicalJson,
  sha256Hex,
  type AvailabilityStatus,
  type CapabilityAuthorisationPayload,
  type PrincipalType,
  type RegistryEntryStatus,
} from "./domain";

export interface HyperdriveLike {
  connectionString: string;
}

export class SqlRegistryStore implements RegistryStore {
  constructor(private readonly hyperdrive: HyperdriveLike) {}

  async healthCheck(): Promise<void> {
    await this.withClient(async (client) => {
      await client.query("select 1");
    });
  }

  async withMaintenanceLease<T>(name: string, task: () => Promise<T>): Promise<T | null> {
    return this.withClient(async (client) => {
      const acquired = await client.query("select pg_try_advisory_lock(hashtext($1)) as acquired", [name]);
      if (acquired.rows[0]?.acquired !== true) return null;
      try {
        return await task();
      } finally {
        await client.query("select pg_advisory_unlock(hashtext($1))", [name]);
      }
    });
  }

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
    principal_signature: unknown;
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
            // The production schema keeps the original column name for a
            // non-destructive migration; it stores either supported wallet
            // signature envelope.
            JSON.stringify(input.principal_signature),
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

  async createAuthorisationSession(
    input: AuthorisationSessionRecord & { request_id: string },
  ): Promise<AuthorisationSessionRecord> {
    await this.withClient(async (client) => {
      const inserted = await client.query(
        `insert into authorisation_sessions(
           session_id, poll_token_hash, browser_token_hash, registry_origin, website_origin,
           capability_pubkey, requested_scopes, capability_expires_at, cli_version,
           namespace, name, artifact_kind, status, expires_at, audit_request_id
         ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'pending', $13, $14)
         on conflict (session_id) do nothing`,
        [
          input.session_id,
          input.poll_token_hash,
          input.browser_token_hash,
          input.registry_origin,
          input.website_origin,
          input.capability_pubkey,
          input.requested_scopes,
          input.capability_expires_at,
          input.cli_version,
          input.namespace,
          input.name,
          input.artifact_kind,
          input.expires_at,
          input.request_id,
        ],
      );
      if (inserted.rowCount !== 1) {
        throw new ApiError(409, "authorisation_session_exists", "authorisation session already exists");
      }
    });
    const record = await this.getAuthorisationSession(input.session_id);
    if (!record) throw new Error("authorisation session insert did not return a readable record");
    return record;
  }

  async getAuthorisationSession(sessionId: string): Promise<AuthorisationSessionRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select session_id, poll_token_hash, browser_token_hash, registry_origin, website_origin,
                capability_pubkey, requested_scopes, capability_expires_at, cli_version,
                namespace, name, artifact_kind, status, principal_type, principal_id, payload,
                challenge_token_hash, capability_key_id, namespace_status,
                created_at, updated_at, expires_at, completed_at
         from authorisation_sessions where session_id = $1`,
        [sessionId],
      );
      return result.rows[0] ? authorisationSessionFromRow(result.rows[0]) : null;
    });
  }

  async prepareAuthorisationSession(input: {
    session_id: string;
    principal_type: PrincipalType;
    principal_id: string;
    payload: CapabilityAuthorisationPayload;
    challenge_token_hash: string;
    request_id: string;
  }): Promise<AuthorisationSessionRecord> {
    await this.withClient(async (client) => {
      const updated = await client.query(
        `update authorisation_sessions
         set principal_type = $2,
             principal_id = $3,
             payload = $4::jsonb,
             challenge_token_hash = $5,
             updated_at = now()
         where session_id = $1 and status = 'pending' and expires_at > now()`,
        [input.session_id, input.principal_type, input.principal_id, JSON.stringify(input.payload), input.challenge_token_hash],
      );
      if (updated.rowCount !== 1) {
        const existing = await client.query("select status, expires_at from authorisation_sessions where session_id = $1", [input.session_id]);
        if (!existing.rows[0]) throw new ApiError(404, "authorisation_session_not_found", "authorisation session was not found");
        if (new Date(existing.rows[0].expires_at).getTime() <= Date.now()) {
          throw new ApiError(410, "authorisation_session_expired", "authorisation session has expired");
        }
        throw new ApiError(409, "authorisation_session_complete", "authorisation session has already completed");
      }
    });
    const record = await this.getAuthorisationSession(input.session_id);
    if (!record) throw new Error("prepared authorisation session was not readable");
    return record;
  }

  async finaliseAuthorisationSession(
    input: AuthorisationSessionCompletionInput,
  ): Promise<AuthorisationSessionCompletionResult> {
    const keyId = await capabilityKeyId(input.payload.capability_pubkey);
    const payloadHash = await sha256Hex(canonicalJson(input.payload));
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const sessionResult = await client.query(
          `select session_id, poll_token_hash, browser_token_hash, registry_origin, website_origin,
                  capability_pubkey, requested_scopes, capability_expires_at, cli_version,
                  namespace, name, artifact_kind, status, principal_type, principal_id, payload,
                  challenge_token_hash, capability_key_id, namespace_status,
                  created_at, updated_at, expires_at, completed_at
           from authorisation_sessions
           where session_id = $1
           for update`,
          [input.session_id],
        );
        const sessionRow = sessionResult.rows[0];
        if (!sessionRow) throw new ApiError(404, "authorisation_session_not_found", "authorisation session was not found");
        const session = authorisationSessionFromRow(sessionRow);
        if (Date.parse(session.expires_at) <= Date.parse(input.now_iso)) {
          throw new ApiError(410, "authorisation_session_expired", "authorisation session has expired");
        }
        if (session.status !== "pending") {
          await client.query("commit");
          return { session, replayed: true };
        }
        if (session.challenge_token_hash !== input.expected_challenge_token_hash
          || !session.payload
          || canonicalJson(session.payload) !== canonicalJson(input.payload)) {
          throw new ApiError(409, "authorisation_challenge_stale", "authorisation challenge was replaced; request a new wallet challenge");
        }

        const nonceInsert = await client.query(
          `insert into used_nonces(
             nonce_key, protocol, action, nonce, request_id, expires_at,
             principal_type, principal_id, capability_key_id
           ) values ($1, $2, $3, $4, $5, $6, $7, $8, null)
           on conflict (nonce_key) do nothing`,
          [
            input.nonce.nonce_key,
            input.nonce.protocol,
            input.nonce.action,
            input.nonce.nonce,
            input.request_id,
            input.nonce.expires_at,
            input.nonce.principal_type,
            input.nonce.principal_id,
          ],
        );
        if (nonceInsert.rowCount !== 1) {
          throw new ApiError(409, "nonce_replay", "signed nonce has already been used");
        }

        await client.query(
          `insert into principals(principal_type, principal_id)
           values ($1, $2)
           on conflict (principal_type, principal_id)
           do update set updated_at = now()`,
          [input.payload.principal_type, input.payload.principal_id],
        );

        let namespaceResult = await client.query(
          `select namespace, owner_principal_type, owner_principal_id, status, review_reason
           from namespaces where namespace = $1 for update`,
          [session.namespace],
        );
        let namespaceInserted = false;
        if (!namespaceResult.rows[0]) {
          if (input.namespace_claim_cooldown_seconds > 0) {
            const cooldownSince = new Date(
              Date.parse(input.now_iso) - input.namespace_claim_cooldown_seconds * 1000,
            ).toISOString();
            const recentClaims = await client.query(
              `select count(*)::bigint as count
               from quota_events
               where quota_key = $1 and bucket = 'namespace_claim_cooldown' and created_at >= $2`,
              [`principal:${input.payload.principal_type}:${input.payload.principal_id}`, cooldownSince],
            );
            if (Number(recentClaims.rows[0]?.count ?? 0) >= 1) {
              throw new ApiError(429, "namespace_claim_cooldown", "namespace claim cooldown is active");
            }
            await client.query(
              `insert into quota_events(quota_key, bucket)
               values ($1, 'namespace_claim_cooldown')`,
              [`principal:${input.payload.principal_type}:${input.payload.principal_id}`],
            );
          }
          const reserved = await client.query(
            `select reason from reserved_namespaces
             where (match_type in ('exact', 'typosquat') and namespace = $1)
                or (match_type = 'prefix' and $1 like namespace || '%')
             limit 1`,
            [session.namespace],
          );
          const reviewReason = reserved.rows[0]?.reason as string | undefined
            ?? (session.namespace.length <= 3 ? "short_namespace_review" : undefined);
          const inserted = await client.query(
            `insert into namespaces(
               namespace, owner_principal_type, owner_principal_id, status, review_reason, audit_request_id
             ) values ($1, $2, $3, $4, $5, $6)
             on conflict (namespace) do nothing`,
            [
              session.namespace,
              input.payload.principal_type,
              input.payload.principal_id,
              reviewReason ? "review_pending" : "active",
              reviewReason ?? null,
              input.request_id,
            ],
          );
          namespaceInserted = inserted.rowCount === 1;
          namespaceResult = await client.query(
            `select namespace, owner_principal_type, owner_principal_id, status, review_reason
             from namespaces where namespace = $1 for update`,
            [session.namespace],
          );
        }
        const namespace = namespaceResult.rows[0];
        if (!namespace) throw new Error("namespace claim did not return a readable record");
        if (namespace.owner_principal_type !== input.payload.principal_type
          || namespace.owner_principal_id !== input.payload.principal_id) {
          throw new ApiError(409, "namespace_already_claimed", "namespace is already claimed by another principal");
        }
        const namespaceStatus: NamespaceClaimResult["status"] = namespace.status === "active" ? "active" : "review_pending";
        if (namespaceInserted) {
          await client.query(
            `insert into audit_events(request_id, event_type, principal_type, principal_id, namespace, data)
             values ($1, 'namespace.claimed', $2, $3, $4, $5::jsonb)`,
            [
              input.request_id,
              input.payload.principal_type,
              input.payload.principal_id,
              session.namespace,
              JSON.stringify({ review_reason: namespace.review_reason ?? null }),
            ],
          );
        }

        const capabilityInsert = await client.query(
          `insert into capabilities(
             key_id, principal_type, principal_id, capability_pubkey, scopes,
             expires_at, authorisation_payload, joyid_signature
           ) values ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb)
           on conflict (key_id)
           do update set scopes = excluded.scopes,
                         expires_at = excluded.expires_at,
                         authorisation_payload = excluded.authorisation_payload,
                         joyid_signature = excluded.joyid_signature
           where capabilities.revoked_at is null
           returning key_id, principal_type, principal_id`,
          [
            keyId,
            input.payload.principal_type,
            input.payload.principal_id,
            input.payload.capability_pubkey,
            input.payload.requested_scopes,
            input.payload.capability_expires_at,
            JSON.stringify(input.payload),
            JSON.stringify(input.principal_signature),
          ],
        );
        const capabilityRow = capabilityInsert.rows[0];
        if (!capabilityRow) {
          throw new ApiError(409, "capability_key_revoked", "revoked capability keys cannot be reactivated");
        }
        if (capabilityRow.principal_type !== input.payload.principal_type
          || capabilityRow.principal_id !== input.payload.principal_id) {
          throw new ApiError(409, "capability_principal_mismatch", "publishing key is already bound to another principal");
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id, data
           ) values ($1, 'capability.created', $2, $3, $4, $5::jsonb)`,
          [
            input.request_id,
            input.payload.principal_type,
            input.payload.principal_id,
            keyId,
            JSON.stringify({ scopes: input.payload.requested_scopes, payload_hash: payloadHash }),
          ],
        );

        const completedResult = await client.query(
          `update authorisation_sessions
           set status = $2,
               capability_key_id = $3,
               namespace_status = $4,
               challenge_token_hash = null,
               completed_at = $5,
               updated_at = $5
           where session_id = $1
           returning session_id, poll_token_hash, browser_token_hash, registry_origin, website_origin,
                     capability_pubkey, requested_scopes, capability_expires_at, cli_version,
                     namespace, name, artifact_kind, status, principal_type, principal_id, payload,
                     challenge_token_hash, capability_key_id, namespace_status,
                     created_at, updated_at, expires_at, completed_at`,
          [
            input.session_id,
            namespaceStatus === "active" ? "authorised" : "review_pending",
            keyId,
            namespaceStatus,
            input.now_iso,
          ],
        );
        const completedRow = completedResult.rows[0];
        if (!completedRow) throw new Error("completed authorisation session was not readable");
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id, namespace, name, data
           ) values ($1, 'authorisation_session.completed', $2, $3, $4, $5, $6, $7::jsonb)`,
          [
            input.request_id,
            input.payload.principal_type,
            input.payload.principal_id,
            keyId,
            session.namespace,
            session.name,
            JSON.stringify({ session_id: session.session_id, namespace_status: namespaceStatus }),
          ],
        );
        await client.query("commit");
        return { session: authorisationSessionFromRow(completedRow), replayed: false };
      } catch (error) {
        await client.query("rollback");
        if (error instanceof ApiError && error.code === "nonce_replay") {
          await client.query(
            `insert into audit_events(request_id, event_type, principal_type, principal_id, data)
             values ($1, 'nonce.replay_blocked', $2, $3, $4::jsonb)`,
            [
              input.request_id,
              input.payload.principal_type,
              input.payload.principal_id,
              JSON.stringify({ protocol: input.nonce.protocol, action: input.nonce.action, nonce_key: input.nonce.nonce_key }),
            ],
          );
        }
        throw error;
      }
    });
  }

  async revokeCapability(input: {
    key_id: string;
    principal_type: PrincipalType;
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
    principal_type: PrincipalType;
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
    principal_type: PrincipalType;
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

  async getSnapshot(snapshotHash: string): Promise<SnapshotRecord | null> {
    return (await this.getSnapshots([snapshotHash])).get(snapshotHash) ?? null;
  }

  async getSnapshots(snapshotHashes: string[]): Promise<Map<string, SnapshotRecord>> {
    const uniqueHashes = [...new Set(snapshotHashes)];
    if (uniqueHashes.length === 0) return new Map();
    return this.withClient(async (client) => {
      const result = await client.query(
        `select snapshot_hash, r2_key, source_hash, size_bytes, content_type
         from source_snapshots
         where snapshot_hash = any($1::text[]) and hidden_at is null`,
        [uniqueHashes],
      );
      return new Map(result.rows.map((row) => {
        const snapshot: SnapshotRecord = {
          snapshot_hash: String(row.snapshot_hash),
          r2_key: String(row.r2_key),
          source_hash: String(row.source_hash),
          size_bytes: Number(row.size_bytes),
          content_type: String(row.content_type),
        };
        return [snapshot.snapshot_hash, snapshot];
      }));
    });
  }

  async getPackageVersion(namespace: string, name: string, version: string): Promise<PackageVersionRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                current_commitment_evidence_hash,
                source_hash, manifest_hash,
                edition, compatibility_profile_hash,
                capability_key_id, principal_type, principal_id, registry_entry,
                snapshot_hash, direct_url, created_at,
                registry_environment, chain_network, expires_at, expired_at, purge_after,
                static_purged_at, source_purged_at
         from package_versions
         where namespace = $1 and name = $2 and version = $3
           and (expires_at is null or expires_at > now())`,
        [namespace, name, version],
      );
      const row = result.rows[0];
      return row ? packageVersionFromRow(row) : null;
    });
  }

  async listPackageVersions(input: PackageVersionQuery): Promise<PackageVersionRecord[]> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select pv.namespace, pv.name, pv.version, pv.status, pv.artifact,
                pv.verification_status, pv.deployment_status, pv.availability_status,
                pv.current_commitment_evidence_hash,
                pv.source_hash, pv.manifest_hash,
                pv.edition, pv.compatibility_profile_hash,
                pv.capability_key_id, pv.principal_type, pv.principal_id, pv.registry_entry,
                pv.snapshot_hash, pv.direct_url, pv.created_at,
                pv.registry_environment, pv.chain_network, pv.expires_at, pv.expired_at, pv.purge_after,
                pv.static_purged_at, pv.source_purged_at
         from package_versions pv
         join packages p on p.namespace = pv.namespace and p.name = pv.name
         where ($1::text is null or pv.namespace = $1)
           and ($2::text is null or pv.name = $2)
           and ($3::text is null or pv.status = $3)
           and ($7::text[] is null or pv.status = any($7::text[]))
           and ($8::text is null or pv.artifact->>'kind' = $8)
           and ($9::text is null or pv.verification_status = $9)
           and ($12::text[] is null or pv.verification_status = any($12::text[]))
           and ($10::text is null or pv.deployment_status = $10)
           and ($11::text is null or pv.availability_status = $11)
           and (pv.expires_at is null or pv.expires_at > now())
           and (
             $4::text is null
             or pv.namespace ilike '%' || $4 || '%'
             or pv.name ilike '%' || $4 || '%'
             or pv.version ilike '%' || $4 || '%'
             or coalesce(p.source_repo, '') ilike '%' || $4 || '%'
             or pv.registry_entry::text ilike '%' || $4 || '%'
           )
         order by pv.created_at desc, pv.namespace, pv.name, pv.version desc
         limit $5 offset $6`,
        [
          input.namespace ?? null,
          input.name ?? null,
          input.status ?? null,
          input.query ?? null,
          input.limit,
          input.offset,
          input.statuses ?? null,
          input.artifact_kind ?? null,
          input.verification_status ?? null,
          input.deployment_status ?? null,
          input.availability_status ?? null,
          input.verification_statuses ?? null,
        ],
      );
      return result.rows.map(packageVersionFromRow);
    });
  }

  async listArtifactPackagePage(input: PackageVersionQuery): Promise<{ records: PackageVersionRecord[]; has_more: boolean }> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `with matching as (
           select pv.namespace, pv.name, pv.version, pv.status, pv.artifact,
                  pv.verification_status, pv.deployment_status, pv.availability_status,
                  pv.current_commitment_evidence_hash,
                  pv.source_hash, pv.manifest_hash, pv.edition, pv.compatibility_profile_hash,
                  pv.capability_key_id, pv.principal_type, pv.principal_id, pv.registry_entry,
                  pv.snapshot_hash, pv.direct_url, pv.created_at,
                  pv.registry_environment, pv.chain_network, pv.expires_at, pv.expired_at, pv.purge_after,
                  pv.static_purged_at, pv.source_purged_at
           from package_versions pv
           join packages p on p.namespace = pv.namespace and p.name = pv.name
           where ($1::text is null or pv.namespace = $1)
             and ($2::text is null or pv.name = $2)
             and ($3::text is null or pv.status = $3)
             and ($7::text[] is null or pv.status = any($7::text[]))
             and ($8::text is null or pv.artifact->>'kind' = $8)
             and ($9::text is null or pv.verification_status = $9)
             and ($12::text[] is null or pv.verification_status = any($12::text[]))
             and ($10::text is null or pv.deployment_status = $10)
             and ($11::text is null or pv.availability_status = $11)
             and (pv.expires_at is null or pv.expires_at > now())
             and (
               $4::text is null
               or pv.namespace ilike '%' || $4 || '%'
               or pv.name ilike '%' || $4 || '%'
               or pv.version ilike '%' || $4 || '%'
               or coalesce(p.source_repo, '') ilike '%' || $4 || '%'
               or pv.registry_entry::text ilike '%' || $4 || '%'
             )
         ), package_page as (
           select namespace, name, max(created_at) as package_updated_at,
                  row_number() over (order by max(created_at) desc, namespace, name) as page_position
           from matching
           group by namespace, name
           order by package_updated_at desc, namespace, name
           limit $5 + 1 offset $6
         )
         select m.*, (select count(*) > $5 from package_page) as has_more
         from matching m
         join package_page pp on pp.namespace = m.namespace and pp.name = m.name
         where pp.page_position <= $6 + $5
         order by pp.package_updated_at desc, m.namespace, m.name, m.created_at desc, m.version desc`,
        [
          input.namespace ?? null,
          input.name ?? null,
          input.status ?? null,
          input.query ?? null,
          input.limit,
          input.offset,
          input.statuses ?? null,
          input.artifact_kind ?? null,
          input.verification_status ?? null,
          input.deployment_status ?? null,
          input.availability_status ?? null,
          input.verification_statuses ?? null,
        ],
      );
      return {
        records: result.rows.map(packageVersionFromRow),
        has_more: result.rows[0]?.has_more === true,
      };
    });
  }

  async recordPackageVersion(input: PackageVersionRecord): Promise<PackageVersionRecord> {
    await this.withClient(async (client) => {
      const result = await client.query(
        `insert into package_versions(
           namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
           source_hash, manifest_hash,
           edition, compatibility_profile_hash,
           capability_key_id, principal_type, principal_id, registry_entry,
           snapshot_hash, direct_url,
           registry_environment, chain_network, expires_at, purge_after
         )
         values ($1, $2, $3, $4, $5::jsonb, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16::jsonb, $17, $18,
                 $19, $20, $21, $22)
         on conflict (namespace, name, version) do nothing
         returning namespace`,
        [
          input.namespace,
          input.name,
          input.version,
          input.status,
          JSON.stringify(input.artifact),
          input.verification_status,
          input.deployment_status,
          input.availability_status,
          input.source_hash,
          input.manifest_hash,
          input.edition,
          input.compatibility_profile_hash,
          input.capability_key_id,
          input.principal_type,
          input.principal_id,
          JSON.stringify(input.registry_entry),
          input.snapshot_hash,
          input.direct_url,
          input.registry_environment ?? "production",
          input.network ?? "mainnet",
          input.expires_at ?? null,
          input.purge_after ?? null,
        ],
      );
      if (result.rowCount !== 1) {
        throw new ApiError(409, "artifact_release_exists", "artifact release already exists and cannot be overwritten");
      }
    });
    return input;
  }

  async admitPackageVersion(input: PublishAdmissionInput): Promise<PackageVersionRecord> {
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        await client.query(
          `insert into packages(namespace, name, source_repo)
           values ($1, $2, $3)
           on conflict (namespace, name)
           do update set source_repo = coalesce(excluded.source_repo, packages.source_repo),
                         updated_at = now()`,
          [input.package.namespace, input.package.name, input.package.source_repo ?? null],
        );
        await client.query(
          `insert into source_snapshots(snapshot_hash, r2_key, source_hash, size_bytes, content_type)
           values ($1, $2, $3, $4, $5)
           on conflict (snapshot_hash) do nothing`,
          [
            input.snapshot.snapshot_hash,
            input.snapshot.r2_key,
            input.snapshot.source_hash,
            input.snapshot.size_bytes,
            input.snapshot.content_type,
          ],
        );
        const insertedVersion = await client.query(
          `insert into package_versions(
             namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
             source_hash, manifest_hash,
             edition, compatibility_profile_hash,
             capability_key_id, principal_type, principal_id, registry_entry,
             snapshot_hash, direct_url,
             registry_environment, chain_network, expires_at, purge_after
           )
           values ($1, $2, $3, $4, $5::jsonb, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16::jsonb, $17, $18,
                   $19, $20, $21, $22)
           on conflict (namespace, name, version) do nothing
           returning namespace`,
          [
            input.version.namespace,
            input.version.name,
            input.version.version,
            input.version.status,
            JSON.stringify(input.version.artifact),
            input.version.verification_status,
            input.version.deployment_status,
            input.version.availability_status,
            input.version.source_hash,
            input.version.manifest_hash,
            input.version.edition,
            input.version.compatibility_profile_hash,
            input.version.capability_key_id,
            input.version.principal_type,
            input.version.principal_id,
            JSON.stringify(input.version.registry_entry),
            input.version.snapshot_hash,
            input.version.direct_url,
            input.version.registry_environment ?? "production",
            input.version.network ?? "mainnet",
            input.version.expires_at ?? null,
            input.version.purge_after ?? null,
          ],
        );
        if (insertedVersion.rowCount !== 1) {
          throw new ApiError(409, "artifact_release_exists", "artifact release already exists and cannot be overwritten");
        }
        await client.query(
          `insert into verification_jobs(namespace, name, version)
           values ($1, $2, $3)
           on conflict (namespace, name, version) do nothing`,
          [input.version.namespace, input.version.name, input.version.version],
        );
        await client.query("update capabilities set last_used_at = now() where key_id = $1", [input.capability_usage.key_id]);
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           )
           values ($1, 'capability.used', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.capability_usage.request_id,
            input.capability_usage.principal_type,
            input.capability_usage.principal_id,
            input.capability_usage.key_id,
            input.capability_usage.namespace ?? null,
            input.capability_usage.name ?? null,
            input.capability_usage.version ?? null,
            JSON.stringify({ action: input.capability_usage.action }),
          ],
        );
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, ip_hash, user_agent, data
           )
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb)`,
          [
            input.audit_event.request_id,
            input.audit_event.event_type,
            input.audit_event.principal_type ?? null,
            input.audit_event.principal_id ?? null,
            input.audit_event.capability_key_id ?? null,
            input.audit_event.namespace ?? null,
            input.audit_event.name ?? null,
            input.audit_event.version ?? null,
            input.audit_event.ip_hash ?? null,
            input.audit_event.user_agent ?? null,
            JSON.stringify(input.audit_event.data ?? {}),
          ],
        );
        if (input.idempotency) {
          const completed = await client.query(
            `update idempotency_keys
             set status = 'completed',
                 response_status = $3,
                 response = $4::jsonb,
                 completed_at = now()
             where key = $1 and request_hash = $2 and status = 'processing'`,
            [
              input.idempotency.key,
              input.idempotency.request_hash,
              input.idempotency.response_status,
              JSON.stringify(input.idempotency.response_body),
            ],
          );
          if (completed.rowCount !== 1) {
            throw new ApiError(409, "idempotency_key_conflict", "idempotency key is reserved for another request");
          }
        }
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
    return input.version;
  }

  async listPackageEvidence(namespace: string, name: string, version: string): Promise<PackageEvidenceRecord[]> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select namespace, name, version, kind, evidence_hash, evidence,
                request_id, admin_actor, created_at
         from package_version_evidence
         where namespace = $1 and name = $2 and version = $3
         order by created_at, kind`,
        [namespace, name, version],
      );
      return result.rows.map(packageEvidenceFromRow);
    });
  }

  async listPackageEvidenceForPackage(namespace: string, name: string): Promise<PackageEvidenceRecord[]> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select namespace, name, version, kind, evidence_hash, evidence,
                request_id, admin_actor, created_at
         from package_version_evidence
         where namespace = $1 and name = $2
         order by created_at, version, kind`,
        [namespace, name],
      );
      return result.rows.map(packageEvidenceFromRow);
    });
  }

  async promotePackageVersion(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const locked = await client.query(
          `select namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                  current_commitment_evidence_hash,
                  source_hash, manifest_hash,
                  edition, compatibility_profile_hash,
                  capability_key_id, principal_type, principal_id, registry_entry,
                  snapshot_hash, direct_url, created_at,
                  registry_environment, chain_network, expires_at, expired_at, purge_after,
                  static_purged_at, source_purged_at
           from package_versions
           where namespace = $1 and name = $2 and version = $3
           for update`,
          [input.namespace, input.name, input.version],
        );
        const currentRow = locked.rows[0];
        if (!currentRow) {
          throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
        }
        const current = packageVersionFromRow(currentRow);
        assertPromotionTransition(current, input.kind);
        await client.query(
          `insert into package_version_evidence(
             namespace, name, version, kind, evidence_hash, evidence,
             request_id, admin_actor
           ) values ($1, $2, $3, $4, $5, $6::jsonb, $7, $8)
           on conflict (namespace, name, version, kind, evidence_hash) do nothing`,
          [
            input.namespace,
            input.name,
            input.version,
            input.kind,
            input.evidence_hash,
            JSON.stringify(input.evidence),
            input.request_id,
            input.admin_actor,
          ],
        );
        const updated = await client.query(
          `update package_versions
           set status = case
                 when availability_status <> 'active' then availability_status
                 when $4 = 'on_chain_committed' then 'on_chain_committed'
                 when current_commitment_evidence_hash is not null then 'on_chain_committed'
                 when $4 = 'deployed' then 'deployed'
                 when deployment_status in ('deployed', 'chain_verified') then 'deployed'
                 else 'verified_build'
               end,
               verification_status = case
                 when $4 = 'reproduced_build' then 'verified'
                 when $4 = 'verified_build' and $5 = 'compiled' then 'verified'
                 when $4 = 'verified_build' and $5 = 'hash_bound' then 'hash_bound'
                 when $4 = 'verified_build' and $5 = 'evidence_required' then 'evidence_required'
                 else verification_status
               end,
               deployment_status = case
                 when $4 = 'deployed' then 'deployed'
                 when $4 = 'on_chain_committed' then 'chain_verified'
                 else deployment_status
               end,
               current_commitment_evidence_hash = case
                 when $4 = 'on_chain_committed' then $6
                 else current_commitment_evidence_hash
               end,
               indexed_at = coalesce(indexed_at, now()),
               verified_at = case when $4 in ('verified_build', 'reproduced_build', 'deployed', 'on_chain_committed') then coalesce(verified_at, now()) else verified_at end
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                     current_commitment_evidence_hash,
                     source_hash, manifest_hash,
                     edition, compatibility_profile_hash,
                     capability_key_id, principal_type, principal_id, registry_entry,
                     snapshot_hash, direct_url, created_at,
                     registry_environment, chain_network, expires_at, expired_at, purge_after,
                     static_purged_at, source_purged_at`,
          [input.namespace, input.name, input.version, input.kind, input.evidence["verification_level"] ?? null, input.evidence_hash],
        );
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb)`,
          [
            input.request_id,
            `evidence.${input.kind}.accepted`,
            current.principal_type,
            current.principal_id,
            current.capability_key_id,
            input.namespace,
            input.name,
            input.version,
            JSON.stringify({ admin_actor: input.admin_actor, evidence_hash: input.evidence_hash }),
          ],
        );
        if (input.capability_usage) {
          await client.query("update capabilities set last_used_at = now() where key_id = $1", [input.capability_usage.key_id]);
          await client.query(
            `insert into audit_events(
               request_id, event_type, principal_type, principal_id, capability_key_id,
               namespace, name, version, data
             ) values ($1, 'capability.used', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
            [
              input.capability_usage.request_id,
              input.capability_usage.principal_type,
              input.capability_usage.principal_id,
              input.capability_usage.key_id,
              input.capability_usage.namespace ?? null,
              input.capability_usage.name ?? null,
              input.capability_usage.version ?? null,
              JSON.stringify({ action: input.capability_usage.action }),
            ],
          );
        }
        if (input.idempotency) {
          await completeIdempotencyInTransaction(client, input.idempotency);
        }
        const evidenceResult = await client.query(
          `select namespace, name, version, kind, evidence_hash, evidence,
                  request_id, admin_actor, created_at
           from package_version_evidence
           where namespace = $1 and name = $2 and version = $3 and kind = $4 and evidence_hash = $5`,
          [input.namespace, input.name, input.version, input.kind, input.evidence_hash],
        );
        await client.query("commit");
        return {
          version: packageVersionFromRow(updated.rows[0]),
          evidence: packageEvidenceFromRow(evidenceResult.rows[0]),
        };
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async recordChainVerifiedDeployment(input: PromotePackageVersionInput): Promise<{
    version: PackageVersionRecord;
    evidence: PackageEvidenceRecord;
  }> {
    if (input.kind !== "deployed") {
      throw new ApiError(500, "invalid_deployment_evidence_kind", "chain-verified deployment evidence must use kind deployed");
    }
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const locked = await client.query(
          `select namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                  current_commitment_evidence_hash,
                  source_hash, manifest_hash, edition, compatibility_profile_hash,
                  capability_key_id, principal_type, principal_id, registry_entry,
                  snapshot_hash, direct_url, created_at,
                  registry_environment, chain_network, expires_at, expired_at, purge_after,
                  static_purged_at, source_purged_at
           from package_versions
           where namespace = $1 and name = $2 and version = $3
           for update`,
          [input.namespace, input.name, input.version],
        );
        const currentRow = locked.rows[0];
        if (!currentRow) {
          throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
        }
        const current = packageVersionFromRow(currentRow);
        if (current.deployment_status === "not_applicable") {
          throw new ApiError(409, "deployment_not_applicable", "this artifact profile cannot have a CKB deployment");
        }
        if (!(current.verification_status === "verified" || current.verification_status === "hash_bound" || current.verification_status === "evidence_required")) {
          throw new ApiError(409, "artifact_not_verified", "artifact verification must finish before recording a deployment");
        }
        if (packageVersionRequiresReproduction(current) && current.verification_status !== "verified") {
          throw new ApiError(409, "reproduction_evidence_missing", "reproducible artifacts require accepted independent reproduction evidence before deployment");
        }
        await client.query(
          `insert into package_version_evidence(
             namespace, name, version, kind, evidence_hash, evidence, request_id, admin_actor
           ) values ($1, $2, $3, 'deployed', $4, $5::jsonb, $6, $7)
           on conflict (namespace, name, version, kind, evidence_hash) do nothing`,
          [input.namespace, input.name, input.version, input.evidence_hash, JSON.stringify(input.evidence), input.request_id, input.admin_actor],
        );
        const updated = await client.query(
          `update package_versions
           set status = case when availability_status = 'active' then 'deployed' else status end,
               deployment_status = 'chain_verified',
               current_commitment_evidence_hash = null,
               indexed_at = coalesce(indexed_at, now())
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                     current_commitment_evidence_hash,
                     source_hash, manifest_hash, edition, compatibility_profile_hash,
                     capability_key_id, principal_type, principal_id, registry_entry,
                     snapshot_hash, direct_url, created_at,
                     registry_environment, chain_network, expires_at, expired_at, purge_after,
                     static_purged_at, source_purged_at`,
          [input.namespace, input.name, input.version],
        );
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           ) values ($1, 'deployment.chain_verified', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            current.principal_type,
            current.principal_id,
            current.capability_key_id,
            input.namespace,
            input.name,
            input.version,
            JSON.stringify({ actor: input.admin_actor, evidence_hash: input.evidence_hash }),
          ],
        );
        if (input.capability_usage) {
          await client.query("update capabilities set last_used_at = now() where key_id = $1", [input.capability_usage.key_id]);
          await client.query(
            `insert into audit_events(
               request_id, event_type, principal_type, principal_id, capability_key_id,
               namespace, name, version, data
             ) values ($1, 'capability.used', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
            [
              input.capability_usage.request_id,
              input.capability_usage.principal_type,
              input.capability_usage.principal_id,
              input.capability_usage.key_id,
              input.capability_usage.namespace ?? null,
              input.capability_usage.name ?? null,
              input.capability_usage.version ?? null,
              JSON.stringify({ action: input.capability_usage.action }),
            ],
          );
        }
        if (input.idempotency) {
          await completeIdempotencyInTransaction(client, input.idempotency);
        }
        const evidenceResult = await client.query(
          `select namespace, name, version, kind, evidence_hash, evidence,
                  request_id, admin_actor, created_at
           from package_version_evidence
           where namespace = $1 and name = $2 and version = $3 and kind = 'deployed' and evidence_hash = $4`,
          [input.namespace, input.name, input.version, input.evidence_hash],
        );
        await client.query("commit");
        return {
          version: packageVersionFromRow(updated.rows[0]),
          evidence: packageEvidenceFromRow(evidenceResult.rows[0]),
        };
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async reconcilePackageVersionLifecycle(input: {
    namespace: string;
    name: string;
    version: string;
    status: "verified_build" | "deployed";
    deployment_status: "undeployed" | "deployed" | "chain_verified";
    request_id: string;
    reason: string;
  }): Promise<PackageVersionRecord> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update package_versions
           set status = case when availability_status = 'active' then $4 else status end,
               deployment_status = $5,
               current_commitment_evidence_hash = null
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                     current_commitment_evidence_hash,
                     source_hash, manifest_hash, edition, compatibility_profile_hash,
                     capability_key_id, principal_type, principal_id, registry_entry,
                     snapshot_hash, direct_url, created_at,
                     registry_environment, chain_network, expires_at, expired_at, purge_after,
                     static_purged_at, source_purged_at`,
          [input.namespace, input.name, input.version, input.status, input.deployment_status],
        );
        const record = updated.rows[0];
        if (!record) {
          throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           ) values ($1, 'lifecycle.chain_state_reconciled', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            record.principal_type,
            record.principal_id,
            record.capability_key_id,
            input.namespace,
            input.name,
            input.version,
            JSON.stringify({ status: input.status, deployment_status: input.deployment_status, reason: input.reason }),
          ],
        );
        await client.query("commit");
        return packageVersionFromRow(record);
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
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
    status: AvailabilityStatus;
    reason?: string;
    request_id: string;
    admin_actor: string;
    audit_event_type?: string;
    capability_usage?: PublishAdmissionInput["capability_usage"];
    idempotency?: PublishAdmissionInput["idempotency"];
  }): Promise<PackageVersionRecord> {
    const row = await this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update package_versions
           set status = case
                 when $4 <> 'active' then $4
                 when current_commitment_evidence_hash is not null then 'on_chain_committed'
                 when deployment_status in ('chain_verified', 'deployed') then 'deployed'
                 when verification_status in ('verified', 'hash_bound', 'evidence_required') then 'verified_build'
                 else 'source_published'
               end,
               availability_status = $4,
               yanked_at = case when $4 = 'yanked' then coalesce(yanked_at, now()) else yanked_at end,
               yanked_reason = case when $4 = 'yanked' then $5 else yanked_reason end,
               quarantined_at = case when $4 = 'quarantined' then coalesce(quarantined_at, now()) else quarantined_at end,
               quarantine_reason = case when $4 = 'quarantined' then $5 else quarantine_reason end
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                     current_commitment_evidence_hash,
                     source_hash, manifest_hash,
                     edition, compatibility_profile_hash,
                     capability_key_id, principal_type, principal_id, registry_entry,
                     snapshot_hash, direct_url, created_at,
                     registry_environment, chain_network, expires_at, expired_at, purge_after,
                     static_purged_at, source_purged_at`,
          [input.namespace, input.name, input.version, input.status, input.reason ?? null],
        );
        const record = updated.rows[0];
        if (!record) {
          throw new ApiError(404, "artifact_release_not_found", "artifact release is not known to the registry");
        }
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           )
           values ($1, $9, $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            record.principal_type,
            record.principal_id,
            record.capability_key_id,
            input.namespace,
            input.name,
            input.version,
            JSON.stringify({ admin_actor: input.admin_actor, status: input.status, reason: input.reason ?? null }),
            input.audit_event_type ?? "admin.package_version.status_updated",
          ],
        );
        if (input.capability_usage) {
          await client.query("update capabilities set last_used_at = now() where key_id = $1", [input.capability_usage.key_id]);
          await client.query(
            `insert into audit_events(
               request_id, event_type, principal_type, principal_id, capability_key_id,
               namespace, name, version, data
             ) values ($1, 'capability.used', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
            [
              input.capability_usage.request_id,
              input.capability_usage.principal_type,
              input.capability_usage.principal_id,
              input.capability_usage.key_id,
              input.capability_usage.namespace ?? null,
              input.capability_usage.name ?? null,
              input.capability_usage.version ?? null,
              JSON.stringify({ action: input.capability_usage.action }),
            ],
          );
        }
        if (input.idempotency) {
          await completeIdempotencyInTransaction(client, input.idempotency);
        }
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

  async releaseNonce(input: { nonce_key: string; request_id: string }): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `delete from used_nonces
         where nonce_key = $1 and request_id = $2`,
        [input.nonce_key, input.request_id],
      );
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
         where key = $1 and request_hash = $2 and status = 'processing'
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

  async claimVerificationJob(input: {
    worker_id: string;
    lease_seconds: number;
    now_iso: string;
  }): Promise<VerificationJobRecord | null> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `with candidate as (
           select id
           from verification_jobs
           where (
             status in ('queued', 'retry_wait') and available_at <= $3
           ) or (
             status in ('running', 'publishing') and lease_expires_at <= $3
           )
           order by available_at, created_at
           for update skip locked
           limit 1
         ), claimed as (
           update verification_jobs job
           set status = case when job.evidence_hash is null then 'running' else 'publishing' end,
               attempt_count = job.attempt_count + 1,
               lease_owner = $1,
               lease_expires_at = $3::timestamptz + make_interval(secs => $2),
               started_at = coalesce(job.started_at, $3::timestamptz),
               updated_at = $3::timestamptz
           from candidate
           where job.id = candidate.id
           returning job.*
         )
         select claimed.*,
                pv.artifact, pv.source_hash, pv.manifest_hash, pv.compatibility_profile_hash, pv.snapshot_hash,
                ss.r2_key as snapshot_object_key, ss.size_bytes as snapshot_size_bytes,
                ss.content_type as snapshot_content_type
         from claimed
         join package_versions pv using (namespace, name, version)
         join source_snapshots ss on ss.snapshot_hash = pv.snapshot_hash`,
        [input.worker_id, input.lease_seconds, input.now_iso],
      );
      return result.rows[0] ? verificationJobFromRow(result.rows[0]) : null;
    });
  }

  async promoteVerifiedBuildForJob(input: {
    job_id: string;
    worker_id: string;
    evidence_hash: string;
    evidence: Record<string, unknown>;
    request_id: string;
    admin_actor: string;
  }): Promise<{ job: VerificationJobRecord; version: PackageVersionRecord; evidence: PackageEvidenceRecord }> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const locked = await client.query(
          `select job.namespace, job.name, job.version,
                  pv.status, pv.artifact, pv.verification_status, pv.deployment_status, pv.availability_status,
                  pv.source_hash, pv.manifest_hash, pv.edition,
                  pv.compatibility_profile_hash, pv.capability_key_id,
                  pv.principal_type, pv.principal_id, pv.registry_entry,
                  pv.snapshot_hash, pv.direct_url, pv.created_at
           from verification_jobs job
           join package_versions pv using (namespace, name, version)
           where job.id = $1
             and job.status = 'running'
             and job.lease_owner = $2
             and job.lease_expires_at > now()
           for update of job, pv`,
          [input.job_id, input.worker_id],
        );
        const currentRow = locked.rows[0];
        if (!currentRow) {
          throw new ApiError(409, "verification_job_lease_lost", "verification job lease is no longer owned by this worker");
        }
        const current = packageVersionFromRow(currentRow);
        assertPromotionTransition(current, "verified_build");
        await client.query(
          `insert into package_version_evidence(
             namespace, name, version, kind, evidence_hash, evidence,
             request_id, admin_actor
           ) values ($1, $2, $3, 'verified_build', $4, $5::jsonb, $6, $7)
           on conflict (namespace, name, version, kind, evidence_hash) do nothing`,
          [
            current.namespace,
            current.name,
            current.version,
            input.evidence_hash,
            JSON.stringify(input.evidence),
            input.request_id,
            input.admin_actor,
          ],
        );
        const updatedVersion = await client.query(
          `update package_versions
           set status = case
                 when availability_status <> 'active' then availability_status
                 when current_commitment_evidence_hash is not null then 'on_chain_committed'
                 when deployment_status in ('deployed', 'chain_verified') then 'deployed'
                 else 'verified_build'
               end,
               verification_status = case
                 when $4 = 'compiled' then 'verified'
                 when $4 = 'hash_bound' then 'hash_bound'
                 when $4 = 'evidence_required' then 'evidence_required'
                 else verification_status
               end,
               indexed_at = coalesce(indexed_at, now()),
               verified_at = coalesce(verified_at, now())
           where namespace = $1 and name = $2 and version = $3
           returning namespace, name, version, status, artifact, verification_status, deployment_status, availability_status,
                     current_commitment_evidence_hash,
                     source_hash, manifest_hash,
                     edition, compatibility_profile_hash, capability_key_id,
                     principal_type, principal_id, registry_entry, snapshot_hash,
                     direct_url, created_at,
                     registry_environment, chain_network, expires_at, expired_at, purge_after,
                     static_purged_at, source_purged_at`,
          [current.namespace, current.name, current.version, input.evidence["verification_level"] ?? null],
        );
        await client.query(
          `update verification_jobs
           set status = 'publishing', evidence_hash = $3, evidence = $4::jsonb,
               updated_at = now()
           where id = $1 and lease_owner = $2`,
          [input.job_id, input.worker_id, input.evidence_hash, JSON.stringify(input.evidence)],
        );
        await client.query(
          `insert into audit_events(
             request_id, event_type, principal_type, principal_id, capability_key_id,
             namespace, name, version, data
           ) values ($1, 'evidence.verified_build.accepted', $2, $3, $4, $5, $6, $7, $8::jsonb)`,
          [
            input.request_id,
            current.principal_type,
            current.principal_id,
            current.capability_key_id,
            current.namespace,
            current.name,
            current.version,
            JSON.stringify({ admin_actor: input.admin_actor, evidence_hash: input.evidence_hash, job_id: input.job_id }),
          ],
        );
        const evidenceResult = await client.query(
          `select namespace, name, version, kind, evidence_hash, evidence,
                  request_id, admin_actor, created_at
           from package_version_evidence
           where namespace = $1 and name = $2 and version = $3
             and kind = 'verified_build' and evidence_hash = $4`,
          [current.namespace, current.name, current.version, input.evidence_hash],
        );
        const job = await this.verificationJobById(client, input.job_id);
        await client.query("commit");
        return {
          job,
          version: packageVersionFromRow(updatedVersion.rows[0]),
          evidence: packageEvidenceFromRow(evidenceResult.rows[0]),
        };
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async completeVerificationJob(input: { job_id: string; worker_id: string }): Promise<VerificationJobRecord> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update verification_jobs
           set status = 'succeeded', lease_owner = null, lease_expires_at = null,
               completed_at = now(), updated_at = now(),
               last_error_code = null, last_error_message = null
           where id = $1 and status = 'publishing' and lease_owner = $2
             and lease_expires_at > now()
           returning namespace, name, version, attempt_count, evidence_hash`,
          [input.job_id, input.worker_id],
        );
        const row = updated.rows[0];
        if (!row) {
          throw new ApiError(409, "verification_job_lease_lost", "verification job lease is no longer owned by this worker");
        }
        await client.query(
          `insert into audit_events(request_id, event_type, namespace, name, version, data)
           values ($1, 'verification.succeeded', $2, $3, $4, $5::jsonb)`,
          [
            `verification:${input.job_id}`,
            row.namespace,
            row.name,
            row.version,
            JSON.stringify({ job_id: input.job_id, attempt_count: row.attempt_count, evidence_hash: row.evidence_hash }),
          ],
        );
        const job = await this.verificationJobById(client, input.job_id);
        await client.query("commit");
        return job;
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async requestStaticSync(input: { namespace: string; name: string; version: string; error_message: string }): Promise<void> {
    await this.withClient(async (client) => {
      await client.query(
        `update verification_jobs
         set status = 'retry_wait', lease_owner = null, lease_expires_at = null,
             available_at = now(), completed_at = null, updated_at = now(),
             last_error_code = 'static_registry_sync_deferred', last_error_message = $4
         where namespace = $1 and name = $2 and version = $3
           and status not in ('running', 'publishing')`,
        [input.namespace, input.name, input.version, input.error_message],
      );
    });
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
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update verification_jobs
           set status = case
                 when $3::boolean and attempt_count < max_attempts
                   then 'retry_wait'
                 else 'dead_letter'
               end,
               available_at = now() + make_interval(secs => case
                 when $3::boolean and attempt_count < max_attempts then $4
                 else 0
               end),
               lease_owner = null,
               lease_expires_at = null,
               last_error_code = $5,
               last_error_message = $6,
               updated_at = now()
           where id = $1 and lease_owner = $2
             and status in ('running', 'publishing')
             and lease_expires_at > now()
           returning namespace, name, version, status, attempt_count`,
          [
            input.job_id,
            input.worker_id,
            input.retryable,
            input.retry_after_seconds,
            input.error_code,
            input.error_message,
          ],
        );
        const row = updated.rows[0];
        if (!row) {
          throw new ApiError(409, "verification_job_lease_lost", "verification job lease is no longer owned by this worker");
        }
        const retry = row.status === "retry_wait";
        await client.query(
          `insert into audit_events(request_id, event_type, namespace, name, version, data)
           values ($1, $2, $3, $4, $5, $6::jsonb)`,
          [
            input.request_id,
            retry ? "verification.retry_scheduled" : "verification.dead_lettered",
            row.namespace,
            row.name,
            row.version,
            JSON.stringify({
              job_id: input.job_id,
              attempt_count: row.attempt_count,
              error_code: input.error_code,
              retry_after_seconds: retry ? input.retry_after_seconds : null,
            }),
          ],
        );
        const job = await this.verificationJobById(client, input.job_id);
        await client.query("commit");
        return job;
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async retryVerificationJob(input: {
    job_id: string;
    request_id: string;
    admin_actor: string;
  }): Promise<VerificationJobRecord> {
    return this.withClient(async (client) => {
      await client.query("begin");
      try {
        const updated = await client.query(
          `update verification_jobs
           set status = 'queued', attempt_count = 0, available_at = now(),
               lease_owner = null, lease_expires_at = null,
               last_error_code = null, last_error_message = null,
               updated_at = now()
           where id = $1 and status = 'dead_letter'
           returning namespace, name, version`,
          [input.job_id],
        );
        const row = updated.rows[0];
        if (!row) {
          const exists = await client.query("select status from verification_jobs where id = $1", [input.job_id]);
          if (!exists.rows[0]) throw new ApiError(404, "verification_job_not_found", "verification job was not found");
          throw new ApiError(409, "verification_job_not_dead_letter", "only dead-letter verification jobs can be retried manually");
        }
        await client.query(
          `insert into audit_events(request_id, event_type, namespace, name, version, data)
           values ($1, 'verification.requeued', $2, $3, $4, $5::jsonb)`,
          [input.request_id, row.namespace, row.name, row.version, JSON.stringify({ job_id: input.job_id, admin_actor: input.admin_actor })],
        );
        const job = await this.verificationJobById(client, input.job_id);
        await client.query("commit");
        return job;
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async getVerificationQueueMetrics(): Promise<VerificationQueueMetrics> {
    return this.withClient(async (client) => {
      const result = await client.query(
        `select status, count(*)::integer as count,
                min(available_at) filter (where status in ('queued', 'retry_wait')) as oldest_available_at,
                min(updated_at) filter (where status = 'dead_letter') as oldest_dead_letter_at
         from verification_jobs
         group by status`,
      );
      const counts: Record<VerificationJobStatus, number> = {
        queued: 0,
        running: 0,
        publishing: 0,
        retry_wait: 0,
        succeeded: 0,
        dead_letter: 0,
      };
      let oldestAvailable: string | null = null;
      let oldestDeadLetter: string | null = null;
      for (const row of result.rows) {
        counts[row.status as VerificationJobStatus] = Number(row.count);
        if (row.oldest_available_at) oldestAvailable = new Date(row.oldest_available_at).toISOString();
        if (row.oldest_dead_letter_at) oldestDeadLetter = new Date(row.oldest_dead_letter_at).toISOString();
      }
      return { counts, oldest_available_at: oldestAvailable, oldest_dead_letter_at: oldestDeadLetter };
    });
  }

  private async verificationJobById(client: Client, jobId: string): Promise<VerificationJobRecord> {
    const result = await client.query(
      `select job.*,
              pv.artifact, pv.source_hash, pv.manifest_hash, pv.compatibility_profile_hash, pv.snapshot_hash,
              ss.r2_key as snapshot_object_key, ss.size_bytes as snapshot_size_bytes,
              ss.content_type as snapshot_content_type
       from verification_jobs job
       join package_versions pv using (namespace, name, version)
       join source_snapshots ss on ss.snapshot_hash = pv.snapshot_hash
       where job.id = $1`,
      [jobId],
    );
    if (!result.rows[0]) throw new ApiError(404, "verification_job_not_found", "verification job was not found");
    return verificationJobFromRow(result.rows[0]);
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
        const authorisationSessions = await client.query("delete from authorisation_sessions where expires_at < $1", [input.now_iso]);
        const quotaEvents = await client.query("delete from quota_events where created_at < $1", [input.quota_events_before_iso]);
        const expiredVersions = await client.query(
          `update package_versions
           set expired_at = $1
           where registry_environment = 'testnet-sandbox'
             and expires_at <= $1
             and expired_at is null`,
          [input.now_iso],
        );
        const staticObjects = await client.query(
          `select namespace, name, version
           from package_versions
           where registry_environment = 'testnet-sandbox'
             and expires_at <= $1
             and static_purged_at is null`,
          [input.now_iso],
        );
        const sourceObjects = await client.query(
          `select distinct ss.r2_key, ss.snapshot_hash
           from source_snapshots ss
           join package_versions due on due.snapshot_hash = ss.snapshot_hash
           where due.registry_environment = 'testnet-sandbox'
             and due.purge_after <= $1
             and due.source_purged_at is null
             and not exists (
               select 1 from package_versions active
               where active.snapshot_hash = ss.snapshot_hash
                 and (active.purge_after is null or active.purge_after > $1)
             )`,
          [input.now_iso],
        );
        await client.query("commit");
        return {
          used_nonces_deleted: usedNonces.rowCount ?? 0,
          idempotency_keys_deleted: idempotencyKeys.rowCount ?? 0,
          authorisation_sessions_deleted: authorisationSessions.rowCount ?? 0,
          quota_events_deleted: quotaEvents.rowCount ?? 0,
          package_versions_expired: expiredVersions.rowCount ?? 0,
          static_objects: staticObjects.rows.map((row) => ({
            key: `artifacts/${row.namespace}/${row.name}/releases/${row.version}.json`,
            namespace: String(row.namespace),
            name: String(row.name),
            version: String(row.version),
          })),
          source_objects: sourceObjects.rows.map((row) => ({
            key: String(row.r2_key),
            snapshot_hash: String(row.snapshot_hash),
          })),
        };
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }

  async markSandboxObjectsPurged(input: {
    static_objects: import("./store").SandboxObjectCandidate[];
    source_objects: import("./store").SandboxObjectCandidate[];
    purged_at: string;
  }): Promise<void> {
    await this.withClient(async (client) => {
      await client.query("begin");
      try {
        for (const candidate of input.static_objects) {
          if (!candidate.namespace || !candidate.name || !candidate.version) continue;
          await client.query(
            `update package_versions set static_purged_at = $4
             where namespace = $1 and name = $2 and version = $3
               and registry_environment = 'testnet-sandbox'`,
            [candidate.namespace, candidate.name, candidate.version, input.purged_at],
          );
        }
        const snapshotHashes = input.source_objects
          .map((candidate) => candidate.snapshot_hash)
          .filter((value): value is string => !!value);
        if (snapshotHashes.length > 0) {
          await client.query(
            `update package_versions set source_purged_at = $2
             where snapshot_hash = any($1::text[])
               and registry_environment = 'testnet-sandbox'`,
            [snapshotHashes, input.purged_at],
          );
          await client.query(
            `update source_snapshots set hidden_at = coalesce(hidden_at, $2), hidden_reason = 'testnet_sandbox_expired'
             where snapshot_hash = any($1::text[])`,
            [snapshotHashes, input.purged_at],
          );
        }
        await client.query("commit");
      } catch (error) {
        await client.query("rollback");
        throw error;
      }
    });
  }
}

async function completeIdempotencyInTransaction(
  client: Client,
  input: NonNullable<PublishAdmissionInput["idempotency"]>,
): Promise<void> {
  const completed = await client.query(
    `update idempotency_keys
     set status = 'completed',
         response_status = $3,
         response = $4::jsonb,
         completed_at = now()
     where key = $1 and request_hash = $2 and status = 'processing'`,
    [input.key, input.request_hash, input.response_status, JSON.stringify(input.response_body)],
  );
  if (completed.rowCount !== 1) {
    throw new ApiError(409, "idempotency_key_conflict", "idempotency key is not owned by this command");
  }
}

function packageVersionFromRow(row: any): PackageVersionRecord {
  const record: PackageVersionRecord = {
    namespace: row.namespace,
    name: row.name,
    version: row.version,
    status: row.status,
    artifact: row.artifact,
    verification_status: row.verification_status,
    deployment_status: row.deployment_status,
    availability_status: row.availability_status,
    current_commitment_evidence_hash: row.current_commitment_evidence_hash ? String(row.current_commitment_evidence_hash) : null,
    source_hash: row.source_hash,
    manifest_hash: row.manifest_hash,
    ...(row.edition ? { edition: row.edition } : {}),
    ...(row.compatibility_profile_hash ? { compatibility_profile_hash: row.compatibility_profile_hash } : {}),
    capability_key_id: row.capability_key_id,
    principal_type: row.principal_type,
    principal_id: row.principal_id,
    registry_entry: row.registry_entry,
    snapshot_hash: row.snapshot_hash,
    direct_url: row.direct_url,
    created_at: new Date(row.created_at).toISOString(),
    registry_environment: row.registry_environment ?? "production",
    network: row.chain_network ?? "mainnet",
    expires_at: row.expires_at ? new Date(row.expires_at).toISOString() : null,
    expired_at: row.expired_at ? new Date(row.expired_at).toISOString() : null,
    purge_after: row.purge_after ? new Date(row.purge_after).toISOString() : null,
    static_purged_at: row.static_purged_at ? new Date(row.static_purged_at).toISOString() : null,
    source_purged_at: row.source_purged_at ? new Date(row.source_purged_at).toISOString() : null,
  };
  record.status = deriveRegistryEntryStatus(record, record.status);
  return record;
}

function authorisationSessionFromRow(row: any): AuthorisationSessionRecord {
  return {
    session_id: String(row.session_id),
    poll_token_hash: String(row.poll_token_hash),
    browser_token_hash: String(row.browser_token_hash),
    registry_origin: String(row.registry_origin),
    website_origin: String(row.website_origin),
    capability_pubkey: String(row.capability_pubkey),
    requested_scopes: Array.isArray(row.requested_scopes) ? row.requested_scopes.map(String) : [],
    capability_expires_at: new Date(row.capability_expires_at).toISOString(),
    cli_version: String(row.cli_version),
    namespace: String(row.namespace),
    name: String(row.name),
    artifact_kind: row.artifact_kind,
    status: row.status,
    principal_type: row.principal_type ?? null,
    principal_id: row.principal_id ? String(row.principal_id) : null,
    payload: row.payload && typeof row.payload === "object" && !Array.isArray(row.payload) ? row.payload : null,
    challenge_token_hash: row.challenge_token_hash ? String(row.challenge_token_hash) : null,
    capability_key_id: row.capability_key_id ? String(row.capability_key_id) : null,
    namespace_status: row.namespace_status ?? null,
    created_at: new Date(row.created_at).toISOString(),
    updated_at: new Date(row.updated_at).toISOString(),
    expires_at: new Date(row.expires_at).toISOString(),
    completed_at: row.completed_at ? new Date(row.completed_at).toISOString() : null,
  };
}

function packageEvidenceFromRow(row: any): PackageEvidenceRecord {
  if (!row) {
    throw new ApiError(500, "evidence_record_missing", "package evidence write did not return a readable record");
  }
  return {
    namespace: row.namespace,
    name: row.name,
    version: row.version,
    kind: row.kind,
    evidence_hash: row.evidence_hash,
    evidence: row.evidence && typeof row.evidence === "object" && !Array.isArray(row.evidence) ? row.evidence : {},
    request_id: row.request_id,
    admin_actor: row.admin_actor,
    created_at: new Date(row.created_at).toISOString(),
  };
}

function verificationJobFromRow(row: any): VerificationJobRecord {
  if (!row) {
    throw new ApiError(500, "verification_job_record_missing", "verification job write did not return a readable record");
  }
  return {
    id: String(row.id),
    namespace: String(row.namespace),
    name: String(row.name),
    version: String(row.version),
    status: row.status as VerificationJobStatus,
    attempt_count: Number(row.attempt_count),
    max_attempts: Number(row.max_attempts),
    available_at: new Date(row.available_at).toISOString(),
    lease_owner: row.lease_owner ? String(row.lease_owner) : null,
    lease_expires_at: row.lease_expires_at ? new Date(row.lease_expires_at).toISOString() : null,
    evidence_hash: row.evidence_hash ? String(row.evidence_hash) : null,
    evidence: row.evidence && typeof row.evidence === "object" && !Array.isArray(row.evidence) ? row.evidence : null,
    last_error_code: row.last_error_code ? String(row.last_error_code) : null,
    last_error_message: row.last_error_message ? String(row.last_error_message) : null,
    created_at: new Date(row.created_at).toISOString(),
    updated_at: new Date(row.updated_at).toISOString(),
    started_at: row.started_at ? new Date(row.started_at).toISOString() : null,
    completed_at: row.completed_at ? new Date(row.completed_at).toISOString() : null,
    source_hash: String(row.source_hash),
    manifest_hash: String(row.manifest_hash),
    artifact: row.artifact,
    ...(row.compatibility_profile_hash ? { compatibility_profile_hash: String(row.compatibility_profile_hash) } : {}),
    snapshot_hash: String(row.snapshot_hash),
    snapshot_object_key: String(row.snapshot_object_key),
    snapshot_size_bytes: Number(row.snapshot_size_bytes),
    snapshot_content_type: String(row.snapshot_content_type),
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
