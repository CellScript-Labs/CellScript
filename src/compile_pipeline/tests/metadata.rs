// Compiler integration metadata behavior; retains crate::tests identities.

    #[test]
    fn compile_lowers_assert_invariant_into_fail_closed_cfg() {
        let result = compile(
            ASSERT_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("beqz t0"), "assert condition did not branch on failure:\n{}", asm);
        assert!(asm.contains("li a0, 5"), "assert failure path did not return assertion-failed code:\n{}", asm);
        assert!(!asm.contains("assert_invariant"), "assert was not lowered out of source form:\n{}", asm);
    }

    #[test]
    fn compile_rejects_dynamic_assert_invariant_messages() {
        let err = compile(ASSERT_DYNAMIC_MESSAGE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown function 'assert'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_binding_assert_invariant_results() {
        let err = compile(ASSERT_BINDING_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("unknown function 'assert'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_assert_invariant_as_tail_return_value() {
        let err = compile(ASSERT_TAIL_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("unknown function 'assert'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_cell_metadata_stdlib_on_non_cell_args() {
        let source = r#"
module test

resource Coin has store {
    amount: u64
}

action bad(amount: u64) -> out: Coin {
    verification
        std::cell::preserve_capacity(out, amount)
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("std::cell::preserve_capacity input must be a cell-backed value"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_lowers_bounded_sha256_and_merkle_primitives_to_elf() {
        let source = r#"
module bounded_sha256;

action verify(
    witness input: Hash,
    witness other: Hash,
    witness siblings: [Hash; 16],
    witness expected_sha256: Hash,
    witness expected_sha256d: Hash,
    witness expected_pair: Hash,
    witness expected_root: Hash,
) -> bool {
    verification
    require ckb::hash_sha256(input) == expected_sha256
    require ckb::hash_sha256d(input) == expected_sha256d
    require ckb::hash_sha256d_pair(input, other) == expected_pair
    ckb::require_sha256d_merkle_root(input, siblings, 0, 0, expected_root)
    true
}
"#;
        let result = compile(
            source,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "verify").expect("verify metadata");

        assert!(result.artifact_bytes.starts_with(b"\x7fELF"), "SHA-256 helpers must assemble into a RISC-V ELF");
        for feature in ["bounded-sha256", "bounded-sha256d", "bounded-sha256d-merkle-proof"] {
            assert!(
                action.ckb_runtime_features.iter().any(|candidate| candidate == feature),
                "missing {feature} in runtime features: {:?}",
                action.ckb_runtime_features
            );
        }
    }

    #[test]
    fn compile_metadata_declares_molecule_vm_abi() {
        let result =
            compile(SIMPLE_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.metadata.metadata_schema_version, crate::METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.source_metadata_schema_version, crate::SOURCE_METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.artifact_metadata_schema_version, crate::ARTIFACT_METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.constraints_metadata_schema_version, crate::CONSTRAINTS_METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.compiler_version, crate::VERSION);
        assert_eq!(result.metadata.runtime.vm_abi.format, "molecule");
        assert_eq!(result.metadata.runtime.vm_abi.version, 0x8001);
        assert!(result.metadata.runtime.vm_abi.default);
        // CKB profile does not embed VM ABI trailer
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
        assert!(result.metadata.runtime.vm_abi.scope.contains("LOAD_SCRIPT"));
        assert!(result.metadata.runtime.vm_abi.selection.contains("embed"));
        assert_eq!(result.metadata.target_profile.name.as_str(), "ckb");
        assert_eq!(result.metadata.target_profile.target_chain.as_str(), "ckb");
        assert_eq!(result.metadata.target_profile.vm_abi.as_str(), "ckb-molecule");
        assert_eq!(result.metadata.target_profile.hash_domain.as_str(), "ckb-packed-molecule-blake2b");
        assert_eq!(result.metadata.target_profile.syscall_set.as_str(), "ckb-mainnet-syscalls");
        assert_eq!(result.metadata.target_profile.artifact_packaging.as_str(), "ckb-elf");
        assert_eq!(result.metadata.target_profile.header_abi.as_str(), "ckb-header");
        assert_eq!(result.metadata.target_profile.scheduler_abi.as_str(), "none");
        assert_eq!(result.metadata.target_profile.minimum_vm_version, 2);
        assert_eq!(result.metadata.target_profile.riscv_isa, "rv64imac_zbb");
        assert_eq!(result.metadata.target_profile.deployment_hash_types, ["data2"]);
        assert_eq!(result.metadata.artifact_hash.as_deref(), Some(crate::hex_encode(&result.artifact_hash).as_str()));
        assert_eq!(result.metadata.artifact_size_bytes, Some(result.artifact_bytes.len()));
        assert!(result.metadata.source_hash.is_some());
        assert!(result.metadata.source_content_hash.is_some());
        assert_eq!(result.metadata.source_units.len(), 1);
        assert_eq!(result.metadata.source_units[0].path, "<memory>");
        assert_eq!(result.metadata.source_units[0].role, "memory");
        assert_eq!(result.metadata.cell_data_codec_manifest.schema, "cellscript-cell-data-codec-manifest-v1");
        assert_eq!(result.metadata.cell_data_codec_manifest.abi, "molecule");
        assert_eq!(result.metadata.cell_data_codec_manifest.default_codec, "molecule");
        assert!(!result.metadata.cell_data_codec_manifest.raw_bytes_required);
        assert_eq!(result.metadata.cell_data_codec_manifest.raw_runtime_access_count, 0);
        assert!(result.metadata.cell_data_codec_manifest.raw_runtime_accesses.is_empty());
        assert_eq!(
            result.metadata.cell_data_codec_manifest.molecule_schema_manifest_hash,
            result.metadata.molecule_schema_manifest.manifest_hash
        );
        assert_eq!(
            result.metadata.cell_data_codec_manifest.manifest_hash,
            crate::cell_data_codec_manifest_hash(&result.metadata.cell_data_codec_manifest)
        );
    }

    #[test]
    fn compile_metadata_declares_raw_bytes_codec_for_cell_data_primitives() {
        let program = r#"
module test::raw_codec

action inspect() -> u64 {
    verification
        let input = source::group_input(0)
        let quantity = ckb::cell_data_u32_le(input, 0)
        let amount = ckb::cell_data_u64_le(input, 4)
        if quantity != 7 {
            return 90
        }
        if amount != 123456789 {
            return 91
        }
        return 0
}
"#;
        let result =
            compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        let manifest = &result.metadata.cell_data_codec_manifest;

        assert_eq!(manifest.schema, "cellscript-cell-data-codec-manifest-v1");
        assert_eq!(manifest.abi, "molecule+raw-bytes-v1");
        assert_eq!(manifest.default_codec, "molecule");
        assert!(manifest.raw_bytes_required);
        assert_eq!(manifest.raw_runtime_access_count, manifest.raw_runtime_accesses.len());
        assert!(manifest.raw_runtime_accesses.iter().any(|access| {
            access.operation == "cell-data-u32-le" && access.binding == "ckb::cell_data_u32_le" && access.codec == "raw-bytes-v1"
        }));
        assert!(manifest.raw_runtime_accesses.iter().any(|access| {
            access.operation == "cell-data-u64-le" && access.binding == "ckb::cell_data_u64_le" && access.codec == "raw-bytes-v1"
        }));
        assert_eq!(manifest.molecule_schema_manifest_hash, result.metadata.molecule_schema_manifest.manifest_hash);
        assert_eq!(manifest.manifest_hash, crate::cell_data_codec_manifest_hash(manifest));
        result.validate().unwrap();
    }

    #[test]
    fn compile_result_validation_rejects_tampered_compatibility_profile() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.compatibility_profile.entry_witness_placement_source = "global-input-0".to_string();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("compatibility_profile"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_tampered_profile_schema_axis() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.compatibility_profile.metadata_schema_version -= 1;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("compatibility_profile"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_accepts_pure_ckb_target_profile() {
        let result =
            compile(SIMPLE_PROGRAM, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.metadata.target_profile.name.as_str(), "ckb");
        assert_eq!(result.metadata.target_profile.artifact_packaging.as_str(), "ckb-asm-sidecar");
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
        result.validate().unwrap();
    }

    #[test]
    fn compile_accepts_distinct_ckb_type_hash_deployment_profile() {
        let result = compile(
            SIMPLE_PROGRAM,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb-type-hash".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.metadata.target_profile.name, "ckb-type-hash");
        assert_eq!(result.metadata.target_profile.target_chain, "ckb");
        assert_eq!(result.metadata.target_profile.deployment_hash_types, ["type"]);
        let constraints = result.metadata.constraints.ckb.as_ref().expect("CKB constraints");
        assert_eq!(constraints.profile_abi_contract.deployment_hash_types, ["type"]);
        assert_eq!(constraints.hash_type_policy.default_script_hash_type, "type");
        assert_eq!(constraints.hash_type_policy.status, "builder-must-use-type-for-vm2-zbb-artifact");
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
        result.validate().unwrap();
    }

    #[test]
    fn compile_accepts_ckb_target_profile_timepoint() {
        let result = compile(
            r#"
module test::timepoint

action now() -> u64 {
    verification
        return env::current_timepoint()
}
"#,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        )
        .unwrap();

        assert_eq!(result.metadata.target_profile.name, "ckb");
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains("call __env_current_timepoint"), "timepoint call was not lowered:\n{}", asm);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"load-header-timepoint".to_string()));
    }

    #[test]
    fn compile_accepts_chain_neutral_timepoint_under_ckb_profile() {
        let result = compile(
            r#"
module test::timepoint

action now() -> u64 {
    verification
        return env::current_timepoint()
}
"#,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("call __env_current_timepoint"), "timepoint call was not lowered:\n{}", asm);
        assert!(
            asm.contains("LOAD_HEADER_BY_FIELD field=ckb_epoch_number source=HeaderDep index=0"),
            "CKB timepoint should use the CKB epoch-number header field:\n{}",
            asm
        );
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"load-header-timepoint".to_string()));
    }

    #[test]
    fn compile_accepts_ckb_header_epoch_api_only_for_ckb_profile() {
        let result =
            compile(CKB_HEADER_EPOCH_PROGRAM, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
                .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("call __ckb_header_epoch_number"), "epoch number call was not lowered:\n{}", asm);
        assert!(
            asm.contains("LOAD_HEADER_BY_FIELD field=ckb_epoch_number source=HeaderDep index=0"),
            "epoch number helper did not document CKB header ABI:\n{}",
            asm
        );
        assert!(asm.contains("call __ckb_input_since"), "since call was not lowered:\n{}", asm);
        assert!(
            asm.contains("LOAD_INPUT_BY_FIELD field=ckb_input_since source=GroupInput index=0"),
            "since helper did not document CKB input ABI:\n{}",
            asm
        );
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-epoch-number".to_string()));
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-epoch-start-block-number".to_string()));
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-header-epoch-length".to_string()));
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-input-since".to_string()));
        let ckb_constraints = result.metadata.constraints.ckb.as_ref().expect("ckb constraints metadata");
        assert!(ckb_constraints.uses_input_since, "ckb constraints must surface input_since usage");
        assert!(ckb_constraints.uses_header_epoch, "ckb constraints must surface epoch-header usage");
        assert!(
            ckb_constraints.ckb_runtime_features.iter().any(|feature| feature == "ckb-input-since"),
            "ckb constraints must carry ckb runtime features"
        );
        assert_eq!(ckb_constraints.hash_domain, "ckb-packed-molecule-blake2b");
        assert_eq!(ckb_constraints.declared_type_id_hash_type, crate::CKB_TYPE_ID_HASH_TYPE);
        assert!(
            ckb_constraints.supported_script_hash_types.iter().any(|hash_type| hash_type == "data2"),
            "CKB constraints must expose supported script hash_type set"
        );
        assert!(ckb_constraints.tx_size_measurement_required, "CKB production constraints must require tx-size measurement");
        assert_eq!(ckb_constraints.timelock_policy_surface, "runtime-metadata-visible; declarative-dsl-policy-not-yet-first-class");
        assert_eq!(ckb_constraints.timelock_policy.policy_kind, "runtime-assertion-policy");
        assert!(ckb_constraints.timelock_policy.uses_input_since);
        assert!(ckb_constraints.timelock_policy.uses_header_epoch);
        assert!(ckb_constraints.timelock_policy.runtime_features.iter().any(|feature| feature == "ckb-input-since"));
        assert!(
            !ckb_constraints.capacity_planning_required,
            "pure epoch/since reads must not claim output-capacity planning is required"
        );
        assert_eq!(ckb_constraints.created_output_count, 0, "pure epoch/since reads must not report created outputs");
        assert_eq!(ckb_constraints.mutated_output_count, 0, "pure epoch/since reads must not report mutated outputs");
        assert_eq!(ckb_constraints.capacity_policy_surface, "not-applicable");
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
            access.syscall == "LOAD_INPUT_BY_FIELD" && access.source == "GroupInput" && access.operation == "input-since"
        }));
        result.validate().unwrap();
    }

    #[test]
    fn ckb_constraints_surface_capacity_planning_for_created_outputs() {
        let source = r#"
module ckb_capacity_surface

resource Receipt {
    amount: u64,
}

action mint(amount: u64) -> Receipt {
    verification
        let receipt = create Receipt {
            amount: amount,
        };
        receipt
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let ckb_constraints = result.metadata.constraints.ckb.as_ref().expect("ckb constraints metadata");

        assert_eq!(ckb_constraints.created_output_count, 1, "ckb constraints must count created outputs");
        assert_eq!(ckb_constraints.mutated_output_count, 0, "create-only action must not report mutated outputs");
        assert!(ckb_constraints.capacity_planning_required, "create outputs must mark capacity planning as required");
        assert!(
            ckb_constraints.occupied_capacity_measurement_required,
            "create outputs must require builder occupied-capacity measurement"
        );
        assert_eq!(ckb_constraints.capacity_status, "builder-occupied-capacity-measurement-required");
        assert_eq!(ckb_constraints.capacity_policy_surface, "builder/runtime-required; declarative-dsl-capacity-not-yet-first-class");
        assert!(ckb_constraints.capacity_evidence_contract.required);
        assert!(ckb_constraints.capacity_evidence_contract.occupied_capacity_measurement_required);
        assert!(ckb_constraints.capacity_evidence_contract.tx_size_measurement_required);
        assert!(ckb_constraints.capacity_evidence_contract.code_cell_lower_bound_shannons > 0);
    }

    #[test]
    fn ckb_type_hash_profile_requires_type_deployment_manifest() {
        let dir = tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "type_hash_deploy_manifest"
version = "1.0.0"
entry = "src/main.cell"

[build]
target_profile = "ckb-type-hash"

[deploy.ckb]
hash_type = "type"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/main.cell"),
            r#"
module type_hash_deploy_manifest

action add(a: u64, b: u64) -> u64 {
    verification
        a + b
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        let ckb = result.metadata.constraints.ckb.as_ref().expect("CKB constraints");
        assert_eq!(result.metadata.target_profile.name, "ckb-type-hash");
        assert_eq!(ckb.hash_type_policy.declared_hash_type.as_deref(), Some("type"));
        assert_eq!(ckb.hash_type_policy.default_script_hash_type, "type");
        result.validate().unwrap();

        let manifest_path = root.join("Cell.toml");
        let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace("hash_type = \"type\"", "hash_type = \"data2\"");
        std::fs::write(manifest_path, manifest).unwrap();
        let error = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("target profile 'ckb-type-hash' artifacts require deploy.ckb.hash_type = 'type'"));
    }

    #[test]
    fn compile_surfaces_type_level_hash_type_dsl_metadata() {
        let result = compile(
            r#"
module hash_type_dsl

resource Token has store, replace, relock
with_default_hash_type(Data1)
{
    amount: u64,
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        )
        .unwrap();

        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token metadata");
        assert_eq!(token.default_hash_type.as_deref(), Some("data1"));
        assert_eq!(token.hash_type_source, "dsl-with_default_hash_type");
    }

    #[test]
    fn ckb_target_profile_has_no_policy_exception() {
        let result = compile(
            r#"
module test

action main() -> u64 {
    verification
        return env::current_timepoint()
}
"#,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        )
        .unwrap();

        assert_eq!(result.metadata.target_profile.name, "ckb");
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"load-header-timepoint".to_string()));
    }

    #[test]
    fn compile_rejects_unknown_target_profile() {
        let err = compile(SIMPLE_PROGRAM, CompileOptions { target_profile: Some("unknown".to_string()), ..CompileOptions::default() })
            .unwrap_err();

        assert!(err.message.contains("unsupported target profile 'unknown'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_artifact_hash_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.artifact_hash = Some("00".repeat(32));

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata artifact_hash"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_artifact_size_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.artifact_size_bytes = Some(result.artifact_bytes.len() + 1);

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata artifact_size_bytes"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_missing_metadata_artifact_size() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.artifact_size_bytes = None;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata is missing artifact_size_bytes"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_artifact_format_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.artifact_format = ArtifactFormat::RiscvElf.display_name().to_string();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata artifact_format"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_target_profile_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.target_profile.artifact_packaging = "ckb-elf-no-vm_abi-trailer".to_string();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata target_profile.artifact_packaging"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_target_profile_v0_14_abi_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.target_profile.capacity_floor_abi = "missing-capacity-floor-contract".to_string();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata target_profile.capacity_floor_abi"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_source_content_hash_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.source_content_hash = Some("00".repeat(32));

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata source_content_hash"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_schema_version_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.metadata_schema_version = crate::METADATA_SCHEMA_VERSION + 1;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("unsupported metadata_schema_version"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_schema_downgrade() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.metadata_schema_version = crate::METADATA_SCHEMA_VERSION - 1;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("unsupported metadata_schema_version"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_metadata_component_schema_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.source_metadata_schema_version = crate::SOURCE_METADATA_SCHEMA_VERSION + 1;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("unsupported source_metadata_schema_version"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_asymmetric_where_branch_output_constraints() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    price: u64
    note: u64
}

flow Offer.state {
    Live -> Filled;
}

action fill(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        if input.price > 0 {
            require output.price == input.price
        } else {
            require output.note == input.note
        }
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("incomplete branch constraints"), "unexpected error: {}", err.message);
        assert!(
            err.message.contains("field 'note' of output 'output'"),
            "unexpected field or output binding: {}",
            err.message
        );
    }

    #[test]
    fn compile_accepts_symmetric_where_branch_output_constraints() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    price: u64
    note: u64
}

flow Offer.state {
    Live -> Filled;
}

action fill(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        if input.price > 0 {
            require output.price == input.price
        } else {
            require output.price == input.price + 1
        }

        require output.note == input.note
}
"#;
        compile(source, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_metadata_exposes_covenant_proof_plan_for_transfer() {
        let source = r#"
module test

resource Token has store, replace, relock, consume {
    amount: u64,
}

action transfer_token(token: Token, to: Address) -> next_token: Token {
    verification
        std::lifecycle::transfer(token, next_token, to) {
            amount
        }
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        // std::lifecycle::transfer decomposes into consume + create, not a combined transfer record.
        let consume_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature.starts_with("consume-input:Token"))
            .expect("consume-input ProofPlan record");
        let create_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature.starts_with("create-output:Token"))
            .expect("create-output ProofPlan record");

        assert_eq!(consume_plan.trigger, "explicit_entry");
        assert_eq!(consume_plan.scope, "transaction");
        assert_eq!(create_plan.trigger, "explicit_entry");
        assert_eq!(create_plan.scope, "transaction");
        assert!(consume_plan.reads.contains(&"input".to_string()), "{:?}", consume_plan.reads);
        assert!(create_plan.reads.contains(&"output".to_string()), "{:?}", create_plan.reads);
        assert!(
            consume_plan.coverage.iter().any(|coverage| coverage.contains("transaction-scoped relation")),
            "{:?}",
            consume_plan.coverage
        );
        // Verify the action-level proof plan has both consume and create records.
        assert_eq!(
            result
                .metadata
                .actions
                .iter()
                .find(|action| action.name == "transfer_token")
                .expect("action metadata")
                .proof_plan
                .iter()
                .filter(|plan| plan.feature.starts_with("consume-input:Token") || plan.feature.starts_with("create-output:Token"))
                .count(),
            2
        );
    }

    #[test]
    fn compile_metadata_exposes_lock_group_proof_plan_for_lock_entry() {
        let source = r#"
module test

resource Config {
    digest: Hash,
}

lock config_guard() -> bool {
    verification
        let config = read_ref<Config>()
        require config.digest == Hash::zero()
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let plan =
            result.metadata.runtime.proof_plan.iter().find(|plan| plan.origin == "lock:config_guard").expect("lock ProofPlan record");

        assert_eq!(plan.trigger, "lock_group");
        assert_eq!(plan.scope, "group");
        assert!(plan.reads.contains(&"cell_dep".to_string()), "{:?}", plan.reads);
        assert!(plan.coverage.iter().any(|coverage| coverage.contains("lock ScriptGroup coverage")), "{:?}", plan.coverage);
    }

    #[test]
    fn compile_metadata_proof_plan_preserves_lock_args_source() {
        let result =
            compile(LOCK_ARGS_PARAM_PROGRAM, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() })
                .unwrap();

        assert!(
            result.metadata.runtime.proof_plan.iter().any(|plan| plan.feature == "ckb-lock-args"),
            "{:?}",
            result.metadata.runtime.proof_plan
        );

        let runtime_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "lock-args:ScriptArgs#0")
            .expect("runtime ProofPlan lock_args cell-access record");
        assert_eq!(runtime_plan.category, "cell-access");
        assert!(runtime_plan.reads.contains(&"lock_args".to_string()), "{:?}", runtime_plan.reads);
        assert!(!runtime_plan.reads.contains(&"witness".to_string()), "{:?}", runtime_plan.reads);
        assert!(runtime_plan.lock_args_fields.contains(&"lock_args.owner".to_string()), "{:?}", runtime_plan.lock_args_fields);
        assert!(runtime_plan.lock_args_fields.contains(&"ScriptArgs#0:owner".to_string()), "{:?}", runtime_plan.lock_args_fields);
        assert!(runtime_plan.witness_fields.is_empty(), "{:?}", runtime_plan.witness_fields);

        let lock_plan = result
            .metadata
            .locks
            .iter()
            .find(|lock| lock.name == "bad")
            .expect("lock metadata")
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "lock-args:ScriptArgs#0")
            .expect("lock ProofPlan lock_args cell-access record");
        assert_eq!(runtime_plan.reads, lock_plan.reads);
        assert_eq!(runtime_plan.witness_fields, lock_plan.witness_fields);
        assert_eq!(runtime_plan.lock_args_fields, lock_plan.lock_args_fields);
    }

    #[test]
    fn compile_metadata_exposes_declared_invariant_proof_plan() {
        let source = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_invariant(true, "token amount is conserved")
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:token_conservation")
            .expect("declared invariant ProofPlan record");

        assert_eq!(plan.category, "declared-invariant");
        assert_eq!(plan.trigger, "type_group");
        assert_eq!(plan.scope, "group");
        assert_eq!(plan.reads, vec!["group_inputs<Token>.amount", "group_outputs<Token>.amount"]);
        assert!(!plan.on_chain_checked);
        assert_eq!(plan.codegen_coverage_status, "gap:metadata-only");
        assert!(plan.coverage.contains(&"declared_invariant_assertions:1".to_string()), "{:?}", plan.coverage);
        assert!(plan.source_span.is_some());
    }

    #[test]
    fn compile_metadata_exposes_aggregate_invariant_primitives_in_proof_plan() {
        let source = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_conserved(Token.amount, scope = group)
    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:token_conservation")
            .expect("declared invariant ProofPlan record");

        assert_eq!(plan.codegen_coverage_status, "gap:metadata-only");
        assert!(plan.reads.contains(&"group_input".to_string()), "{:?}", plan.reads);
        assert!(plan.reads.contains(&"group_output".to_string()), "{:?}", plan.reads);
        assert!(
            plan.input_output_relation_checks.iter().any(|check| check == "assert_conserved:Token.amount=metadata-only"),
            "{:?}",
            plan.input_output_relation_checks
        );
        assert!(
            plan.input_output_relation_checks
                .iter()
                .any(|check| check == "assert_sum:group_outputs<Token>.amount<=group_inputs<Token>.amount=metadata-only"),
            "{:?}",
            plan.input_output_relation_checks
        );
        assert!(
            plan.coverage.iter().any(|coverage| coverage.contains("aggregate_assertion:conserved(Token.amount) scope=group")),
            "{:?}",
            plan.coverage
        );
        assert!(plan.builder_assumptions.contains(&"declared(aggregate_invariant_count:2)".to_string()));

        let aggregate_plans = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .filter(|plan| plan.origin.starts_with("invariant:token_conservation#aggregate:"))
            .collect::<Vec<_>>();
        assert_eq!(aggregate_plans.len(), 2, "{:?}", aggregate_plans);
        assert!(aggregate_plans.iter().any(|plan| {
            plan.category == "aggregate-invariant"
                && plan.feature == "assert_conserved:Token.amount"
                && plan.input_output_relation_checks == vec!["assert_conserved:Token.amount=metadata-only"]
                && plan.source_span.is_some()
        }));
        assert!(aggregate_plans.iter().any(|plan| {
            plan.category == "aggregate-invariant"
                && plan.feature == "assert_sum:group_outputs<Token>.amount<=group_inputs<Token>.amount"
                && plan.reads.contains(&"group_input".to_string())
                && plan.reads.contains(&"group_output".to_string())
                && plan.source_span.is_some()
        }));
    }

    #[test]
    fn compile_metadata_marks_helper_backed_xudt_group_amount_invariants_checked() {
        let source = r#"
module test

invariant xudt_group_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

invariant xudt_group_mint_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_delta(group_outputs<Token>.amount, minted, scope = group)
}

resource Token {
    amount: u128,
}

action transfer(token: Token, to: Address) -> next_token: Token {
    verification
    xudt::require_group_amount_conserved()

    consume token

    create next_token = Token {
        amount: token.amount
    } with_lock(to)
}

action mint(existing: Token, minted: u128, to: Address) -> token: Token {
    verification
    xudt::require_group_amount_minted(minted)

    consume existing

    create token = Token {
        amount: existing.amount + minted
    } with_lock(to)
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();

        assert!(
            result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.binding == "xudt::require_group_amount_conserved"),
            "{:?}",
            result.metadata.runtime.ckb_runtime_accesses
        );
        assert!(
            result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.binding == "xudt::require_group_amount_minted"),
            "{:?}",
            result.metadata.runtime.ckb_runtime_accesses
        );

        let conservation_summary = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:xudt_group_transfer_conservation")
            .expect("conservation invariant summary ProofPlan record");
        assert_eq!(conservation_summary.status, "checked-runtime");
        assert!(conservation_summary.on_chain_checked);
        assert_eq!(conservation_summary.codegen_coverage_status, "covered");
        assert!(
            conservation_summary
                .coverage
                .iter()
                .any(|coverage| coverage == "runtime_helper_checked:xudt::require_group_amount_conserved"),
            "{:?}",
            conservation_summary.coverage
        );

        let conservation = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:xudt_group_transfer_conservation#aggregate:0")
            .expect("conservation aggregate ProofPlan record");
        assert_eq!(conservation.status, "checked-runtime");
        assert!(conservation.on_chain_checked);
        assert_eq!(conservation.codegen_coverage_status, "covered");
        assert_eq!(
            conservation.input_output_relation_checks,
            vec!["assert_sum:group_outputs<Token>.amount==group_inputs<Token>.amount=checked-runtime:xudt::require_group_amount_conserved"]
        );

        let mint_delta = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:xudt_group_mint_delta#aggregate:0")
            .expect("mint delta aggregate ProofPlan record");
        assert_eq!(mint_delta.status, "checked-runtime");
        assert!(mint_delta.on_chain_checked);
        assert_eq!(mint_delta.codegen_coverage_status, "covered");
        assert_eq!(
            mint_delta.input_output_relation_checks,
            vec!["assert_delta:group_outputs<Token>.amount:minted=checked-runtime:xudt::require_group_amount_minted"]
        );
    }

    #[test]
    fn compile_auto_lowers_xudt_group_amount_conservation_invariant() {
        let source = r#"
module test

invariant xudt_group_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

resource Token {
    amount: u128,
}

action transfer(token: Token, to: Address) -> next_token: Token {
    verification
    consume token

    create next_token = Token {
        amount: token.amount
    } with_lock(to)
}

action mint(minted: u128, to: Address) -> token: Token {
    verification
    create token = Token {
        amount: minted
    } with_lock(to)
}
"#;

        let result = compile(
            source,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                primitive_compat: Some("0.17".to_string()),
                target: Some("riscv64-asm".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action_section = |label: &str| {
            let marker = format!("\n{label}:\n");
            let start = asm.find(&marker).unwrap_or_else(|| panic!("missing action label {label}:\n{asm}"));
            let rest = &asm[start..];
            let end = rest[1..].find("\n.global ").map(|offset| offset + 1).unwrap_or(rest.len());
            &rest[..end]
        };
        let transfer_asm = action_section("transfer");
        let mint_asm = action_section("mint");
        assert!(
            transfer_asm.contains("# cellscript aggregate invariant: auto-lowered xUDT group amount conservation"),
            "auto-lowered aggregate check missing from transfer:\n{}",
            transfer_asm
        );
        assert!(
            transfer_asm.contains("call __xudt_require_group_amount_conserved"),
            "auto-lowered helper call missing from transfer:\n{}",
            transfer_asm
        );
        assert!(
            !mint_asm.contains("call __xudt_require_group_amount_conserved"),
            "mint action must not be over-constrained by conservation lowering:\n{}",
            mint_asm
        );

        let aggregate = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:xudt_group_transfer_conservation#aggregate:0")
            .expect("xUDT conservation aggregate ProofPlan record");
        assert_eq!(aggregate.status, "checked-runtime");
        assert_eq!(aggregate.codegen_coverage_status, "covered");
    }

    #[test]
    fn compile_primitive_strict_017_rejects_xudt_conservation_without_matching_action() {
        let source = r#"
module test

invariant xudt_group_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

resource Token {
    amount: u128,
}

action mint(minted: u128, to: Address) -> token: Token {
    verification
    create token = Token {
        amount: minted
    } with_lock(to)
}
"#;

        let err = compile(
            source,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                primitive_compat: Some("0.17".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(err.message.contains("PP0170 invariant:xudt_group_transfer_conservation"), "unexpected error: {}", err.message);
        assert!(
            err.message.contains("runtime-helper-required:xudt::require_group_amount_conserved"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_primitive_strict_017_rejects_xudt_group_delta_without_helper() {
        let source = r#"
module test

invariant xudt_group_mint_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_delta(group_outputs<Token>.amount, minted, scope = group)
}

resource Token {
    amount: u128,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let err = compile(
            source,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                primitive_compat: Some("0.17".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(err.message.contains("PP0170 invariant:xudt_group_mint_delta"), "unexpected error: {}", err.message);
        assert!(
            err.message.contains("runtime-helper-required:xudt::require_group_amount_minted"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn primitive_strict_017_rejects_stale_xudt_runtime_helper_gap_metadata() {
        let source = r#"
module test

invariant xudt_group_mint_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_delta(group_outputs<Token>.amount, minted, scope = group)
}

resource Token {
    amount: u128,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let mut result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        result.metadata.runtime.ckb_runtime_accesses.push(crate::runtime_access_metadata(
            "xudt-group-amount-minted-delta",
            "LOAD_CELL_DATA",
            "GroupInput/GroupOutput",
            "xudt::require_group_amount_minted",
            "GroupInput/GroupOutput",
            "metadata-summary",
            None,
            crate::runtime_scalar_metadata_ordinal(0),
            crate::runtime_range_whole_value(),
        ));

        let err = validate_primitive_strict_017_metadata(&result.metadata).unwrap_err();

        assert!(err.message.contains("PP0170 invariant:xudt_group_mint_delta"), "unexpected error: {}", err.message);
        assert!(
            err.message.contains("runtime-helper-required:xudt::require_group_amount_minted"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_metadata_exposes_transaction_and_selected_cell_aggregate_invariants() {
        let source = r#"
module test

invariant nft_uniqueness {
    trigger: explicit_entry
    scope: transaction
    reads: outputs<Nft>.id
    assert_distinct(outputs<Nft>.id, scope = transaction)
}

invariant selected_token_delta {
    trigger: explicit_entry
    scope: selected_cells
    reads: input<Token>.amount, output<Token>.amount
    assert_delta(Token.amount, expected_delta, scope = selected_cells)
}

resource Nft {
    id: Hash,
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let distinct = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "assert_distinct:outputs<Nft>.id")
            .expect("transaction distinct aggregate ProofPlan");
        assert_eq!(distinct.category, "aggregate-invariant");
        assert_eq!(distinct.scope, "transaction");
        assert!(distinct.reads.contains(&"output".to_string()), "{:?}", distinct.reads);

        let delta = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "assert_delta:Token.amount:expected_delta")
            .expect("selected cell delta aggregate ProofPlan");
        assert_eq!(delta.category, "aggregate-invariant");
        assert_eq!(delta.scope, "selected_cells");
        assert_eq!(delta.input_output_relation_checks, vec!["assert_delta:Token.amount:expected_delta=metadata-only"]);
        assert_eq!(delta.codegen_coverage_status, "gap:metadata-only");
    }

    #[test]
    fn compile_metadata_warns_for_lock_group_transaction_invariant_scope() {
        let source = r#"
module test

invariant lock_scans_transaction {
    trigger: lock_group
    scope: transaction
    reads: inputs<Token>.amount, outputs<Token>.amount
    assert_sum(outputs<Token>.amount) <= assert_sum(inputs<Token>.amount)
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let declared = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:lock_scans_transaction")
            .expect("declared invariant ProofPlan record");

        assert_eq!(declared.trigger, "lock_group");
        assert_eq!(declared.scope, "transaction");
        assert!(
            declared.coverage.iter().any(|coverage| coverage.contains("only inputs sharing this lock script")),
            "{:?}",
            declared.coverage
        );
        assert!(
            declared.builder_assumptions.iter().any(|assumption| assumption.contains("only protects the lock group")),
            "{:?}",
            declared.builder_assumptions
        );
        assert!(
            declared
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "warning"
                    && diagnostic.message.contains("do not imply type-group conservation")),
            "{:?}",
            declared.diagnostics
        );

        let aggregate = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "invariant:lock_scans_transaction#aggregate:0")
            .expect("aggregate invariant ProofPlan record");
        assert_eq!(aggregate.category, "aggregate-invariant");
        assert!(
            aggregate
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "warning"
                    && diagnostic.message.contains("do not imply type-group conservation")),
            "{:?}",
            aggregate.diagnostics
        );
    }

    #[test]
    fn compile_rejects_aggregate_invariant_non_fixed_field() {
        let source = r#"
module test

invariant flag_distinct {
    trigger: type_group
    scope: group
    reads: group_inputs<Flag>.enabled, group_outputs<Flag>.enabled
    assert_distinct(group_outputs<Flag>.enabled, scope = group)
}

resource Flag {
    enabled: bool,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let err = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap_err();

        assert!(err.message.contains("must be a fixed-width integer or fixed bytes"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_invariant_without_explicit_trigger_and_scope() {
        let source = r#"
module test

invariant token_conservation {
    reads: group_inputs<Token>.amount
    assert_invariant(true, "token amount is conserved")
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let err = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap_err();

        assert!(err.message.contains("must declare explicit trigger and scope"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_invalid_invariant_assert_expression() {
        let source = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_invariant(1, "token amount is conserved")
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let err = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap_err();

        assert!(err.message.contains("assert condition must be boolean"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_invariant_assert_runtime_operation() {
        let source = r#"
module test

invariant config_digest {
    trigger: explicit_entry
    scope: selected_cells
    reads: cell_dep
    assert_invariant(read_ref<Config>().digest == Hash::zero(), "config digest must be zero")
}

resource Config {
    digest: Hash,
}

action run() -> u64 {
    verification
        return 0
}
"#;

        let err = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap_err();

        assert!(err.message.contains("pure function cannot contain 'read_ref'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_emits_replace_unique_capability_and_identity_proof() {
        let source = r#"
module test

resource Token has store, replace
    identity(field(id))
{
    id: u64,
    amount: u64,
}

action rotate(old: Token, amount: u64) -> Token {
    verification
        replace_unique<Token>(identity = field(id)) old { id: old.id, amount: amount }
}
"#;

        let result = compile(source, CompileOptions::default()).unwrap();
        let proof = result.metadata.runtime.capability_proofs.first().expect("replace_unique capability proof");
        assert_eq!(proof.required, ["replace", "identity-preservation"]);
        assert_eq!(proof.provided, ["store", "replace"]);
        assert_eq!(proof.entailed, ["replace", "identity-preservation"]);
        assert!(proof.missing.is_empty());
        assert_eq!(proof.identity_condition.as_deref(), Some("identity(field(id))"));
    }

    #[test]
    fn compile_result_exposes_scheduler_metadata_sidecar() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.module, "test");
        assert_eq!(result.metadata.runtime.vm_version, "VERSION2");
        assert!(result.metadata.runtime.ckb_runtime_required);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "Input"));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "CellDep"));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "Output"));
        assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
        let action = result.metadata.actions.iter().find(|action| action.name == "update").expect("update metadata");
        assert_eq!(action.read_refs.len(), 1);
        assert_eq!(action.create_set.len(), 1);
        assert_eq!(action.consume_set.len(), 1);
        assert_eq!(action.create_set[0].operation, "create");
        assert!(action.ckb_runtime_accesses.iter().any(|access| access.source == "Output" && access.operation == "create"));
        assert!(action.elf_compatible);
        assert!(action.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(action.fail_closed_runtime_features.is_empty());
        assert!(!action.touches_shared.is_empty());
        assert!(action.estimated_cycles > 32);
        assert_eq!(action.scheduler_witness_abi, SCHEDULER_WITNESS_ABI_MOLECULE);
        assert!(!action.scheduler_witness_hex.is_empty());
        assert!(!action.scheduler_witness_hex.starts_with("11ce"));
        assert_eq!(
            action.scheduler_witness_bytes().expect("scheduler witness hex should decode"),
            decode_scheduler_witness_hex(&action.scheduler_witness_hex).expect("scheduler witness hex should decode")
        );

        let witness = decode_molecule_scheduler_witness_hex(&action.scheduler_witness_hex);
        assert_eq!(witness.magic, 0xCE11);
        assert_eq!(witness.version, 2);
        assert_eq!(witness.effects, action.effects);
        assert!(!witness.parallelizable);
        assert_eq!(witness.touches_shared_count as usize, witness.touches_shared.len());
        assert_eq!(witness.estimated_cycles, action.estimated_cycles);
        assert_eq!(witness.access_count as usize, witness.accesses.len());

        let access_ops = witness.accesses.iter().map(|access| access.operation).collect::<std::collections::BTreeSet<_>>();
        let access_sources = witness.accesses.iter().map(|access| access.source).collect::<std::collections::BTreeSet<_>>();
        assert!(access_ops.contains(&1), "consume access missing from scheduler witness");
        assert!(access_ops.contains(&6), "read_ref access missing from scheduler witness");
        assert!(access_ops.contains(&7), "create access missing from scheduler witness");
        assert!(access_sources.contains(&1), "Input source missing from scheduler witness");
        assert!(access_sources.contains(&2), "CellDep source missing from scheduler witness");
        assert!(access_sources.contains(&3), "Output source missing from scheduler witness");
        assert!(witness.accesses.iter().any(|access| access.index == 0));
        assert!(witness.accesses.iter().any(|access| access.binding_hash != [0u8; 32]));
    }

    #[test]
    fn scheduler_witness_hex_decode_rejects_invalid_metadata_hex() {
        let odd = decode_scheduler_witness_hex("11c").unwrap_err();
        assert!(odd.message.contains("full bytes"));
        let invalid = decode_scheduler_witness_hex("11xz").unwrap_err();
        assert!(invalid.message.contains("invalid scheduler witness hex byte"));
    }

    #[test]
    fn action_scheduler_witness_bytes_rejects_old_wire_and_retired_alias() {
        let action = action_metadata_with_scheduler_fields("11ce01");
        assert!(action.scheduler_witness_bytes().is_err());
        let mut value = serde_json::to_value(action).unwrap();
        value["scheduler_witness_molecule_hex"] = "11ce01".into();
        assert!(serde_json::from_value::<ActionMetadata>(value).is_err());
    }

    #[test]
    fn generic_shared_mutation_does_not_emit_pool_pattern_metadata() {
        let source = r#"
module test

shared Ledger has store {
    balance: u64,
    owner: Address,
}

action credit(ledger_before: Ledger, delta: u64) -> ledger_after: Ledger {
    verification
        require ledger_after.owner == ledger_before.owner
        require ledger_after.balance == ledger_before.balance + delta
}
"#;

        let result = compile(source, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "credit").expect("credit metadata");

        assert!(action
            .effects
            .lifecycle_ops()
            .iter()
            .any(|operation| *operation == cellscript_artifact_checker::effect_facts::LifecycleOperation::Consume));
        assert!(action
            .effects
            .lifecycle_ops()
            .iter()
            .any(|operation| *operation == cellscript_artifact_checker::effect_facts::LifecycleOperation::Create));
        assert!(!action.parallelizable, "generic shared mutation must not default to parallel execution");
        assert!(!action.touches_shared.is_empty(), "generic shared mutation must expose scheduler-visible shared type hash");
        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "ledger_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "ledger_after"));
        assert!(
            action.pool_primitives.is_empty(),
            "generic shared mutation must not emit Pool pattern metadata: {:?}",
            action.pool_primitives
        );
        assert!(
            !action.verifier_obligations.iter().any(|obligation| obligation.category == "pool-pattern"),
            "generic shared mutation must not report Pool pattern obligations: {:?}",
            action.verifier_obligations
        );
        assert!(
            result.metadata.runtime.pool_primitives.is_empty(),
            "runtime metadata must not aggregate Pool primitives for generic shared mutation: {:?}",
            result.metadata.runtime.pool_primitives
        );
    }

    #[test]
    fn fixed_byte_mutable_state_set_transition_is_checked_under_ckb_profile() {
        let source = r#"
	module test

resource NFT has store, destroy {
    token_id: u64
    owner: Address
    metadata_hash: Hash
    royalty_recipient: Address
    royalty_bps: u16
}

action transfer(nft_before: NFT, to: Address) -> nft_after: NFT {
    verification
        require nft_before.owner != to, "cannot transfer to self"
        require nft_after.token_id == nft_before.token_id
        require nft_after.owner == to
        require nft_after.metadata_hash == nft_before.metadata_hash
        require nft_after.royalty_recipient == nft_before.royalty_recipient
        require nft_after.royalty_bps == nft_before.royalty_bps
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "transfer").expect("transfer metadata");

        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "nft_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "nft_after"));
        assert!(
            asm.contains("# cellscript abi: fixed-byte field Named(\"NFT\").owner offset=8 size=32"),
            "fixed-byte output owner requirement should read the proposed output field:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte Eq comparison size=32"),
            "fixed-byte output requirements should compare address/hash fields:\n{}",
            asm
        );
        assert!(
            asm.contains("li a0, 0\n    j .Ltransfer_epilogue"),
            "void action success path must clear a0 before returning to ckb-vm:\n{}",
            asm
        );
        assert!(!action
            .transaction_runtime_input_requirements
            .iter()
            .any(|requirement| { requirement.feature == "mutable-cell:NFT" && requirement.component == "mutate-field-transition" }));
    }

    #[test]
    fn dynamic_named_output_constraints_are_proven_in_verification_section() {
        let source = r#"
module test

resource Collection has store {
    name: String,
    creator: Address,
    total_supply: u64,
    max_supply: u64,
}

action mint(collection_before: Collection) -> collection_after: Collection {
    verification
        require collection_after.name == collection_before.name
        require collection_after.creator == collection_before.creator
        require collection_after.total_supply == collection_before.total_supply + 1
        require collection_after.max_supply == collection_before.max_supply
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: output field verification deferred to explicit verification constraints"),
            "dynamic named outputs should be bound in the prelude and proven by verification constraints:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: fail closed because the output state is not fully verified"),
            "signature output constraints should not fail closed before verification requirements run:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: dynamic schema field Collection.name index=0 as Molecule vector bytes"),
            "dynamic verification constraint should still emit runtime Molecule field comparison:\n{}",
            asm
        );
        assert!(
            asm.contains("# binary Eq over dynamic byte operands") && asm.contains("call __cellscript_memcmp_fixed"),
            "dynamic String equality must compare field bytes, not pointer addresses:\n{}",
            asm
        );
    }

    #[test]
    fn ordered_named_output_create_constraints_are_checked_in_body_order() {
        let source = r#"
module test

resource NFT has store {
    token_id: u64,
    owner: Address,
    metadata_hash: Hash,
    royalty_recipient: Address,
    royalty_bps: u16,
}

resource Collection has store {
    creator: Address,
    total_supply: u64,
    max_supply: u64,
}

action batch(collection_before: Collection, recipients: [Address; 4], metadata_hashes: [Hash; 4]) -> (collection_after: Collection, nft0: NFT) {
    verification
        let first_token_id = collection_before.total_supply + 1
        require collection_after.creator == collection_before.creator
        require collection_after.total_supply == first_token_id
        require collection_after.max_supply == collection_before.max_supply
        create nft0 = NFT {
            token_id: first_token_id,
            owner: recipients[0],
            metadata_hash: metadata_hashes[0],
            royalty_recipient: collection_before.creator,
            royalty_bps: 250,
        }
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        let body_start = asm.find(".Lbatch_block_0:").expect("batch body label");
        let prelude = &asm[..body_start];
        let ordered_create = asm.find("# constrain named output NFT").expect("ordered named output constraint");
        let ordered_section = &asm[ordered_create..];

        assert!(
            prelude.contains("# cellscript abi: output field verification deferred to ordered create constraint"),
            "prelude should bind named outputs but defer explicit create field checks:\n{}",
            asm
        );
        assert!(
            !prelude.contains("# cellscript abi: verify output bytes field NFT.owner"),
            "prelude must not compare against recipients[0] before body temporaries are assigned:\n{}",
            asm
        );
        assert!(ordered_create > body_start, "named output create check should stay in body order:\n{}", asm);
        assert!(
            ordered_section.contains("# cellscript abi: verify output bytes field NFT.owner")
                && ordered_section.contains("# cellscript abi: verify output bytes field NFT.metadata_hash"),
            "ordered create constraint must verify dynamic indexed expected byte fields after body lowering:\n{}",
            asm
        );
    }

    #[test]
    fn compile_result_exposes_schema_layout_metadata() {
        let result = compile(PARAM_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let snapshot = result.metadata.types.iter().find(|ty| ty.name == "Snapshot").expect("Snapshot type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "inspect").expect("inspect metadata");
        assert_eq!(action.params.len(), 1);
        assert_eq!(action.params[0].name, "snapshot");
        assert_eq!(action.params[0].ty, "Snapshot");
        assert!(action.params[0].schema_pointer_abi);
        assert!(action.params[0].schema_length_abi);
        assert_eq!(snapshot.kind, "Struct");
        assert_eq!(snapshot.encoded_size, Some(8));
        let molecule_schema = snapshot.molecule_schema.as_ref().expect("Snapshot molecule schema metadata");
        assert_eq!(molecule_schema.abi, "molecule");
        assert_eq!(molecule_schema.layout, "fixed-struct-v1");
        assert_eq!(molecule_schema.fixed_size, 8);
        assert!(molecule_schema.schema.contains("struct Snapshot"));
        assert!(molecule_schema.schema.contains("amount: CellScriptUint64"));
        assert_eq!(molecule_schema.schema_hash, crate::hex_encode(&crate::ckb_blake2b256(molecule_schema.schema.as_bytes())));
        let amount = snapshot.fields.iter().find(|field| field.name == "amount").expect("amount field metadata");
        assert_eq!(amount.ty, "u64");
        assert_eq!(amount.offset, 0);
        assert_eq!(amount.encoded_size, Some(8));
        assert!(amount.fixed_width);
    }

    #[test]
    fn fixed_enum_fields_have_molecule_schema_metadata() {
        let source = r#"
module enum_schema

enum LockType {
    Absolute,
    Relative,
}

resource TimeLock {
    owner: Address,
    lock_type: LockType,
    unlock_height: u64,
}

action create_lock(owner: Address) -> TimeLock {
    verification
        let lock = create TimeLock {
            owner: owner,
            lock_type: LockType::Absolute,
            unlock_height: 42,
        };
        lock
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let time_lock = result.metadata.types.iter().find(|ty| ty.name == "TimeLock").expect("TimeLock metadata");
        assert_eq!(time_lock.encoded_size, Some(41));
        let lock_type = time_lock.fields.iter().find(|field| field.name == "lock_type").expect("lock_type field");
        assert_eq!(lock_type.encoded_size, Some(1));
        let schema = time_lock.molecule_schema.as_ref().expect("TimeLock molecule schema");
        assert_eq!(schema.layout, "fixed-struct-v1");
        assert_eq!(schema.fixed_size, 41);
        assert!(schema.schema.contains("array CellScriptEnumTag [byte; 1];"));
        assert!(schema.schema.contains("lock_type: CellScriptEnumTag"));
    }

    #[test]
    fn payload_enum_fields_use_fixed_tagged_union_molecule_schema_metadata() {
        let source = r#"
module payload_enum_schema

enum AssetType {
    Native,
    Token(Hash),
}

resource LockedAsset {
    asset_type: AssetType,
    amount: u64,
    lock_hash: Hash,
}

action lock_asset(asset_type: AssetType, amount: u64) -> LockedAsset {
    verification
        let locked = create LockedAsset {
            asset_type: asset_type,
            amount: amount,
            lock_hash: Hash::zero(),
        };
        locked

}
lock asset_matches(protected locked_asset: LockedAsset, witness expected: Hash) -> bool {
    verification
        locked_asset.lock_hash == expected
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let locked = result.metadata.types.iter().find(|ty| ty.name == "LockedAsset").expect("LockedAsset metadata");
        assert_eq!(locked.encoded_size, Some(73));
        let asset_type = locked.fields.iter().find(|field| field.name == "asset_type").expect("asset_type field");
        assert_eq!(asset_type.encoded_size, Some(33));
        let schema = locked.molecule_schema.as_ref().expect("LockedAsset should have a fixed Molecule schema");
        assert_eq!(schema.layout, "fixed-struct-v1");
        assert!(schema.dynamic_fields.is_empty());
        assert!(schema.schema.contains("array LockedAssetAssetTypeFixedBytes [byte; 33];"));
        assert!(schema.schema.contains("asset_type: LockedAssetAssetTypeFixedBytes"));

        let ckb = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        assert!(
            ckb.metadata.types.iter().any(|ty| ty.name == "LockedAsset" && ty.molecule_schema.is_some()),
            "CKB policy should accept payload enum table metadata"
        );

        let temp = tempdir().unwrap();
        let entry = Utf8Path::from_path(temp.path()).unwrap().join("payload_enum_schema.cell");
        std::fs::write(&entry, source).unwrap();
        let scoped = compile_file_with_entry_lock(
            &entry,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
            "asset_matches",
        )
        .unwrap();
        assert!(scoped.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_result_exposes_nested_fixed_molecule_schema_metadata() {
        let source = r#"
module audit::nested_schema

struct Owner {
    pubkey: Hash,
    flags: [u8; 2],
}

resource Token has store {
    owner: Owner,
    pair: (u64, Owner),
    checkpoints: [(Owner, u64); 2],
    amount: u64,
}

action value() -> u64 {
    verification
        return 1
}
"#;

        let result = compile(source, CompileOptions::default()).unwrap();
        let owner = result.metadata.types.iter().find(|ty| ty.name == "Owner").expect("Owner type metadata");
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token type metadata");

        assert_eq!(owner.encoded_size, Some(34));
        assert_eq!(token.encoded_size, Some(168));
        let owner_field = token.fields.iter().find(|field| field.name == "owner").expect("owner field metadata");
        let pair_field = token.fields.iter().find(|field| field.name == "pair").expect("pair field metadata");
        let checkpoints_field = token.fields.iter().find(|field| field.name == "checkpoints").expect("checkpoints field metadata");
        let amount_field = token.fields.iter().find(|field| field.name == "amount").expect("amount field metadata");
        assert_eq!(owner_field.offset, 0);
        assert_eq!(owner_field.encoded_size, Some(34));
        assert_eq!(pair_field.offset, 34);
        assert_eq!(pair_field.encoded_size, Some(42));
        assert_eq!(checkpoints_field.offset, 76);
        assert_eq!(checkpoints_field.encoded_size, Some(84));
        assert_eq!(amount_field.offset, 160);
        assert_eq!(amount_field.encoded_size, Some(8));

        let molecule_schema = token.molecule_schema.as_ref().expect("Token molecule schema metadata");
        assert_eq!(molecule_schema.fixed_size, 168);
        assert!(molecule_schema.schema.contains("struct Owner"));
        assert!(molecule_schema.schema.contains("struct CellScriptTupleTokenPair"));
        assert!(molecule_schema.schema.contains("struct CellScriptTupleTokenCheckpoints"));
        assert!(molecule_schema.schema.contains("struct Token"));
        assert!(molecule_schema.schema.contains("owner: Owner"));
        assert!(molecule_schema.schema.contains("pair: CellScriptTupleTokenPair"));
        assert!(molecule_schema.schema.contains("array TokenCheckpointsArray2 [CellScriptTupleTokenCheckpoints; 2];"));
        assert!(molecule_schema.schema.contains("checkpoints: TokenCheckpointsArray2"));
        assert_eq!(molecule_schema.schema_hash, crate::hex_encode(&crate::ckb_blake2b256(molecule_schema.schema.as_bytes())));
    }

    #[test]
    fn compile_exposes_ckb_type_id_contract_under_ckb_profile() {
        let program = r#"
module audit::type_id

#[type_id("cellscript::asset::Token:v1")]
resource Token has store {
    amount: u64
}

action value() -> u64 {
    verification
        return 1
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token type metadata");
        let ckb_type_id = token.ckb_type_id.as_ref().expect("CKB TYPE_ID metadata");

        assert_eq!(result.metadata.target_profile.name, "ckb");
        assert_eq!(token.type_id.as_deref(), Some("cellscript::asset::Token:v1"));
        assert_eq!(ckb_type_id.abi, crate::CKB_TYPE_ID_ABI);
        assert_eq!(ckb_type_id.script_code_hash, crate::hex_encode(&crate::CKB_TYPE_ID_CODE_HASH));
        assert_eq!(ckb_type_id.hash_type, crate::CKB_TYPE_ID_HASH_TYPE);
        assert_eq!(ckb_type_id.args_source, crate::CKB_TYPE_ID_ARGS_SOURCE);
        assert_eq!(ckb_type_id.group_rule, crate::CKB_TYPE_ID_GROUP_RULE);
        assert_eq!(ckb_type_id.builder, crate::CKB_TYPE_ID_BUILDER);
        assert_eq!(ckb_type_id.verifier, crate::CKB_TYPE_ID_VERIFIER);
    }

    #[test]
    fn compile_metadata_exposes_ckb_type_id_create_output_plan_under_ckb_profile() {
        let program = r#"
module audit::type_id_create

#[type_id("cellscript::asset::Token:v1")]
resource Token has store {
    amount: u64
}

resource PlainToken has store {
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        return create Token { amount: amount }

}
action mint_plain(amount: u64) -> PlainToken {
    verification
        return create PlainToken { amount: amount }
}
"#;

        let ckb_metadata = compile_metadata_for_profile_without_artifact_policy(program, crate::TargetProfile::Ckb);
        let mint = ckb_metadata.actions.iter().find(|action| action.name == "mint").expect("mint metadata");
        let plan = mint.create_set[0].ckb_type_id.as_ref().expect("CKB TYPE_ID create output plan");

        assert_eq!(mint.ckb_type_id_output_indexes(), vec![0]);
        assert_eq!(plan.abi, crate::CKB_TYPE_ID_ABI);
        assert_eq!(plan.type_id, "cellscript::asset::Token:v1");
        assert_eq!(plan.output_source, crate::CKB_TYPE_ID_OUTPUT_SOURCE);
        assert_eq!(plan.output_index, 0);
        assert_eq!(plan.script_code_hash, crate::hex_encode(&crate::CKB_TYPE_ID_CODE_HASH));
        assert_eq!(plan.hash_type, crate::CKB_TYPE_ID_HASH_TYPE);
        assert_eq!(plan.args_source, crate::CKB_TYPE_ID_ARGS_SOURCE);
        assert_eq!(plan.builder, crate::CKB_TYPE_ID_BUILDER);
        assert_eq!(plan.generator_setting, crate::CKB_TYPE_ID_GENERATOR_SETTING);
        assert_eq!(plan.wasm_setting, crate::CKB_TYPE_ID_WASM_SETTING);

        let plain = ckb_metadata.actions.iter().find(|action| action.name == "mint_plain").expect("mint_plain metadata");
        assert!(plain.create_set[0].ckb_type_id.is_none());
        assert!(plain.ckb_type_id_output_indexes().is_empty());
    }

    #[test]
    fn compile_allows_struct_type_id_under_ckb_profile() {
        let program = r#"
module audit::type_id

#[type_id("cellscript::asset::TokenSnapshot:v1")]
struct TokenSnapshot {
    amount: u64
}

action value() -> u64 {
    verification
        return 1
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.metadata.target_profile.name, "ckb");
        let token = result.metadata.types.iter().find(|ty| ty.name == "TokenSnapshot").expect("TokenSnapshot metadata");
        assert_eq!(token.type_id.as_deref(), Some("cellscript::asset::TokenSnapshot:v1"));
    }

    #[test]
    fn compile_metadata_exposes_template_layout_for_cyclic_flow_type() {
        let program = r#"
module test

enum PoolState {
    Idle,
    Active,
}

resource Pool has store {
    state: PoolState,
    reserve: u64,
}

flow Pool.state {
    Idle -> Active;
    Active -> Idle;
}

action activate(input: Pool) -> output: Pool {
    transition input.state: Idle -> output.state: Active
    verification
        require output.reserve == input.reserve
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let pool = result.metadata.types.iter().find(|ty| ty.name == "Pool").expect("Pool type metadata");
        let pool_schema = pool.molecule_schema.as_ref().expect("Pool molecule schema metadata");
        let layout =
            result.metadata.template_layouts.iter().find(|layout| layout.type_name == "Pool").expect("Pool TemplateLayout metadata");

        assert_eq!(layout.schema, "cellscript-template-layout-v0.21");
        assert_eq!(layout.scope, "Type");
        assert_eq!(layout.state_field.as_deref(), Some("state"));
        assert!(!layout.state_machine_acyclic);
        assert_eq!(layout.cycle_policy, "RootRequired");
        assert_eq!(layout.layout, "Flat");
        assert_eq!(layout.root_hash, None);
        assert!(!layout.consensus_checked);
        assert_eq!(layout.leaf_schema.abi, "molecule");
        assert_eq!(layout.leaf_schema.schema_hash, pool_schema.schema_hash);
        assert_eq!(layout.leaf_schema.field_count, pool.fields.len());
        assert!(crate::is_canonical_hash_hex(&layout.template_layout_hash));
        result.validate().unwrap();
    }

    #[test]
    fn compile_metadata_preserves_protocol_role_sources_without_authorization_claims() {
        let program = r#"
module test

enum SwapState {
    Pending,
    Claimed,
}

resource SwapLock has store {
    state: SwapState,
    participant: Address,
}

flow SwapLock.state {
    initial Pending;
    terminal Claimed;
    Pending -> Claimed;
}

action claim(input swap: SwapLock, witness participant: Address) -> output: SwapLock {
    transition swap.state: Pending -> output.state: Claimed
    verification
        require participant == swap.participant
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "claim").expect("claim metadata");
        let selected = action.protocol_role_candidates.first().expect("explicit role candidate");

        assert_eq!(selected.role, "participant");
        assert_eq!(selected.source, "verification-predicate");
        assert_eq!(selected.strength, "explicit");
        assert_eq!(selected.actor_binding.as_deref(), Some("participant"));
        assert_eq!(selected.actor_source.as_deref(), Some("witness"));
        assert_eq!(selected.cell_binding.as_deref(), Some("swap"));
        assert_eq!(selected.field_name.as_deref(), Some("participant"));
        assert_eq!(selected.evidence_tier, crate::EvidenceTier::MetadataOnly);
        assert!(!selected.authorization_proven);
        assert!(action.protocol_role_candidates.iter().any(|candidate| candidate.source == "witness-binding"));
        assert!(action.protocol_role_candidates.iter().any(|candidate| candidate.source == "field-name"));
        assert!(!action.proof_plan.iter().any(|plan| plan.category == "protocol-role"));
        assert!(!result.metadata.runtime.proof_plan.iter().any(|plan| plan.category == "protocol-role"));
        result.validate().unwrap();
    }

    #[test]
    fn compile_result_validation_rejects_protocol_role_authorization_overclaim() {
        let program = r#"
module test

resource Vault has store {
    owner: Address,
}

action inspect(input vault: Vault) -> output: Vault {
    verification
        require output.owner == vault.owner
}
"#;
        let mut result = compile(program, CompileOptions::default()).unwrap();
        let candidate = result.metadata.actions[0].protocol_role_candidates.first_mut().expect("weak owner role candidate");
        candidate.authorization_proven = true;

        let error = result.validate().unwrap_err();

        assert!(error.message.contains("metadata-only"), "unexpected error: {error}");
        assert!(error.message.contains("authorization_proven=false"), "unexpected error: {error}");
    }

    #[test]
    fn compile_identity_ckb_type_id_emits_metadata() {
        let program = r#"
module audit::identity_type_id

resource Token has store
    identity(ckb_type_id)
{
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token type metadata");

        assert_eq!(token.identity_policy.as_deref(), Some("ckb_type_id"));
    }

    #[test]
    fn compile_identity_singleton_type_emits_metadata() {
        let program = r#"
module audit::identity_singleton

resource Config has store
    identity(singleton_type)
{
    value: u64
}

action init(value: u64) -> Config {
    verification
        create Config { value: value }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let config = result.metadata.types.iter().find(|ty| ty.name == "Config").expect("Config type metadata");

        assert_eq!(config.identity_policy.as_deref(), Some("singleton_type"));
    }

    #[test]
    fn compile_identity_script_args_emits_metadata() {
        let program = r#"
module audit::identity_script_args

resource Token has store
    identity(script_args)
{
    amount: u64
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token type metadata");

        assert_eq!(token.identity_policy.as_deref(), Some("script_args"));
    }

    #[test]
    fn bounded_quantifier_proof_plan_records_scan_and_vacuity_contracts() {
        let source = r#"
module test

resource Token {
    amount: u64
}

invariant positive_outputs {
    trigger: type_group
    scope: group
    reads: group_outputs<Token>.amount
    forall output token in group_outputs<Token> {
        require token.amount > 0
    }
}

invariant one_claim {
    trigger: explicit_entry
    scope: transaction
    reads: outputs<Token>.amount
    count(outputs<Token> where amount == 7) == 1
}

action run() -> u64 {
    verification
        return 0
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..Default::default() }).unwrap();
        let forall = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "forall:group_outputs<Token>")
            .expect("forall ProofPlan record");
        assert_eq!(forall.category, "bounded-source-quantifier");
        assert_eq!(forall.evidence_tier, crate::EvidenceTier::RuntimeHelperRequired);
        assert!(forall.coverage.contains(&"complexity:O(group_outputs)".to_string()));
        assert!(forall.coverage.contains(&"vacuous:true-when-cardinality-zero".to_string()));
        assert!(forall.coverage.contains(&"field_read:group_outputs<Token>.amount".to_string()));

        let count = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.feature == "count:outputs<Token>==1")
            .expect("count ProofPlan record");
        assert!(count.coverage.contains(&"accumulator_width:u64".to_string()));
        assert!(count.coverage.contains(&"overflow_policy:fail-closed".to_string()));
        assert!(count.coverage.contains(&"actual_scanned_cardinality:runtime-recorded".to_string()));
    }

    #[test]
    fn payload_enum_create_fields_are_checked_in_strict_proof_plan_mode() {
        let source = r#"
module payload::output_field

enum AssetType {
    Native,
    Token(Hash),
    NFT(Hash, u64),
}

resource LockedAsset has store, create {
    asset_type: AssetType,
    amount: u64,
}

action lock_asset(asset_type: AssetType, amount: u64) -> locked: LockedAsset {
    verification
        require amount > 0
        create locked = LockedAsset { asset_type, amount }
}
"#;
        let result = compile(
            source,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                primitive_compat: Some("0.16".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "lock_asset").expect("lock_asset metadata");
        let asset_type = action.params.iter().find(|param| param.name == "asset_type").expect("asset_type parameter metadata");
        assert!(!asset_type.schema_pointer_abi);
        assert!(!asset_type.schema_length_abi);
        assert!(asset_type.fixed_byte_pointer_abi);
        assert!(asset_type.fixed_byte_length_abi);
        assert_eq!(asset_type.fixed_byte_len, Some(41));
        let native_asset_type = vec![0; 41];
        let witness =
            action.entry_witness_args(&[EntryWitnessArg::Bytes(native_asset_type.clone()), EntryWitnessArg::U64(7)]).unwrap();
        let mut expected_witness = ENTRY_WITNESS_ABI_MAGIC.to_vec();
        expected_witness.extend_from_slice(&native_asset_type);
        expected_witness.extend_from_slice(&7u64.to_le_bytes());
        assert_eq!(witness, expected_witness);
        let constraints = result
            .metadata
            .constraints
            .entry_abi
            .iter()
            .find(|entry| entry.entry_kind == "action" && entry.entry_name == "lock_asset")
            .expect("lock_asset entry ABI constraints");
        assert_eq!(constraints.witness_payload_bytes, 49);
        assert_eq!(constraints.min_witness_bytes, 57);
        let obligation = action
            .verifier_obligations
            .iter()
            .find(|obligation| obligation.feature == "create-output:LockedAsset:locked")
            .expect("create-output obligation");
        assert_eq!(obligation.status, "checked-runtime");
        assert!(obligation.detail.contains("create-output-fields=checked-runtime"));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:LockedAsset:locked"
                && requirement.component == "create-output-fields"
                && requirement.status == "checked-runtime"
        }));
        assert!(!action.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()));
    }
