import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ccc } from '@ckb-ccc/shell';
import { deriveStatement } from './client.ts';
import type { CounterSdk } from './client.ts';
import { MigrationDeployment, PreparedMigration, expandMigrationDependencies } from './migration.ts';
const output = process.env.CELLSCRIPT_MIGRATION_SDK;
if (!output) throw new Error('run examples/zk/run-migration.sh');
const root = resolve(import.meta.dirname, '../..'), target = join(root, 'contracts/zk-private-counter/target');
const sdks = await Promise.all([0, 1].map((i) => import(pathToFileURL(join(output, `sdk-${i}/src/index.ts`)).href))) as [CounterSdk, CounterSdk];
const handles = [0, 1].map((i) => readFileSync(join(target, `migration-parent-${i}-handle.bin`))) as [Buffer, Buffer];
const manifestBytes = readFileSync(join(target, 'migration-client-manifest.json'));
const manifest = JSON.parse(manifestBytes.toString());
const load = () => new MigrationDeployment(manifestBytes, ccc.hashCkb(manifestBytes), sdks, handles);
const fixture = JSON.parse(readFileSync(join(target, 'migration-client-fixtures.json'), 'utf8'));
assert.equal(fixture.status, 'passed');
const raw = (value: string) => Uint8Array.from(Buffer.from(value, 'hex'));
const cases = fixture.transactions.map((item: { transaction: string; cells: { out_point: string; output: string; data: string }[] }) => {
  const tx = ccc.Transaction.fromBytes(raw(item.transaction));
  const cells = item.cells.map((cell) => ccc.Cell.from({ outPoint: ccc.OutPoint.fromBytes(raw(cell.out_point)), cellOutput: ccc.CellOutput.fromBytes(raw(cell.output)), outputData: raw(cell.data) }));
  return { tx, cells: cells.slice(0, tx.inputs.length), observations: cells.slice(tx.inputs.length) };
}) as { tx: ccc.Transaction; cells: ccc.Cell[]; observations: ccc.Cell[] }[];
const payload = (tx: ccc.Transaction) => ccc.bytesFrom(tx.getWitnessArgs(0)!.inputType!).slice(8, 136);
const proofs = cases.map((c) => payload(c.tx));

test('CCC migration reproduces all three native transaction statements and proof witnesses', async () => {
  const rows = [];
  for (let i = 0; i < cases.length; i++) {
    const { tx, cells, observations } = cases[i], deployment = load(), version = i === 2 ? 1 : 0;
    const p = new PreparedMigration(tx, cells, observations, deployment);
    assert.equal(p.selectedVersion, version); assert.equal(p.isMigration, i === 1);
    const statement = deriveStatement(tx, cells, deployment.counterScript());
    assert.deepEqual(p.statementBytes, sdks[version].encodeZkStatement(statement));
    const proved = await p.prove(async () => ({ proof: proofs[i], publicInputs: sdks[version].encodeZkPublicInputs(statement) }));
    assert.deepEqual(proved.toBytes(), tx.toBytes()); p.checkSigned(proved, proved);
    const signed = proved.clone(); signed.setWitnessArgs(0, { ...signed.getWitnessArgs(0), lock: new Uint8Array(65) });
    p.checkSigned(proved, signed);
    const forged = proved.clone(); forged.setWitnessArgs(0, { inputType: new Uint8Array(ccc.bytesFrom(proved.getWitnessArgs(0)!.inputType!).length) });
    assert.throws(() => p.checkSigned(forged, forged), /non-Lock witness/);
    const overflow = proved.clone(); overflow.setWitnessArgs(0, { ...overflow.getWitnessArgs(0), lock: new Uint8Array(16384) });
    assert.throws(() => p.checkSigned(proved, overflow), /16384 bytes/);
    rows.push({ version, migration: p.isMigration, transaction_hash: tx.hash(), byte_identical: true });
  }
  writeFileSync(join(output, 'ccc-migration-parity.json'), JSON.stringify({ status: 'passed', scope: 'CCC/native fixture byte parity; not node evidence', manifest_data_hash: ccc.hashCkb(manifestBytes), rows }, null, 2));
});
test('migration rejects manifest substitutions and another internally consistent instance', () => {
  assert.throws(() => new MigrationDeployment(manifestBytes, `0x${'00'.repeat(32)}`, sdks, handles), /trusted digest/);
  const wrongSdk = { ...sdks[1], zkVerifierContracts: sdks[1].zkVerifierContracts.map((c) => ({ ...c, verification_key_hash: '00'.repeat(32) })) };
  assert.throws(() => new MigrationDeployment(manifestBytes, ccc.hashCkb(manifestBytes), [sdks[0], wrongSdk], handles), /VK differs/);
  const { tx, cells, observations } = cases[1], d = load();
  const another = tx.clone(), otherCells = cells.map((c) => c.clone());
  const cf = d.configScript(), ca = d.counterScript(), args = ccc.bytesFrom(cf.args); args.fill(0x66, 0, 64); cf.args = ccc.hexFrom(args);
  ca.args = ccc.hexFrom(Uint8Array.from([...new Uint8Array(32).fill(0x66), ...ccc.bytesFrom(cf.hash())]));
  another.outputs[0].type = ca; another.outputs[1].type = cf; otherCells[0].cellOutput.type = ca; otherCells[1].cellOutput.type = cf;
  assert.throws(() => new PreparedMigration(another, otherCells, observations, d), /exactly one Type-group/);
  for (const mutate of [
    (m: typeof manifest) => { m.extra = true; },
    (m: typeof manifest) => { m.versions[0].setup.production_admitted = true; },
    (m: typeof manifest) => { m.versions[0].setup.circuit.constraints += 1; },
    (m: typeof manifest) => { m.versions[1].verifier.genesis_hash = '00'.repeat(32); },
  ]) { const m = structuredClone(manifest); mutate(m); const b = Buffer.from(JSON.stringify(m)); assert.throws(() => new MigrationDeployment(b, ccc.hashCkb(b), sdks, handles)); }
});
test('configuration custody, overlap, rollback and ambiguous dependencies reject before proving', () => {
  const { tx, cells, observations } = cases[1], d = load();
  const capacity = tx.clone(); capacity.outputs[1].capacity -= 1n;
  assert.throws(() => new PreparedMigration(capacity, cells, observations, d), /Lock\/capacity/);
  const rollback = tx.clone(); rollback.outputsData[1] = d.configData(0);
  assert.throws(() => new PreparedMigration(rollback, cells, observations, d), /0 -> 1/);
  const overlap = tx.clone(); overlap.cellDeps[overlap.cellDeps.length - 1] = ccc.CellDep.from({ outPoint: cells[1].outPoint, depType: 'code' });
  assert.throws(() => new PreparedMigration(overlap, cells, [...observations, cells[1]], d), /without overlap/);
  const parent = observations.find((cell) => cell.outPoint.eq(d.counterDeployment(0).parent.outPoint))!.clone();
  parent.outPoint.txHash = `0x${'ab'.repeat(32)}`;
  const ambiguous = tx.clone(); ambiguous.cellDeps[ambiguous.cellDeps.length - 1] = ccc.CellDep.from({ outPoint: parent.outPoint, depType: 'code' });
  assert.throws(() => new PreparedMigration(ambiguous, cells, [...observations, parent], d), /ambiguous/);
});
test('dep-group expansion preserves child slot, duplicates and the 64-member bound', () => {
  const { tx, cells, observations } = cases[1], d = load();
  const group = ccc.Cell.from({ outPoint: { txHash: `0x${'ba'.repeat(32)}`, index: 0 }, cellOutput: observations[0].cellOutput, outputData: ccc.mol.vector(ccc.OutPoint).encode(tx.cellDeps.map((dep) => dep.outPoint)) });
  const grouped = tx.clone(); grouped.cellDeps = [ccc.CellDep.from({ outPoint: group.outPoint, depType: 'depGroup' })];
  const resolved = expandMigrationDependencies(grouped, [...observations, group]);
  assert.equal(resolved.length, 64); assert.ok(resolved[0].outPoint.eq(tx.cellDeps[0].outPoint));
  assert.equal(new PreparedMigration(grouped, cells, [...observations, group], d).selectedVersion, 0);
  group.outputData = ccc.hexFrom(ccc.mol.vector(ccc.OutPoint).encode([...tx.cellDeps.map((dep) => dep.outPoint), tx.cellDeps[0].outPoint]));
  assert.throws(() => expandMigrationDependencies(grouped, [...observations, group]), /dep-group exceeds/);
});
test('spent configuration dependency fails before the wallet', async () => {
  const { tx, cells, observations } = cases[0], d = load(), p = new PreparedMigration(tx, cells, observations, d);
  const s = deriveStatement(tx, cells, d.counterScript());
  const proved = await p.prove(async () => ({ proof: proofs[0], publicInputs: sdks[0].encodeZkPublicInputs(s) }));
  const config = observations.find((cell) => cell.cellOutput.type?.eq(d.configScript()))!;
  const universe = [...cases[1].observations, ...cells, ...observations];
  let invoked = false;
  const client = { getBlockByNumber: async () => ({ header: { hash: `0x${manifest.genesis_hash}` } }), getCellLive: async (point: ccc.OutPointLike) => config.outPoint.eq(point) ? undefined : universe.find((cell) => cell.outPoint.eq(point))?.clone() };
  const signer = { client, signOnlyTransaction: async () => { invoked = true; return proved; } } as unknown as ccc.Signer;
  await assert.rejects(() => p.signAndSend(signer, proved), /spent, missing or inconsistent/); assert.equal(invoked, false);
});
test('migration snapshots remain unchanged when caller-owned transaction, Cells and Scripts change', async () => {
  const item = cases[1], tx = item.tx.clone(), cells = item.cells.map((cell) => cell.clone()), d = load();
  const p = new PreparedMigration(tx, cells, item.observations, d);
  const statement = deriveStatement(item.tx, item.cells, d.counterScript());
  tx.outputsData[1] = '0x'; cells[1].outputData = '0x'; d.counterScript().args = '0x'; d.configScript().args = '0x';
  p.statementBytes.fill(0);
  const proved = await p.prove(async () => ({ proof: proofs[1], publicInputs: sdks[0].encodeZkPublicInputs(statement) }));
  assert.deepEqual(proved.toBytes(), item.tx.toBytes()); p.checkSigned(proved, proved);
});
