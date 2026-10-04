import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ccc } from '@ckb-ccc/shell';
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveStatement, PreparedCounter } from './client.ts';
import type { CounterDeployment, CounterSdk } from './client.ts';

const directory = process.env.CELLSCRIPT_ZK_WALKTHROUGH;
if (!directory) throw new Error('run examples/zk/run.sh or set CELLSCRIPT_ZK_WALKTHROUGH to its output');
const output = resolve(directory);
const sdk: CounterSdk = await import(pathToFileURL(join(output, 'sdk/src/index.ts')).href);
const bridge = JSON.parse(readFileSync(join(output, 'bridge.json'), 'utf8'));
const bytes = (value: string) => Uint8Array.from(Buffer.from(value, 'hex'));
const tx = ccc.Transaction.fromBytes(readFileSync(join(output, 'transaction.bin')));
const cell = ccc.Cell.from({ outPoint: tx.inputs[0].previousOutput, cellOutput: ccc.CellOutput.fromBytes(bytes(bridge.input_output)), outputData: bytes(bridge.input_data) });
const script = cell.cellOutput.type!;
const code = { outPoint: tx.cellDeps[0].outPoint, dataHash: ccc.hexFrom(new Uint8Array(32)) };
const deployment: CounterDeployment = { genesisHash: code.dataHash, child: code, verificationKey: { ...code, dataHash: ccc.hexFrom(bytes(bridge.verification_key_hash)) }, parent: code, lifecycle: code, handle: ccc.hexFrom(bytes(bridge.handle)) };
const prepare = () => new PreparedCounter(tx, [cell], script, sdk, deployment);
const prover = async () => ({ proof: bytes(bridge.proof), publicInputs: bytes(bridge.public_inputs) });

test('CCC derives statement independently and encodes the exact Rust witness', async () => {
  const statement = deriveStatement(tx, [cell], script);
  assert.deepEqual(sdk.encodeZkStatement(statement), bytes(bridge.statement_bytes));
  const prepared = prepare();
  const proved = await prepared.prove(prover);
  assert.equal(proved.witnesses[0], ccc.hexFrom(bytes(bridge.entry_witness)));
  assert.equal(proved.hash(), tx.hash());
  prepared.checkSigned(proved, proved);
});
test('raw mutations require reproving; Lock signatures preserve proof bytes', async () => {
  const prepared = prepare(), proved = await prepared.prove(prover);
  for (const mutate of [
    (tx: ccc.Transaction) => { tx.version += 1n; },
    (tx: ccc.Transaction) => { tx.outputs[0].capacity += 1n; },
    (tx: ccc.Transaction) => { tx.inputs[0].since += 1n; },
    (tx: ccc.Transaction) => { tx.cellDeps.reverse(); },
    (tx: ccc.Transaction) => { tx.outputsData[0] = '0x'; },
  ]) {
    const changed = proved.clone(); mutate(changed);
    assert.throws(() => prepared.checkSigned(proved, changed), /raw transaction changed/);
  }
  const signed = proved.clone();
  signed.setWitnessArgs(0, { ...signed.getWitnessArgs(0), lock: ccc.hexFrom(new Uint8Array(65)) });
  prepared.checkSigned(proved, signed);
  signed.setWitnessArgs(0, { lock: '0x' });
  assert.throws(() => prepared.checkSigned(proved, signed), /non-Lock witness/);
});
test('rejects incorrect resolution, group cardinality, owner, capacity and overflow', () => {
  const wrong = cell.clone(); wrong.outPoint.index += 1n;
  assert.throws(() => deriveStatement(tx, [wrong], script), /input resolution/);
  const duplicate = tx.clone(); duplicate.addOutput(tx.outputs[0], tx.outputsData[0]);
  assert.throws(() => deriveStatement(duplicate, [cell], script), /exactly one/);
  const capacity = tx.clone(); capacity.outputs[0].capacity -= 1n;
  assert.throws(() => deriveStatement(capacity, [cell], script), /capacity\/Lock/);
  const owner = tx.clone(); const data = ccc.bytesFrom(owner.outputsData[0]); data[8] ^= 1; owner.outputsData[0] = ccc.hexFrom(data);
  assert.throws(() => deriveStatement(owner, [cell], script), /owner or increment/);
  const max = cell.clone(); const maxData = ccc.bytesFrom(max.outputData); maxData.fill(255, 40); max.outputData = ccc.hexFrom(maxData);
  assert.throws(() => deriveStatement(tx, [max], script), /owner or increment/);
});
test('rejects wrong VK/handle/metadata, stale public inputs and malformed proofs', async () => {
  assert.throws(() => new PreparedCounter(tx, [cell], script, sdk, { ...deployment, verificationKey: code }), /VK differs/);
  assert.throws(() => new PreparedCounter(tx, [cell], script, sdk, { ...deployment, handle: new Uint8Array(202) }), /exact handle differs/);
  assert.throws(() => sdk.encodeZkTransitionWitness('missing', bytes(bridge.proof), bytes(bridge.handle)), /metadata/);
  await assert.rejects(() => prepare().prove(async () => ({ proof: new Uint8Array(127), publicInputs: bytes(bridge.public_inputs) })), /128 bytes/);
  const stale = bytes(bridge.public_inputs); stale[4] ^= 1;
  await assert.rejects(() => prepare().prove(async () => ({ proof: bytes(bridge.proof), publicInputs: stale })), /public inputs differ/);
});
test('snapshot cannot be changed through input objects or statement bytes', () => {
  const original = tx.clone(), cells = [cell.clone()];
  const prepared = new PreparedCounter(original, cells, script, sdk, deployment);
  original.version += 1n; cells[0].outputData = '0x';
  prepared.statementBytes.fill(0);
  assert.equal(prepared.transactionHash, tx.hash());
  assert.deepEqual(prepared.statementBytes, bytes(bridge.statement_bytes));
});
