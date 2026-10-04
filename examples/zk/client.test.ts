import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ccc } from '@ckb-ccc/shell';
import { readFileSync, mkdtempSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { tmpdir } from 'node:os';
import { localProver } from './increment.ts';
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
test('caller cannot substitute both sides of the proof comparison or sign a placeholder', async () => {
  const prepared = prepare();
  assert.throws(() => prepared.checkSigned(tx, tx), /no completed proof/);
  const proved = await prepared.prove(prover);
  const changed = proved.clone();
  const witness = ccc.bytesFrom(changed.getWitnessArgs(0)!.inputType!);
  witness[witness.length - 1] ^= 1;
  changed.setWitnessArgs(0, { ...changed.getWitnessArgs(0), inputType: witness });
  assert.throws(() => prepared.checkSigned(changed, changed), /non-Lock witness/);
  prepared.checkSigned(proved, proved);
});
test('cancelled and failed proving invalidate signing; late completion cannot overwrite a retry', async () => {
  const prepared = prepare(), oldProof = await prepared.prove(prover);
  const controller = new AbortController();
  let finish!: (value: Awaited<ReturnType<typeof prover>>) => void;
  const pending = prepared.prove(async (_statement, _counters, options) => {
    assert.equal(options?.signal, controller.signal);
    return new Promise((resolve) => { finish = resolve; });
  }, { signal: controller.signal });
  assert.throws(() => prepared.checkSigned(oldProof, oldProof), /no completed proof/);
  await assert.rejects(() => prepared.prove(prover), /already in progress/);
  controller.abort();
  await assert.rejects(() => pending, /cancelled/);
  assert.throws(() => prepared.checkSigned(oldProof, oldProof), /no completed proof/);
  const retry = await prepared.prove(prover);
  finish({ proof: new Uint8Array(127), publicInputs: new Uint8Array(0) });
  await Promise.resolve();
  prepared.checkSigned(retry, retry);
  await assert.rejects(() => prepared.prove(async () => { throw new Error('prover interrupted'); }), /interrupted/);
  assert.throws(() => prepared.checkSigned(retry, retry), /no completed proof/);
  const recovered = await prepared.prove(prover);
  prepared.checkSigned(recovered, recovered);
});
test('spent input fails before invoking the wallet', async () => {
  const prepared = prepare(), proved = await prepared.prove(prover);
  let signed = false;
  const signer = { client: { getCellLive: async () => undefined }, signOnlyTransaction: async () => { signed = true; return proved; } } as unknown as ccc.Signer;
  await assert.rejects(() => prepared.signAndSend(signer, proved), /spent, missing or inconsistent/);
  assert.equal(signed, false);
});
test('local prover failure redacts captured private data and removes witness files', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'counter-prover-test-'));
  try {
    const executable = join(directory, 'prover'), secret = join(directory, 'secret.bin');
    writeFileSync(secret, new Uint8Array(32).fill(123), { mode: 0o600 });
    writeFileSync(executable, '#!/bin/sh\ndirname "$4" > "$2/recorded"\ncat "$4" >&2\nexit 9\n', { mode: 0o700 });
    const prepared = prepare();
    await assert.rejects(() => prepared.prove(localProver(executable, directory, secret, sdk)), (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.equal(error.message, 'local prover failed or timed out; no proof accepted');
      assert.equal(error.cause, undefined);
      assert.ok(!String(error.stack).includes('123,123'));
      return true;
    });
    assert.equal(existsSync(readFileSync(join(directory, 'recorded'), 'utf8').trim()), false);
    assert.throws(() => prepared.checkSigned(tx, tx), /no completed proof/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
