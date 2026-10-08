// Invoked only by the disposable pinned-node test. Keys below are public fixtures.
import assert from 'node:assert/strict';
import { ccc } from '@ckb-ccc/shell';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { prepareIncrement } from './client.ts';
import { localProver } from './increment.ts';
import type { CounterSdk } from './client.ts';
const config = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const sdk: CounterSdk = await import(pathToFileURL(join(config.sdkDirectory, 'src/index.ts')).href);
const bootstrap = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl] });
let dao: ccc.ScriptInfoLike;
try {
  const genesis = await bootstrap.value.getBlockByNumber(0);
  assert.ok(genesis);
  // Pinned integration spec: genesis transaction output 2 contains the DAO script.
  dao = { codeHash: genesis.transactions[0].outputs[2].type!.hash(), hashType: 'type', cellDeps: [{ cellDep: { outPoint: { txHash: genesis.transactions[0].hash(), index: 2 }, depType: 'code' } }] };
} finally { await bootstrap.dispose(); }
const owner = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl], scripts: { [ccc.KnownScript.NervosDao]: dao, [ccc.KnownScript.Secp256k1Blake160]: { codeHash: config.secpCodeHash, hashType: 'type', cellDeps: [{ cellDep: config.secpDep }] } } });
// The integration genesis has secp256k1 but no ACP deployment. Limit discovery
// to that actual script; inherited preparation/signing still uses CCC sighash-all.
class LocalSecpSigner extends ccc.SignerCkbPrivateKey {
  async getAddressObjs() { return [await this.getAddressObjSecp256k1()]; }
  async getRelatedScripts(txLike: ccc.TransactionLike) {
    const address = await this.getAddressObjSecp256k1();
    const cells = await Promise.all(ccc.Transaction.from(txLike).inputs.map((input) => input.getCell(this.client)));
    return cells.some((cell) => cell.cellOutput.lock.eq(address.script))
      ? [{ script: address.script, cellDeps: (await this.client.getKnownScript(ccc.KnownScript.Secp256k1Blake160)).cellDeps }] : [];
  }
}
try {
  const signer = new LocalSecpSigner(owner.value, `0x${'45'.repeat(32)}`);
  const tool = { executable: config.proverExecutable, sha256: config.proverSha256 };
  const prover = localProver(tool, config.setupPackage, config.ownerSecretFile, sdk);
  await assert.rejects(() => prepareIncrement(signer, config.counter, { ...config.deployment, genesisHash: `0x${'00'.repeat(32)}` }, sdk), /genesis mismatch/);
  await assert.rejects(() => prepareIncrement(signer, config.counter, { ...config.deployment, child: { ...config.deployment.child, dataHash: `0x${'00'.repeat(32)}` } }, sdk), /child: live data hash/);
  const prepared = await prepareIncrement(signer, config.counter, config.deployment, sdk);
  await assert.rejects(() => prepared.prove(localProver({ ...tool, sha256: '00'.repeat(32) }, config.setupPackage, config.ownerSecretFile, sdk)), /SHA-256 mismatch/);
  const controller = new AbortController();
  const cancelled = prepared.prove(prover, { signal: controller.signal });
  controller.abort();
  await assert.rejects(() => cancelled, /cancelled|abort/i);
  const proved = await prepared.prove(prover);
  const signed = await signer.signOnlyTransaction(proved.clone());
  assert.ok(signed.witnesses.some((_, i) => ccc.bytesFrom(signed.getWitnessArgs(i)?.lock ?? '0x').length === 65), 'real secp signature missing');
  prepared.checkSigned(proved, signed);
  const altered = proved.clone(); altered.outputs[0].capacity -= 1n;
  assert.throws(() => prepared.checkSigned(proved, altered), /raw transaction changed/);
  const corrupt = proved.clone(); const args = corrupt.getWitnessArgs(0)!; const inputType = ccc.bytesFrom(args.inputType!); inputType[8] ^= 1;
  corrupt.setWitnessArgs(0, { ...args, inputType });
  await assert.rejects(() => prepared.signAndSend(signer, corrupt), /non-Lock witness/);
  // TypeScript binds bytes/PI but does not perform Groth16 verification. Keep a
  // separate prover-output corruption case which must reach the node dry-run.
  const corruptPrepared = await prepareIncrement(signer, config.counter, config.deployment, sdk);
  const corruptProved = await corruptPrepared.prove(async (statement, counters, options) => {
    const result = await prover(statement, counters, options);
    result.proof[0] ^= 1;
    return result;
  });
  await assert.rejects(() => corruptPrepared.signAndSend(signer, corruptProved), (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.match(error.message, /CKB dry-run did not succeed/);
    assert.ok(error.cause instanceof ccc.ErrorClientVerification, 'expected Script validation failure, not an unavailable RPC result');
    assert.equal(error.cause.source, 'inputType');
    assert.equal(error.cause.sourceIndex, 0n);
    assert.equal(error.cause.errorCode, 79, 'expected parent rejection of invalid child proof');
    return true;
  });
  const result = await prepared.signAndSend(signer, proved, { confirmations: 1 });
  const next = { txHash: result.hash, index: 0 };
  const second = await prepareIncrement(signer, next, config.deployment, sdk);
  const secondProved = await second.prove(prover);
  const secondResult = await second.signAndSend(signer, secondProved, { confirmations: 1 });
  await assert.rejects(() => prepareIncrement(signer, config.counter, config.deployment, sdk), /spent, missing/);
  writeFileSync(join(dirname(process.argv[2]), 'ccc-report.json'), JSON.stringify({
    status: 'passed', case: 'CCC live client', network: 'disposable pinned CKB node',
    wallet: 'real secp256k1 fee-input signature; public fixture key',
    first: result.hash, second: secondResult.hash, confirmed: true, stale_input_rejected: true,
    mutated_transaction_rejected: true, substituted_proof_pre_sign_rejected: true,
    cancelled_proving_recovered: true,
    prover_sha256: config.proverSha256, wrong_prover_pin_rejected: true,
    corrupt_proof_dry_run_rejected: true, corrupt_proof_parent_error: 79,
    wrong_genesis_rejected: true, wrong_child_rejected: true,
  }, null, 2));
} finally { await owner.dispose(); }
