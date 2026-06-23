#[allow(dead_code)]
#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

use cellscript::{
    ckb_blake2b256, compile, strip_vm_abi_trailer, validate_compile_metadata, ArtifactFormat, CompileOptions, EntryWitnessArg,
};
use ckb_testtool::ckb_types::bytes::Bytes;

#[test]
fn v0_14_exposes_spawn_ipc_source_witness_time_capacity_metadata() {
    let source = r#"
module cellscript::v0_14_surface

resource Token has store {
    amount: u64,
}

resource Wallet has store {
    owner: Address,
}

struct Proof {
    pubkey: Hash,
    signature: Hash,
}

action delegate_verify(proof: Proof) -> u64 {
    verification
        let pid = spawn("secp256k1_verifier")
        let status = wait()
        require status == 0, "delegate failed"
        return pid

}
action pipe_pipeline(value: u64) -> u64 {
    verification
        let fds = pipe()
        let read_fd = fds.0
        let write_fd = fds.1
        pipe_write(write_fd, value)
        let echoed = pipe_read(read_fd)
        close(read_fd)
        close(write_fd)
        return echoed

}
action capacity_and_time(amount: u64) -> output: Token {
    verification
        require_maturity(100)
        require_time(1714000000)
        require_epoch_after(10, 0, 1)
        require_epoch_relative(10, 0, 1)
        let floor = occupied_capacity("Token")
        require floor >= 0, "capacity floor visible"
        create output = Token { amount }

}
lock owner_lock(wallet: protected Wallet, owner: lock_args Address, claimed_owner: witness Address) -> bool {
    verification
        let view = source::group_input(0)
        let sig = witness::lock(view)
        let digest = env::sighash_all(view)
        require owner == wallet.owner
        require sig == digest
}

lock output_witness_lock(wallet: protected Wallet, claimed_owner: witness Address) -> bool {
    verification
        let input = source::input(0)
        let output = source::group_output(0)
        let input_type = witness::input_type(input)
        let output_type = witness::output_type(output)
        require input_type == output_type
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let features = &result.metadata.runtime.ckb_runtime_features;
    for expected in [
        "ckb-spawn-ipc",
        "ckb-lock-args",
        "ckb-source-view",
        "ckb-witness-args",
        "ckb-sighash-all",
        "ckb-declarative-since",
        "ckb-declarative-capacity",
    ] {
        assert!(features.iter().any(|feature| feature == expected), "missing {expected}: {features:?}");
    }

    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "witness-lock" && access.syscall == "LOAD_WITNESS_ARGS_LOCK" && access.source == "GroupInput"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "lock-args"
            && access.syscall == "LOAD_SCRIPT_ARGS"
            && access.source == "ScriptArgs"
            && access.binding == "owner"
    }));
    assert!(result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .any(|access| { access.operation == "source-input" && access.syscall == "SOURCE_VIEW" && access.source == "Input" }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "source-group-output" && access.syscall == "SOURCE_VIEW" && access.source == "GroupOutput"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "witness-input-type" && access.syscall == "LOAD_WITNESS_ARGS_INPUT_TYPE" && access.source == "GroupInput"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "witness-output-type"
            && access.syscall == "LOAD_WITNESS_ARGS_OUTPUT_TYPE"
            && access.source == "GroupOutput"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "require-epoch-after"
            && access.syscall == "LOAD_INPUT_BY_FIELD"
            && access.source == "GroupInput"
            && access.binding == "require_epoch_after"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "require-epoch-relative"
            && access.syscall == "LOAD_INPUT_BY_FIELD"
            && access.source == "GroupInput"
            && access.binding == "require_epoch_relative"
    }));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.operation == "spawn"));
    let delegate_verify =
        result.metadata.actions.iter().find(|action| action.name == "delegate_verify").expect("delegate_verify metadata");
    let delegate_group = delegate_verify.ckb_script_group.as_ref().expect("delegate_verify CKB script group metadata");
    assert_eq!(delegate_group.entry_kind, "action");
    assert_eq!(delegate_group.group_kind, "type");
    assert!(delegate_group.cell_dep_sources.contains(&"CellDep".to_string()));
    assert!(delegate_verify.verifier_obligations.iter().any(|obligation| {
        obligation.category == "spawn-target"
            && obligation.feature == "spawn-target:CellDep#0"
            && obligation.status == "runtime-required"
            && obligation.detail.contains("CellDep or DepGroup")
    }));
    assert!(delegate_verify.transaction_runtime_input_requirements.iter().any(|requirement| {
        requirement.feature == "spawn-target:CellDep#0"
            && requirement.component == "spawn-target-cell-dep"
            && requirement.status == "runtime-required"
            && requirement.source == "CellDep"
            && requirement.binding == "spawn-target"
            && requirement.field.as_deref() == Some("script")
            && requirement.abi == "ckb-spawn-cell-dep-script-reference"
            && requirement.blocker_class.as_deref() == Some("spawn-target-cell-dep-gap")
    }));
    let ckb_constraints = result.metadata.constraints.ckb.as_ref().expect("CKB constraints");
    assert!(ckb_constraints.script_references.iter().any(|reference| {
        reference.scope == "action:delegate_verify"
            && reference.purpose == "spawn-target"
            && reference.dep_source == "CellDep-or-DepGroup"
            && reference.status == "runtime-required-builder-resolved"
    }));
    let owner_lock = result.metadata.locks.iter().find(|lock| lock.name == "owner_lock").expect("owner_lock metadata");
    let owner_group = owner_lock.ckb_script_group.as_ref().expect("owner_lock CKB script group metadata");
    assert_eq!(owner_group.entry_kind, "lock");
    assert_eq!(owner_group.group_kind, "lock");
    assert_eq!(owner_group.active_script_group, "lock-group");
    assert!(owner_group.input_sources.contains(&"GroupInput".to_string()));
    assert!(owner_group.group_scoped_sources.contains(&"GroupInput".to_string()));
    let output_lock =
        result.metadata.locks.iter().find(|lock| lock.name == "output_witness_lock").expect("output_witness_lock metadata");
    let output_group = output_lock.ckb_script_group.as_ref().expect("output_witness_lock CKB script group metadata");
    assert!(output_group.output_sources.contains(&"GroupOutput".to_string()));
    assert!(output_group.group_scoped_sources.contains(&"GroupOutput".to_string()));
    assert_eq!(result.metadata.target_profile.spawn_ipc_abi, "ckb-vm-v2-spawn-ipc-syscalls-2601-2608");
    assert_eq!(result.metadata.target_profile.lock_args_abi, "ckb-script-args-typed-fixed-bytes");
    assert_eq!(result.metadata.target_profile.source_encoding, "ckb-source-group-high-bit");
    assert_eq!(result.metadata.target_profile.cell_dep_abi, "ckb-cell-dep-outpoint-and-dep-group");
    assert_eq!(result.metadata.target_profile.script_ref_abi, "ckb-script-code-hash-hash-type-args");
    assert_eq!(result.metadata.target_profile.output_data_abi, "ckb-outputs-and-outputs-data-index-aligned");
    assert_eq!(result.metadata.target_profile.capacity_floor_abi, "ckb-output-capacity-floor-shannons");
    assert_eq!(result.metadata.target_profile.type_id_abi, "ckb-type-id-v1");

    let profile_abi = &result.metadata.constraints.ckb.as_ref().expect("CKB constraints").profile_abi_contract;
    assert_eq!(profile_abi.witness_abi, result.metadata.target_profile.witness_abi);
    assert_eq!(profile_abi.lock_args_abi, result.metadata.target_profile.lock_args_abi);
    assert_eq!(profile_abi.source_encoding, result.metadata.target_profile.source_encoding);
    assert_eq!(profile_abi.spawn_ipc_abi, result.metadata.target_profile.spawn_ipc_abi);
    assert_eq!(profile_abi.since_abi, result.metadata.target_profile.since_abi);
    assert_eq!(profile_abi.cell_dep_abi, result.metadata.target_profile.cell_dep_abi);
    assert_eq!(profile_abi.script_ref_abi, result.metadata.target_profile.script_ref_abi);
    assert_eq!(profile_abi.output_data_abi, result.metadata.target_profile.output_data_abi);
    assert_eq!(profile_abi.capacity_floor_abi, result.metadata.target_profile.capacity_floor_abi);
    assert_eq!(profile_abi.type_id_abi, result.metadata.target_profile.type_id_abi);
    assert_eq!(profile_abi.tx_version, result.metadata.target_profile.tx_version);
}

#[test]
fn v0_14_exposes_declarative_capacity_floor_metadata() {
    let source = r#"
module cellscript::v0_14_capacity_floor

resource Token has store
with_capacity_floor(6100000000)
{
    amount: u64,
}

action mint(amount: u64) -> output: Token {
    verification
        create output = Token { amount }
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token metadata");
    assert_eq!(token.capacity_floor_shannons, Some(6_100_000_000));
    assert_eq!(token.capacity_floor_source.as_deref(), Some("dsl-with_capacity_floor"));
    let ckb_constraints = result.metadata.constraints.ckb.as_ref().expect("CKB constraints");
    assert_eq!(
        ckb_constraints.capacity_policy_surface,
        "dsl-declared-capacity-floor; builder/runtime-required-for-change-and-measurement"
    );
    assert_eq!(ckb_constraints.declared_capacity_floors.len(), 1);
    let floor = &ckb_constraints.declared_capacity_floors[0];
    assert_eq!(floor.type_name, "Token");
    assert_eq!(floor.shannons, 6_100_000_000);
    assert_eq!(floor.source, "dsl-with_capacity_floor");
    assert_eq!(floor.status, "builder-must-preserve-output-capacity-at-or-above-floor");
    result.validate().unwrap();

    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").declared_capacity_floors.clear();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("declared_capacity_floors") && err.message.contains("missing"), "unexpected error: {}", err.message);

    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").declared_capacity_floors[0].shannons = 6_000_000_000;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(
        err.message.contains("declared_capacity_floors") && err.message.contains("missing") && err.message.contains("extra"),
        "unexpected error: {}",
        err.message
    );

    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").created_output_count = 0;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("created_output_count"), "unexpected error: {}", err.message);

    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").capacity_evidence_contract.occupied_capacity_measurement_required =
        false;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(
        err.message.contains("capacity_evidence_contract.occupied_capacity_measurement_required"),
        "unexpected error: {}",
        err.message
    );

    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb = None;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("missing constraints.ckb"), "unexpected error: {}", err.message);

    let err = compile(
        r#"
module cellscript::bad_capacity_floor

resource Token has store
with_capacity_floor(0)
{
    amount: u64,
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(err.message.contains("capacity floor must be greater than zero"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_exposes_type_id_create_output_plan_and_output_data_boundary() {
    let source = r#"
module cellscript::v0_14_type_id

#[type_id("cellscript::v0_14_type_id::Token:v1")]
resource Token has store
with_default_hash_type(Type)
{
    amount: u64,
}

action mint(amount: u64) -> output: Token {
    verification
        create output = Token { amount }
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token metadata");
    assert_eq!(token.type_id.as_deref(), Some("cellscript::v0_14_type_id::Token:v1"));
    assert_eq!(token.default_hash_type.as_deref(), Some("type"));
    assert_eq!(token.hash_type_source, "dsl-with_default_hash_type");
    let ckb_type_id = token.ckb_type_id.as_ref().expect("CKB TYPE_ID contract metadata");
    assert_eq!(ckb_type_id.abi, "ckb-type-id-v1");
    assert_eq!(ckb_type_id.hash_type, "type");
    assert_eq!(ckb_type_id.args_source, "first-input-output-index");
    assert_eq!(ckb_type_id.group_rule, "at-most-one-input-and-one-output");

    let mint = result.metadata.actions.iter().find(|action| action.name == "mint").expect("mint metadata");
    let mint_group = mint.ckb_script_group.as_ref().expect("mint CKB script group metadata");
    assert_eq!(mint_group.group_kind, "type");
    assert!(mint_group.output_sources.contains(&"Output".to_string()));
    let output_data = mint.create_set[0].ckb_output_data.as_ref().expect("CKB output data binding");
    assert_eq!(output_data.output_source, "Output");
    assert_eq!(output_data.output_index, 0);
    assert_eq!(output_data.output_data_source, "outputs_data");
    assert_eq!(output_data.output_data_index, 0);
    assert_eq!(output_data.relation, "same-index");
    assert_eq!(mint.ckb_type_id_output_indexes(), vec![0]);
    let plan = mint.create_set[0].ckb_type_id.as_ref().expect("TYPE_ID create output plan");
    assert_eq!(plan.abi, "ckb-type-id-v1");
    assert_eq!(plan.output_source, "Output");
    assert_eq!(plan.output_index, 0);
    assert_eq!(plan.generator_setting, "ckb_type_id_output_indexes");
    assert_eq!(plan.wasm_setting, "ckbTypeIdOutputs");
    let ckb_constraints = result.metadata.constraints.ckb.as_ref().expect("CKB constraints");
    assert!(ckb_constraints.script_references.iter().any(|reference| {
        reference.scope == "action:mint"
            && reference.purpose == "type-id-create-output"
            && reference.name == "Token"
            && reference.code_hash.as_deref() == Some(plan.script_code_hash.as_str())
            && reference.hash_type.as_deref() == Some("type")
            && reference.args.as_deref() == Some("first-input-output-index")
    }));

    let create_access = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .find(|access| matches!(access.operation.as_str(), "create" | "output") && access.source == "Output" && access.index == 0)
        .expect("create output runtime access");
    assert_eq!(create_access.syscall, "LOAD_CELL");
    assert_eq!(result.metadata.target_profile.output_data_abi, "ckb-outputs-and-outputs-data-index-aligned");
}

#[test]
fn v0_14_rejects_tampered_type_id_output_data_and_script_reference_metadata() {
    let source = r#"
module cellscript::v0_14_type_id_tamper

#[type_id("cellscript::v0_14_type_id_tamper::Token:v1")]
resource Token has store
with_default_hash_type(Type)
{
    amount: u64,
}

action mint(amount: u64) -> output: Token {
    verification
        create output = Token { amount }
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let base = result.metadata.clone();
    validate_compile_metadata(&base, ArtifactFormat::RiscvAssembly).unwrap();

    let mut metadata = base.clone();
    metadata.actions[0].create_set[0].ckb_output_data.as_mut().expect("CKB output data binding").output_data_index = 1;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("ckb_output_data.output_data_index"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    metadata.actions[0].create_set[0].ckb_output_data = None;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("missing ckb_output_data index binding"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    metadata.actions[0].create_set[0].ckb_type_id.as_mut().expect("CKB TYPE_ID output plan").hash_type = "data".to_string();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("ckb_type_id.hash_type"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    metadata.actions[0].create_set[0].ckb_type_id = None;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("missing ckb_type_id output plan"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    let reference = metadata
        .constraints
        .ckb
        .as_mut()
        .expect("CKB constraints")
        .script_references
        .iter_mut()
        .find(|reference| reference.purpose == "type-id-create-output")
        .expect("TYPE_ID script reference");
    reference.hash_type = Some("data".to_string());
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("script_references") && err.message.contains("hash_type"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    let reference = metadata
        .constraints
        .ckb
        .as_mut()
        .expect("CKB constraints")
        .script_references
        .iter_mut()
        .find(|reference| reference.purpose == "type-id-create-output")
        .expect("TYPE_ID script reference");
    reference.dep_source.clear();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("script_references") && err.message.contains("dep_source"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").script_references.clear();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("script_references") && err.message.contains("missing"), "unexpected error: {}", err.message);

    let mut metadata = base;
    let extra_reference = metadata.constraints.ckb.as_ref().expect("CKB constraints").script_references[0].clone();
    metadata.constraints.ckb.as_mut().expect("CKB constraints").script_references.push(extra_reference);
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("script_references") && err.message.contains("extra"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_tampered_spawn_script_reference_metadata() {
    let source = r#"
module cellscript::v0_14_spawn_reference_tamper

action delegate_verify() -> u64 {
    verification
        let pid = spawn("secp256k1_verifier")
        return pid
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let base = result.metadata.clone();
    let mut metadata = base.clone();
    let reference = metadata
        .constraints
        .ckb
        .as_mut()
        .expect("CKB constraints")
        .script_references
        .iter_mut()
        .find(|reference| reference.purpose == "spawn-target")
        .expect("spawn script reference");
    reference.dep_source = "CellDep#0".to_string();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("spawn-target") || err.message.contains("CellDep-or-DepGroup"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    let reference = metadata
        .constraints
        .ckb
        .as_mut()
        .expect("CKB constraints")
        .script_references
        .iter_mut()
        .find(|reference| reference.purpose == "spawn-target")
        .expect("spawn script reference");
    reference.code_hash = Some("00".repeat(32));
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("code_hash/hash_type/args"), "unexpected error: {}", err.message);

    let mut metadata = base;
    metadata.constraints.ckb.as_mut().expect("CKB constraints").script_references.clear();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("script_references") && err.message.contains("missing"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_tampered_runtime_access_and_script_group_metadata() {
    let source = r#"
module cellscript::v0_14_script_group_tamper

resource Token has store {
    amount: u64,
}

action mint(amount: u64) -> output: Token {
    verification
        create output = Token { amount }
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let base = result.metadata.clone();
    validate_compile_metadata(&base, ArtifactFormat::RiscvAssembly).unwrap();

    let mut metadata = base.clone();
    metadata.actions[0].ckb_script_group = None;
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("missing ckb_script_group"), "unexpected error: {}", err.message);

    let mut metadata = base.clone();
    metadata.actions[0].ckb_script_group.as_mut().expect("CKB script group").group_kind = "lock".to_string();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("ckb_script_group") && err.message.contains("runtime access"), "unexpected error: {}", err.message);

    let mut metadata = base;
    metadata.actions[0].ckb_runtime_accesses[0].source = "Bogus".to_string();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(err.message.contains("ckb_runtime_accesses") && err.message.contains("source"), "unexpected error: {}", err.message);

    let mut metadata = result.metadata;
    metadata.runtime.ckb_runtime_accesses.clear();
    let err = validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
    assert!(
        err.message.contains("runtime.ckb_runtime_accesses") && err.message.contains("missing"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn v0_14_compiles_dynamic_blake2b_hash_helper() {
    let source = r#"
module cellscript::blake2b

action digest(input: Hash, expected: Hash) -> bool {
    verification
        let actual = hash_blake2b(input)
        return actual == expected
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let asm = String::from_utf8(result.artifact_bytes.clone()).expect("assembly should be utf8");
    assert!(asm.contains("__ckb_hash_blake2b"), "missing blake2b helper:\n{}", asm);
    assert!(asm.contains("CKB Blake2b-256 helper"), "helper should not be metadata-only:\n{}", asm);
    assert!(asm.contains("xor "), "Blake2b helper should use real mixing instructions:\n{}", asm);
    assert!(result.metadata.runtime.ckb_runtime_features.iter().any(|feature| feature == "ckb-blake2b"));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "hash-blake2b"
            && access.syscall == "CKB_BLAKE2B"
            && access.source == "Profile"
            && access.binding == "hash_blake2b"
    }));
    result.validate().unwrap();

    let elf = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    assert!(!elf.artifact_bytes.is_empty(), "blake2b helper should assemble to ELF");
}

#[test]
fn v0_14_hash_data_packed_wide_fixed_struct_uses_variable_blake2b_without_code18() {
    let source = r#"
module cellscript::wide_packed_hash

struct WideFixed {
    left: Hash,
    right: Hash,
    nonce: u64,
}

action digest(value: WideFixed, expected: Hash) -> bool {
    verification
        let actual = ckb::hash_data_packed(value)
        return actual == expected
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let asm = String::from_utf8(result.artifact_bytes.clone()).expect("assembly should be utf8");
    assert!(asm.contains("__ckb_hash_blake2b_var"), "wide packed hash should use variable Blake2b helper:\n{}", asm);
    assert!(!asm.contains("hash_data_packed currently reuses the 32-byte Blake2b helper"), "stale 32-byte trap remains:\n{}", asm);
    assert!(!asm.contains("fixed-byte-comparison-unresolved"), "code-18 fail path remains:\n{}", asm);
    assert!(!asm.contains("fail closed because hash_data_packed"), "hash_data_packed fail-closed path remains:\n{}", asm);
    result.validate().unwrap();
}

#[test]
fn v0_14_hash_data_packed_wide_fixed_struct_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::wide_packed_hash_vm

struct WideFixed {
    left: Hash,
    right: Hash,
    nonce: u64,
}

action digest(value: WideFixed, expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(value)
        require actual == expected
        return 0
}
"#;
    let left = [0x11u8; 32];
    let right = [0x22u8; 32];
    let nonce = 0x0102_0304_0506_0708u64;
    let mut packed = Vec::new();
    packed.extend_from_slice(&left);
    packed.extend_from_slice(&right);
    packed.extend_from_slice(&nonce.to_le_bytes());
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(packed), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "wide packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_multi_block_fixed_struct_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::multi_block_packed_hash_vm

struct MultiBlockFixed {
    h0: Hash,
    h1: Hash,
    h2: Hash,
    h3: Hash,
    h4: Hash,
    nonce: u64,
}

action digest(value: MultiBlockFixed, expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(value)
        require actual == expected
        return 0
}
"#;
    let mut packed = Vec::new();
    for byte in [0x10u8, 0x20, 0x30, 0x40, 0x50] {
        packed.extend_from_slice(&[byte; 32]);
    }
    packed.extend_from_slice(&0x1112_1314_1516_1718u64.to_le_bytes());
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(packed), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "multi-block packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_three_block_fixed_struct_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::three_block_packed_hash_vm

struct ThreeBlockFixed {
    h0: Hash,
    h1: Hash,
    h2: Hash,
    h3: Hash,
    h4: Hash,
    h5: Hash,
    h6: Hash,
    h7: Hash,
    nonce: u64,
}

action digest(value: ThreeBlockFixed, expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(value)
        require actual == expected
        return 0
}
"#;
    let mut packed = Vec::new();
    for byte in [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80] {
        packed.extend_from_slice(&[byte; 32]);
    }
    packed.extend_from_slice(&0x2122_2324_2526_2728u64.to_le_bytes());
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(packed), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "three-block packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_matches_group_output_cell_data_hash_in_ckb_vm() {
    let source = r#"
module cellscript::packed_hash_output_binding_vm

struct MultiBlockFixed {
    h0: Hash,
    h1: Hash,
    h2: Hash,
    h3: Hash,
    h4: Hash,
    nonce: u64,
}

action digest(value: MultiBlockFixed) -> u64 {
    verification
        let output = source::group_output(0)
        require ckb::cell_data_hash(output) == ckb::hash_data_packed(value)
        return 0
}
"#;
    let mut packed = Vec::new();
    for byte in [0x10u8, 0x20, 0x30, 0x40, 0x50] {
        packed.extend_from_slice(&[byte; 32]);
    }
    packed.extend_from_slice(&0x1112_1314_1516_1718u64.to_le_bytes());

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action.entry_witness_args(&[EntryWitnessArg::Bytes(packed.clone())]).expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.outputs[0].data = Bytes::from(packed);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "packed hash should match group output data hash in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_nested_fixed_struct_uses_variable_blake2b_without_code18() {
    let source = r#"
module cellscript::nested_packed_hash

struct InnerFixed {
    left: Hash,
    right: Hash,
}

struct OuterFixed {
    tag: u8,
    inner: InnerFixed,
    nonce: u64,
}

action digest(inner: InnerFixed, expected: Hash) -> bool {
    verification
        let value = OuterFixed { tag: 7, inner: inner, nonce: 42 }
        let actual = ckb::hash_data_packed(value)
        return actual == expected
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let asm = String::from_utf8(result.artifact_bytes.clone()).expect("assembly should be utf8");
    assert!(asm.contains("__ckb_hash_blake2b_var"), "nested packed hash should use variable Blake2b helper:\n{}", asm);
    assert!(asm.contains("__cellscript_memcpy_fixed"), "nested fixed aggregate should use compact byte materialisation:\n{}", asm);
    assert!(!asm.contains("field access .inner (unresolved)"), "nested field provenance was lost:\n{}", asm);
    assert!(!asm.contains("fixed-byte-comparison-unresolved"), "code-18 fail path remains:\n{}", asm);
    assert!(!asm.contains("fail closed because hash_data_packed"), "hash_data_packed fail-closed path remains:\n{}", asm);
    result.validate().unwrap();
}

#[test]
fn v0_14_hash_data_packed_nested_fixed_struct_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::nested_packed_hash_vm

struct InnerFixed {
    left: Hash,
    right: Hash,
}

struct OuterFixed {
    tag: u8,
    inner: InnerFixed,
    nonce: u64,
}

action digest(inner: InnerFixed, expected: Hash) -> u64 {
    verification
        let value = OuterFixed { tag: 7, inner: inner, nonce: 42 }
        let actual = ckb::hash_data_packed(value)
        require actual == expected
        return 0
}
"#;
    let left = [0x33u8; 32];
    let right = [0x44u8; 32];
    let mut inner = Vec::new();
    inner.extend_from_slice(&left);
    inner.extend_from_slice(&right);
    let mut packed = Vec::new();
    packed.push(7);
    packed.extend_from_slice(&inner);
    packed.extend_from_slice(&42u64.to_le_bytes());
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(inner), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "nested packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_nested_parameter_struct_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::nested_parameter_packed_hash_vm

struct IntentCore {
    action: u8,
    subject: Hash,
    old_nonce: u64,
    new_nonce: u64,
    payout_hash: Hash,
}

struct SignedIntent {
    core: IntentCore,
    canonical_hash: Hash,
    expected_receipt_hash: Hash,
}

action digest(intent: SignedIntent, expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(intent)
        require actual == expected
        return 0
}
"#;
    let subject = [0x55u8; 32];
    let payout_hash = [0x66u8; 32];
    let canonical_hash = [0x77u8; 32];
    let expected_receipt_hash = [0x88u8; 32];
    let mut packed = Vec::new();
    packed.push(2);
    packed.extend_from_slice(&subject);
    packed.extend_from_slice(&10u64.to_le_bytes());
    packed.extend_from_slice(&11u64.to_le_bytes());
    packed.extend_from_slice(&payout_hash);
    packed.extend_from_slice(&canonical_hash);
    packed.extend_from_slice(&expected_receipt_hash);
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(packed), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "nested parameter packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_agreement_sized_nested_parameter_matches_ckb_vm_hash() {
    let source = r#"
module cellscript::agreement_sized_nested_packed_hash_vm

struct AgreementIntentCore {
    action: u8,
    agreement_id: Hash,
    terms_hash: Hash,
    borrower_authority_hash: Hash,
    lender_authority_hash: Hash,
    old_status: u8,
    new_status: u8,
    old_nonce: u64,
    new_nonce: u64,
    terminal_amount: u64,
    payout_commitment_hash: Hash,
    expiry_timepoint: u64,
}

struct AgreementSignedIntent {
    core: AgreementIntentCore,
    canonical_envelope_hash: Hash,
    expected_receipt_hash: Hash,
}

action digest(intent: AgreementSignedIntent, expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(intent)
        require actual == expected
        return 0
}
"#;
    let mut packed = Vec::new();
    packed.push(0);
    for byte in [0x11u8, 0x22, 0x33, 0x44] {
        packed.extend_from_slice(&[byte; 32]);
    }
    packed.push(0);
    packed.push(1);
    packed.extend_from_slice(&0u64.to_le_bytes());
    packed.extend_from_slice(&1u64.to_le_bytes());
    packed.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
    packed.extend_from_slice(&[0x55; 32]);
    packed.extend_from_slice(&0x1112_1314_1516_1718u64.to_le_bytes());
    packed.extend_from_slice(&[0x66; 32]);
    packed.extend_from_slice(&[0x77; 32]);
    assert_eq!(packed.len(), 259);
    let expected = ckb_blake2b256(&packed);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[EntryWitnessArg::Bytes(packed), EntryWitnessArg::Hash(expected)])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "agreement-sized nested parameter packed hash should match in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_result_survives_later_source_hash_calls() {
    let source = r#"
module cellscript::packed_hash_lifetime_vm

struct AgreementIntentCore {
    action: u8,
    agreement_id: Hash,
    terms_hash: Hash,
    borrower_authority_hash: Hash,
    lender_authority_hash: Hash,
    old_status: u8,
    new_status: u8,
    old_nonce: u64,
    new_nonce: u64,
    terminal_amount: u64,
    payout_commitment_hash: Hash,
    expiry_timepoint: u64,
}

struct AgreementSignedIntent {
    core: AgreementIntentCore,
    canonical_envelope_hash: Hash,
    expected_receipt_hash: Hash,
}

struct LaterValue {
    left: Hash,
    right: Hash,
    nonce: u64,
}

action digest(intent: AgreementSignedIntent, later: LaterValue, expected: Hash, output_expected: Hash) -> u64 {
    verification
        let actual = ckb::hash_data_packed(intent)
        let later_hash = ckb::hash_data_packed(later)
        require later_hash == output_expected
        require ckb::cell_data_hash(source::group_output(0)) == output_expected
        require actual == expected
        return 0
}
"#;
    let mut intent = Vec::new();
    intent.push(0);
    for byte in [0x11u8, 0x22, 0x33, 0x44] {
        intent.extend_from_slice(&[byte; 32]);
    }
    intent.push(0);
    intent.push(1);
    intent.extend_from_slice(&0u64.to_le_bytes());
    intent.extend_from_slice(&1u64.to_le_bytes());
    intent.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
    intent.extend_from_slice(&[0x55; 32]);
    intent.extend_from_slice(&0x1112_1314_1516_1718u64.to_le_bytes());
    intent.extend_from_slice(&[0x66; 32]);
    intent.extend_from_slice(&[0x77; 32]);
    assert_eq!(intent.len(), 259);
    let expected = ckb_blake2b256(&intent);
    let mut later = Vec::new();
    later.extend_from_slice(&[0x88; 32]);
    later.extend_from_slice(&[0x99; 32]);
    later.extend_from_slice(&0x2122_2324_2526_2728u64.to_le_bytes());
    let output_expected = ckb_blake2b256(&later);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[
            EntryWitnessArg::Bytes(intent),
            EntryWitnessArg::Bytes(later.clone()),
            EntryWitnessArg::Hash(expected),
            EntryWitnessArg::Hash(output_expected),
        ])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.outputs[0].data = Bytes::from(later);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "packed hash result should survive later SourceView hash calls: {:?}", execution);
}

#[test]
fn v0_14_hash_data_packed_signature_payload_fields_match_ckb_vm_hash() {
    let source = r#"
module cellscript::signature_payload_field_hash_vm

struct SignaturePayload {
    pubkey: [u8; 32],
    signature: [u8; 64],
}

action digest(payload: SignaturePayload, expected_pubkey: Hash, expected_signature: Hash) -> u64 {
    verification
        require ckb::hash_data_packed(payload.pubkey) == expected_pubkey
        require ckb::hash_data_packed(payload.signature) == expected_signature
        return 0
}
"#;
    let pubkey = [0x31u8; 32];
    let signature = [0x62u8; 64];
    let mut payload = Vec::new();
    payload.extend_from_slice(&pubkey);
    payload.extend_from_slice(&signature);
    let expected_pubkey = ckb_blake2b256(&pubkey);
    let expected_signature = ckb_blake2b256(&signature);

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "digest").expect("digest action metadata");
    let witness = action
        .entry_witness_args(&[
            EntryWitnessArg::Bytes(payload),
            EntryWitnessArg::Hash(expected_pubkey),
            EntryWitnessArg::Hash(expected_signature),
        ])
        .expect("entry witness should encode");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes).to_vec();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];

    let execution = ckb_script_runner::execute_cellscript_script(&elf, &fixture);
    assert_eq!(execution.exit_code, 0, "signature payload fields should materialise canonically in CKB VM: {:?}", execution);
}

#[test]
fn v0_14_bip340_nested_witness_signature_fields_have_no_code18_materialisation_path() {
    let source = r#"
module cellscript::bip340_nested_witness_materialisation

struct SignaturePayload {
    pubkey: [u8; 32],
    signature: [u8; 64],
}

struct SignatureEnvelope {
    signed_intent_hash: Hash,
    valid_sig: SignaturePayload,
    wrong_sig: SignaturePayload,
}

action verify(witness envelope: SignatureEnvelope, witness use_wrong_signature: u8) -> u64 {
    verification
        if use_wrong_signature == 0 {
            verifier::btc::bip340::require_signature(envelope.signed_intent_hash, envelope.valid_sig.pubkey, envelope.valid_sig.signature)
        } else {
            let wrong_sig = envelope.wrong_sig
            verifier::btc::bip340::require_signature(envelope.signed_intent_hash, wrong_sig.pubkey, wrong_sig.signature)
        }
        return 0
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let asm = String::from_utf8(result.artifact_bytes.clone()).expect("assembly should be utf8");
    assert!(asm.contains("__novaseal_bip340_require_signature"), "missing BIP340 runtime helper:\n{}", asm);
    assert!(
        asm.contains("bounds check SignatureEnvelope.signed_intent_hash required=32"),
        "message hash was not materialised for BIP340:\n{}",
        asm
    );
    assert!(
        asm.contains("bounds check SignatureEnvelope.valid_sig.pubkey required=64"),
        "valid pubkey was not materialised for BIP340:\n{}",
        asm
    );
    assert!(
        asm.contains("bounds check SignatureEnvelope.valid_sig.signature required=128"),
        "valid signature was not materialised for BIP340:\n{}",
        asm
    );
    assert!(
        asm.contains("bounds check SignatureEnvelope.wrong_sig.pubkey required=160"),
        "wrong-signature pubkey was not materialised for BIP340:\n{}",
        asm
    );
    assert!(
        asm.contains("bounds check SignatureEnvelope.wrong_sig.signature required=224"),
        "wrong-signature bytes were not materialised for BIP340:\n{}",
        asm
    );
    assert!(!asm.contains("fail closed because BIP340"), "BIP340 materialisation fail-closed path remains:\n{}", asm);
    assert!(!asm.contains("li a0, 18"), "reachable code-18 trap remains in BIP340 nested witness materialisation:\n{}", asm);
    assert!(!asm.contains("li a0, 59"), "BIP340 message materialisation trap remains:\n{}", asm);
    assert!(!asm.contains("li a0, 60"), "BIP340 pubkey materialisation trap remains:\n{}", asm);
    assert!(!asm.contains("li a0, 61"), "BIP340 signature materialisation trap remains:\n{}", asm);
    result.validate().unwrap();
}

#[test]
fn v0_14_compiles_hash_pair_helper() {
    let source = r#"
module cellscript::hash_pair

action pair(left: Hash, right: Hash, expected: Hash) -> bool {
    verification
        let actual = hash_pair(left, right)
        return actual == expected
}
"#;

    let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
    let asm = String::from_utf8(result.artifact_bytes.clone()).expect("assembly should be utf8");
    assert!(asm.contains("__ckb_hash_pair"), "missing hash_pair helper:\n{}", asm);
    assert!(asm.contains("hash_pair combines two 32-byte Hash inputs"), "helper should not be metadata-only:\n{}", asm);
    assert!(asm.contains("xori t0, t0, 64"), "hash_pair should commit a 64-byte message length:\n{}", asm);
    let action = result.metadata.actions.iter().find(|action| action.name == "pair").expect("pair action metadata");
    assert!(
        !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
        "hash_pair result comparison should be verifier-coverable: {:?}",
        action.fail_closed_runtime_features
    );
    assert!(result.metadata.runtime.ckb_runtime_features.iter().any(|feature| feature == "profile-hash-pair"));
    assert!(result.metadata.runtime.ckb_runtime_features.iter().any(|feature| feature == "ckb-blake2b"));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "hash-pair"
            && access.syscall == "CKB_BLAKE2B"
            && access.source == "Profile"
            && access.binding == "hash_pair"
    }));
    result.validate().unwrap();

    let elf = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    assert!(!elf.artifact_bytes.is_empty(), "hash_pair helper should assemble to ELF");
}

#[test]
fn v0_14_rejects_blake2b_non_hash_input() {
    let err = compile(
        r#"
module cellscript::bad_blake2b

action bad(input: u64) -> Hash {
    verification
        return hash_blake2b(input)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("hash_blake2b expects Hash input"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_hash_pair_non_hash_input() {
    let err = compile(
        r#"
module cellscript::bad_hash_pair

action bad(input: u64) -> Hash {
    verification
        return hash_pair(Hash::zero(), input)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("hash_pair expects (Hash, Hash)"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_spawn_ipc_fd_use_after_close() {
    let err = compile(
        r#"
module cellscript::bad_fd

action bad(value: u64) -> u64 {
    verification
        let (read_fd, write_fd) = pipe()
        close(read_fd)
        pipe_read(read_fd)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("pipe_read uses a Spawn/IPC file descriptor after close"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_spawn_ipc_fd_double_close() {
    let err = compile(
        r#"
module cellscript::bad_fd

action bad() -> u64 {
    verification
        let fds = pipe()
        let read_fd = fds.0
        close(read_fd)
        close(read_fd)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("already closed"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_rejects_spawn_ipc_fd_leak() {
    let err = compile(
        r#"
module cellscript::bad_fd

action bad(value: u64) -> u64 {
    verification
        let fds = pipe()
        let read_fd = fds.0
        let write_fd = fds.1
        pipe_write(write_fd, value)
        return pipe_read(read_fd)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("is not closed before callable exit"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_spawn_target_must_be_static() {
    let ok = compile(
        r#"
module cellscript::static_spawn

const VERIFY_TARGET: String = "secp256k1_verifier";

action delegate() -> u64 {
    verification
        return spawn(VERIFY_TARGET)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    assert!(ok.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.operation == "spawn"));

    let err = compile(
        r#"
module cellscript::dynamic_spawn

action delegate(target: String) -> u64 {
    verification
        return spawn(target)
}
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("spawn target must be a static script reference"), "unexpected error: {}", err.message);
}

#[test]
fn v0_14_language_examples_cover_spawn_pipeline_type_id_and_canonical_style() {
    let pipeline = compile(
        include_str!("../examples/language/v0_14_multi_step_pipeline.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let pipeline_action =
        pipeline.metadata.actions.iter().find(|action| action.name == "pipe_to_delegate").expect("pipe_to_delegate metadata");
    for operation in ["pipe", "pipe-write", "spawn", "wait", "pipe-read", "close-fd"] {
        assert!(
            pipeline_action.ckb_runtime_accesses.iter().any(|access| access.operation == operation),
            "missing {operation}: {:?}",
            pipeline_action.ckb_runtime_accesses
        );
    }
    assert!(pipeline_action
        .transaction_runtime_input_requirements
        .iter()
        .any(|requirement| { requirement.component == "spawn-target-cell-dep" && requirement.status == "runtime-required" }));

    let blake2b = compile(
        include_str!("../examples/language/v0_14_hash_blake2b.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let blake2b_lock = blake2b.metadata.locks.iter().find(|lock| lock.name == "blake2b_matches").expect("blake2b lock metadata");
    assert!(blake2b_lock.ckb_runtime_accesses.iter().any(|access| access.operation == "hash-blake2b"));

    let type_id = compile(
        include_str!("../examples/language/v0_14_ckb_type_id_create.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let mint =
        type_id.metadata.actions.iter().find(|action| action.name == "mint_identity_token").expect("mint_identity_token metadata");
    assert_eq!(mint.ckb_type_id_output_indexes(), vec![0]);
    assert!(mint.create_set[0].ckb_type_id.is_some());

    let canonical = compile(
        include_str!("../examples/language/canonical_style.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let vault_owner = canonical.metadata.locks.iter().find(|lock| lock.name == "vault_owner").expect("vault_owner metadata");
    assert!(vault_owner.params.iter().any(|param| param.source == "protected"));
    assert!(vault_owner.params.iter().any(|param| param.source == "lock_args"));
    assert!(vault_owner.params.iter().any(|param| param.source == "witness"));
    assert!(vault_owner.ckb_runtime_accesses.iter().any(|access| access.operation == "sighash-all"));
}
