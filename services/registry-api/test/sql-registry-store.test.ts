import { randomUUID } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Client } from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { SqlRegistryStore } from "../src/sql-store";
import { preparePostgresExtensions } from "./postgres-fixture";
import {
  AUTH_ACTION,
  AUTH_PROTOCOL,
  DEFAULT_REGISTRY_ORIGIN,
  type CapabilityAuthorisationPayload,
} from "../src/domain";

const databaseUrl = process.env.REGISTRY_TEST_DATABASE_URL;
const describePostgres = databaseUrl ? describe : describe.skip;
const migrationsDirectory = fileURLToPath(new URL("../migrations/", import.meta.url));

function schemaConnectionString(connectionString: string, schema: string): string {
  const url = new URL(connectionString);
  url.searchParams.set("options", `-csearch_path=${schema}`);
  return url.toString();
}

describePostgres("SqlRegistryStore PostgreSQL contract", () => {
  const schema = `registry_test_${randomUUID().replaceAll("-", "")}`;
  let admin: Client;
  let scopedConnectionString: string;

  beforeAll(async () => {
    admin = new Client({ connectionString: databaseUrl! });
    await admin.connect();
    await preparePostgresExtensions(admin);
    await admin.query(`create schema ${schema}`);
    scopedConnectionString = schemaConnectionString(databaseUrl!, schema);
  });

  afterAll(async () => {
    if (!admin) return;
    await admin.query(`drop schema if exists ${schema} cascade`);
    await admin.end();
  });

  it("migrates legacy commitments, enforces the current pointer, and serialises maintenance", async () => {
    const client = new Client({ connectionString: scopedConnectionString });
    await client.connect();
    try {
      const migrationFiles = (await readdir(migrationsDirectory))
        .filter((file) => /^[0-9]{4}_.+[.]sql$/.test(file))
        .sort();
      const currentCommitmentMigration = "0007_current_commitment_state.sql";
      const lsIdlInterfacesMigration = "0010_ls_idl_interfaces.sql";
      expect(migrationFiles.at(-1)).toBe(lsIdlInterfacesMigration);

      for (const file of migrationFiles.filter((item) => item < currentCommitmentMigration)) {
        await client.query(await readFile(new URL(`../migrations/${file}`, import.meta.url), "utf8"));
      }

      const evidenceHash = `sha256:${"a1".repeat(32)}`;
      await client.query(`
        insert into principals(principal_type, principal_id)
        values ('joyid_ckb', '0x1111111111111111111111111111111111111111');
        insert into namespaces(namespace, owner_principal_type, owner_principal_id, audit_request_id)
        values ('fixture', 'joyid_ckb', '0x1111111111111111111111111111111111111111', 'fixture');
        insert into packages(namespace, name) values ('fixture', 'contract');
        insert into capabilities(
          key_id, principal_type, principal_id, capability_pubkey, scopes, expires_at,
          authorisation_payload, joyid_signature
        ) values (
          'cap_fixturefixturefixturefixture12', 'joyid_ckb',
          '0x1111111111111111111111111111111111111111', 'p256-spki:fixture',
          array['publish:fixture/contract'], '2099-01-01T00:00:00Z', '{}'::jsonb, '{}'::jsonb
        );
        insert into source_snapshots(snapshot_hash, r2_key, source_hash, size_bytes, content_type)
        values (
          'sha256:${"b2".repeat(32)}', 'fixture/source.tar', '${"c3".repeat(32)}', 1,
          'application/vnd.cellscript.source+tar'
        );
        insert into package_versions(
          namespace, name, version, status, artifact, verification_status, deployment_status,
          availability_status, source_hash, manifest_hash, edition, compatibility_profile_hash,
          capability_key_id, principal_type, principal_id, registry_entry, snapshot_hash, direct_url
        ) values (
          'fixture', 'contract', '1.0.0', 'on_chain_attested',
          '{"kind":"deployable_contract","profile":"ckb_executable","consumption_mode":"deployment","language":"rust"}'::jsonb,
          'verified', 'chain_verified', 'active', '${"c3".repeat(32)}', '${"d4".repeat(32)}',
          '2026', '${"e5".repeat(32)}', 'cap_fixturefixturefixturefixture12', 'joyid_ckb',
          '0x1111111111111111111111111111111111111111', '{}'::jsonb,
          'sha256:${"b2".repeat(32)}', 'https://registry.cellscript.dev/fixture/contract/1.0.0'
        );
        insert into package_version_evidence(
          namespace, name, version, kind, evidence_hash, evidence, request_id, admin_actor
        ) values (
          'fixture', 'contract', '1.0.0', 'on_chain_attested', '${evidenceHash}',
          '{"chain_verification":"get_live_cell+configured_type_index"}'::jsonb,
          'legacy-commitment', 'fixture'
        );
      `);

      await client.query(await readFile(new URL(`../migrations/${currentCommitmentMigration}`, import.meta.url), "utf8"));
      const migrated = await client.query(
        `select status, current_commitment_evidence_hash from package_versions
         where namespace = 'fixture' and name = 'contract' and version = '1.0.0'`,
      );
      expect(migrated.rows[0]).toEqual({
        status: "deployed",
        current_commitment_evidence_hash: null,
      });
      expect((await client.query(
        `select kind from package_version_evidence
         where namespace = 'fixture' and name = 'contract' and version = '1.0.0'`,
      )).rows[0]?.kind).toBe("on_chain_committed");

      for (const file of migrationFiles.filter((item) => item > currentCommitmentMigration)) {
        await client.query(await readFile(new URL(`../migrations/${file}`, import.meta.url), "utf8"));
      }

      expect((await client.query(
        "select to_regclass('package_version_evidence_ls_idl_lookup_idx')::text as name",
      )).rows[0]?.name).toBe("package_version_evidence_ls_idl_lookup_idx");

      const store = new SqlRegistryStore({ connectionString: scopedConnectionString });
      await client.query(`
        insert into source_snapshots(snapshot_hash, r2_key, source_hash, size_bytes, content_type)
        values (
          'sha256:${"b3".repeat(32)}', 'fixture/source-sandbox.tar', '${"c3".repeat(32)}', 1,
          'application/vnd.cellscript.source+tar'
        );
        insert into package_versions(
          namespace, name, version, status, artifact, verification_status, deployment_status,
          availability_status, source_hash, manifest_hash, edition, compatibility_profile_hash,
          capability_key_id, principal_type, principal_id, registry_entry, snapshot_hash, direct_url,
          registry_environment, chain_network, expires_at, purge_after
        ) values (
          'fixture', 'contract', '2.0.0', 'source_published',
          '{"kind":"deployable_contract","profile":"ckb_executable","consumption_mode":"deployment","language":"rust"}'::jsonb,
          'pending', 'undeployed', 'active', '${"c3".repeat(32)}', '${"d4".repeat(32)}',
          '2026', '${"e5".repeat(32)}', 'cap_fixturefixturefixturefixture12', 'joyid_ckb',
          '0x1111111111111111111111111111111111111111', '{}'::jsonb,
          'sha256:${"b3".repeat(32)}', 'https://objects.testnet.registry.cellscript.dev/artifacts/fixture/contract/releases/2.0.0.json',
          'testnet-sandbox', 'testnet', '2026-06-23T12:00:00Z', '2026-06-24T12:00:00Z'
        )
      `);
      const sandboxCleanup = await store.cleanupExpiredState({
        now_iso: "2026-06-25T12:00:00Z",
        quota_events_before_iso: "2026-06-23T12:00:00Z",
      });
      expect(sandboxCleanup).toMatchObject({
        package_versions_expired: 1,
        static_objects: [{ key: "artifacts/fixture/contract/releases/2.0.0.json" }],
        source_objects: [{ key: "fixture/source-sandbox.tar", snapshot_hash: `sha256:${"b3".repeat(32)}` }],
      });
      expect(await store.getPackageVersion("fixture", "contract", "2.0.0")).toBeNull();
      await store.markSandboxObjectsPurged({
        static_objects: sandboxCleanup.static_objects ?? [],
        source_objects: sandboxCleanup.source_objects ?? [],
        purged_at: "2026-06-25T12:00:00Z",
      });
      expect((await client.query(
        `select static_purged_at is not null as static_purged, source_purged_at is not null as source_purged
         from package_versions where namespace = 'fixture' and name = 'contract' and version = '2.0.0'`,
      )).rows[0]).toEqual({ static_purged: true, source_purged: true });

      await expect(client.query(
        `update package_versions
         set status = 'on_chain_committed', current_commitment_evidence_hash = null
         where namespace = 'fixture' and name = 'contract' and version = '1.0.0'`,
      )).rejects.toMatchObject({ code: "23514" });

      const recommitted = await store.promotePackageVersion({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        kind: "on_chain_committed",
        evidence_hash: evidenceHash,
        evidence: {
          chain_verification: "get_live_cell+configured_type_index",
          observed_live: true,
          confirmations: 24,
        },
        request_id: "commitment-reobserved",
        admin_actor: "fixture-indexer",
      });
      expect(recommitted.version.status).toBe("on_chain_committed");
      expect(recommitted.version.current_commitment_evidence_hash).toBe(evidenceHash);
      expect((await store.listPackageVersions({
        deployment_status: "chain_verified",
        limit: 10,
        offset: 0,
      }))[0]?.current_commitment_evidence_hash).toBe(evidenceHash);
      expect((await store.listArtifactPackagePage({
        deployment_status: "chain_verified",
        limit: 10,
        offset: 0,
      })).records[0]?.current_commitment_evidence_hash).toBe(evidenceHash);

      await client.query(`
        insert into packages(namespace, name) values ('fixture', 'page-bulk'), ('fixture', 'page-tail');
        insert into package_versions(
          namespace, name, version, status, artifact, verification_status, deployment_status,
          availability_status, source_hash, manifest_hash, edition, compatibility_profile_hash,
          capability_key_id, principal_type, principal_id, registry_entry, snapshot_hash, direct_url, created_at
        )
        select namespace, case when n <= 1000 then 'page-bulk' else 'page-tail' end,
          '1.0.' || n, 'verified_build', artifact, 'verified', 'undeployed', 'active',
          source_hash, manifest_hash, edition, compatibility_profile_hash,
          capability_key_id, principal_type, principal_id, registry_entry, snapshot_hash, direct_url,
          case when n <= 1000 then '2026-10-29T00:00:00Z'::timestamptz else '2026-10-28T00:00:00Z'::timestamptz end
        from package_versions cross join generate_series(1, 1001) n
        where namespace = 'fixture' and name = 'contract' and version = '1.0.0'
      `);
      const pageQuery = { namespace: "fixture", query: "page-", limit: 2, offset: 0 };
      await expect(store.listArtifactPackagePage(pageQuery)).rejects.toMatchObject({ code: "artifact_page_too_large" });
      const firstPage = await store.listArtifactPackagePage({ ...pageQuery, limit: 1 });
      expect(firstPage.records).toHaveLength(1000);
      expect(new Set(firstPage.records.map((record) => record.name))).toEqual(new Set(["page-bulk"]));
      expect(firstPage.has_more).toBe(true);
      const lastPage = await store.listArtifactPackagePage({ ...pageQuery, limit: 1, offset: 1 });
      expect(lastPage.records.map((record) => record.name)).toEqual(["page-tail"]);
      expect(lastPage.has_more).toBe(false);

      const reconciled = await store.reconcilePackageVersionLifecycle({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "deployed",
        deployment_status: "chain_verified",
        request_id: "commitment-spent",
        reason: "registry_commitment_cell_not_live",
      });
      expect(reconciled.status).toBe("deployed");
      expect(reconciled.current_commitment_evidence_hash).toBeNull();

      await store.updatePackageVersionStatus({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "yanked",
        request_id: "yank-after-spend",
        admin_actor: "fixture",
      });

      await client.query(
        `insert into idempotency_keys(key, request_hash, request_id, expires_at)
         values ('restore-fixture', 'correct-hash', 'restore-after-spend', '2099-01-01T00:00:00Z')`,
      );
      await expect(store.updatePackageVersionStatus({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "active",
        request_id: "wrong-restore",
        admin_actor: "fixture",
        idempotency: {
          key: "restore-fixture",
          request_hash: "wrong-hash",
          response_status: 200,
          response_body: { restored: true },
        },
      })).rejects.toMatchObject({ code: "idempotency_key_conflict" });
      expect((await store.getPackageVersion("fixture", "contract", "1.0.0"))?.availability_status).toBe("yanked");

      const restored = await store.updatePackageVersionStatus({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "active",
        request_id: "restore-after-spend",
        admin_actor: "fixture",
        idempotency: {
          key: "restore-fixture",
          request_hash: "correct-hash",
          response_status: 200,
          response_body: { restored: true },
        },
      });
      expect(restored.status).toBe("deployed");
      expect(restored.current_commitment_evidence_hash).toBeNull();
      expect((await client.query(
        "select status, response from idempotency_keys where key = 'restore-fixture'",
      )).rows[0]).toEqual({ status: "completed", response: { restored: true } });

      const staleDeployment = await store.reconcilePackageVersionLifecycle({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "verified_build",
        deployment_status: "undeployed",
        request_id: "deployment-spent",
        reason: "deployment_cell_not_live",
      });
      expect(staleDeployment.status).toBe("verified_build");
      expect(staleDeployment.deployment_status).toBe("undeployed");
      expect(staleDeployment.current_commitment_evidence_hash).toBeNull();

      await store.updatePackageVersionStatus({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "yanked",
        request_id: "yank-during-reverification",
        admin_actor: "fixture",
      });
      const verifiedWhileYanked = await store.promotePackageVersion({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        kind: "verified_build",
        evidence_hash: `sha256:${"55".repeat(32)}`,
        evidence: { verification_level: "compiled" },
        request_id: "reverify-yanked",
        admin_actor: "fixture-verifier",
      });
      expect(verifiedWhileYanked.version.status).toBe("yanked");
      expect(verifiedWhileYanked.version.verification_status).toBe("verified");
      const restoredAfterVerification = await store.updatePackageVersionStatus({
        namespace: "fixture",
        name: "contract",
        version: "1.0.0",
        status: "active",
        request_id: "restore-after-reverification",
        admin_actor: "fixture",
      });
      expect(restoredAfterVerification.status).toBe("verified_build");

      let releaseLease!: () => void;
      let announceLease!: () => void;
      const leaseAcquired = new Promise<void>((resolve) => { announceLease = resolve; });
      const leaseHeld = new Promise<void>((resolve) => { releaseLease = resolve; });
      const firstLease = store.withMaintenanceLease("registry-maintenance", async () => {
        announceLease();
        await leaseHeld;
        return "complete";
      });
      await leaseAcquired;
      expect(await store.withMaintenanceLease("registry-maintenance", async () => "overlap")).toBeNull();
      releaseLease();
      expect(await firstLease).toBe("complete");
    } finally {
      await client.end();
    }
  }, 30_000);

  it("keeps a delegated capability key bound to its first principal across direct registrations", async () => {
    const client = new Client({ connectionString: scopedConnectionString });
    await client.connect();
    try {
      const schemaReady = await client.query("select to_regclass('principals') is not null as present");
      if (!schemaReady.rows[0]?.present) {
        const files = (await readdir(migrationsDirectory))
          .filter((file) => /^[0-9]{4}_.+[.]sql$/.test(file))
          .sort();
        for (const file of files) {
          await client.query(await readFile(new URL(`../migrations/${file}`, import.meta.url), "utf8"));
        }
      }
      const store = new SqlRegistryStore({ connectionString: scopedConnectionString });
      const ownerPrincipal = "0x1111111111111111111111111111111111111111";
      const payload = (principalId: string, scopes: string[]): CapabilityAuthorisationPayload => ({
        protocol: AUTH_PROTOCOL,
        action: AUTH_ACTION,
        registry_origin: DEFAULT_REGISTRY_ORIGIN,
        principal_type: "joyid_ckb",
        principal_id: principalId,
        capability_pubkey: "p256-spki:fixture-delegated-key",
        requested_scopes: scopes,
        capability_expires_at: "2027-01-01T00:00:00Z",
        nonce: `0x${principalId.slice(2, 10)}`,
        issued_at: "2026-06-23T12:00:00Z",
        expires_at: "2026-06-23T12:10:00Z",
        cli_version: "cellc 0.33.0",
      });

      const owner = await store.recordCapability({
        payload: payload(ownerPrincipal, ["publish:fixture/contract"]),
        principal_signature: { scheme: "test" },
        request_id: "capability-owner",
      });
      expect(owner.key_id).toMatch(/^cap_[0-9a-f]{32}$/);

      await expect(store.recordCapability({
        payload: payload("0x4444444444444444444444444444444444444444", ["publish:*/*", "deployment:*/*"]),
        principal_signature: { scheme: "test" },
        request_id: "capability-hijack",
      })).rejects.toMatchObject({ code: "capability_principal_mismatch" });

      const renewedScopes = ["publish:fixture/contract", "deployment:fixture/contract"];
      const renewed = await store.recordCapability({
        payload: payload(ownerPrincipal, renewedScopes),
        principal_signature: { scheme: "test" },
        request_id: "capability-renewal",
      });
      expect(renewed.principal_id).toBe(ownerPrincipal);
      expect(renewed.scopes).toEqual(renewedScopes);

      const row = (await client.query(
        "select principal_id, scopes from capabilities where key_id = $1",
        [owner.key_id],
      )).rows[0];
      expect(row.principal_id).toBe(ownerPrincipal);
      expect(row.scopes).toEqual(renewedScopes);
    } finally {
      await client.end();
    }
  }, 30_000);

  it("enforces sequential claim cooldown and rejects existing cross-principal owners", async () => {
    const client = new Client({ connectionString: scopedConnectionString });
    await client.connect();
    try {
      if (!(await client.query("select to_regclass('principals') is not null as present")).rows[0]?.present) {
        const files = (await readdir(migrationsDirectory))
          .filter((file) => /^[0-9]{4}_.+[.]sql$/.test(file))
          .sort();
        for (const file of files) {
          await client.query(await readFile(new URL(`../migrations/${file}`, import.meta.url), "utf8"));
        }
      }
      const store = new SqlRegistryStore({ connectionString: scopedConnectionString });
      const owner = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
      const rival = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

      // The first store claim succeeds and the next new namespace is blocked.
      // namespace-claims.test.ts separately covers the real HTTP route and
      // forces concurrent inserts through the SQL conflict branch.
      const first = await store.claimNamespace({
        namespace: "h2claim",
        principal_type: "joyid_ckb",
        principal_id: owner,
        request_id: "h2-first-claim",
        claim_cooldown_seconds: 3600,
      });
      expect(["active", "review_pending"]).toContain(first.status);

      // The same principal is inside its own cooldown now.
      await expect(store.claimNamespace({
        namespace: "h2claim-next",
        principal_type: "joyid_ckb",
        principal_id: owner,
        request_id: "h2-second-claim",
        claim_cooldown_seconds: 3600,
      })).rejects.toMatchObject({ code: "namespace_claim_cooldown" });

      // A different principal gets 409 for an already owned namespace.
      await expect(store.claimNamespace({
        namespace: "h2claim",
        principal_type: "joyid_ckb",
        principal_id: rival,
        request_id: "h2-rival-claim",
        claim_cooldown_seconds: 0,
      })).rejects.toMatchObject({ code: "namespace_already_claimed" });

      const ownerRow = (await client.query(
        "select owner_principal_id from namespaces where namespace = 'h2claim'",
      )).rows[0];
      expect(ownerRow.owner_principal_id).toBe(owner);

      const claimAudits = (await client.query(
        "select principal_id from audit_events where event_type = 'namespace.claimed' and namespace = 'h2claim'",
      )).rows;
      expect(claimAudits).toHaveLength(1);
      expect(claimAudits[0].principal_id).toBe(owner);
    } finally {
      await client.end();
    }
  }, 30_000);
});
