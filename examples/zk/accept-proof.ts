// Run after run.sh. Imports the generated SDK, not a copied codec implementation.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';

const directory = process.argv[2];
if (!directory) throw new Error('usage: node --experimental-strip-types examples/zk/accept-proof.ts OUTPUT_DIR');
const output = resolve(directory);
const sdk = await import(pathToFileURL(join(output, 'sdk/src/index.ts')).href);
const bridge = JSON.parse(readFileSync(join(output, 'bridge.json'), 'utf8'));
const bytes = (hex: string): Uint8Array => {
  assert.match(hex, /^(?:[0-9a-f]{2})*$/i);
  return Uint8Array.from(Buffer.from(hex, 'hex'));
};
// The Rust example derived these fields from its finalized transaction and Cells.
// A real client must derive them from its own chain provider, never trust a prover's fields.
const statement = {
  domain: bytes(bridge.statement.domain), action: bytes(bridge.statement.action),
  scriptHash: bytes(bridge.statement.scriptHash), oldDataHash: bytes(bridge.statement.oldDataHash),
  newDataHash: bytes(bridge.statement.newDataHash), inputTransactionHash: bytes(bridge.statement.inputTransactionHash),
  inputOutputIndex: bridge.statement.inputOutputIndex, transactionHash: bytes(bridge.statement.transactionHash),
};
const proof = bytes(bridge.proof);
const key = bytes(bridge.verification_key_hash);
const inputs = bytes(bridge.public_inputs);
assert.deepEqual(sdk.encodeZkStatement(statement), bytes(bridge.statement_bytes));
assert.deepEqual(sdk.encodeZkPublicInputs(statement), inputs);
const request = sdk.bindZkProverOutput(statement, key, proof, inputs);
assert.deepEqual(request, bytes(bridge.request));

// A changed transaction requires a new proof; stale public inputs must fail locally.
const changedHash = statement.transactionHash.slice();
changedHash[0] ^= 1;
assert.throws(() => sdk.bindZkProverOutput({ ...statement, transactionHash: changedHash }, key, proof, inputs), /public inputs differ/);
assert.throws(() => sdk.bindZkProverOutput(statement, key, proof.slice(1), inputs), /128 bytes/);

// This API validates correspondence/width, not Groth16. Even a corrupt proof can
// pass this check if its public inputs match; the on-chain verifier must reject it.
const corrupt = proof.slice();
corrupt[0] ^= 1;
sdk.bindZkProverOutput(statement, key, corrupt, inputs);
const plan = sdk.planIncrement({ proof, verifier: bytes(bridge.handle) });
assert.equal(plan.canSubmit, false);
assert.equal(plan.requiresLiveCellResolution, true);
writeFileSync(join(output, 'typescript-report.json'), JSON.stringify({
  status: 'passed', evidence_level: 'generated SDK codec and plan; not cryptographic or chain verification',
  statement_bytes: 228, public_input_bytes: inputs.length, proof_bytes: proof.length, request_bytes: request.length,
  matched_rust: true, stale_transaction_rejected: true, truncated_proof_rejected: true,
  corrupt_proof_requires_onchain_verifier: true, plan_status: plan.status, can_submit: plan.canSubmit,
}, null, 2) + '\n');
console.log('Rust/TypeScript bytes match; stale transaction and truncated proof rejected.');
console.log(`Generated plan: ${plan.status}, canSubmit=${plan.canSubmit}. RPC/wallet integration is still required.`);
