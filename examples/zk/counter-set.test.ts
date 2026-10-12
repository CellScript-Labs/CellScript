import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ccc } from '@ckb-ccc/shell';
import type { CounterSdk } from './client.ts';
import { deriveCounterSet, checkCounterSetStatement, PreparedCounterSet } from './counter-set.ts';
import type { CounterSetDeployment } from './counter-set.ts';

const target = resolve(import.meta.dirname, '../../target/counter-set');
const sdk = await import(pathToFileURL(join(target, 'sdk/src/index.ts')).href) as CounterSdk;
const handle = readFileSync(join(target, 'parent-handle.bin'));
const raw = (s: string) => Uint8Array.from(Buffer.from(s, 'hex'));
const fixtures = JSON.parse(readFileSync(join(target, 'fixtures.json'), 'utf8')) as {
  count: number; transaction: string; selected_script: string; set_statement: string; public_inputs: string[];
  cells: { out_point: string; output: string; data: string }[];
}[];
function load(f: typeof fixtures[number]) {
  const tx = ccc.Transaction.fromBytes(raw(f.transaction));
  const all = f.cells.map((c) => ccc.Cell.from({ outPoint: ccc.OutPoint.fromBytes(raw(c.out_point)), cellOutput: ccc.CellOutput.fromBytes(raw(c.output)), outputData: raw(c.data) }));
  const cells = all.slice(0, tx.inputs.length), selected = ccc.Script.fromBytes(raw(f.selected_script));
  const code = (hash: ccc.HexLike) => {
    const cell = all.slice(tx.inputs.length).find((c) => ccc.hashCkb(c.outputData) === ccc.hexFrom(hash))!;
    assert.ok(cell); return { outPoint: cell.outPoint, dataHash: ccc.hashCkb(cell.outputData) };
  };
  const child = all.slice(tx.inputs.length).find((c) => c.outPoint.eq(tx.cellDeps[0].outPoint))!;
  const a = ccc.bytesFrom(selected.args);
  const deployment: CounterSetDeployment = { profile: 'cellscript-zk-counter-set-v1', familyId: a.slice(8, 40), genesisHash: `0x${'11'.repeat(32)}`,
    child: code(ccc.hashCkb(child.outputData)), verificationKey: code(`0x${sdk.zkVerifierContracts[0].verification_key_hash}`),
    parent: code(a.slice(41)), lifecycle: code(selected.codeHash), handle };
  return { tx, cells, selected, deployment };
}

test('CCC derives identical complete sets, public inputs and generated witnesses for 1/2/4 counters', async () => {
  assert.deepEqual(fixtures.map((f) => f.count), [1, 2, 4]); const rows = [];
  for (const f of fixtures) {
    const { tx, cells, selected, deployment } = load(f);
    const derived = deriveCounterSet(tx, cells, selected, sdk);
    assert.deepEqual(derived.encoded, raw(f.set_statement));
    derived.leaves.forEach((leaf, i) => assert.deepEqual(sdk.encodeZkPublicInputs(leaf.statement), raw(f.public_inputs[i])));
    const prepared = new PreparedCounterSet(tx, cells, selected, sdk, deployment);
    assert.deepEqual(prepared.statementBytes, raw(f.set_statement));
    await assert.rejects(prepared.prove([]), /complete prover count/);
    const provers = derived.leaves.map((leaf) => async (statement: Uint8Array) => {
      assert.deepEqual(statement, leaf.encoded);
      return { proof: ccc.bytesFrom(tx.getWitnessArgs(leaf.inputIndex)!.inputType!).slice(8, 136), publicInputs: sdk.encodeZkPublicInputs(leaf.statement) };
    });
    const proved = await prepared.prove(provers); assert.deepEqual(proved.toBytes(), tx.toBytes()); prepared.checkSigned(proved, proved);
    const forged = proved.clone(); forged.setWitnessArgs(0, { inputType: new Uint8Array(ccc.bytesFrom(proved.getWitnessArgs(0)!.inputType!).length) });
    assert.throws(() => prepared.checkSigned(forged, forged), /non-Lock witness/);
    const changed = proved.clone(); changed.outputs[f.count].capacity -= 1n;
    assert.throws(() => prepared.checkSigned(changed, changed), /raw transaction changed/);
    const aborted = new AbortController(); aborted.abort();
    await assert.rejects(prepared.prove(provers, { signal: aborted.signal }));
    assert.throws(() => prepared.checkSigned(proved, proved), /no complete proof set/);
    await prepared.prove(provers); prepared.checkSigned(proved, proved);
    rows.push({ count: f.count, transaction_hash: tx.hash(), complete_statement_parity: true, generated_witness_parity: true });
  }
  writeFileSync(join(target, 'ccc-parity.json'), JSON.stringify({ status: 'passed', scope: 'independent CCC/native byte parity over actual all-group VM fixtures; not live conflict evidence', rows }, null, 2));
});

test('CCC rejects every independent checker rebound set mutation', () => {
  const mutations = JSON.parse(readFileSync(join(target, 'checker-mutations.json'), 'utf8')) as { count: number; statement: string }[];
  assert.equal(mutations.length, 29);
  for (const m of mutations) {
    const { tx, cells, selected } = load(fixtures.find((f) => f.count === m.count)!);
    assert.throws(() => checkCounterSetStatement(tx, cells, selected, sdk, raw(m.statement)), /substitution/);
  }
});

test('CCC independently rejects incomplete, duplicate, reordered and substituted Cell snapshots', () => {
  for (const f of fixtures) {
    const { tx, cells, selected, deployment } = load(f);
    assert.throws(() => deriveCounterSet(tx, cells.slice(1), selected, sdk), /resolution/);
    const wrong = cells.map((c) => c.clone()); wrong[0].outPoint.index += 1n;
    assert.throws(() => deriveCounterSet(tx, wrong, selected, sdk), /resolution/);
    const owner = tx.clone(), data = ccc.bytesFrom(owner.outputsData[0]); data[8] ^= 1; owner.outputsData[0] = ccc.hexFrom(data);
    assert.throws(() => deriveCounterSet(owner, cells, selected, sdk), /owner/);
    const missing = tx.clone(); missing.outputs[0].type = undefined;
    assert.throws(() => deriveCounterSet(missing, cells, selected, sdk), /correspondence/);
    const parent = structuredClone(deployment); parent.familyId = new Uint8Array(32);
    assert.throws(() => new PreparedCounterSet(tx, cells, selected, sdk, parent), /family/);
    if (f.count > 1) {
      const reordered = tx.clone(); [reordered.outputs[0], reordered.outputs[1]] = [reordered.outputs[1], reordered.outputs[0]];
      assert.throws(() => deriveCounterSet(reordered, cells, selected, sdk), /reordered/);
      const duplicate = tx.clone(); duplicate.outputs[1] = duplicate.outputs[0].clone();
      assert.throws(() => deriveCounterSet(duplicate, cells, selected, sdk), /duplicate/);
    }
  }
});
