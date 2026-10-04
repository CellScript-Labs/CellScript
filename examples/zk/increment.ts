// Run a counter update with a CCC signer and the native local prover.
import { ccc } from '@ckb-ccc/shell';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { prepareIncrement } from './client.ts';
import type { CounterDeployment, CounterSdk, Prover } from './client.ts';

export function localProver(executable: string, setupPackage: string, secretFile: string, sdk: CounterSdk): Prover {
  return async (statement, counters) => {
    const directory = mkdtempSync(join(tmpdir(), 'cellscript-counter-'));
    try {
      const secret = readFileSync(secretFile);
      if (secret.length !== 32) throw new Error('owner secret file must contain exactly 32 binary bytes');
      writeFileSync(join(directory, 'statement.bin'), statement, { mode: 0o600 });
      // Emit u64 as JSON integer tokens without conversion through JavaScript Number.
      writeFileSync(join(directory, 'witness.json'), `{"secret":${JSON.stringify([...secret])},"old_counter":${counters.oldCounter},"new_counter":${counters.newCounter}}`, { mode: 0o600 });
      execFileSync(executable, ['prove', setupPackage, join(directory, 'statement.bin'), join(directory, 'witness.json'), join(directory, 'proof.bin')], { stdio: ['ignore', 'ignore', 'pipe'], timeout: 120_000 });
      // Public inputs are canonical limbs of the exact statement sent to this prover.
      const view = new DataView(statement.buffer, statement.byteOffset, statement.byteLength);
      const publicInputs = sdk.encodeZkPublicInputs({ domain: statement.slice(0,32), action: statement.slice(32,64), scriptHash: statement.slice(64,96), oldDataHash: statement.slice(96,128), newDataHash: statement.slice(128,160), inputTransactionHash: statement.slice(160,192), inputOutputIndex: view.getUint32(192,true), transactionHash: statement.slice(196,228) });
      return { proof: readFileSync(join(directory, 'proof.bin')), publicInputs };
    } finally { rmSync(directory, { recursive: true, force: true }); }
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const file = process.argv[2];
  if (!file || process.argv.length !== 3) throw new Error('usage: node --experimental-strip-types examples/zk/increment.ts CONFIG_JSON');
  const config: { rpcUrl: string; sdkDirectory: string; proverExecutable: string; setupPackage: string; ownerSecretFile: string; walletKeyFile: string; counter: ccc.OutPointLike; deployment: CounterDeployment } = JSON.parse(readFileSync(file, 'utf8'));
  const sdk: CounterSdk = await import(pathToFileURL(resolve(config.sdkDirectory, 'src/index.ts')).href);
  const client = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl] });
  try {
    const signer = new ccc.SignerCkbPrivateKey(client.value, readFileSync(config.walletKeyFile, 'utf8').trim() as ccc.Hex);
    const prepared = await prepareIncrement(signer, config.counter, config.deployment, sdk);
    const proved = await prepared.prove(localProver(config.proverExecutable, config.setupPackage, config.ownerSecretFile, sdk));
    const result = await prepared.signAndSend(signer, proved);
    console.log(JSON.stringify({ status: 'committed', transactionHash: result.hash }));
  } finally { await client.dispose(); }
}
