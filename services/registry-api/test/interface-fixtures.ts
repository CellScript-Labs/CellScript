export function temporalInterface() {
  return {
    schema: "cellscript-ckb-temporal-interface-v1",
    wire_representation: "fixed-u64-register-and-little-endian-wire",
    since_abi: "ckb-since-rfc0017-typed-v1",
    constructors: [
      "ckb::since_absolute_block(u64)->AbsoluteBlockSince",
      "ckb::since_absolute_epoch(u64,u64,u64)->AbsoluteEpochSince",
      "ckb::since_absolute_timestamp(u64-seconds)->AbsoluteTimestampSince",
      "ckb::since_relative_block(u64)->RelativeBlockSince",
      "ckb::since_relative_epoch(u64,u64,u64)->RelativeEpochSince",
      "ckb::since_relative_timestamp(u64-seconds)->RelativeTimestampSince",
    ],
    decoder: "ckb::since_decode(EncodedSince)->DecodedSince;ckb::since_from_raw_checked(u64)->DecodedSince",
    domains: ["EpochNumber", "EpochDuration", "BlockNumber", "EpochLength", "TimestampMillis", "EncodedSince", "DecodedSince", "AbsoluteBlockSince", "AbsoluteEpochSince", "AbsoluteTimestampSince", "RelativeBlockSince", "RelativeEpochSince", "RelativeTimestampSince"],
    migration: "legacy-raw-ckb-temporal-to-explicit-typed-v1",
  };
}

export function publicInterface() {
  return {
    schema: "cellscript-package-interface-v5",
    version: 5,
    module: "cellscript::demo",
    module_identity: `blake2b:${"ab".repeat(32)}`,
    edition: "2026",
    visibility_default: "legacy-public (Edition 2026 compatibility)",
    types: [],
    constants: [],
    callables: [],
    runtime_contract: {
      target_profile: "ckb", vm_abi: "ckb-vm-v2", witness_abi: "cellscript-entry-witness-v1",
      lock_args_abi: "lock-args", source_encoding: "source-view", spawn_ipc_abi: "ckb-vm2",
      compatibility_profile_id: "cellscript-source-semantics-2026-0.33-effect-facts-v1",
      temporal: temporalInterface(),
    },
    builder_contract_hash: "33".repeat(32),
    deployment_contract_hash: "22".repeat(32),
  };
}

export function callable() {
  return {
    identity: "cellscript::demo::read", name: "read", kind: "function", visibility: "public",
    type_parameters: [], params: [], return_type: "u64", outputs: [],
    effects: { context_reads: [], lifecycle_ops: [], process_ops: [] },
    entry_witness_abi: null, builder_contract_hash: "33".repeat(32),
  };
}
