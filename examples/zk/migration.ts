// Fixed-pair migration adapter. The manifest digest and generated SDK modules
// come from the application's trusted build; this client does not admit code or setup.
import { createHash } from 'node:crypto';
import { ccc } from '@ckb-ccc/shell';
import { decodeCounter, deriveStatement, PreparedCounter, verifyBinding } from './client.ts';
import type { CodeCell, CounterDeployment, CounterSdk } from './client.ts';

type NativeCode = { tx_hash: string; index: number; data_hash: string };
type Setup = {
  schema: string; circuit: { application: string; r1cs_sha256: string; constraints: number; public_inputs: number; witnesses: number };
  rust_toolchain: string; dependency_lock_sha256: string; proving_key_sha256: string;
  verification_key_sha256: string; verification_key_data_hash: string; setup_kind: string;
  setup_unix_seconds: number; production_admitted: boolean;
};
type Version = {
  verifier: { chain_id: string; genesis_hash: string; child_tx_hash: string; child_index: number; child_data_hash: string; verification_key_hash: string };
  parent: NativeCode; key: NativeCode; setup: Setup;
};
type Manifest = {
  schema: string; chain_id: string; genesis_hash: string; lifecycle: NativeCode; config_guard: NativeCode;
  instance: { counter: string; configuration: string }; versions: [Version, Version];
};
const hex = (value: string): ccc.Hex => ccc.hexFrom(value.startsWith('0x') ? value as ccc.Hex : `0x${value}`);
const equal = (a: ccc.HexLike, b: ccc.HexLike) => ccc.hexFrom(a) === ccc.hexFrom(b);
function requireThat(ok: unknown, message: string): asserts ok { if (!ok) throw new Error(message); }
function exact(value: object, names: string[], label: string) {
  requireThat(value && typeof value === 'object' && !Array.isArray(value), `migration ${label}: object required`);
  requireThat(Object.keys(value).length === names.length && names.every((key) => Object.hasOwn(value, key)), `migration ${label}: missing or unknown fields`);
}
function hash32(value: string): ccc.Hex { const result = hex(value); requireThat(ccc.bytesFrom(result).length === 32, 'migration hash must have 32 bytes'); return result; }
function code(value: NativeCode): CodeCell {
  exact(value, ['tx_hash', 'index', 'data_hash'], 'code');
  requireThat(Number.isInteger(value.index) && value.index >= 0 && value.index <= 0xffffffff, 'migration code index must be u32');
  return { outPoint: { txHash: hash32(value.tx_hash), index: value.index }, dataHash: hash32(value.data_hash) };
}
async function live(client: ccc.Client, point: ccc.OutPointLike, name: string): Promise<ccc.Cell> {
  const cell = await client.getCellLive(point, true, true);
  requireThat(cell && cell.outPoint.eq(point), `${name}: outpoint is spent, missing or inconsistent; re-prepare migration`);
  return cell;
}
export function migrationBounds(tx: ccc.Transaction): void {
  requireThat(tx.inputs.length <= 64 && tx.outputs.length <= 64 && tx.cellDeps.length <= 64, 'migration exceeds 64 Cells/dependencies');
  requireThat(tx.outputs.length === tx.outputsData.length, 'migration output/data count mismatch');
  requireThat(tx.toBytes().length <= 16384, 'migration exceeds 16384 bytes including witnesses');
}
function members(cell: ccc.Cell): ccc.OutPoint[] {
  const data = ccc.bytesFrom(cell.outputData);
  requireThat(data.length <= 4 + 64 * 36, 'migration dep-group exceeds bounds');
  const points = ccc.mol.vector(ccc.OutPoint).decode(data);
  requireThat(points.length > 0, 'migration empty dep-group');
  return points.map(ccc.OutPoint.from);
}
export function expandMigrationDependencies(tx: ccc.Transaction, observations: ccc.Cell[]): ccc.Cell[] {
  requireThat(observations.length <= 128, 'migration dependency observations exceed bounds');
  let bytes = 0;
  const map = new Map<ccc.Hex, ccc.Cell>();
  for (const cell of observations) {
    bytes += ccc.bytesFrom(cell.outputData).length;
    requireThat(bytes <= 16 * 1024 * 1024, 'migration dependency observations exceed 16 MiB');
    const key = ccc.hexFrom(cell.outPoint.toBytes());
    requireThat(!map.has(key), 'migration duplicate dependency observation'); map.set(key, cell);
  }
  const find = (point: ccc.OutPointLike) => { const cell = map.get(ccc.hexFrom(ccc.OutPoint.from(point).toBytes())); requireThat(cell, 'migration dependency observation missing'); return cell; };
  const resolved: ccc.Cell[] = [];
  for (const dep of tx.cellDeps) {
    const cell = find(dep.outPoint);
    if (dep.depType === 'code') resolved.push(cell);
    else { requireThat(dep.depType === 'depGroup', 'migration unknown dependency type'); resolved.push(...members(cell).map(find)); }
    requireThat(resolved.length <= 64, 'migration resolved dependencies exceed 64');
  }
  return resolved;
}
async function resolveDependencies(client: ccc.Client, tx: ccc.Transaction): Promise<ccc.Cell[]> {
  migrationBounds(tx);
  const map = new Map<ccc.Hex, ccc.Cell>();
  let resolvedCount = 0;
  const fetch = async (point: ccc.OutPointLike) => {
    const id = ccc.hexFrom(ccc.OutPoint.from(point).toBytes());
    if (!map.has(id)) map.set(id, await live(client, point, 'dependency'));
    return map.get(id)!;
  };
  for (const dep of tx.cellDeps) {
    const cell = await fetch(dep.outPoint);
    const points = dep.depType === 'depGroup' ? members(cell) : [cell.outPoint];
    resolvedCount += points.length;
    requireThat(resolvedCount <= 64, 'migration resolved dependencies exceed 64');
    for (const point of points) await fetch(point);
  }
  const cells = [...map.values()]; expandMigrationDependencies(tx, cells); return cells;
}

/** Immutable pinned instance and two versions. Pin exact bytes, not JSON object order. */
export class MigrationDeployment {
  #manifest: Manifest;
  #counter: ccc.Script;
  #config: ccc.Script;
  #sdks: readonly [CounterSdk, CounterSdk];
  #handles: [ccc.Hex, ccc.Hex];
  constructor(bytes: Uint8Array, trustedDigest: ccc.HexLike, sdks: readonly [CounterSdk, CounterSdk], handles: readonly [ccc.HexLike, ccc.HexLike]) {
    requireThat(sdks.length === 2 && handles.length === 2, 'migration requires two SDKs and handles');
    requireThat(bytes.length <= 32768 && equal(ccc.hashCkb(bytes), trustedDigest), 'migration manifest exceeds bounds or differs from trusted digest');
    const m: Manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    exact(m, ['schema', 'chain_id', 'genesis_hash', 'lifecycle', 'config_guard', 'instance', 'versions'], 'manifest');
    requireThat(m.schema === 'cellscript-counter-migration-v1' && typeof m.chain_id === 'string' && m.chain_id.length > 0, 'migration manifest schema/chain');
    hash32(m.genesis_hash); code(m.lifecycle); code(m.config_guard);
    exact(m.instance, ['counter', 'configuration'], 'instance');
    requireThat(Array.isArray(m.versions) && m.versions.length === 2, 'migration requires two versions');
    for (const version of m.versions) {
      exact(version, ['verifier', 'parent', 'key', 'setup'], 'version'); code(version.parent); code(version.key);
      exact(version.verifier, ['chain_id', 'genesis_hash', 'child_tx_hash', 'child_index', 'child_data_hash', 'verification_key_hash'], 'verifier');
      code({ tx_hash: version.verifier.child_tx_hash, index: version.verifier.child_index, data_hash: version.verifier.child_data_hash });
      requireThat(version.verifier.chain_id === m.chain_id && hash32(version.verifier.genesis_hash) === hash32(m.genesis_hash), 'migration parent network mismatch');
      const setup = version.setup;
      exact(setup, ['schema', 'circuit', 'rust_toolchain', 'dependency_lock_sha256', 'proving_key_sha256', 'verification_key_sha256', 'verification_key_data_hash', 'setup_kind', 'setup_unix_seconds', 'production_admitted'], 'setup');
      exact(setup.circuit, ['application', 'r1cs_sha256', 'constraints', 'public_inputs', 'witnesses'], 'circuit');
      requireThat(setup.schema === 'cellscript-counter-setup-v1' && setup.production_admitted === false && ['public-test-seed', 'local-single-party'].includes(setup.setup_kind) && setup.rust_toolchain === '1.97.1', 'migration unsupported or self-admitted setup');
      requireThat(setup.circuit.application === 'cellscript-private-counter-v1' && setup.circuit.r1cs_sha256 === 'ec73a1e355384c722dcae3013f026afd8c97b9d6c804376277111f33df7d40d0' && setup.circuit.constraints === 149179 && setup.circuit.public_inputs === 15 && setup.circuit.witnesses === 147904, 'migration circuit identity mismatch');
      for (const digest of [setup.dependency_lock_sha256, setup.proving_key_sha256, setup.verification_key_sha256]) requireThat(/^[0-9a-f]{64}$/.test(digest), 'migration malformed setup hash');
      requireThat(hash32(setup.verification_key_data_hash) === hash32(version.key.data_hash) && hash32(version.verifier.verification_key_hash) === hash32(version.key.data_hash), 'migration VK commitments disagree');
    }
    requireThat(hash32(m.versions[0].parent.data_hash) !== hash32(m.versions[1].parent.data_hash) && hash32(m.versions[0].key.data_hash) !== hash32(m.versions[1].key.data_hash), 'migration versions must have distinct parents and keys');
    requireThat(m.versions[0].setup.dependency_lock_sha256 === m.versions[1].setup.dependency_lock_sha256, 'migration setup dependency identity mismatch');
    this.#manifest = m; this.#sdks = [...sdks] as [CounterSdk, CounterSdk]; this.#handles = handles.map(ccc.hexFrom) as [ccc.Hex, ccc.Hex];
    this.#counter = ccc.Script.fromBytes(ccc.bytesFrom(hex(m.instance.counter)));
    this.#config = ccc.Script.fromBytes(ccc.bytesFrom(hex(m.instance.configuration)));
    const ca = ccc.bytesFrom(this.#counter.args), cf = ccc.bytesFrom(this.#config.args);
    requireThat(this.#counter.hashType === 'data2' && this.#config.hashType === 'data2' && equal(this.#counter.codeHash, code(m.lifecycle).dataHash) && equal(this.#config.codeHash, code(m.config_guard).dataHash), 'migration instance code/hash type mismatch');
    requireThat(ca.length === 64 && cf.length === 160 && equal(ca.slice(32), this.#config.hash()) && equal(ca.slice(0, 32), cf.slice(32, 64)) && equal(cf.slice(64, 96), this.#counter.codeHash) && equal(cf.slice(96, 128), hash32(m.versions[0].parent.data_hash)) && equal(cf.slice(128), hash32(m.versions[1].parent.data_hash)), 'migration paired instance args mismatch');
    for (const version of [0, 1]) verifyBinding(this.sdk(version), this.counterDeployment(version));
  }
  counterScript(): ccc.Script { return this.#counter.clone(); }
  configScript(): ccc.Script { return this.#config.clone(); }
  sdk(version: number): CounterSdk { requireThat(version === 0 || version === 1, 'migration version'); return this.#sdks[version]; }
  counterDeployment(version: number): CounterDeployment {
    requireThat(version === 0 || version === 1, 'migration version'); const m = this.#manifest, v = m.versions[version];
    return { genesisHash: hash32(m.genesis_hash), child: code({ tx_hash: v.verifier.child_tx_hash, index: v.verifier.child_index, data_hash: v.verifier.child_data_hash }), verificationKey: code(v.key), parent: code(v.parent), lifecycle: code(m.lifecycle), handle: this.#handles[version] };
  }
  configData(selector: number): ccc.Hex {
    requireThat(selector === 0 || selector === 1, 'migration selector must be 0 or 1');
    return ccc.hexFrom(Uint8Array.from([...new TextEncoder().encode('CSZKCFG1'), selector, ...ccc.bytesFrom(hash32(this.#manifest.versions[selector].parent.data_hash))]));
  }
  selection(config: ccc.Cell): number {
    requireThat(config.cellOutput.type?.eq(this.#config), 'migration configuration differs from pinned instance');
    const selector = ccc.bytesFrom(config.outputData)[8];
    requireThat(equal(config.outputData, this.configData(selector)), 'migration configuration magic/parent mismatch'); return selector;
  }
  dependencies(version: number, migrating: boolean, config: ccc.OutPointLike): ccc.CellDepLike[] {
    const d = this.counterDeployment(version);
    const codes = [d.child, d.verificationKey, d.parent, d.lifecycle];
    if (migrating) codes.push(code(this.#manifest.config_guard));
    const deps: ccc.CellDepLike[] = codes.map((c) => ({ outPoint: c.outPoint, depType: 'code' }));
    if (!migrating) deps.push({ outPoint: config, depType: 'code' }); return deps;
  }
  validate(tx: ccc.Transaction, cells: ccc.Cell[], observations: ccc.Cell[]): { version: number; migrating: boolean } {
    migrationBounds(tx); deriveStatement(tx, cells, this.#counter);
    requireThat(new Set(cells.map((cell) => ccc.hexFrom(cell.outPoint.toBytes()))).size === cells.length, 'migration duplicate inputs');
    const deps = expandMigrationDependencies(tx, observations);
    for (const dep of deps) { const input = cells.find((cell) => cell.outPoint.eq(dep.outPoint)); if (input) requireThat(equal(input.cellOutput.toBytes(), dep.cellOutput.toBytes()) && input.outputData === dep.outputData, 'migration input/dependency observations disagree'); }
    const inputs = cells.filter((cell) => cell.cellOutput.type?.eq(this.#config));
    const outputs = tx.outputs.map((cell, i) => ({ cell, data: tx.outputsData[i] })).filter(({ cell }) => cell.type?.eq(this.#config));
    const configs = deps.filter((cell) => cell.cellOutput.type?.eq(this.#config));
    let config: ccc.Cell, migrating: boolean;
    if (inputs.length === 0 && outputs.length === 0 && configs.length === 1) { config = configs[0]; migrating = false; }
    else {
      requireThat(inputs.length === 1 && outputs.length === 1 && configs.length === 0, 'migration requires one configuration dependency or input/output pair without overlap');
      config = inputs[0]; migrating = true;
      requireThat(config.cellOutput.capacity === outputs[0].cell.capacity && config.cellOutput.lock.eq(outputs[0].cell.lock), 'migration configuration Lock/capacity changed');
      requireThat(this.selection(config) === 0 && equal(outputs[0].data, this.configData(1)), 'migration requires one 0 -> 1 switch');
    }
    const version = this.selection(config), deployment = this.counterDeployment(version);
    requireThat(deps[0]?.outPoint.eq(deployment.child.outPoint), 'migration selected child must occupy resolved dependency slot 0');
    const required = [deployment.child, deployment.verificationKey, deployment.parent, deployment.lifecycle];
    if (migrating) required.push(code(this.#manifest.config_guard));
    for (const expected of required) {
      const matches = deps.filter((cell) => equal(ccc.hashCkb(cell.outputData), expected.dataHash));
      requireThat(matches.length === 1 && matches[0].outPoint.eq(expected.outPoint), 'migration artifact dependency missing, ambiguous or at another OutPoint');
    }
    const key = deps.find((cell) => cell.outPoint.eq(deployment.verificationKey.outPoint))!;
    requireThat(createHash('sha256').update(ccc.bytesFrom(key.outputData)).digest('hex') === this.#manifest.versions[version].setup.verification_key_sha256, 'migration VK SHA-256 differs from setup');
    return { version, migrating };
  }
  async verifyNetwork(client: ccc.Client): Promise<void> {
    const genesis = await client.getBlockByNumber(0);
    requireThat(genesis && equal(genesis.header.hash, hash32(this.#manifest.genesis_hash)), 'migration genesis mismatch');
  }
  async verifyArtifacts(client: ccc.Client): Promise<void> {
    await this.verifyNetwork(client);
    // Check both versions before authorizing a switch, including code that is
    // not executed until the next update. These observations cannot promise
    // future liveness, so repeat them immediately before signing/dry-run.
    for (const version of [0, 1]) {
      const d = this.counterDeployment(version);
      for (const expected of [d.child, d.verificationKey, d.parent, d.lifecycle, code(this.#manifest.config_guard)]) {
        const cell = await live(client, expected.outPoint, 'migration artifact');
        requireThat(equal(ccc.hashCkb(cell.outputData), expected.dataHash), 'migration live artifact data hash mismatch');
        if (cell.outPoint.eq(d.verificationKey.outPoint)) requireThat(createHash('sha256').update(ccc.bytesFrom(cell.outputData)).digest('hex') === this.#manifest.versions[version].setup.verification_key_sha256, 'migration live VK SHA-256 mismatch');
      }
    }
  }
}

export class PreparedMigration extends PreparedCounter {
  #deployment: MigrationDeployment;
  #tx: ccc.Transaction;
  #cells: ccc.Cell[];
  readonly selectedVersion: number;
  readonly isMigration: boolean;
  constructor(tx: ccc.Transaction, cells: ccc.Cell[], observations: ccc.Cell[], deployment: MigrationDeployment) {
    const selected = deployment.validate(tx, cells, observations);
    super(tx, cells, deployment.counterScript(), deployment.sdk(selected.version), deployment.counterDeployment(selected.version));
    this.#deployment = deployment; this.#tx = tx.clone(); this.#cells = cells.map((cell) => cell.clone());
    this.selectedVersion = selected.version; this.isMigration = selected.migrating;
  }
  protected override checkTransaction(tx: ccc.Transaction): void { migrationBounds(tx); }
  protected override async verifyApplication(client: ccc.Client): Promise<void> {
    await this.#deployment.verifyArtifacts(client);
    const deps = await resolveDependencies(client, this.#tx);
    const selected = this.#deployment.validate(this.#tx, this.#cells, deps);
    requireThat(selected.version === this.selectedVersion && selected.migrating === this.isMigration, 'migration configuration selection changed');
  }
}

/** Finalize fee/Lock dependencies before deriving the statement. The old
 * configuration authorizes a switch; the successor config is used next time. */
export async function prepareMigration(
  signer: ccc.Signer, counterPoint: ccc.OutPointLike, configPoint: ccc.OutPointLike,
  deployment: MigrationDeployment, migrate: boolean, lockDeps: ccc.CellDepLike[] = [], feeRate = 1000n,
): Promise<PreparedMigration> {
  await deployment.verifyArtifacts(signer.client);
  const counter = await live(signer.client, counterPoint, 'counter'), config = await live(signer.client, configPoint, 'configuration');
  requireThat(counter.cellOutput.type?.eq(deployment.counterScript()), 'migration counter differs from pinned instance');
  const selected = deployment.selection(config);
  requireThat(!migrate || selected === 0, 'migration already completed; second migration forbidden');
  const state = decodeCounter(counter.outputData);
  requireThat(state.counter < 0xffffffffffffffffn, 'counter overflow: this instance cannot advance or migrate');
  const next = ccc.bytesFrom(counter.outputData); new DataView(next.buffer, next.byteOffset, next.byteLength).setBigUint64(40, state.counter + 1n, true);
  let tx = ccc.Transaction.from({ inputs: [{ previousOutput: counter.outPoint }], outputs: [counter.cellOutput], outputsData: [next], cellDeps: deployment.dependencies(selected, migrate, config.outPoint) });
  if (migrate) { tx.inputs.push(ccc.CellInput.from({ previousOutput: config.outPoint })); tx.addOutput(config.cellOutput, deployment.configData(1)); }
  for (const dep of lockDeps) tx.addCellDeps(dep);
  tx.setWitnessArgs(0, { inputType: deployment.sdk(selected).encodeZkTransitionWitness('increment', new Uint8Array(128), ccc.bytesFrom(deployment.counterDeployment(selected).handle)) });
  tx = await signer.prepareTransaction(tx); await tx.completeFeeBy(signer, feeRate);
  requireThat(tx.outputs[0]?.type?.eq(deployment.counterScript()) && (!migrate || tx.outputs[1]?.type?.eq(deployment.configScript())), 'wallet changed migration successor output order; re-prepare transaction');
  const cells = await Promise.all(tx.inputs.map((input) => live(signer.client, input.previousOutput, 'input')));
  return new PreparedMigration(tx, cells, await resolveDependencies(signer.client, tx), deployment);
}
