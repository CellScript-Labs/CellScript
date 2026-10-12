import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compareLifecycle, readLifecycleManifest, lifecycleSha256, validateLifecycleCurrent, checkLifecycleDeployment } from './lifecycle.ts';

const vectorBytes = readFileSync(new URL('../../target/zk-lifecycle-vectors.json', import.meta.url));
const vectors = JSON.parse(vectorBytes.toString()) as {
  old_manifest_bytes: string; new_manifest_bytes: string; expected: unknown;
}[];
const migration = JSON.parse(readFileSync(new URL('../../contracts/zk-private-counter/target/migration-lifecycle-versions.json', import.meta.url), 'utf8'));
const compare = (a: Uint8Array, b: Uint8Array) => compareLifecycle(a, lifecycleSha256(a), b, lifecycleSha256(b));

test('independent lifecycle consumer reproduces every native change-class vector', () => {
  assert.equal(vectors.length, 18);
  for (const v of vectors) {
    const a = Buffer.from(v.old_manifest_bytes, 'hex'), b = Buffer.from(v.new_manifest_bytes, 'hex');
    assert.deepEqual(compare(a, b), v.expected);
  }
});

test('independent lifecycle consumer reproduces actual checked migration version decisions', () => {
  const a = Buffer.from(migration.old_manifest_bytes, 'hex'), b = Buffer.from(migration.new_manifest_bytes, 'hex');
  assert.deepEqual(compare(a, b), migration.plan);
  for (const [i, bytes] of [a, b].entries()) {
    checkLifecycleDeployment(bytes, lifecycleSha256(bytes), migration.deployed_records[i], migration.old.deployment.genesis_hash, 'candidate');
  }
});

test('independent consumer binds the retained continuous node migration', {
  skip: !process.env.CELLSCRIPT_ZK_LIFECYCLE_NODE_EVIDENCE,
}, () => {
  const report = JSON.parse(readFileSync(`${process.env.CELLSCRIPT_ZK_LIFECYCLE_NODE_EVIDENCE}/lifecycle-report.json`, 'utf8'));
  assert.equal(report.status, 'passed');
  const a = Buffer.from(report.old_manifest_bytes, 'hex'), b = Buffer.from(report.new_manifest_bytes, 'hex');
  assert.deepEqual(compare(a, b), report.plan);
  for (const [i, bytes] of [a, b].entries()) {
    const manifest = readLifecycleManifest(bytes, lifecycleSha256(bytes));
    checkLifecycleDeployment(bytes, lifecycleSha256(bytes), report.deployed_records[i], manifest.deployment.genesis_hash, 'candidate');
  }
});

test('independent deployment bridge agrees on rebound code, coordinate and status negatives', () => {
  const cases = JSON.parse(readFileSync(new URL('../../target/zk-lifecycle-deployment-vectors.json', import.meta.url), 'utf8'));
  assert.equal(cases.length, 13);
  for (const v of cases) {
    const bytes = Buffer.from(v.manifest_bytes, 'hex');
    for (const [usage, accepted] of [['candidate', v.candidate_ok], ['existing-spend', v.existing_ok]] as const) {
      const run = () => checkLifecycleDeployment(bytes, lifecycleSha256(bytes), v.record, v.genesis, usage);
      if (accepted) assert.doesNotThrow(run, v.case); else assert.throws(run, v.case);
    }
  }
});

test('package-plan identity and stale lifecycle decisions agree with native receipts', () => {
  const v = JSON.parse(readFileSync(new URL('../../target/zk-lifecycle-package-vector.json', import.meta.url), 'utf8'));
  const a = Buffer.from(v.old_manifest_bytes, 'hex'), b = Buffer.from(v.new_manifest_bytes, 'hex');
  const ah = lifecycleSha256(a), bh = lifecycleSha256(b);
  const canonical = (value: unknown): string => {
    if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
    if (value !== null && typeof value === 'object') {
      const o = value as Record<string, unknown>;
      return `{${Object.keys(o).sort((x, y) => Buffer.compare(Buffer.from(x), Buffer.from(y))).map((k) => `${JSON.stringify(k)}:${canonical(o[k])}`).join(',')}}`;
    }
    return JSON.stringify(value);
  };
  const reseal = (p: typeof v.package_plan) => { p.plan_hash = ''; p.plan_hash = `sha256:${lifecycleSha256(Buffer.from(canonical(p)))}`; };
  assert.deepEqual(compareLifecycle(a, ah, b, bh, v.package_plan), v.expected);
  validateLifecycleCurrent(v.expected, a, ah, b, bh, v.package_plan);
  assert.throws(() => compareLifecycle(a, ah, b, bh), /upgrade plan/);
  for (const mutate of [
    (p: typeof v.package_plan) => { p.compiler_version = 'other'; },
    (p: typeof v.package_plan) => { p.performs_deployment = true; },
    (p: typeof v.package_plan) => { p.unknown = true; },
    (p: typeof v.package_plan) => { p.selection.unknown = true; },
    (p: typeof v.package_plan) => { p.locks[0].member = 'other'; },
    (p: typeof v.package_plan) => { p.locks[0].old_lock_content += ' '; },
    (p: typeof v.package_plan) => { p.locks.push(structuredClone(p.locks[0])); },
  ]) {
    const p = structuredClone(v.package_plan); mutate(p); reseal(p);
    assert.throws(() => compareLifecycle(a, ah, b, bh, p));
  }
  const forged = structuredClone(v.expected); forged.discovery_authorizes_spend = true;
  assert.throws(() => validateLifecycleCurrent(forged, a, ah, b, bh, v.package_plan), /stale or substituted/);
  const changed = JSON.parse(b.toString()); changed.version += 1;
  const stale = Buffer.from(JSON.stringify(changed));
  assert.throws(() => validateLifecycleCurrent(v.expected, a, ah, stale, lifecycleSha256(stale), v.package_plan), /stale or substituted/);
});

test('lifecycle consumer rejects malformed identities, unknown fields, unsafe versions and duplicate keys', () => {
  const original = Buffer.from(vectors[0].old_manifest_bytes, 'hex');
  const m = JSON.parse(original.toString());
  for (const mutate of [
    (v: typeof m) => { v.unknown = true; },
    (v: typeof m) => { v.circuit.codec_sha256 = 'ff'; },
    (v: typeof m) => { v.version = Number.MAX_SAFE_INTEGER + 1; },
    (v: typeof m) => { delete v.setup.transcript_sha256; },
    (v: typeof m) => { v.deployment.code_out_point = '00'; },
    (v: typeof m) => { v.application = '\ud800'; },
    (v: typeof m) => { v.setup.trust = ['public-test-only']; },
  ]) {
    const changed = structuredClone(m); mutate(changed); const bytes = Buffer.from(JSON.stringify(changed));
    assert.throws(() => readLifecycleManifest(bytes, lifecycleSha256(bytes)));
  }
  const duplicate = Buffer.from(`{"application":"substitution",${original.toString().slice(1)}`);
  assert.throws(() => readLifecycleManifest(duplicate, lifecycleSha256(duplicate)), /duplicate/);
  assert.throws(() => readLifecycleManifest(original, '00'.repeat(32)), /trusted digest/);
  const floatVersion = Buffer.from(original.toString().replace('"version":1', '"version":1.0'));
  assert.throws(() => readLifecycleManifest(floatVersion, lifecycleSha256(floatVersion)), /noncanonical integer/);
  for (const mutate of [
    (v: typeof m) => { v.version = 0; },
    (v: typeof m) => { v.version += 1; v.status = 'yanked'; },
    (v: typeof m) => { v.version += 1; v.deployment.genesis_hash = '00'.repeat(32); },
    (v: typeof m) => { v.version += 1; v.package.lock_sha256 = '00'.repeat(32); },
  ]) {
    const changed = structuredClone(m); mutate(changed);
    assert.throws(() => compare(original, Buffer.from(JSON.stringify(changed))));
  }
});
