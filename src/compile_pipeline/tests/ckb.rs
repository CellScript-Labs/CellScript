// Compiler integration ckb behavior; retains crate::tests identities.

    #[test]
    fn compile_accepts_lock_args_script_args_binding() {
        let result = compile(LOCK_ARGS_PARAM_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(
            asm.contains("# cellscript abi: LOAD_SCRIPT reason=entry_lock_args"),
            "lock_args entry wrapper must load the executing Script.args:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript entry abi: lock_args param owner consumes 32 script arg byte(s)"),
            "lock_args Address should consume exactly 32 script arg bytes:\n{}",
            asm
        );
        let script_args_decode = asm
            .split("# cellscript entry abi: lock_args param owner consumes 32 script arg byte(s)")
            .next()
            .expect("Script.args decoder prefix");
        assert!(
            script_args_decode.contains("lbu t1, 1(t0)"),
            "Script.args u32 decoder must keep its base pointer live:\n{}",
            script_args_decode
        );
        assert!(
            !script_args_decode.contains("lbu t0, 1(t0)"),
            "Script.args u32 decoder must not overwrite its base pointer:\n{}",
            script_args_decode
        );
        assert!(
            script_args_decode.contains("sd t6, 0(t3)") && script_args_decode.contains("sd t5, 0(t6)"),
            "Script.args start/length stores must preserve the decoded byte length:\n{}",
            script_args_decode
        );
        assert!(
            !asm.contains("# cellscript abi: bind read-only param owner to Input#")
                && !asm.contains("# cellscript abi: bind read-only param owner to CellDep#"),
            "lock_args must remain bound to Script.args instead of transaction cell data:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: LOAD_WITNESS reason=entry_args"),
            "lock_args-only wrapper should not require a witness payload:\n{}",
            asm
        );

        let lock = result.metadata.locks.iter().find(|lock| lock.name == "bad").expect("lock metadata");
        let owner = lock.params.iter().find(|param| param.name == "owner").expect("owner param");
        assert_eq!(owner.source, "lock_args");
        assert!(owner.lock_args_data_source);
        assert_eq!(result.metadata.constraints.ckb.as_ref().expect("ckb constraints").max_entry_witness_bytes, 0);
    }

    #[test]
    fn compile_preserves_dynamic_witness_cursor_across_lock_args() {
        let source = r#"
module lock_args_dynamic_witness;

resource Token {
    owner: Address,
}

struct Claim {
    owner: Address,
}

lock guarded(protected token: Token, lock_args owner: Address, witness proof: Claim, witness nonce: u64) -> bool {
    verification
    require owner == token.owner
    require proof.owner == token.owner
    require nonce > 0
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        let lock_args_decode = asm
            .split("# cellscript entry abi: lock_args param owner consumes 32 script arg byte(s)")
            .nth(1)
            .and_then(|suffix| suffix.split("# cellscript entry abi: schema param proof").next())
            .expect("lock args decode before dynamic witness decode");

        assert!(
            !lock_args_decode.contains("t5") && !lock_args_decode.contains("t6") && !lock_args_decode.contains("\n    li t3,"),
            "lock_args decoding must preserve the live witness and Script.args cursor registers:\n{}",
            lock_args_decode
        );

        let scalar_decode = asm
            .split("# cellscript entry abi: scalar param nonce")
            .nth(1)
            .and_then(|suffix| suffix.split("# cellscript entry abi: reject trailing witness payload bytes").next())
            .expect("scalar witness decode");
        assert!(
            scalar_decode.contains("lbu t1, 1(t0)") && !scalar_decode.contains("lbu t0, 1(t0)"),
            "scalar witness decoding must keep its base pointer live:\n{}",
            scalar_decode
        );
    }

    #[test]
    fn compile_lowers_read_ref_schema_field_to_ckb_runtime_assembly() {
        let result = compile(READ_REF_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref source=CellDep index=0"),
            "read_ref did not lower to CKB LOAD_CELL CellDep ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check fixed-byte scalar field access loaded bytes var0 expected=8"),
            "read_ref schema field access did not enforce exact fixed schema size:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte scalar field Ref(Named(\"Config\")).threshold offset=0 size=8"),
            "read_ref schema field access did not expose concrete layout:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 0(t4)") && asm.contains("slli t2, t2, 56"),
            "read_ref u64 field access did not lower to an unaligned-safe byte load sequence:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "inspect").expect("inspect metadata");
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "read-ref:read_ref_Config#0"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("read-ref-cell-dep-data=checked-runtime")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "read-ref:read_ref_Config#0"
                && requirement.status == "checked-runtime"
                && requirement.component == "read-ref-cell-dep-data"
                && requirement.source == "CellDep"
                && requirement.binding == "read_ref_Config"
                && requirement.field.as_deref() == Some("data")
                && requirement.abi == "read-ref-load-cell-dep"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
    }

    #[test]
    fn compile_emits_ckb_style_load_cell_abi_for_cell_runtime_summary() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=consume source=Input index=0"),
            "consume summary did not use CKB Source::Input LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref source=CellDep index=0"),
            "read_ref summary did not use CKB Source::CellDep LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=create source=Output index=0"),
            "create summary did not use CKB Source::Output LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(asm.contains("addi a0, sp,"), "store_data buffer address was not prepared:\n{}", asm);
        assert!(asm.contains("addi a1, sp,"), "store_data size pointer was not prepared:\n{}", asm);
        assert!(asm.contains("li a2, 0"), "store_data offset was not prepared:\n{}", asm);
        assert!(asm.contains("li a3, 0"), "LOAD_CELL index register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 1"), "Input source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 2"), "Output source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 3"), "CellDep source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a7, 2092"), "LOAD_CELL_DATA syscall number was not emitted:\n{}", asm);
        assert!(!asm.contains("li a7, 2073"), "summary consume regressed to LOAD_INPUT with stale argument order:\n{}", asm);
    }

    #[test]
    fn bundled_token_example_strict_ckb_compile_is_admitted() {
        let result = compile(
            include_str!("../../../examples/token.cell"),
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();
        assert_eq!(result.metadata.target_profile.name, "ckb");
        assert_eq!(result.metadata.target_profile.artifact_packaging, "ckb-elf");
        let merge = result.metadata.actions.iter().find(|action| action.name == "merge").expect("merge metadata");
        assert!(
            merge
                .transaction_runtime_input_requirements
                .iter()
                .filter(|requirement| {
                    requirement.feature == "resource-conservation:Token" && requirement.component == "resource-conservation-proof"
                })
                .all(|requirement| requirement.status == "checked-runtime" && requirement.blocker_class.is_none()),
            "token merge should no longer require a CKB policy exception: {:?}",
            merge.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_entry_witness_rejects_payloads_larger_than_buffer() {
        let result = compile(U128_EQ_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript entry abi: reject witnesses larger than the local entry buffer"),
            "entry witness wrapper should reject sizes above its local buffer:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 4097"), "entry witness wrapper should enforce ENTRY_WITNESS_BUFFER_SIZE=4096:\n{}", asm);
    }

    #[test]
    fn compile_produces_ckb_elf_without_vm_abi_trailer() {
        let result = compile(
            SIMPLE_PROGRAM,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert_eq!(result.metadata.target_profile.name.as_str(), "ckb");
        assert_eq!(result.metadata.target_profile.artifact_packaging.as_str(), "ckb-elf");
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert_eq!(result.artifact_bytes.len(), crate::strip_vm_abi_trailer(&result.artifact_bytes).len());
        result.validate().unwrap();
    }

    #[test]
    fn compile_accepts_ckb_shared_create_when_verifier_covered() {
        let source = r#"
module test::shared_create

shared Config has store {
    admin: Address,
    enabled: bool,
}

action create_config(admin: Address, enabled: bool) -> Config {
    verification
        create Config {
            admin: admin,
            enabled: enabled,
        } with_lock(admin)
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "create_config").expect("create_config metadata");

        assert_eq!(result.metadata.target_profile.name.as_str(), "ckb");
        assert!(!action.touches_shared.is_empty(), "shared creation should remain scheduler-visible metadata");
        assert!(action.fail_closed_runtime_features.is_empty(), "shared create should not carry fail-closed debt");
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:Config:create_Config"
                && requirement.status == "checked-runtime"
                && requirement.component == "create-output-fields"
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:Config:create_Config"
                && requirement.status == "checked-runtime"
                && requirement.component == "create-output-lock"
        }));
        result.validate().unwrap();
    }

    #[test]
    fn ckb_deploy_manifest_surfaces_hash_type_and_dep_group_policy() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "deploy_manifest"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[deploy.ckb]
hash_type = "data2"
out_point = "0x1111111111111111111111111111111111111111111111111111111111111111:0"
dep_type = "code"
data_hash = "0x2222222222222222222222222222222222222222222222222222222222222222"

[[deploy.ckb.cell_deps]]
name = "secp256k1"
out_point = "0x3333333333333333333333333333333333333333333333333333333333333333:1"
dep_type = "dep_group"
hash_type = "type"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module deploy_manifest

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        let ckb = result.metadata.constraints.ckb.as_ref().expect("ckb constraints");
        assert_eq!(ckb.hash_type_policy.source, "Cell.toml deploy.ckb.hash_type");
        assert_eq!(ckb.hash_type_policy.declared_hash_type.as_deref(), Some("data2"));
        assert_eq!(ckb.hash_type_policy.status, "manifest-declared-builder-must-match");
        assert_eq!(ckb.profile_abi_contract.minimum_vm_version, 2);
        assert_eq!(ckb.profile_abi_contract.riscv_isa, "rv64imac_zbb");
        assert_eq!(ckb.profile_abi_contract.deployment_hash_types, ["data2"]);
        assert_eq!(ckb.dep_group_manifest.source, "Cell.toml deploy.ckb");
        assert!(ckb.dep_group_manifest.dep_group_supported);
        assert_eq!(ckb.dep_group_manifest.declared_cell_deps.len(), 2);
        assert!(ckb.dep_group_manifest.declared_cell_deps.iter().any(|dep| {
            dep.name == "primary"
                && dep.dep_type == "code"
                && dep.tx_hash.as_deref() == Some("0x1111111111111111111111111111111111111111111111111111111111111111")
                && dep.index == Some(0)
        }));
        assert!(ckb.dep_group_manifest.declared_cell_deps.iter().any(|dep| {
            dep.name == "secp256k1"
                && dep.dep_type == "dep_group"
                && dep.tx_hash.as_deref() == Some("0x3333333333333333333333333333333333333333333333333333333333333333")
                && dep.index == Some(1)
                && dep.hash_type.as_deref() == Some("type")
        }));
        assert_eq!(ckb.dep_group_manifest.status, "manifest-declares-dep-group-builder-must-expand-or-reference");
    }

    #[test]
    fn ckb_deploy_manifest_rejects_conflicting_cell_dep_locations() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "conflicting_cell_dep_location"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "secp256k1"
out_point = "0x3333333333333333333333333333333333333333333333333333333333333333:1"
tx_hash = "0x4444444444444444444444444444444444444444444444444444444444444444"
index = 2
dep_type = "dep_group"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module conflicting_cell_dep_location

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).expect_err("conflicting CKB cell_dep locations must fail closed");
        assert!(
            err.message.contains("CKB cell_dep location must use either out_point or tx_hash/index"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn ckb_deploy_manifest_rejects_incomplete_split_cell_dep_location() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "incomplete_cell_dep_location"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "secp256k1"
tx_hash = "0x4444444444444444444444444444444444444444444444444444444444444444"
dep_type = "dep_group"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module incomplete_cell_dep_location

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).expect_err("incomplete CKB cell_dep split location must fail closed");
        assert!(
            err.message.contains("CKB cell_dep split location must provide both tx_hash and index"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn ckb_deploy_manifest_rejects_invalid_dep_type() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bad_deploy_manifest"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[deploy.ckb]
dep_type = "unknown"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module bad_deploy_manifest

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unsupported CKB dep_type 'unknown'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ckb_deploy_manifest_rejects_invalid_hash_type() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bad_hash_type_manifest"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[deploy.ckb]
hash_type = "unsupported"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module bad_hash_type_manifest

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unsupported CKB hash_type 'unsupported'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ckb_deploy_manifest_rejects_pre_vm2_hash_type_for_generated_artifact() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "vm1_deploy_manifest"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[deploy.ckb]
hash_type = "data1"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module vm1_deploy_manifest

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("require deploy.ckb.hash_type = 'data2'"), "unexpected error: {}", err.message);
        assert!(err.message.contains("rv64imac_zbb"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_ckb_group_source_large_immediate_to_riscv_elf() {
        let result = compile(
            CKB_HEADER_EPOCH_PROGRAM,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert_eq!(result.artifact_bytes.len(), crate::strip_vm_abi_trailer(&result.artifact_bytes).len());
        result.validate().unwrap();
    }

    #[test]
    fn compile_lowers_read_ref_schema_field_to_ckb_runtime_elf() {
        let result =
            compile(READ_REF_FIELD_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(result.metadata.runtime.ckb_runtime_required);
        assert!(!result.metadata.runtime.standalone_runner_compatible);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    }

    #[test]
    fn compile_infers_read_only_effect_for_ckb_header_runtime_builtins() {
        let result = compile(
            FN_CKB_HEADER_RUNTIME_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "helper").expect("helper metadata");
        assert!(!function.effects.context_reads().is_empty());
        assert!(function.effects.allows_observation());
        assert_eq!(function.effect_evidence_tier, crate::EvidenceTier::CheckedStatic);
    }

    #[test]
    fn compile_accepts_action_witness_source_qualifier() {
        let source = r#"
module test

resource Token has store {
    amount: u64,
}

action mint(witness amount: u64) -> token: Token {
    verification
        create token = Token {
            amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "mint").expect("mint metadata");
        let amount = action.params.iter().find(|param| param.name == "amount").expect("amount metadata");
        assert_eq!(amount.source, "witness");
        assert!(amount.witness_data_source);
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "token"));
    }

    #[test]
    fn ckb_entry_action_scope_excludes_unselected_unsupported_code() {
        let source = r#"
module scoped_ckb

resource Token has destroy {
    amount: u64,
}

action burn(token: Token) {
    verification
        destroy token

}
action unsupported_timepoint() -> u64 {
    verification
        return env::current_timepoint()
}
"#;
        let temp = tempdir().unwrap();
        let entry = Utf8Path::from_path(temp.path()).unwrap().join("scoped.cell");
        std::fs::write(&entry, source).unwrap();

        let full =
            compile_file(&entry, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        assert!(
            full.metadata.runtime.ckb_runtime_features.contains(&"load-header-timepoint".to_string()),
            "full CKB compile should include timepoint"
        );

        let scoped = compile_file_with_entry_action(
            &entry,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
            "burn",
        )
        .unwrap();
        assert_eq!(scoped.metadata.actions.len(), 1);
        assert_eq!(scoped.metadata.actions[0].name, "burn");
        assert!(scoped.metadata.types.iter().any(|ty| ty.name == "Token"));
        assert!(!scoped.metadata.actions.iter().any(|action| action.name == "unsupported_timepoint"));
        assert!(scoped.artifact_bytes.starts_with(b"\x7fELF"));
        assert_eq!(scoped.metadata.target_profile.name, "ckb");
    }

    #[test]
    fn ckb_entry_lock_scope_selects_lock_entrypoint() {
        let source = r#"
module scoped_lock

resource DynamicCell {
    name: String,
}

lock owner_lock(owner: Address) -> bool {
    verification
        true
}

action unsupported(name: String) -> DynamicCell {
    verification
        let cell = create DynamicCell {
            name: name,
        };
        cell
}
"#;
        let temp = tempdir().unwrap();
        let entry = Utf8Path::from_path(temp.path()).unwrap().join("scoped_lock.cell");
        std::fs::write(&entry, source).unwrap();

        let scoped = compile_file_with_entry_lock(
            &entry,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
            "owner_lock",
        )
        .unwrap();
        assert_eq!(scoped.metadata.locks.len(), 1);
        assert_eq!(scoped.metadata.locks[0].name, "owner_lock");
        assert!(scoped.metadata.actions.is_empty());
        assert!(!scoped.metadata.types.iter().any(|ty| ty.name == "DynamicCell"));
        assert!(scoped.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn ckb_lock_false_return_lowers_to_script_failure() {
        let source = r#"
module scoped_lock

lock owner_lock(owner: Address, claimed: Address) -> bool {
    verification
        owner == claimed
}
"#;
        let temp = tempdir().unwrap();
        let entry = Utf8Path::from_path(temp.path()).unwrap().join("scoped_lock.cell");
        std::fs::write(&entry, source).unwrap();

        let scoped = compile_file_with_entry_lock(
            &entry,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-asm".to_string()),
                ..CompileOptions::default()
            },
            "owner_lock",
        )
        .unwrap();
        let asm = String::from_utf8(scoped.artifact_bytes).unwrap();
        assert!(
            asm.contains("cellscript runtime error 5 assertion-failed"),
            "lock false return must lower to a non-zero CKB script exit:\n{}",
            asm
        );
        assert!(asm.contains("lock_predicate_true"), "lock predicate true branch missing:\n{}", asm);
    }

    #[test]
    fn compile_result_validation_rejects_mismatched_ckb_type_id_create_output_plan() {
        let program = r#"
module audit::type_id_create

#[type_id("cellscript::asset::Token:v1")]
resource Token has store {
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        return create Token { amount: amount }
}
"#;

        let mut metadata = compile_metadata_for_profile_without_artifact_policy(program, crate::TargetProfile::Ckb);
        metadata.actions[0].create_set[0].ckb_type_id.as_mut().expect("CKB TYPE_ID output plan").output_index = 1;

        let err = crate::validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
        assert!(err.message.contains("ckb_type_id.output_index"), "unexpected error: {}", err.message);

        metadata.actions[0].create_set[0].ckb_type_id = None;
        let err = crate::validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
        assert!(err.message.contains("missing ckb_type_id output plan"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_mismatched_ckb_output_data_binding() {
        let program = r#"
module audit::output_data

resource Token has store {
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        return create Token { amount: amount }
}
"#;

        let mut metadata = compile_metadata_for_profile_without_artifact_policy(program, crate::TargetProfile::Ckb);
        metadata.actions[0].create_set[0].ckb_output_data.as_mut().expect("CKB output data binding").output_data_index = 1;

        let err = crate::validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
        assert!(err.message.contains("ckb_output_data.output_data_index"), "unexpected error: {}", err.message);

        metadata.actions[0].create_set[0].ckb_output_data = None;
        let err = crate::validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap_err();
        assert!(err.message.contains("missing ckb_output_data index binding"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_unique_script_args_and_singleton_identity_emit_hash_checks() {
        let create_script_args_program = r#"
module audit::create_unique_script_args_runtime

resource Token has store
    identity(script_args)
{
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        create_unique<Token>(identity = script_args) { amount: amount }
}
"#;
        let create_script_args = compile(
            create_script_args_program,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        )
        .unwrap();
        let create_script_args_asm = String::from_utf8(create_script_args.artifact_bytes.clone()).unwrap();
        assert!(
            create_script_args.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
                access.operation == "create_unique-identity-lock_hash"
                    && access.syscall == "LOAD_CELL_BY_FIELD"
                    && access.source == "GroupInput"
                    && access.index == 0
            }),
            "missing create_unique script_args identity runtime access evidence"
        );
        assert!(
            create_script_args_asm.contains("create_unique script_args identity anchor LockHash GroupInput#0 == Output#0"),
            "missing create_unique script_args lock-hash anchor:\n{}",
            create_script_args_asm
        );

        let script_args_program = r#"
module audit::unique_script_args_runtime

resource Token has store, replace
    identity(script_args)
{
    amount: u64
}

action replace(old: Token, amount: u64) -> Token {
    verification
        replace_unique<Token>(identity = script_args) old { amount: amount }
}
"#;
        let script_args =
            compile(script_args_program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
                .unwrap();
        let script_args_asm = String::from_utf8(script_args.artifact_bytes.clone()).unwrap();
        assert!(
            script_args.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
                access.operation == "replace_unique-identity-lock_hash"
                    && access.syscall == "LOAD_CELL_BY_FIELD"
                    && access.source == "Input"
                    && access.index == 0
            }),
            "missing script_args identity runtime access evidence"
        );
        assert!(
            script_args_asm.contains("replace_unique script_args identity preservation LockHash Input#0 == Output#0"),
            "missing script_args lock-hash preservation check:\n{}",
            script_args_asm
        );

        let singleton_program = r#"
module audit::unique_singleton_runtime

resource Config has store, replace
    identity(singleton_type)
{
    value: u64
}

action replace(old: Config, value: u64) -> Config {
    verification
        replace_unique<Config>(identity = singleton_type) old { value: value }
}
"#;
        let singleton =
            compile(singleton_program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
                .unwrap();
        let singleton_asm = String::from_utf8(singleton.artifact_bytes.clone()).unwrap();
        assert!(
            singleton.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
                access.operation == "replace_unique-identity-type_hash"
                    && access.syscall == "LOAD_CELL_BY_FIELD"
                    && access.source == "Output"
                    && access.index == 0
            }),
            "missing singleton identity runtime access evidence"
        );
        assert!(
            singleton_asm.contains("replace_unique type identity preservation TypeHash Input#0 == Output#0"),
            "missing singleton type-hash preservation check:\n{}",
            singleton_asm
        );
    }

    #[test]
    fn parameterized_entrypoint_emits_witness_entry_wrapper() {
        let program = r#"
module vm::entry_abi

action spend(amount: u64) -> u64 {
    verification
        return amount
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains(".global _cellscript_entry"), "parameterized entrypoints need a generated ELF entry wrapper:\n{}", asm);
        assert!(
            asm.contains(
                "# cellscript entry abi: _cellscript_entry loads GroupInput#0 witness args for spend and falls back to GroupOutput#0"
            ),
            "entry wrapper did not document its target ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_WITNESS reason=entry_args source=GroupInput index=0")
                && asm.contains("# cellscript abi: LOAD_WITNESS reason=entry_args_fallback_group_output source=GroupOutput index=0")
                && !asm.contains("LOAD_WITNESS reason=entry_args source=Input index=0"),
            "entry wrapper did not use script-group-relative witness sourcing:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript entry placement profile: validate the exact three-field WitnessArgs table")
                && asm.contains("# cellscript entry placement v2: copy input_type payload over the table envelope")
                && !asm.contains("detect raw-v1"),
            "entry wrapper did not expose the versioned WitnessArgs.input_type placement ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript entry abi: scalar param amount -> a0 size=8"),
            "entry wrapper did not lower u64 witness payload into the action ABI register:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript entry abi: reject trailing witness payload bytes"),
            "entry wrapper should reject non-canonical trailing witness bytes:\n{}",
            asm
        );
        assert!(asm.contains("call spend"), "entry wrapper did not call the original action label:\n{}", asm);
        assert!(
            asm.contains("# cellscript entry abi: spend requires-explicit-parameter-abi"),
            "direct action entry still needs an explicit ABI marker for non-wrapper callers:\n{}",
            asm
        );

        let elf = compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert!(elf.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn entry_witness_encoder_matches_u64_wrapper_abi() {
        let program = r#"
module vm::entry_abi

action spend(amount: u64) -> u64 {
    verification
        return amount
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "spend").unwrap();
        let witness = action.entry_witness_args(&[EntryWitnessArg::U64(77)]).unwrap();

        let mut expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        expected.extend_from_slice(&77u64.to_le_bytes());
        assert_eq!(witness, expected);
        assert_eq!(encode_entry_witness_args_for_params(&action.params, &[EntryWitnessArg::U64(77)]).unwrap(), expected);
    }

    #[test]
    fn entry_witness_encoder_supports_fixed_byte_params() {
        let program = r#"
module vm::entry_abi

action owned(owner: Address) -> u64 {
    verification
        return 0
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "owned").unwrap();
        let owner = [9u8; 32];
        let witness = action.entry_witness_args(&[EntryWitnessArg::Address(owner)]).unwrap();

        let mut expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        expected.extend_from_slice(&owner);
        assert_eq!(witness, expected);

        let err = action.entry_witness_args(&[EntryWitnessArg::U64(9)]).unwrap_err();
        assert!(err.message.contains("expects 32 fixed bytes for type 'Address'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn entry_witness_wrapper_supports_scalar_stack_args() {
        let program = r#"
module vm::entry_abi

resource A has store, destroy {
    value: u64,
}

resource B has store, destroy {
    value: u64,
}

resource C has store, destroy {
    value: u64,
}

action stack_arg(a: A, b: B, c: C, owner: Address, required: u64) -> u64 {
    verification
        destroy a
        destroy b
        destroy c
        return required
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(
            asm.contains("# cellscript entry abi: scalar param required -> stack+0 size=8"),
            "entry wrapper did not map the ninth ABI argument to the caller stack:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript entry abi: stage stack arg8 at pre-call sp-16")
                && asm.contains("# cellscript entry abi: reserve 16 bytes for outgoing stack call arguments"),
            "entry wrapper did not stage the stack scalar argument below the witness frame before calling the action:\n{}",
            asm
        );
        assert!(asm.contains("sd t3, -16(sp)"), "entry wrapper did not emit the staged stack argument store:\n{}", asm);

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_arg").unwrap();
        let owner = [7u8; 32];
        let witness = action.entry_witness_args(&[EntryWitnessArg::Address(owner), EntryWitnessArg::U64(2)]).unwrap();

        let mut expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        expected.extend_from_slice(&owner);
        expected.extend_from_slice(&2u64.to_le_bytes());
        assert_eq!(witness, expected);
    }

    #[test]
    fn entry_witness_encoder_includes_schema_backed_params_as_length_prefixed_bytes() {
        let program = r#"
module vm::entry_abi

action bad(items: Vec<Address>) -> u64 {
    verification
        return 0

}
action hashes(items: Vec<Hash>) -> u64 {
    verification
        return 0

}
action nested(items: Vec<Vec<u8>>) -> u64 {
    verification
        return 0

}
action raw(data: Vec<u8>) -> u64 {
    verification
        return 0
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(
            asm.contains("# cellscript entry abi: reject trailing witness payload bytes") && asm.contains("sub t2, t5, t6"),
            "dynamic entry wrapper should reject non-canonical trailing witness bytes after consuming dynamic payload:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "bad").unwrap();
        let schema_bytes = vec![1u8, 2, 3, 4];
        let mut expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        expected.extend_from_slice(&(schema_bytes.len() as u32).to_le_bytes());
        expected.extend_from_slice(&schema_bytes);
        assert_eq!(action.entry_witness_args(&[EntryWitnessArg::Bytes(schema_bytes.clone())]).unwrap(), expected);

        let err = action.entry_witness_args(&[]).unwrap_err();
        assert!(err.message.contains("missing payload arg"), "unexpected error: {}", err.message);

        let hashes = result.metadata.actions.iter().find(|action| action.name == "hashes").unwrap();
        let hash_bytes = vec![0x11; 64];
        let mut hash_expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        hash_expected.extend_from_slice(&(hash_bytes.len() as u32).to_le_bytes());
        hash_expected.extend_from_slice(&hash_bytes);
        assert_eq!(hashes.entry_witness_args(&[EntryWitnessArg::Bytes(hash_bytes)]).unwrap(), hash_expected);

        let nested = result.metadata.actions.iter().find(|action| action.name == "nested").unwrap();
        let nested_bytes = vec![0x03, 0x00, 0x00, 0x00, 0xaa, 0xbb, 0xcc];
        let mut nested_expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        nested_expected.extend_from_slice(&(nested_bytes.len() as u32).to_le_bytes());
        nested_expected.extend_from_slice(&nested_bytes);
        assert_eq!(nested.entry_witness_args(&[EntryWitnessArg::Bytes(nested_bytes)]).unwrap(), nested_expected);

        let raw = result.metadata.actions.iter().find(|action| action.name == "raw").unwrap();
        let raw_bytes = vec![0xaa, 0xbb, 0xcc];
        let mut raw_expected = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        raw_expected.extend_from_slice(&(raw_bytes.len() as u32).to_le_bytes());
        raw_expected.extend_from_slice(&raw_bytes);
        assert_eq!(raw.entry_witness_args(&[EntryWitnessArg::Bytes(raw_bytes)]).unwrap(), raw_expected);
    }

    #[test]
    fn typed_transaction_views_lower_to_existing_ckb_runtime_helpers() {
        let program = r#"
module ckb::typed_views

resource Token has store {
    amount: u64
}

action inspect() -> u64 {
    verification
        let input = ckb::input<Token>(0)
        let output = ckb::group_output<Token>(0)
        let dep = ckb::cell_dep(0)
        let header = ckb::header_dep(0)
        let witness_args = witness::args(0)
        let out_point = ckb::input_out_point(input)
        let lock = ckb::lock_script(input)
        require lock.args_empty || lock.hash_type <= 4
        require dep.data_hash == dep.data_hash
        return input.capacity + input.occupied_capacity + input.unoccupied_capacity + ckb::since_to_raw(input.since) + output.output_index + dep.data_size + witness_args.size + out_point.index + ckb::epoch_number_to_u64(header.epoch_number) + ckb::block_number_to_u64(header.epoch_start_block_number) + ckb::epoch_length_to_u64(header.epoch_length) + ckb::block_number_to_u64(header.block_number) + ckb::timestamp_millis_to_u64(header.timestamp)
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.runtime.ckb_runtime_view_contract, crate::CKB_RUNTIME_VIEW_CONTRACT);
        assert_eq!(result.metadata.runtime.ckb_runtime_access_provenance_contract, crate::CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT);
        assert!(result.metadata.runtime.transaction_view_handles.iter().any(|handle| {
            handle.handle_type == "InputView<Token>"
                && handle.source == "Input"
                && handle.ownership == "read-only-view"
                && !handle.lifecycle_authority
                && handle.typing_evidence_tier == crate::EvidenceTier::CheckedStatic
                && handle.read_evidence_tier == crate::EvidenceTier::CheckedRuntime
        }));
        assert!(result
            .metadata
            .runtime
            .transaction_view_handles
            .iter()
            .any(|handle| handle.handle_type == "OutPoint" && handle.source == "Input"));
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        for helper in [
            "__ckb_cell_capacity",
            "__ckb_cell_occupied_capacity",
            "__ckb_cell_unoccupied_capacity",
            "__ckb_cell_output_index",
            "__ckb_cell_data_size",
            "__ckb_cell_data_hash_field",
            "__ckb_witness_size",
            "__ckb_input_out_point_index",
            "__ckb_input_since_at",
            "__ckb_header_dep_epoch_number",
            "__ckb_header_dep_epoch_start_block_number",
            "__ckb_header_dep_epoch_length",
            "__ckb_header_dep_block_number",
            "__ckb_header_dep_timestamp_millis",
            "__ckb_cell_lock_args_empty",
            "__ckb_cell_lock_hash_type",
        ] {
            assert!(asm.contains(helper), "typed transaction view did not retain runtime helper {helper}:\n{asm}");
        }
        assert_eq!(
            asm.matches("exact size check HeaderDep scalar return expected=8\n").count(),
            3,
            "each independent HeaderDep field helper must check its own syscall return length:\n{asm}"
        );
        assert!(!asm.contains("exact size check HeaderDep scalar return expected=8 elided by proven size"), "{asm}");

        let mut tampered = result.metadata.clone();
        tampered.runtime.ckb_runtime_view_contract = "cellscript-ckb-runtime-view-v0".to_string();
        let error = crate::validate_compile_metadata(&tampered, result.artifact_format)
            .expect_err("runtime-view contract downgrade must fail");
        assert!(error.message.contains("ckb_runtime_view_contract"), "{error}");

        let mut tampered = result.metadata.clone();
        tampered.runtime.ckb_runtime_access_provenance_contract = "cellscript-ckb-runtime-access-provenance-v0".to_string();
        let error = crate::validate_compile_metadata(&tampered, result.artifact_format)
            .expect_err("runtime-access provenance contract downgrade must fail");
        assert!(error.message.contains("ckb_runtime_access_provenance_contract"), "{error}");
    }

    #[test]
    fn tx_validation_binds_bounded_output_plan_to_witness_order_and_concrete_outputs() {
        let source = r#"
module bounded::tx_validation

struct Plan { owner: Address, amount: u64 }
resource Token has store, create
with_capacity_floor(10000000000)
{ amount: u64 }

action mint(witness plans: BoundedList<Plan, 2>) -> u64 {
    verification
        create_each plan in plans {
            require plan.amount > 0
            create Token { amount: plan.amount } with_lock(plan.owner)
        }
        return 0
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "mint").unwrap();
        let mut element = vec![1_u8; 40];
        element[32..].copy_from_slice(&7_u64.to_le_bytes());
        let plan = encode_bounded_output_plan_v1(&[element], 40, 2).unwrap();
        let witness = action.entry_witness_args(&[EntryWitnessArg::Bytes(plan.clone())]).unwrap();
        let current_script_hash = format!("0x{}", "09".repeat(32));
        let mut tx = serde_json::json!({
            "current_script_hash": current_script_hash.clone(),
            "witnesses": [{ "input_type": format!("0x{}", crate::hex_encode(&witness)) }],
            "outputs": [{
                "data": format!("0x{}", crate::hex_encode(&7_u64.to_le_bytes())),
                "lock_hash": format!("0x{}", "01".repeat(32)),
                "type_hash": current_script_hash.clone(),
                "capacity_shannons": 10_000_000_000_u64
            }],
            "bounded_output_plan_evidence": [{
                "schema": "cellscript-bounded-output-plan-evidence-v1",
                "version": 1,
                "action": "mint",
                "binding": "plans",
                "witness_index": 0,
                "witness_field": "input_type",
                "plan_payload": format!("0x{}", crate::hex_encode(&plan)),
                "current_script_hash": current_script_hash,
                "group_output_indexes": [0]
            }]
        });
        let report = crate::assumptions::validate_transaction_against_metadata(&result.metadata, &tx);
        assert_eq!(report.status, "ok", "{:?}", report.violations);
        assert_eq!(report.checked_assumptions, ["bounded-output-plan:mint:plans"]);

        for (pointer, replacement, expected) in [
            ("/outputs/0/data", serde_json::json!("0x0800000000000000"), "data does not exactly materialize"),
            ("/outputs/0/lock_hash", serde_json::json!(format!("0x{}", "02".repeat(32))), "lock_hash does not equal"),
            ("/outputs/0/type_hash", serde_json::json!(format!("0x{}", "08".repeat(32))), "do not exactly enumerate"),
            ("/outputs/0/capacity_shannons", serde_json::json!(9_999_999_999_u64), "is below floor"),
            ("/bounded_output_plan_evidence/0/group_output_indexes", serde_json::json!([0, 0]), "unique, strictly"),
            ("/bounded_output_plan_evidence/0/plan_payload", serde_json::json!("0x435342504c76310000000000"), "does not match"),
        ] {
            let previous = tx.pointer(pointer).cloned().unwrap();
            *tx.pointer_mut(pointer).unwrap() = replacement;
            let report = crate::assumptions::validate_transaction_against_metadata(&result.metadata, &tx);
            assert_eq!(report.status, "failed");
            assert!(report.violations.iter().any(|violation| violation.message.contains(expected)), "{:?}", report.violations);
            *tx.pointer_mut(pointer).unwrap() = previous;
        }
    }
