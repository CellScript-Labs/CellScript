import { randomUUID } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { Client } from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { AUTH_ACTION, AUTH_PROTOCOL, DEFAULT_REGISTRY_ORIGIN, canonicalJson, type CapabilityAuthorisationPayload } from "../src/domain";
import { createApp } from "../src/index";
import { MemoryRegistryStore, type RegistryStore } from "../src/store";
import { SqlRegistryStore } from "../src/sql-store";
import { preparePostgresExtensions } from "./postgres-fixture";

const databaseUrl = process.env.REGISTRY_TEST_DATABASE_URL;
const principal = () => `0x${randomUUID().replaceAll("-", "")}12345678`;
const namespace = () => `claim-${randomUUID().slice(0, 8)}`;

function authorisationPayload(name: string, owner: string, now: Date): CapabilityAuthorisationPayload {
  return {
    protocol: AUTH_PROTOCOL, action: AUTH_ACTION, registry_origin: DEFAULT_REGISTRY_ORIGIN,
    principal_type: "joyid_ckb", principal_id: owner, capability_pubkey: `p256-spki:${owner.slice(2)}`,
    requested_scopes: [`publish:${name}/demo`], capability_expires_at: new Date(now.getTime() + 3_600_000).toISOString(),
    nonce: `0x${randomUUID().replaceAll("-", "")}`, issued_at: now.toISOString(),
    expires_at: new Date(now.getTime() + 600_000).toISOString(), cli_version: "cellc 0.33.0-alpha.1",
  };
}

for (const backend of ["memory", "postgres"] as const) {
  describe.skipIf(backend === "postgres" && !databaseUrl)(`${backend} namespace claim contract`, () => {
    let store: RegistryStore;
    let admin: Client | undefined;
    const schema = `namespace_test_${randomUUID().replaceAll("-", "")}`;

    beforeAll(async () => {
      if (backend === "memory") {
        store = new MemoryRegistryStore();
        return;
      }
      admin = new Client({ connectionString: databaseUrl! });
      await admin.connect();
      await preparePostgresExtensions(admin);
      await admin.query(`create schema ${schema}`);
      const url = new URL(databaseUrl!);
      url.searchParams.set("options", `-csearch_path=${schema}`);
      const client = new Client({ connectionString: url.toString() });
      await client.connect();
      try {
        const directory = new URL("../migrations/", import.meta.url);
        const files = (await readdir(directory)).filter((file) => /^[0-9]{4}_.+[.]sql$/.test(file)).sort();
        for (const file of files) await client.query(await readFile(new URL(file, directory), "utf8"));
      } finally {
        await client.end();
      }
      store = new SqlRegistryStore({ connectionString: url.toString() });
    }, 30_000);

    afterAll(async () => {
      if (!admin) return;
      await admin.query(`drop schema if exists ${schema} cascade`);
      await admin.end();
    });

    function claim(name: string, owner: string) {
      return store.claimNamespace({ namespace: name, principal_type: "joyid_ckb", principal_id: owner,
        request_id: randomUUID(), claim_cooldown_seconds: 3600 });
    }

    async function quota(owner: string) {
      return store.countRecentQuotaEvents(`principal:joyid_ckb:${owner}`, "namespace_claim_cooldown", new Date(Date.now() - 3_600_000).toISOString());
    }

    it("admits exactly one concurrent new namespace per principal and replays without extra quota or audit", async () => {
      const owner = principal();
      const names = [namespace(), namespace()];
      const results = await Promise.allSettled(names.map((name) => claim(name, owner)));
      expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
      const loser = results.find((result) => result.status === "rejected") as PromiseRejectedResult;
      expect(loser.reason).toMatchObject({ status: 429, code: "namespace_claim_cooldown" });
      const winner = names[results.findIndex((result) => result.status === "fulfilled")]!;
      await expect(claim(winner, owner)).resolves.toMatchObject({ namespace: winner, status: "active" });
      expect(await quota(owner)).toBe(1);
      const audits = await store.listAuditEvents({ namespace: winner, event_type: "namespace.claimed", limit: 100 });
      expect(audits).toHaveLength(1);
      expect(audits[0]?.principal_id).toBe(owner);
    });

    it("rejects concurrent cross-principal losers with 409 and rolls back their quota and audit", async () => {
      const name = namespace();
      const owners = [principal(), principal()];
      // Independent SQL transactions contend on one namespace; Promise.all
      // also reproduces the old memory count/await/record interleaving.
      let pending: Promise<PromiseSettledResult<unknown>[]>;
      if (admin) {
        // Hold INSERTs while permitting both SELECT ... FOR UPDATE prechecks.
        // Release only after both transactions reach the conflicting INSERT,
        // so this exercises ON CONFLICT's loser path deterministically.
        await admin.query("begin");
        await admin.query(`lock table ${schema}.namespaces in share mode`);
        pending = Promise.allSettled(owners.map((owner) => claim(name, owner)));
        try {
          await expect.poll(async () => {
            await admin!.query("select pg_stat_clear_snapshot()");
            return Number((await admin!.query(
              `select count(*) from pg_stat_activity where datname = current_database()
               and query like 'insert into namespaces(%' and wait_event_type = 'Lock'`,
            )).rows[0].count);
          }).toBe(2);
        } finally {
          await admin.query("commit");
        }
      } else {
        pending = Promise.allSettled(owners.map((owner) => claim(name, owner)));
      }
      const results = await pending;
      expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
      const loserIndex = results.findIndex((result) => result.status === "rejected");
      expect((results[loserIndex] as PromiseRejectedResult).reason).toMatchObject({ status: 409, code: "namespace_already_claimed" });
      const winner = owners[1 - loserIndex]!;
      const loser = owners[loserIndex]!;
      expect((await store.getNamespace(name))?.owner_principal_id).toBe(winner);
      const audits = await store.listAuditEvents({ namespace: name, event_type: "namespace.claimed", limit: 100 });
      expect(audits.map((event) => event.principal_id)).toEqual([winner]);
      expect(await quota(loser)).toBe(0);
      await expect(claim(namespace(), loser)).resolves.toMatchObject({ status: "active" });
      expect(await quota(loser)).toBe(1);
    });

    it("serializes direct claims with session completion under the same principal cooldown", async () => {
      for (const sessionFirst of [true, false]) {
        const now = new Date();
        const owner = principal();
        const name = namespace();
        const payload = authorisationPayload(name, owner, now);
        const sessionId = `auth_${randomUUID().replaceAll("-", "")}`;
        await store.createAuthorisationSession({
          session_id: sessionId, poll_token_hash: `sha256:${"11".repeat(32)}`, browser_token_hash: `sha256:${"22".repeat(32)}`,
          registry_origin: DEFAULT_REGISTRY_ORIGIN, website_origin: "https://cellscript.dev",
          capability_pubkey: payload.capability_pubkey, requested_scopes: payload.requested_scopes,
          capability_expires_at: payload.capability_expires_at, cli_version: payload.cli_version,
          namespace: name, name: "demo", artifact_kind: "deployable_contract", status: "pending",
          created_at: now.toISOString(), updated_at: now.toISOString(),
          expires_at: payload.expires_at, request_id: randomUUID(),
        });
        await store.prepareAuthorisationSession({
          session_id: sessionId, principal_type: "joyid_ckb", principal_id: owner, payload,
          challenge_token_hash: "test-challenge", request_id: randomUUID(),
        });
        const complete = () => store.finaliseAuthorisationSession({
          session_id: sessionId, expected_challenge_token_hash: "test-challenge", payload, principal_signature: { scheme: "test" },
          nonce: { nonce_key: randomUUID(), protocol: AUTH_PROTOCOL, action: AUTH_ACTION, nonce: payload.nonce,
            expires_at: payload.expires_at, principal_type: "joyid_ckb", principal_id: owner },
          request_id: randomUUID(), now_iso: now.toISOString(), namespace_claim_cooldown_seconds: 3600,
        });
        const direct = () => claim(namespace(), owner);
        const tasks = sessionFirst ? [complete, direct] : [direct, complete];
        const results = await Promise.allSettled(tasks.map((task) => task()));
        expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
        expect((results.find((result) => result.status === "rejected") as PromiseRejectedResult).reason)
          .toMatchObject({ status: 429, code: "namespace_claim_cooldown" });
        expect(await quota(owner)).toBe(1);
        const audits = await store.listAuditEvents({ principal_id: owner, event_type: "namespace.claimed", limit: 100 });
        expect(audits).toHaveLength(1);
      }
    });

    it("enforces cooldown exactly once through the HTTP route, preserving conflict and replay semantics", async () => {
      const owner = principal();
      const rival = principal();
      const name = namespace();
      const now = new Date();
      const app = createApp({ store, now: () => now, joyidVerifier: { verifySignature: async () => true } });
      async function post(name: string, owner: string) {
        const payload = authorisationPayload(name, owner, now);
        return app.fetch(new Request(`${DEFAULT_REGISTRY_ORIGIN}/v1/namespaces/claim`, {
          method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "203.0.113.9" },
          body: JSON.stringify({ namespace: name, payload, joyid_signature: {
            challenge: canonicalJson(payload), signature: "sig", message: "message", pubkey: owner.slice(2), keyType: "main_key", alg: -7,
          } }),
        }), { REGISTRY_ORIGIN: DEFAULT_REGISTRY_ORIGIN });
      }
      expect((await post(name, owner)).status).toBe(201);
      expect((await post(name, owner)).status).toBe(201);
      expect(await quota(owner)).toBe(1);
      const blocked = await post(namespace(), owner);
      expect(blocked.status).toBe(429);
      expect(await blocked.json()).toMatchObject({ error: { code: "namespace_claim_cooldown" } });
      const conflict = await post(name, rival);
      expect(conflict.status).toBe(409);
      expect(await conflict.json()).toMatchObject({ error: { code: "namespace_already_claimed" } });
      expect(await quota(rival)).toBe(0);
      expect((await post(namespace(), rival)).status).toBe(201);
    });
  });
}
