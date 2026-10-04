// Exact v2 codecs. Callers derive hashes from a finalized raw transaction and
// resolved Cells; these functions neither query a chain nor verify a proof.
export type ZkStatementInput = {
  domain: Uint8Array; action: Uint8Array; scriptHash: Uint8Array;
  oldDataHash: Uint8Array; newDataHash: Uint8Array; inputTransactionHash: Uint8Array;
  inputOutputIndex: number; transactionHash: Uint8Array;
};
const zkProfile = Uint8Array.from([0x43,0x0e,0x4c,0xf7,0xa6,0x67,0x7d,0x9b,0xfa,0x16,0x19,0xd7,0x24,0x40,0xd3,0x92,0xe9,0xec,0x77,0x24,0x86,0xc4,0xe7,0x85,0x1a,0xaf,0x91,0xdf,0x58,0x88,0x11,0x01]);
function zkBytes(value: Uint8Array, length: number): Uint8Array {
  if (!(value instanceof Uint8Array) || value.length !== length) throw new Error(`ZK expected ${length} bytes`);
  return value;
}
export function encodeZkStatement(value: ZkStatementInput): Uint8Array {
  const result = new Uint8Array(228);
  [value.domain, value.action, value.scriptHash, value.oldDataHash, value.newDataHash, value.inputTransactionHash].forEach((hash, index) => result.set(zkBytes(hash, 32), index * 32));
  if (!Number.isInteger(value.inputOutputIndex) || value.inputOutputIndex < 0 || value.inputOutputIndex > 0xffffffff) throw new Error("ZK index must be u32");
  new DataView(result.buffer).setUint32(192, value.inputOutputIndex, true);
  result.set(zkBytes(value.transactionHash, 32), 196);
  return result;
}
export function encodeZkPublicInputs(value: ZkStatementInput): Uint8Array {
  const statement = encodeZkStatement(value);
  const result = new Uint8Array(484);
  new DataView(result.buffer).setUint32(0, 15, true);
  for (let i = 0; i < 12; i++) result.set(statement.subarray(i * 16, i * 16 + 16), 4 + i * 32);
  result.set(statement.subarray(192, 196), 388);
  result.set(statement.subarray(196, 212), 420);
  result.set(statement.subarray(212, 228), 452);
  return result;
}
export function encodeZkRequest(value: ZkStatementInput, keyHash: Uint8Array, proof: Uint8Array): Uint8Array {
  const result = new Uint8Array(464);
  const view = new DataView(result.buffer);
  [464, 28, 36, 44, 76, 108, 236].forEach((offset, index) => view.setUint32(index * 4, offset, true));
  result.set([0x43,0x53,0x5a,0x4b,0x49,0x50,0x43,0x31], 28);
  view.setUint32(36, 1, true);
  result.set(zkProfile, 44);
  result.set(zkBytes(keyHash, 32), 76);
  result.set(zkBytes(proof, 128), 108);
  result.set(encodeZkStatement(value), 236);
  return result;
}
export function bindZkProverOutput(value: ZkStatementInput, keyHash: Uint8Array, proof: Uint8Array, publicInputs: Uint8Array): Uint8Array {
  const expected = encodeZkPublicInputs(value);
  zkBytes(publicInputs, expected.length);
  if (expected.some((byte, index) => publicInputs[index] !== byte)) throw new Error("ZK prover public inputs differ from transaction statement");
  return encodeZkRequest(value, keyHash, proof);
}
