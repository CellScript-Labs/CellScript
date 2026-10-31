import { randomUUID } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { Client } from "pg";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { createApp } from "../src/index";
import { REGISTRY_SCHEMA_VERSION } from "../src/domain";
import { SqlRegistryStore } from "../src/sql-store";
import { preparePostgresExtensions } from "./postgres-fixture";
import { MemoryRegistryStore, type PackageVersionRecord, type RegistryStore, type SnapshotRecord, type VerificationJobRecord } from "../src/store";

it("keeps the authentication response when best-effort quota auditing fails", async () => {
  const store = new MemoryRegistryStore();
  vi.spyOn(store, "tryConsumeQuota").mockRejectedValue(new Error("database unavailable"));
  const app = createApp({ store });
  const response = await app.fetch(new Request("https://api.registry.cellscript.dev/v1/admin/audit-events"), { REGISTRY_ADMIN_TOKEN: "secret" });
  expect(response.status).toBe(401);
  expect(await response.json()).toMatchObject({ error: { code: "admin_unauthorized" } });
});

for (const backend of ["memory", "postgres"] as const) {
  describe.skipIf(backend === "postgres" && !process.env.REGISTRY_TEST_DATABASE_URL)(`${backend} verification retention`, () => {
    let store: RegistryStore;
    let client: Client | undefined;
    let admin: Client | undefined;
    let scopedDatabaseUrl: string | undefined;
    const schema = `retention_${randomUUID().replaceAll("-", "")}`;
    beforeAll(async () => {
      if (backend === "memory") { store = new MemoryRegistryStore(); return; }
      admin = new Client({ connectionString: process.env.REGISTRY_TEST_DATABASE_URL! });
      await admin.connect();
      await preparePostgresExtensions(admin);
      await admin.query(`create schema ${schema}`);
      const url = new URL(process.env.REGISTRY_TEST_DATABASE_URL!);
      url.searchParams.set("options", `-csearch_path=${schema}`);
      client = new Client({ connectionString: url.toString() });
      await client.connect();
      const directory = new URL("../migrations/", import.meta.url);
      for (const file of (await readdir(directory)).filter((name) => /^[0-9]{4}_.+[.]sql$/.test(name)).sort()) {
        await client.query(await readFile(new URL(file, directory), "utf8"));
      }
      scopedDatabaseUrl = url.toString();
      store = new SqlRegistryStore({ connectionString: scopedDatabaseUrl });
    });
    afterAll(async () => {
      await client?.end();
      if (admin) { await admin.query(`drop schema ${schema} cascade`); await admin.end(); }
    });

    async function fixture() {
      const id = randomUUID();
      const namespace = `audit-${id.slice(0, 8)}`;
      const hash = id.replaceAll("-", "").repeat(2);
      const artifact = { kind: "source_library", profile: "cellscript_source", consumption_mode: "dependency", language: "cellscript" } as const;
      const snapshot: SnapshotRecord = { snapshot_hash: `sha256:${hash}`, r2_key: `source-snapshots/${namespace}/source.json`, source_hash: hash, size_bytes: 1, content_type: "application/json" };
      const version: PackageVersionRecord = {
        namespace, name: "demo", version: "1.0.0", status: "source_published", artifact,
        verification_status: "pending", deployment_status: "not_applicable", availability_status: "active",
        source_hash: hash, manifest_hash: hash, capability_key_id: `cap_${hash}`, principal_type: "joyid_ckb", principal_id: namespace,
        snapshot_hash: snapshot.snapshot_hash, direct_url: "https://registry.cellscript.dev/source", created_at: new Date().toISOString(),
        registry_entry: { schema_version: REGISTRY_SCHEMA_VERSION, namespace, name: "demo", artifact, versions: [{ version: "1.0.0", tag: "v1.0.0", source_hash: hash, verification_status: "pending", deployment_status: "not_applicable", availability_status: "active" }] },
      };
      const job: VerificationJobRecord = { id, namespace, name: "demo", version: "1.0.0", status: "dead_letter", attempt_count: 1, max_attempts: 3, available_at: version.created_at, created_at: version.created_at, updated_at: version.created_at, source_hash: hash, manifest_hash: hash, artifact, snapshot_hash: snapshot.snapshot_hash, snapshot_object_key: snapshot.r2_key, snapshot_size_bytes: 1, snapshot_content_type: snapshot.content_type, last_error_code: "compile_rejected" };
      if (client) {
        await client.query("insert into principals(principal_type, principal_id) values ($1, $2)", [version.principal_type, version.principal_id]);
        await client.query("insert into namespaces(namespace, owner_principal_type, owner_principal_id, audit_request_id) values ($1, $2, $3, 'fixture')", [namespace, version.principal_type, version.principal_id]);
        await client.query("insert into packages(namespace, name) values ($1, $2)", [namespace, version.name]);
        await client.query("insert into capabilities(key_id, principal_type, principal_id, capability_pubkey, scopes, expires_at, authorisation_payload, joyid_signature) values ($1, $2, $3, $4, $5, '2099-01-01', '{}'::jsonb, '{}'::jsonb)", [version.capability_key_id, version.principal_type, version.principal_id, `p256-spki:${hash}`, [`publish:${namespace}/demo`]]);
      }
      await store.recordSnapshot(snapshot);
      await store.recordPackageVersion(version);
      if (client) {
        await client.query("insert into verification_jobs(id, namespace, name, version, status, attempt_count, last_error_code) values ($1, $2, $3, $4, 'dead_letter', 1, 'compile_rejected')", [id, namespace, version.name, version.version]);
      } else (store as MemoryRegistryStore).verificationJobs.set(id, job);
      return { snapshot, version, job, input: { job_id: id, attempt_count: 1, error_code: "compile_rejected", retryable: false } };
    }

    it("admits one concurrent quota slot", async () => {
      const key = randomUUID();
      const results = await Promise.all(Array.from({ length: 32 }, () => store.tryConsumeQuota(key, "audit", 1, 3600)));
      expect(results.filter(Boolean)).toHaveLength(1);
    });
    it("purges a permanent rejection without deleting relational identity and refuses retry", async () => {
      const { input, snapshot, version, job } = await fixture();
      const remove = vi.fn(async (_key: string) => {});
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(true);
      expect(remove).toHaveBeenCalledExactlyOnceWith(snapshot.r2_key);
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
      await expect(store.recordSnapshot(snapshot)).rejects.toMatchObject({ code: "verification_snapshot_purged" });
      await expect(store.recordPackageVersion({ ...version, version: "3.0.0" })).rejects.toMatchObject({ code: "verification_snapshot_purged" });
      expect((await store.getSnapshots([snapshot.snapshot_hash])).size).toBe(0);
      expect(await store.getPackageVersion(version.namespace, version.name, version.version)).toMatchObject({ verification_status: "rejected", source_purged_at: expect.any(String) });
      if (client) expect((await client.query("select 1 from source_snapshots where snapshot_hash = $1", [snapshot.snapshot_hash])).rowCount).toBe(1);
      await expect(store.retryVerificationJob({ job_id: job.id, request_id: "retry", admin_actor: "test" })).rejects.toMatchObject({ code: "verification_snapshot_purged" });
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(false);
    });
    it("preserves exhausted infrastructure failures for retry", async () => {
      const { input, snapshot, job } = await fixture();
      const remove = vi.fn(async (_key: string) => {});
      expect(await store.purgeRejectedSnapshot({ ...input, retryable: true, delete_object: remove })).toBe(false);
      expect(remove).not.toHaveBeenCalled();
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeTruthy();
      await expect(store.retryVerificationJob({ job_id: job.id, request_id: "retry", admin_actor: "test" })).resolves.toMatchObject({ status: "queued" });
    });
    it("does not overwrite accepted evidence with a stale rejection", async () => {
      const { input, version } = await fixture();
      if (client) {
        await client.query("update package_versions set verification_status = 'verified', status = 'verified_build' where namespace = $1", [version.namespace]);
      } else (store as MemoryRegistryStore).packageVersions.set(`${version.namespace}/${version.name}@${version.version}`, { ...version, verification_status: "verified", status: "verified_build" });
      const remove = vi.fn(async (_key: string) => {});
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(false);
      expect(remove).not.toHaveBeenCalled();
      expect(await store.getPackageVersion(version.namespace, version.name, version.version)).toMatchObject({ verification_status: "verified" });
    });
    it("serializes deletion against administrative retry", async () => {
      const { input, job } = await fixture();
      let entered!: () => void;
      let release!: () => void;
      const ready = new Promise<void>((resolve) => { entered = resolve; });
      const blocked = new Promise<void>((resolve) => { release = resolve; });
      const purge = store.purgeRejectedSnapshot({ ...input, delete_object: async () => { entered(); await blocked; } });
      await ready;
      const retry = expect(store.retryVerificationJob({ job_id: job.id, request_id: "retry", admin_actor: "test" }))
        .rejects.toMatchObject({ code: "verification_snapshot_purged" });
      release();
      expect(await purge).toBe(true);
      await retry;
    });
    it("preserves every other release reference even when it has expired", async () => {
      const { input, snapshot, version } = await fixture();
      await store.recordPackageVersion({ ...version, version: "2.0.0", registry_environment: "testnet-sandbox", network: "testnet", expires_at: "2020-01-01T00:00:00Z", purge_after: "2020-01-02T00:00:00Z" });
      const remove = vi.fn(async (_key: string) => {});
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(false);
      expect(remove).not.toHaveBeenCalled();
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeTruthy();
    });
    for (const deletedBeforeError of [false, true]) {
      it("keeps a resumable tombstone when deletion " + (deletedBeforeError ? "succeeds but loses its acknowledgement" : "fails before removing bytes"), async () => {
        const { input, snapshot, version, job } = await fixture();
        const objects = new Set([snapshot.r2_key]);
        const remove = vi.fn(async (key: string) => {
          if (deletedBeforeError) objects.delete(key);
          throw new Error("storage acknowledgement unavailable");
        });
        expect(await store.purgeRejectedSnapshot({ ...input, attempt_count: 2, delete_object: remove })).toBe(false);
        expect(remove).not.toHaveBeenCalled();
        await expect(store.purgeRejectedSnapshot({ ...input, delete_object: remove })).rejects.toThrow("storage acknowledgement unavailable");
        expect(objects.has(snapshot.r2_key)).toBe(!deletedBeforeError);
        expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
        expect((await store.getSnapshots([snapshot.snapshot_hash])).size).toBe(0);
        expect(await store.getPackageVersion(version.namespace, version.name, version.version))
          .toMatchObject({ verification_status: "rejected", source_purged_at: expect.any(String) });
        await expect(store.recordSnapshot(snapshot)).rejects.toMatchObject({ code: "verification_snapshot_purged" });
        await expect(store.recordPackageVersion({ ...version, version: "3.0.0" })).rejects.toMatchObject({ code: "verification_snapshot_purged" });
        await expect(store.retryVerificationJob({ job_id: job.id, request_id: "retry", admin_actor: "test" }))
          .rejects.toMatchObject({ code: "verification_snapshot_purged" });
        const retryDelete = vi.fn(async (key: string) => { objects.delete(key); });
        expect(await store.purgeRejectedSnapshot({ ...input, attempt_count: 2, delete_object: retryDelete })).toBe(false);
        expect(await store.purgeRejectedSnapshot({ ...input, error_code: "different_rejection", delete_object: retryDelete })).toBe(false);
        expect(retryDelete).not.toHaveBeenCalled();
        expect(await store.purgeRejectedSnapshot({ ...input, delete_object: retryDelete })).toBe(true);
        expect(retryDelete).toHaveBeenCalledExactlyOnceWith(snapshot.r2_key);
        expect(objects.size).toBe(0);
        expect(await store.purgeRejectedSnapshot({ ...input, delete_object: retryDelete })).toBe(false);
      });
    }

    it("publishes an unreadable tombstone before object I/O and forbids new references", async () => {
      const { input, snapshot, version } = await fixture();
      await store.purgeRejectedSnapshot({ ...input, delete_object: async () => {
        expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
        expect((await store.getSnapshots([snapshot.snapshot_hash])).size).toBe(0);
        await expect(store.recordPackageVersion({ ...version, version: "2.0.0" }))
          .rejects.toMatchObject({ code: "verification_snapshot_purged" });
        if (client) {
          const row = (await client.query("select hidden_at, hidden_reason from source_snapshots where snapshot_hash = $1", [snapshot.snapshot_hash])).rows[0];
          expect(row.hidden_at).not.toBeNull();
          expect(row.hidden_reason).toContain("verification_rejected_pending_delete:");
        }
      } });
    });

    it("does not repin an existing coordinate through release admission", async () => {
      const { snapshot, version } = await fixture();
      const other = { ...snapshot, snapshot_hash: snapshot.snapshot_hash + "-other", r2_key: snapshot.r2_key + "-other" };
      await store.recordSnapshot(other);
      await expect(store.recordPackageVersion({ ...version, snapshot_hash: other.snapshot_hash }))
        .rejects.toMatchObject({ code: "artifact_release_exists" });
      expect((await store.getPackageVersion(version.namespace, version.name, version.version))?.snapshot_hash).toBe(snapshot.snapshot_hash);
    });

    it.skipIf(backend !== "memory")("does not purge a memory job whose version no longer binds its snapshot", async () => {
      if (client) return;
      const { input, snapshot, version } = await fixture();
      const memory = store as MemoryRegistryStore;
      memory.packageVersions.set(version.namespace + "/" + version.name + "@" + version.version, { ...version, snapshot_hash: snapshot.snapshot_hash + "-other" });
      const remove = vi.fn(async (_key: string) => {});
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(false);
      expect(remove).not.toHaveBeenCalled();
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeTruthy();
    });

    it("serializes duplicate purge calls through object deletion completion", async () => {
      const { input, snapshot } = await fixture();
      let enter!: () => void;
      let release!: () => void;
      const entered = new Promise<void>((resolve) => { enter = resolve; });
      const blocked = new Promise<void>((resolve) => { release = resolve; });
      const remove = vi.fn(async (_key: string) => { enter(); await blocked; });
      const first = store.purgeRejectedSnapshot({ ...input, delete_object: remove });
      await entered;
      const second = store.purgeRejectedSnapshot({ ...input, delete_object: remove });
      release();
      expect(await first).toBe(true);
      expect(await second).toBe(false);
      expect(remove).toHaveBeenCalledExactlyOnceWith(snapshot.r2_key);
    });

    it.skipIf(backend !== "postgres")("revalidates the job snapshot binding after acquiring the purge lock", async () => {
      if (!client) return;
      const { input, snapshot, version } = await fixture();
      const otherHash = randomUUID().replaceAll("-", "").repeat(2);
      const other = { ...snapshot, snapshot_hash: "sha256:" + otherHash, source_hash: otherHash, r2_key: snapshot.r2_key + "-other" };
      await store.recordSnapshot(other);
      const originalQuery = Client.prototype.query;
      let rebind = true;
      const query = vi.spyOn(Client.prototype, "query").mockImplementation(function (this: Client, ...args: unknown[]) {
        const text = typeof args[0] === "string" ? args[0] : "";
        const result = Reflect.apply(originalQuery, this, args) as Promise<unknown>;
        if (rebind && text.includes("select pv.snapshot_hash from verification_jobs j")) {
          rebind = false;
          return result.then(async (value) => {
            await client!.query("update package_versions set snapshot_hash = $1 where namespace = $2 and name = $3 and version = $4",
              [other.snapshot_hash, version.namespace, version.name, version.version]);
            return value;
          });
        }
        return result;
      } as typeof Client.prototype.query);
      const remove = vi.fn(async (_key: string) => {});
      try {
        expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(false);
      } finally { query.mockRestore(); }
      expect(remove).not.toHaveBeenCalled();
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeTruthy();
      expect(await store.getSnapshot(other.snapshot_hash)).toBeTruthy();
    });

    it.skipIf(backend !== "postgres")("does not delete bytes if PostgreSQL rejects the tombstone commit", async () => {
      if (!client) return;
      const { input, snapshot } = await fixture();
      await client.query("create table purge_commit_fault (snapshot_hash text primary key)");
      await client.query("insert into purge_commit_fault values ($1)", [snapshot.snapshot_hash]);
      await client.query("create function reject_purge_commit() returns trigger language plpgsql as $$ begin if new.hidden_reason like 'verification_rejected_pending_delete:%' and exists(select 1 from purge_commit_fault where snapshot_hash = new.snapshot_hash) then raise exception 'purge commit rejected'; end if; return new; end $$");
      await client.query("create constraint trigger reject_purge_commit after update on source_snapshots deferrable initially deferred for each row execute function reject_purge_commit()");
      const remove = vi.fn(async (_key: string) => {});
      try {
        await expect(store.purgeRejectedSnapshot({ ...input, delete_object: remove })).rejects.toThrow("purge commit rejected");
        expect(remove).not.toHaveBeenCalled();
        expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeTruthy();
      } finally {
        await client.query("drop trigger reject_purge_commit on source_snapshots");
        await client.query("drop function reject_purge_commit()");
        await client.query("drop table purge_commit_fault");
      }
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(true);
      expect(remove).toHaveBeenCalledExactlyOnceWith(snapshot.r2_key);
    });

    it.skipIf(backend !== "postgres")("retains the tombstone when COMMIT succeeds but its acknowledgement is lost", async () => {
      if (!client) return;
      const { input, snapshot, job } = await fixture();
      const originalQuery = Client.prototype.query;
      let marked = false;
      const query = vi.spyOn(Client.prototype, "query").mockImplementation(function (this: Client, ...args: unknown[]) {
        const text = typeof args[0] === "string" ? args[0] : "";
        if (text.includes("update source_snapshots set hidden_at")) marked = true;
        const result = Reflect.apply(originalQuery, this, args) as Promise<unknown>;
        if (marked && text === "commit") {
          marked = false;
          return result.then(() => { throw new Error("commit acknowledgement lost"); });
        }
        return result;
      } as typeof Client.prototype.query);
      const remove = vi.fn(async (_key: string) => {});
      try {
        await expect(store.purgeRejectedSnapshot({ ...input, delete_object: remove })).rejects.toThrow("commit acknowledgement lost");
        expect(remove).not.toHaveBeenCalled();
      } finally { query.mockRestore(); }
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
      await expect(store.retryVerificationJob({ job_id: job.id, request_id: "retry", admin_actor: "test" }))
        .rejects.toMatchObject({ code: "verification_snapshot_purged" });
      // A new store/connection resumes the persisted state, without reviving it.
      const recreated = new SqlRegistryStore({ connectionString: scopedDatabaseUrl! });
      expect(await recreated.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(true);
      expect(remove).toHaveBeenCalledExactlyOnceWith(snapshot.r2_key);
    });

    for (const committedBeforeError of [false, true]) {
      it.skipIf(backend !== "postgres")("keeps deletion hidden when the completion commit " + (committedBeforeError ? "loses its acknowledgement" : "is rejected"), async () => {
        if (!client) return;
        const { input, snapshot } = await fixture();
        const objects = new Set([snapshot.r2_key]);
        const remove = vi.fn(async (key: string) => { objects.delete(key); });
        let restore = () => {};
        if (committedBeforeError) {
          const originalQuery = Client.prototype.query;
          let completing = false;
          const query = vi.spyOn(Client.prototype, "query").mockImplementation(function (this: Client, ...args: unknown[]) {
            const text = typeof args[0] === "string" ? args[0] : "";
            if (text.includes("update source_snapshots set hidden_reason = 'verification_rejected'")) completing = true;
            const result = Reflect.apply(originalQuery, this, args) as Promise<unknown>;
            if (completing && text === "commit") {
              completing = false;
              return result.then(() => { throw new Error("completion commit acknowledgement lost"); });
            }
            return result;
          } as typeof Client.prototype.query);
          restore = () => query.mockRestore();
        } else {
          await client.query("create table purge_completion_fault (snapshot_hash text primary key)");
          await client.query("insert into purge_completion_fault values ($1)", [snapshot.snapshot_hash]);
          await client.query("create function reject_purge_completion() returns trigger language plpgsql as $$ begin if new.hidden_reason = 'verification_rejected' and exists(select 1 from purge_completion_fault where snapshot_hash = new.snapshot_hash) then raise exception 'completion commit rejected'; end if; return new; end $$");
          await client.query("create constraint trigger reject_purge_completion after update on source_snapshots deferrable initially deferred for each row execute function reject_purge_completion()");
        }
        try {
          await expect(store.purgeRejectedSnapshot({ ...input, delete_object: remove }))
            .rejects.toThrow(committedBeforeError ? "completion commit acknowledgement lost" : "completion commit rejected");
        } finally {
          restore();
          if (!committedBeforeError) {
            await client.query("drop trigger reject_purge_completion on source_snapshots");
            await client.query("drop function reject_purge_completion()");
            await client.query("drop table purge_completion_fault");
          }
        }
        expect(objects.size).toBe(0);
        expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
        expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(!committedBeforeError);
        expect(remove).toHaveBeenCalledTimes(committedBeforeError ? 1 : 2);
      });
    }

    it.skipIf(backend !== "postgres")("retries an idempotent delete when completion acknowledgement fails", async () => {
      if (!client) return;
      const { input, snapshot } = await fixture();
      const originalQuery = Client.prototype.query;
      let fail = true;
      const query = vi.spyOn(Client.prototype, "query").mockImplementation(function (this: Client, ...args: unknown[]) {
        const text = typeof args[0] === "string" ? args[0] : "";
        if (fail && text.includes("update source_snapshots set hidden_reason = 'verification_rejected'")) {
          fail = false;
          return Promise.reject(new Error("completion acknowledgement lost"));
        }
        return Reflect.apply(originalQuery, this, args);
      } as typeof Client.prototype.query);
      const objects = new Set([snapshot.r2_key]);
      const remove = vi.fn(async (key: string) => { objects.delete(key); });
      try {
        await expect(store.purgeRejectedSnapshot({ ...input, delete_object: remove })).rejects.toThrow("completion acknowledgement lost");
      } finally { query.mockRestore(); }
      expect(objects.size).toBe(0);
      expect(await store.getSnapshot(snapshot.snapshot_hash)).toBeNull();
      expect(await store.purgeRejectedSnapshot({ ...input, delete_object: remove })).toBe(true);
      expect(remove).toHaveBeenCalledTimes(2);
    });

  });
}
