import { randomUUID } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Client } from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { SqlRegistryStore } from "../src/sql-store";

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
      expect(migrationFiles.at(-1)).toBe(currentCommitmentMigration);

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

      const store = new SqlRegistryStore({ connectionString: scopedConnectionString });
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
});
