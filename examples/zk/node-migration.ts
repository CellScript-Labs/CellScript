// Disposable pinned-node acceptance only. All wallet/owner secrets are public fixtures.
import assert from 'node:assert/strict';
import { ccc } from '@ckb-ccc/shell';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { MigrationDeployment, prepareMigration } from './migration.ts';
import { localProver } from './increment.ts';
import type { CounterSdk } from './client.ts';
const config = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const sdks = await Promise.all([0, 1].map((i) => import(pathToFileURL(join(config.directory, `sdk-${i}/src/index.ts`)).href))) as [CounterSdk, CounterSdk];
const deployment = new MigrationDeployment(readFileSync(join(config.directory, 'manifest.json')), config.manifestDigest, sdks, config.handles);
const bootstrap = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl] });
let dao: ccc.ScriptInfoLike;
try {
  const genesis = await bootstrap.value.getBlockByNumber(0); assert.ok(genesis);
  dao = { codeHash: genesis.transactions[0].outputs[2].type!.hash(), hashType: 'type', cellDeps: [{ cellDep: { outPoint: { txHash: genesis.transactions[0].hash(), index: 2 }, depType: 'code' } }] };
} finally { await bootstrap.dispose(); }
const owner = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl], scripts: { [ccc.KnownScript.NervosDao]: dao, [ccc.KnownScript.Secp256k1Blake160]: { codeHash: config.secpCodeHash, hashType: 'type', cellDeps: [{ cellDep: config.secpDep }] } } });
class LocalSecpSigner extends ccc.SignerCkbPrivateKey {
  async getAddressObjs() { return [await this.getAddressObjSecp256k1()]; }
  async getRelatedScripts(txLike: ccc.TransactionLike) {
    const address = await this.getAddressObjSecp256k1();
    const cells = await Promise.all(ccc.Transaction.from(txLike).inputs.map((input) => input.getCell(this.client)));
    return cells.some((cell) => cell.cellOutput.lock.eq(address.script)) ? [{ script: address.script, cellDeps: (await this.client.getKnownScript(ccc.KnownScript.Secp256k1Blake160)).cellDeps }] : [];
  }
}
try {
  const signer = new LocalSecpSigner(owner.value, `0x${'45'.repeat(32)}`);
  const tool = { executable: config.proverExecutable, sha256: config.proverSha256 };
  const provers = [0, 1].map((i) => localProver(tool, config.setupPackages[i], config.ownerSecretFile, sdks[i]));
  let counter = config.counter, currentConfig = config.configuration;
  const rows = [];
  for (let i = 0; i < 3; i++) {
    const migrating = i === 1, selected = i === 2 ? 1 : 0;
    const prepared = await prepareMigration(signer, counter, currentConfig, deployment, migrating, config.lockDeps);
    assert.equal(prepared.selectedVersion, selected); assert.equal(prepared.isMigration, migrating);
    // A proof from the other VK is well-formed for the same final statement.
    // CCC performs binding/encoding; the actual node must reject it at parent 79.
    if (i > 0) {
      const wrong = await prepared.prove(provers[1 - selected]);
      await assert.rejects(() => prepared.signAndSend(signer, wrong), (error: unknown) => {
        assert.ok(error instanceof Error && error.cause instanceof ccc.ErrorClientVerification);
        assert.equal(error.cause.source, 'inputType'); assert.equal(error.cause.sourceIndex, 0n); assert.equal(error.cause.errorCode, 79); return true;
      });
    }
    const proved = await prepared.prove(provers[selected]);
    const signed = await signer.signOnlyTransaction(proved.clone());
    assert.ok(signed.witnesses.some((_, index) => ccc.bytesFrom(signed.getWitnessArgs(index)?.lock ?? '0x').length === 65));
    prepared.checkSigned(proved, signed);
    const cycles = await owner.value.sendTransactionDry(signed, 'passthrough');
    assert.ok(cycles > 0n && cycles < 250_000_000n, 'migration whole-transaction cycle budget');
    const changed = proved.clone(); changed.outputsData[migrating ? 1 : 0] = '0x';
    assert.throws(() => prepared.checkSigned(proved, changed), /raw transaction changed/);
    const result = await prepared.signAndSend(signer, proved, { confirmations: 1 });
    const oldCounter = counter, oldConfig = currentConfig;
    counter = { txHash: result.hash, index: 0 };
    if (migrating) currentConfig = { txHash: result.hash, index: 1 };
    assert.equal(await owner.value.getCellLive(oldCounter, true, true), undefined);
    const next = await owner.value.getCellLive(counter, true, true); assert.ok(next);
    const successorData = ccc.bytesFrom(next.outputData);
    assert.equal(new DataView(successorData.buffer, successorData.byteOffset, successorData.byteLength).getBigUint64(40, true), BigInt(i + 1));
    if (migrating) {
      assert.equal(await owner.value.getCellLive(oldConfig, true, true), undefined);
      const configCell = await owner.value.getCellLive(currentConfig, true, true); assert.ok(configCell); assert.equal(deployment.selection(configCell), 1);
      await assert.rejects(() => prepareMigration(signer, counter, oldConfig, deployment, false, config.lockDeps), /configuration: outpoint is spent/);
      await assert.rejects(() => prepareMigration(signer, counter, currentConfig, deployment, true, config.lockDeps), /second migration forbidden/);
    }
    rows.push({ case: ['old-key-update', 'old-key-authorized-migration', 'new-key-successor-update'][i], transaction_hash: result.hash, selected_version: selected, confirmed: true, fee_signature: true, cycles: Number(cycles), transaction_bytes: signed.toBytes().length });
  }
  writeFileSync(join(config.directory, 'ccc-migration-report.json'), JSON.stringify({ status: 'passed', scope: 'disposable pinned CKB node; normal sendTransaction admission', production_admitted: false,
    manifest_data_hash: config.manifestDigest, wrong_key_parent_error: 79, new_key_self_installation_rejected: true, old_key_after_migration_rejected: true,
    actual_successor_lineage: true, stale_configuration_rejected: true, second_migration_rejected: true, raw_mutation_rejected: true, rows }, null, 2));
} finally { await owner.dispose(); }
