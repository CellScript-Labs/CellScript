// Run a counter update with a CCC signer and the native local prover.
import { ccc } from '@ckb-ccc/shell';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, fstatSync, mkdtempSync, openSync, readFileSync, readSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import { prepareIncrement } from './client.ts';
import type { CounterDeployment, CounterSdk, Prover } from './client.ts';

export interface PinnedProver { executable: string; sha256: string }

// Read and hash a bounded snapshot, then execute that snapshot. Checking the
// source path and later executing it would permit substitution between steps.
function proverSnapshot(tool: PinnedProver): Buffer {
  if (!/^[0-9a-f]{64}$/.test(tool.sha256)) throw new Error('local prover pin must be a lowercase SHA-256 digest');
  const fd = openSync(tool.executable, 'r');
  try {
    const info = fstatSync(fd);
    if (!info.isFile() || info.size === 0 || info.size > 64 * 1024 * 1024) throw new Error('local prover must be a regular file of 1–67108864 bytes');
    const buffer = Buffer.alloc(info.size + 1);
    let length = 0;
    while (length < buffer.length) {
      const read = readSync(fd, buffer, length, buffer.length - length, null);
      if (read === 0) break;
      length += read;
    }
    if (length !== info.size) throw new Error('local prover changed while reading');
    const bytes = buffer.subarray(0, length);
    if (createHash('sha256').update(bytes).digest('hex') !== tool.sha256) throw new Error('local prover SHA-256 mismatch; no private witness read');
    return bytes;
  } finally { closeSync(fd); }
}

export function localProver(tool: PinnedProver, setupPackage: string, secretFile: string, sdk: CounterSdk): Prover {
  // Capture the caller-approved identity, not a mutable configuration object.
  const pin = { ...tool };
  return async (statement, counters, { signal } = {}) => {
    signal?.throwIfAborted();
    const executableBytes = proverSnapshot(pin);
    const directory = mkdtempSync(join(tmpdir(), 'cellscript-counter-'));
    try {
      const executable = join(directory, 'prover');
      writeFileSync(executable, executableBytes, { mode: 0o700, flag: 'wx' });
      signal?.throwIfAborted();
      const secret = readFileSync(secretFile);
      if (secret.length !== 32) throw new Error('owner secret file must contain exactly 32 binary bytes');
      writeFileSync(join(directory, 'statement.bin'), statement, { mode: 0o600 });
      // Emit u64 as JSON integer tokens without conversion through JavaScript Number.
      writeFileSync(join(directory, 'witness.json'), `{"secret":${JSON.stringify([...secret])},"old_counter":${counters.oldCounter},"new_counter":${counters.newCounter}}`, { mode: 0o600 });
      try {
        await promisify(execFile)(executable, ['prove', setupPackage, join(directory, 'statement.bin'), join(directory, 'witness.json'), join(directory, 'proof.bin')], { timeout: 120_000, maxBuffer: 64 * 1024, killSignal: 'SIGKILL', signal });
      } catch {
        signal?.throwIfAborted();
        // Child diagnostics may contain the private witness; never expose them.
        throw new Error('local prover failed or timed out; no proof accepted');
      }
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
  const config: { rpcUrl: string; sdkDirectory: string; proverExecutable: string; proverSha256: string; setupPackage: string; ownerSecretFile: string; walletKeyFile: string; counter: ccc.OutPointLike; deployment: CounterDeployment } = JSON.parse(readFileSync(file, 'utf8'));
  const sdk: CounterSdk = await import(pathToFileURL(resolve(config.sdkDirectory, 'src/index.ts')).href);
  const client = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl] });
  try {
    const signer = new ccc.SignerCkbPrivateKey(client.value, readFileSync(config.walletKeyFile, 'utf8').trim() as ccc.Hex);
    const prepared = await prepareIncrement(signer, config.counter, config.deployment, sdk);
    const proved = await prepared.prove(localProver({ executable: config.proverExecutable, sha256: config.proverSha256 }, config.setupPackage, config.ownerSecretFile, sdk));
    const result = await prepared.signAndSend(signer, proved);
    console.log(JSON.stringify({ status: 'committed', transactionHash: result.hash }));
  } finally { await client.dispose(); }
}
