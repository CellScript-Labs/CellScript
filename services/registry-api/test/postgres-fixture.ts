import type { Client } from "pg";

/** Extensions belong to the database, not to a disposable per-suite schema. */
export async function preparePostgresExtensions(client: Client): Promise<void> {
  await client.query("begin");
  try {
    await client.query("select pg_advisory_xact_lock(hashtext('cellscript-registry-test-extensions'))");
    await client.query("create extension if not exists pgcrypto with schema public");
    await client.query("commit");
  } catch (error) {
    await client.query("rollback");
    throw error;
  }
}
