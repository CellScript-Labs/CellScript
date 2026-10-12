// Bounded conjunction of exact counter proofs; no aggregation or peer authority.
import { ccc } from '@ckb-ccc/shell';
import { decodeCounter, deriveStatement, PreparedCounter, verifyBinding } from './client.ts';
import type { CounterDeployment, CounterSdk, Prover } from './client.ts';

export interface CounterSetDeployment extends CounterDeployment {
  profile: 'cellscript-zk-counter-set-v1';
  familyId: ccc.HexLike;
}
function need(value: unknown, message: string): asserts value { if (!value) throw new Error(`counter set: ${message}`); }
const equal = (a: ccc.HexLike, b: ccc.HexLike) => ccc.hexFrom(a) === ccc.hexFrom(b);
const u32 = (n: number) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, n, true); return b; };
const concat = (...bytes: Uint8Array[]) => Uint8Array.from(bytes.flatMap((b) => [...b]));
function args(script: ccc.Script): Uint8Array {
  const a = ccc.bytesFrom(script.args);
  need(a.length === 73 && script.hashType === 'data2' && new TextDecoder().decode(a.slice(0, 8)) === 'CSZKSET1'
    && a[40] < 4 && a.slice(41).some((b) => b !== 0), 'Script args/profile');
  return a;
}
function bounds(tx: ccc.Transaction): void {
  need(tx.toBytes().length <= 16384 && tx.inputs.length <= 64 && tx.outputs.length <= 64 && tx.cellDeps.length <= 64, 'transaction bounds');
  need(tx.version === 0n && tx.outputsData.length === tx.outputs.length && tx.cellDeps.every((dep) => dep.depType === 'code'), 'canonical version/data/direct dependencies');
  need(new Set(tx.cellDeps.map((d) => ccc.hexFrom(d.outPoint.toBytes()))).size === tx.cellDeps.length
    && new Set(tx.headerDeps).size === tx.headerDeps.length, 'duplicate dependencies');
}

/** Independent CCC selection and Molecule set-statement encoding. */
export function deriveCounterSet(tx: ccc.Transaction, cells: ccc.Cell[], selected: ccc.Script, sdk: CounterSdk) {
  bounds(tx);
  const a = args(selected), family = a.slice(8, 40);
  need(cells.length === tx.inputs.length && cells.every((c, i) => c.outPoint.eq(tx.inputs[i].previousOutput)), 'complete input resolution');
  need(new Set(cells.map((c) => ccc.hexFrom(c.outPoint.toBytes()))).size === cells.length, 'duplicate input OutPoint');
  need(cells.every((c) => c.cellOutput.toBytes().length <= 4096 && ccc.bytesFrom(c.outputData).length <= 4096), 'input snapshot budget');
  const member = (cell: ccc.CellOutput): { lane: number; script: ccc.Script } | undefined => {
    const script = cell.type;
    if (!script || script.codeHash !== selected.codeHash) return;
    const candidate = ccc.bytesFrom(script.args);
    need(candidate.length >= 40 && new TextDecoder().decode(candidate.slice(0, 8)) === 'CSZKSET1', 'malformed same-code family');
    if (!equal(candidate.slice(8, 40), family)) return;
    args(script); need(equal(candidate.slice(41), a.slice(41)), 'same-family parent drift');
    return { lane: candidate[40], script };
  };
  const inputs = cells.flatMap((cell, index) => { const m = member(cell.cellOutput); return m ? [{ ...m, index }] : []; });
  const outputs = tx.outputs.flatMap((cell, index) => { const m = member(cell); return m ? [{ ...m, index }] : []; });
  need(inputs.length >= 1 && inputs.length <= 4 && inputs.some((c) => c.script.eq(selected)) && outputs.length === inputs.length, 'selection/count/correspondence');
  for (const items of [inputs, outputs]) need(items.every((c, i) => i === 0 || c.lane > items[i - 1].lane), 'duplicate or reordered protected set');
  const leaves = inputs.map((input, i) => {
    const output = outputs[i]; need(input.lane === output.lane && input.script.eq(output.script), 'complete successor Script');
    const statement = deriveStatement(tx, cells, input.script);
    const encoded = sdk.encodeZkStatement(statement); need(encoded.length === 228, 'leaf statement codec');
    return { inputIndex: input.index, outputIndex: output.index, script: input.script.clone(), statement, encoded };
  });
  const encoded = concat(...[112 + 236 * leaves.length, 28, 36, 40, 72, 76, 108].map(u32),
    new TextEncoder().encode('CSZKSET1'), u32(1), family, u32(leaves.length), ccc.bytesFrom(tx.hash()), u32(leaves.length),
    ...leaves.map((leaf) => concat(u32(leaf.inputIndex), u32(leaf.outputIndex), leaf.encoded)));
  return { family, leaves, encoded };
}

export function checkCounterSetStatement(tx: ccc.Transaction, cells: ccc.Cell[], selected: ccc.Script, sdk: CounterSdk, supplied: Uint8Array): void {
  need(supplied.length <= 1056 && equal(supplied, deriveCounterSet(tx, cells, selected, sdk).encoded), 'selection/count/correspondence/public-input substitution');
}

function checkInstance(script: ccc.Script, deployment: CounterSetDeployment): void {
  need(deployment.profile === 'cellscript-zk-counter-set-v1', 'unsupported deployment profile');
  const a = args(script);
  need(equal(script.codeHash, deployment.lifecycle.dataHash) && equal(a.slice(8, 40), deployment.familyId)
    && equal(a.slice(41), deployment.parent.dataHash), 'deployment/family/parent mismatch');
}
async function live(client: ccc.Client, point: ccc.OutPointLike): Promise<ccc.Cell> {
  const cell = await client.getCellLive(point, true, true);
  need(cell && cell.outPoint.eq(point), 'spent/missing input or code; resolve, rebuild and reprove the complete set'); return cell;
}
async function checkDeployment(client: ccc.Client, script: ccc.Script, deployment: CounterSetDeployment) {
  checkInstance(script, deployment);
  const genesis = await client.getBlockByNumber(0); need(genesis && equal(genesis.header.hash, deployment.genesisHash), 'genesis mismatch');
  for (const name of ['child', 'verificationKey', 'parent', 'lifecycle'] as const) {
    const cell = await live(client, deployment[name].outPoint);
    need(equal(ccc.hashCkb(cell.outputData), deployment[name].dataHash), `${name} code/key substitution`);
  }
}

class SetLeaf extends PreparedCounter {
  #deployment: CounterSetDeployment;
  constructor(tx: ccc.Transaction, cells: ccc.Cell[], script: ccc.Script, sdk: CounterSdk, deployment: CounterSetDeployment) {
    super(tx, cells, script, sdk, deployment); this.#deployment = structuredClone(deployment);
  }
  protected override checkTransaction(tx: ccc.Transaction): void { bounds(tx); }
  protected override async verifyApplication(client: ccc.Client, script: ccc.Script): Promise<void> {
    await checkDeployment(client, script, this.#deployment);
  }
}

export class PreparedCounterSet {
  #tx: ccc.Transaction; #cells: ccc.Cell[]; #sdk: CounterSdk; #deployment: CounterSetDeployment;
  #set: ReturnType<typeof deriveCounterSet>; #complete?: SetLeaf; #proving = false;
  constructor(tx: ccc.Transaction, cells: ccc.Cell[], selected: ccc.Script, sdk: CounterSdk, deployment: CounterSetDeployment) {
    verifyBinding(sdk, deployment); checkInstance(selected, deployment);
    this.#tx = tx.clone(); this.#cells = cells.map((c) => c.clone()); this.#sdk = sdk; this.#deployment = structuredClone(deployment);
    this.#set = deriveCounterSet(this.#tx, this.#cells, selected, sdk);
    need(this.#tx.cellDeps[0]?.outPoint.eq(deployment.child.outPoint), 'exact child must occupy dependency slot zero');
    for (const name of ['child', 'verificationKey', 'parent', 'lifecycle'] as const) {
      need(this.#tx.cellDeps.filter((dep) => dep.outPoint.eq(deployment[name].outPoint)).length === 1, `exact ${name} dependency missing/duplicated`);
    }
  }
  get statementBytes(): Uint8Array { return this.#set.encoded.slice(); }
  get transactionHash(): ccc.Hex { return this.#tx.hash(); }
  async prove(provers: Prover[], options: { signal?: AbortSignal } = {}): Promise<ccc.Transaction> {
    need(!this.#proving, 'proving already in progress'); this.#complete = undefined;
    need(provers.length === this.#set.leaves.length, 'complete prover count required'); this.#proving = true;
    try {
      let tx = this.#tx.clone(); let last: SetLeaf | undefined;
      for (let i = 0; i < provers.length; i++) {
        options.signal?.throwIfAborted();
        last = new SetLeaf(tx, this.#cells, this.#set.leaves[i].script, this.#sdk, this.#deployment);
        tx = await last.prove(provers[i], options);
      }
      this.#complete = last; return tx;
    } finally { this.#proving = false; }
  }
  checkSigned(proved: ccc.Transaction, signed: ccc.Transaction): void {
    need(this.#complete && !this.#proving, 'no complete proof set'); this.#complete.checkSigned(proved, signed);
  }
  async signAndSend(signer: ccc.Signer, proved: ccc.Transaction, options: { confirmations?: number; timeoutMs?: number } = {}) {
    this.checkSigned(proved, proved); return this.#complete!.signAndSend(signer, proved, options);
  }
}

/** Resolve the selected subset, reserve every proof, then finish fees before proving. */
export async function prepareCounterSet(signer: ccc.Signer, points: ccc.OutPointLike[], deployment: CounterSetDeployment, sdk: CounterSdk, feeRate = 1000n) {
  need(points.length >= 1 && points.length <= 4, 'select 1..4 counters'); verifyBinding(sdk, deployment);
  const protectedCells = await Promise.all(points.map((p) => live(signer.client, p)));
  for (const cell of protectedCells) { need(cell.cellOutput.type, 'counter Type missing'); checkInstance(cell.cellOutput.type, deployment); }
  protectedCells.sort((a, b) => args(a.cellOutput.type!)[40] - args(b.cellOutput.type!)[40]);
  await checkDeployment(signer.client, protectedCells[0].cellOutput.type!, deployment);
  const data = protectedCells.map((cell) => {
    const old = decodeCounter(cell.outputData); need(old.counter < 0xffffffffffffffffn, 'counter overflow');
    const bytes = ccc.bytesFrom(cell.outputData); new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setBigUint64(40, old.counter + 1n, true); return bytes;
  });
  let tx = ccc.Transaction.from({ inputs: protectedCells.map((cell) => ({ previousOutput: cell.outPoint })),
    outputs: protectedCells.map((cell) => cell.cellOutput), outputsData: data,
    cellDeps: ['child', 'verificationKey', 'parent', 'lifecycle'].map((name) => ({ outPoint: deployment[name as 'child'].outPoint, depType: 'code' as const })) });
  for (const dep of deployment.lockDeps ?? []) tx.addCellDeps(dep);
  protectedCells.forEach((_, i) => tx.setWitnessArgs(i, { inputType: sdk.encodeZkTransitionWitness('increment', new Uint8Array(128), ccc.bytesFrom(deployment.handle)) }));
  tx = await signer.prepareTransaction(tx); await tx.completeFeeBy(signer, feeRate);
  const cells = await Promise.all(tx.inputs.map((input) => live(signer.client, input.previousOutput)));
  return new PreparedCounterSet(tx, cells, protectedCells[0].cellOutput.type!, sdk, deployment);
}
