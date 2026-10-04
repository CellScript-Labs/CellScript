/** Encode the two-argument exact transition action from this generated package's
 * metadata. Returns the inner input_type payload, not the child IPC request. */
export function encodeZkTransitionWitness(action: string, proof: Uint8Array, handle: Uint8Array): Uint8Array {
  const entry = (builderManifest.actions as readonly {
    name: string; params: readonly { ty: string; source: string; fixed_byte_len?: number | null }[];
  }[]).find((entry) => entry.name === action);
  if (!entry || entry.params.length !== 2
      || entry.params[0].ty !== "ZkTransitionProof" || entry.params[0].source !== "witness"
      || entry.params[1].ty !== "ExactScriptHandle" || entry.params[1].source !== "witness"
      || entry.params[0].fixed_byte_len !== 128 || entry.params[1].fixed_byte_len !== 202) {
    throw new Error("ZK action metadata must declare (witness ZkTransitionProof, witness ExactScriptHandle)");
  }
  zkBytes(proof, 128); zkBytes(handle, 202);
  const payload = new Uint8Array(8 + proof.length + handle.length);
  payload.set([67, 83, 65, 82, 71, 118, 49, 0]);
  payload.set(proof, 8); payload.set(handle, 136);
  return payload;
}
