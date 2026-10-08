// Application adapter for @ckb-ccc/shell 1.3.15. The prover never supplies chain state.
import { ccc } from '@ckb-ccc/shell';
// Structural SDK boundary keeps this adapter usable outside the compiler repo.
export type ZkStatementInput = {
  domain: Uint8Array; action: Uint8Array; scriptHash: Uint8Array;
  oldDataHash: Uint8Array; newDataHash: Uint8Array; inputTransactionHash: Uint8Array;
  inputOutputIndex: number; transactionHash: Uint8Array;
};

export interface CounterSdk {
  readonly zkVerifierContracts: readonly { entry: string; profile: string; exact_handle_hash: string; verification_key_hash: string; domain: string; action: string }[];
  encodeZkStatement(statement: ZkStatementInput): Uint8Array;
  encodeZkPublicInputs(statement: ZkStatementInput): Uint8Array;
  bindZkProverOutput(statement: ZkStatementInput, key: Uint8Array, proof: Uint8Array, inputs: Uint8Array): Uint8Array;
  encodeZkTransitionWitness(action: string, proof: Uint8Array, handle: Uint8Array): Uint8Array;
}
export type CodeCell = { outPoint: ccc.OutPointLike; dataHash: ccc.HexLike };
export interface CounterDeployment {
  genesisHash: ccc.HexLike;
  child: CodeCell;
  verificationKey: CodeCell;
  parent: CodeCell;
  lifecycle: CodeCell;
  handle: ccc.HexLike;
  lockDeps?: ccc.CellDepLike[];
}
export type Prover = (statement: Uint8Array, counters: { oldCounter: bigint; newCounter: bigint }, options?: { signal?: AbortSignal }) => Promise<{
  proof: Uint8Array; publicInputs: Uint8Array;
}>;
const domain = ccc.bytesFrom(ccc.hashCkb(new TextEncoder().encode('cellscript-private-counter-v1/domain')));
const action = ccc.bytesFrom(ccc.hashCkb(new TextEncoder().encode('cellscript-private-counter-v1/increment')));
const equal = (a: ccc.HexLike, b: ccc.HexLike) => ccc.hexFrom(a) === ccc.hexFrom(b);
function requireThat(ok: unknown, message: string): asserts ok { if (!ok) throw new Error(message); }

export function decodeCounter(data: ccc.HexLike): { owner: ccc.Hex; counter: bigint } {
  const bytes = ccc.bytesFrom(data);
  requireThat(bytes.length === 48 && new TextDecoder().decode(bytes.slice(0, 8)) === 'CSZKCNT1', 'counter state: expected CSZKCNT1 and 48 bytes');
  return { owner: ccc.hexFrom(bytes.slice(8, 40)), counter: new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(40, true) };
}

/** Derive every state/hash field from the finalized CCC transaction and ordered resolved Cells. */
export function deriveStatement(tx: ccc.Transaction, cells: ccc.Cell[], script: ccc.Script): ZkStatementInput {
  requireThat(cells.length === tx.inputs.length && cells.every((cell, i) => cell.outPoint.eq(tx.inputs[i].previousOutput)), 'ZK input resolution differs from finalized transaction');
  const selected = cells.filter((cell) => cell.cellOutput.type?.eq(script));
  const outputs = tx.outputs.map((cell, index) => ({ cell, index })).filter(({ cell }) => cell.type?.eq(script));
  requireThat(selected.length === 1 && outputs.length === 1, 'ZK profile requires exactly one Type-group input/output');
  const old = selected[0], next = outputs[0];
  const oldState = decodeCounter(old.outputData), nextState = decodeCounter(tx.outputsData[next.index]);
  requireThat(old.cellOutput.capacity === next.cell.capacity && old.cellOutput.lock.eq(next.cell.lock), 'counter capacity/Lock changed; use a separate fee input');
  requireThat(oldState.owner === nextState.owner && oldState.counter < 0xffffffffffffffffn && nextState.counter === oldState.counter + 1n, 'counter owner or increment invalid');
  return { domain: domain.slice(), action: action.slice(), scriptHash: ccc.bytesFrom(script.hash()),
    oldDataHash: ccc.bytesFrom(ccc.hashCkb(old.outputData)), newDataHash: ccc.bytesFrom(ccc.hashCkb(tx.outputsData[next.index])),
    inputTransactionHash: ccc.bytesFrom(old.outPoint.txHash), inputOutputIndex: Number(old.outPoint.index), transactionHash: ccc.bytesFrom(tx.hash()) };
}

async function live(client: ccc.Client, outPoint: ccc.OutPointLike, label: string): Promise<ccc.Cell> {
  const cell = await client.getCellLive(outPoint, true, true);
  requireThat(cell && cell.outPoint.eq(outPoint), `${label}: outpoint is spent, missing or inconsistent; resolve a fresh Cell before proving`);
  return cell;
}

export function verifyBinding(sdk: CounterSdk, deployment: CounterDeployment): void {
  const contracts = sdk.zkVerifierContracts.filter((contract) => contract.entry === 'action:increment');
  requireThat(contracts.length === 1, 'SDK must contain exactly one increment ZK contract');
  const contract = contracts[0];
  const hex = (value: string) => `0x${value.replace(/^0x/, '')}` as ccc.Hex;
  requireThat(contract.profile === 'cellscript-zk-transition-groth16-bn254-v2' && equal(hex(contract.domain), domain) && equal(hex(contract.action), action), 'SDK counter profile/domain/action mismatch');
  requireThat(equal(hex(contract.exact_handle_hash), ccc.hashCkb(deployment.handle)), 'exact handle differs from generated parent metadata');
  requireThat(equal(hex(contract.verification_key_hash), deployment.verificationKey.dataHash), 'VK differs from generated parent metadata');
}

async function verifyDeployment(client: ccc.Client, deployment: CounterDeployment, script: ccc.Script): Promise<void> {
  const genesis = await client.getBlockByNumber(0);
  requireThat(genesis && equal(genesis.header.hash, deployment.genesisHash), 'ZK genesis mismatch: select the deployment for this node');
  for (const name of ['child', 'verificationKey', 'parent', 'lifecycle'] as const) {
    const expected = deployment[name];
    const cell = await live(client, expected.outPoint, name);
    requireThat(equal(ccc.hashCkb(cell.outputData), expected.dataHash), `${name}: live data hash differs from deployment manifest`);
  }
  const args = ccc.bytesFrom(script.args);
  requireThat(['data', 'data1', 'data2'].includes(script.hashType) && equal(script.codeHash, deployment.lifecycle.dataHash), 'counter lifecycle code hash/hash type differs from deployment');
  requireThat(args.length === 64 && equal(args.slice(32), deployment.parent.dataHash), 'counter Type args do not commit to the selected parent');
}

/** Snapshot with no public mutable transaction. Proof and signing callbacks receive copies. */
export class PreparedCounter {
  #tx: ccc.Transaction;
  #statement: ZkStatementInput;
  #cells: ccc.Cell[];
  #sdk: CounterSdk;
  #deployment: CounterDeployment;
  #witnessIndex: number;
  #oldCounter: bigint;
  #proved?: ccc.Transaction;
  #proving = false;
  constructor(tx: ccc.Transaction, cells: ccc.Cell[], script: ccc.Script, sdk: CounterSdk, deployment: CounterDeployment) {
    verifyBinding(sdk, deployment);
    this.#tx = tx.clone(); this.#cells = cells.map((cell) => cell.clone());
    this.#statement = deriveStatement(this.#tx, this.#cells, script);
    this.#sdk = sdk;
    this.#deployment = structuredClone(deployment);
    this.#witnessIndex = cells.findIndex((cell) => cell.cellOutput.type?.eq(script));
    this.#oldCounter = decodeCounter(cells[this.#witnessIndex].outputData).counter;
  }
  get transactionHash(): ccc.Hex { return this.#tx.hash(); }
  get statementBytes(): Uint8Array { return this.#sdk.encodeZkStatement(this.#statement); }
  async prove(prover: Prover, options: { signal?: AbortSignal } = {}): Promise<ccc.Transaction> {
    requireThat(!this.#proving, 'proving already in progress for this prepared transaction');
    this.#proved = undefined;
    this.#proving = true;
    const { signal } = options;
    let abort: (() => void) | undefined;
    try {
      signal?.throwIfAborted();
      const cancelled = new Promise<never>((_, reject) => {
        abort = () => reject(new Error('ZK proving cancelled; run prove again before signing'));
        signal?.addEventListener('abort', abort, { once: true });
      });
      const result = await Promise.race([
        prover(this.statementBytes, { oldCounter: this.#oldCounter, newCounter: this.#oldCounter + 1n }, options),
        cancelled,
      ]);
      signal?.throwIfAborted();
      this.#sdk.bindZkProverOutput(this.#statement, ccc.bytesFrom(this.#deployment.verificationKey.dataHash), result.proof, result.publicInputs);
      const tx = this.#tx.clone();
      const previous = tx.getWitnessArgs(this.#witnessIndex);
      tx.setWitnessArgs(this.#witnessIndex, { ...previous, inputType: this.#sdk.encodeZkTransitionWitness('increment', result.proof, ccc.bytesFrom(this.#deployment.handle)) });
      this.checkTransaction(tx);
      this.#proved = tx.clone();
      return tx;
    } finally {
      if (abort) signal?.removeEventListener('abort', abort);
      this.#proving = false;
    }
  }
  checkSigned(proved: ccc.Transaction, signed: ccc.Transaction): void {
    this.checkTransaction(proved);
    this.checkTransaction(signed);
    requireThat(this.#proved && !this.#proving, 'no completed proof for this prepared transaction; run prove before signing');
    requireThat(proved.hash() === this.transactionHash && signed.hash() === this.transactionHash, 'ZK raw transaction changed after proving; finalize fees/dependencies and generate a new proof');
    this.#checkWitnesses(this.#proved, proved);
    this.#checkWitnesses(proved, signed);
    requireThat(signed.getWitnessArgs(this.#witnessIndex)?.inputType != null, 'counter proof witness missing');
  }
  #checkWitnesses(proved: ccc.Transaction, signed: ccc.Transaction): void {
    // Preserve all non-Lock witness fields, including other script groups/extra witnesses.
    requireThat(proved.witnesses.length === signed.witnesses.length, 'wallet changed witness count after proving');
    proved.witnesses.forEach((witness, index) => {
      if (witness === signed.witnesses[index]) return;
      const before = proved.getWitnessArgs(index), after = signed.getWitnessArgs(index);
      requireThat(before && after && before.inputType === after.inputType && before.outputType === after.outputType, 'wallet changed a proof or non-Lock witness field');
    });
  }
  async #checkLive(client: ccc.Client): Promise<void> {
    for (const cell of this.#cells) {
      const fresh = await live(client, cell.outPoint, 'input');
      requireThat(equal(fresh.cellOutput.toBytes(), cell.cellOutput.toBytes()) && equal(fresh.outputData, cell.outputData), 'resolved input changed; re-prepare transaction');
    }
    const script = this.#cells[this.#witnessIndex].cellOutput.type!;
    await this.verifyApplication(client, script);
  }
  /** Application-specific checks run inside the shared immutable proof/signing flow. */
  protected checkTransaction(_tx: ccc.Transaction): void {}
  protected async verifyApplication(client: ccc.Client, script: ccc.Script): Promise<void> {
    await verifyDeployment(client, this.#deployment, script);
  }
  async signAndSend(signer: ccc.Signer, proved: ccc.Transaction, options: { confirmations?: number; timeoutMs?: number } = {}) {
    this.checkSigned(proved, proved);
    // Snapshot before the first asynchronous call; caller mutation cannot alter it.
    const toSign = proved.clone();
    await this.#checkLive(signer.client);
    this.checkSigned(toSign, toSign);
    // Sign only: prepareTransaction belongs before proving because it can add deps.
    const signed = (await signer.signOnlyTransaction(toSign.clone())).clone();
    this.checkSigned(toSign, signed);
    await this.#checkLive(signer.client);
    try { await signer.client.sendTransactionDry(signed, 'passthrough'); }
    catch (error) { throw new Error('CKB dry-run did not succeed; transaction was not submitted (inspect the cause to distinguish node rejection from an unavailable result; parent 79 means child rejection)', { cause: error }); }
    const hash = await signer.client.sendTransaction(signed, 'passthrough');
    requireThat(hash === this.transactionHash, 'node returned an unexpected transaction hash');
    const receipt = await signer.client.waitTransaction(hash, options.confirmations ?? 1, options.timeoutMs ?? 120_000);
    requireThat(receipt?.status === 'committed', `transaction ${hash} was not committed`);
    return { hash, receipt };
  }
}

/** Resolve live state/code, reserve the full proof witness, then complete wallet
 * dependencies and fees. State capacity is preserved; CCC selects fee Cells. */
export async function prepareIncrement(signer: ccc.Signer, outPoint: ccc.OutPointLike, deployment: CounterDeployment, sdk: CounterSdk, feeRate = 1000n): Promise<PreparedCounter> {
  verifyBinding(sdk, deployment);
  const cell = await live(signer.client, outPoint, 'counter');
  const script = cell.cellOutput.type;
  requireThat(script, 'counter input has no Type Script');
  await verifyDeployment(signer.client, deployment, script);
  const state = decodeCounter(cell.outputData);
  requireThat(state.counter < 0xffffffffffffffffn, 'counter overflow: this instance cannot advance');
  const data = ccc.bytesFrom(cell.outputData);
  new DataView(data.buffer, data.byteOffset, data.byteLength).setBigUint64(40, state.counter + 1n, true);
  let tx = ccc.Transaction.from({ inputs: [{ previousOutput: cell.outPoint }], outputs: [cell.cellOutput], outputsData: [data],
    cellDeps: ['child', 'verificationKey', 'parent', 'lifecycle'].map((name) => ({ outPoint: deployment[name as 'child'].outPoint, depType: 'code' as const })) });
  for (const dep of deployment.lockDeps ?? []) tx.addCellDeps(dep);
  tx.setWitnessArgs(0, { inputType: sdk.encodeZkTransitionWitness('increment', new Uint8Array(128), ccc.bytesFrom(deployment.handle)) });
  tx = await signer.prepareTransaction(tx);
  await tx.completeFeeBy(signer, feeRate);
  requireThat(tx.cellDeps[0]?.depType === 'code' && tx.cellDeps[0].outPoint.eq(deployment.child.outPoint), 'wallet changed exact child CellDep slot 0');
  const cells = await Promise.all(tx.inputs.map((input) => live(signer.client, input.previousOutput, 'input')));
  return new PreparedCounter(tx, cells, script, sdk, deployment);
}
