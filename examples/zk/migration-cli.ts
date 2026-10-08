// Use a locally trusted manifest digest and generated SDKs. Does not deploy or
// admit a new profile. This command spends the explicitly configured instance.
import { ccc } from '@ckb-ccc/shell';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { MigrationDeployment, prepareMigration } from './migration.ts';
import { localProver } from './increment.ts';
import type { CounterSdk } from './client.ts';
if (process.argv.length !== 3) throw new Error('usage: node --experimental-strip-types examples/zk/migration-cli.ts CONFIG_JSON');
const config: {
  rpcUrl: string; manifestFile: string; manifestDigest: ccc.HexLike; sdkDirectories: [string, string]; handles: [ccc.HexLike, ccc.HexLike];
  counter: ccc.OutPointLike; configuration: ccc.OutPointLike; mode: 'update' | 'migrate'; lockDeps: ccc.CellDepLike[];
  proverExecutable: string; proverSha256: string; setupPackages: [string, string]; ownerSecretFile: string; walletKeyFile: string;
} = JSON.parse(readFileSync(process.argv[2], 'utf8'));
if (!['update', 'migrate'].includes(config.mode)) throw new Error('mode must explicitly select update or migrate');
const sdks = await Promise.all(config.sdkDirectories.map((dir) => import(pathToFileURL(resolve(dir, 'src/index.ts')).href))) as [CounterSdk, CounterSdk];
const deployment = new MigrationDeployment(readFileSync(config.manifestFile), config.manifestDigest, sdks, config.handles);
const owner = ccc.ClientPublicTestnet.open({ urls: [config.rpcUrl] });
try {
  const signer = new ccc.SignerCkbPrivateKey(owner.value, readFileSync(config.walletKeyFile, 'utf8').trim() as ccc.Hex);
  const prepared = await prepareMigration(signer, config.counter, config.configuration, deployment, config.mode === 'migrate', config.lockDeps);
  const version = prepared.selectedVersion;
  const proof = await prepared.prove(localProver({ executable: config.proverExecutable, sha256: config.proverSha256 }, config.setupPackages[version], config.ownerSecretFile, sdks[version]));
  console.error(JSON.stringify({ stage: 'prepared', transactionHash: prepared.transactionHash }));
  const result = await prepared.signAndSend(signer, proof);
  console.log(JSON.stringify({ status: 'committed', transactionHash: result.hash, counter: { txHash: result.hash, index: 0 }, configuration: config.mode === 'migrate' ? { txHash: result.hash, index: 1 } : config.configuration }));
} finally { await owner.dispose(); }
