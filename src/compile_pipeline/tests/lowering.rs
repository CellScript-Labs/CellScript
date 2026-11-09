// Compiler integration lowering behavior; retains crate::tests identities.

    #[test]
    fn compile_produces_non_empty_riscv_assembly() {
        let result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());

        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(asm.contains(".section .text"));
        assert!(asm.contains(".global add"));
    }

    #[test]
    fn compile_spills_parameters_and_returns_computed_value() {
        let result = compile(
            SIMPLE_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("sd a0, 0(sp)"), "missing parameter spill for x:\n{}", asm);
        assert!(asm.contains("sd a1, 8(sp)"), "missing parameter spill for y:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing add instruction:\n{}", asm);
        assert!(asm.contains("sd t0, 0(sp)\n    ld a0, 0(sp)"), "missing return load for z:\n{}", asm);
    }

    #[test]
    fn compile_lowers_if_statement_into_basic_blocks() {
        let result = compile(IF_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("beqz t0, .Lincrement_if_block_2"), "missing conditional branch to else block:\n{}", asm);
        assert!(
            !asm.contains("j .Lincrement_if_block_1"),
            "then block is already the physical fall-through and should not need an unconditional jump:\n{}",
            asm
        );
        assert!(asm.contains(".Lincrement_if_block_1:"), "missing then block label:\n{}", asm);
        assert!(asm.contains(".Lincrement_if_block_2:"), "missing else block label:\n{}", asm);
        assert!(asm.contains(".Lincrement_if_block_3:"), "missing join block label:\n{}", asm);
        assert!(!asm.contains(".Lelse:"), "stale hard-coded else label leaked into assembly:\n{}", asm);
    }

    #[test]
    fn compile_emits_direct_user_function_calls() {
        let result = compile(
            CALL_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".global double"), "missing callee symbol:\n{}", asm);
        assert!(asm.contains(".global run"), "missing caller symbol:\n{}", asm);
        assert!(asm.contains("call double"), "missing direct call instruction:\n{}", asm);
        assert!(asm.contains("call double\n    sd a0, 0(sp)\n    ld a0, 0(sp)"), "missing call result spill:\n{}", asm);
    }

    #[test]
    fn compile_lowers_if_expression_with_join_move() {
        let result = compile(
            IF_EXPR_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("beqz t0, .Lchoose_block_2"), "missing conditional branch for if expression:\n{}", asm);
        assert!(
            !asm.contains("j .Lchoose_block_1"),
            "then block is already the physical fall-through and should not need an unconditional jump:\n{}",
            asm
        );
        assert!(asm.contains(".Lchoose_block_3:"), "missing join block for if expression:\n{}", asm);
        for block in [1, 2] {
            let body = asm.split(&format!(".Lchoose_block_{block}:\n")).nth(1).unwrap().split(".Lchoose_block_").next().unwrap();
            assert!(body.contains("ld t0, 0(sp)\n    sd t0, 0(sp)"), "missing branch value move into reused join slot:\n{body}");
        }
        assert!(asm.contains(".Lchoose_block_3:\n    ld a0, 0(sp)"), "join must return the selected value:\n{asm}");
    }

    #[test]
    fn compile_lowers_if_expression_fixed_byte_const_join_move() {
        let result = compile(IF_EXPR_FIXED_BYTE_CONST_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("la a0, __cellscript_const_data_"),
            "fixed-byte constant move must materialize a rodata pointer instead of a null pointer:\n{}",
            asm
        );
    }

    #[test]
    fn compile_lowers_while_statement_into_loop_cfg() {
        let result = compile(WHILE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".Lspin_block_1:"), "missing while condition block:\n{}", asm);
        assert!(asm.contains(".Lspin_block_2:"), "missing while body block:\n{}", asm);
        assert!(asm.contains(".Lspin_block_3:"), "missing while exit block:\n{}", asm);
        assert!(asm.contains("beqz t0, .Lspin_block_3"), "missing while false-branch jump:\n{}", asm);
        assert!(
            !asm.contains("j .Lspin_block_2"),
            "loop body is already the physical fall-through and should not need an unconditional jump:\n{}",
            asm
        );
        assert!(asm.contains("j .Lspin_block_1"), "missing while back edge:\n{}", asm);
    }

    #[test]
    fn compile_lowers_for_range_into_counted_loop_cfg() {
        let result = compile(
            FOR_RANGE_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".Lvisit_block_1:"), "missing for-loop condition block:\n{}", asm);
        assert!(asm.contains(".Lvisit_block_2:"), "missing for-loop body block:\n{}", asm);
        assert!(asm.contains(".Lvisit_block_3:"), "missing for-loop exit block:\n{}", asm);
        assert!(asm.contains("sltu t0, t0, t1"), "missing range bound comparison:\n{}", asm);
        assert!(asm.contains("li t1, 1"), "missing range increment constant:\n{}", asm);
        assert!(asm.contains("j .Lvisit_block_1"), "missing for-loop back edge:\n{}", asm);
    }

    #[test]
    fn compile_lowers_mutable_assignments_in_loop_bodies() {
        let result = compile(
            ASSIGN_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("sub t0, t0, t1"), "missing subtraction for x = x - 1:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing addition for x += 1:\n{}", asm);
        assert!(asm.contains("sd t0, 8(sp)"), "missing assignment write-back into x slot:\n{}", asm);
    }

    #[test]
    fn compile_rejects_assignment_to_temporary_field_targets() {
        let err = compile(TEMPORARY_FIELD_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("assignment target must be rooted at a named local or parameter"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(READ_REF_FIELD_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("assignment target must be rooted at a named local or parameter"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_unsound_mutable_parameter_forms() {
        let err = compile(MUT_CELL_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("cell-backed parameter 'token' cannot use leading 'mut'"), "unexpected error: {}", err.message);

        let err = compile(MUT_READ_REF_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("parameter 'cfg' is a read-only reference"), "unexpected error: {}", err.message);

        let err = compile(REDUNDANT_MUT_REF_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("`&mut` Cell parameters are not valid"), "unexpected error: {}", err.message);

        let err = compile(FUNCTION_MUT_REF_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("`&mut` Cell parameters are not valid"), "unexpected error: {}", err.message);

        let err = compile(LOCK_MUT_REF_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("`&mut` Cell parameters are not valid"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_if_expression_branch_type_mismatch() {
        let err = compile(IF_MISMATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("if expression branches must have matching types"));
    }

    #[test]
    fn compile_rejects_unstable_schema_field_names() {
        let err = compile(DUPLICATE_RESOURCE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate field 'amount' in resource 'Token'"), "unexpected error: {}", err.message);

        let err = compile(DUPLICATE_SHARED_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate field 'reserve' in shared 'Pool'"), "unexpected error: {}", err.message);

        let err = compile(DUPLICATE_RECEIPT_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate field 'amount' in receipt 'Grant'"), "unexpected error: {}", err.message);

        let err = compile(WILDCARD_STRUCT_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("struct 'Point' field must have a stable name"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_unknown_functions() {
        let err = compile(UNKNOWN_FUNCTION_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown function 'missing'"));
    }

    #[test]
    fn compile_lowers_local_constants_into_real_operands() {
        let result = compile(CONSTANT_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("li t1, 3") || asm.contains("li t0, 3"), "missing literal load for local constant:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing arithmetic using lowered constant:\n{}", asm);
    }

    #[test]
    fn compile_lowers_numeric_cast_without_zero_fallback() {
        let result = compile(CAST_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("li t0, 0"), "cast lowering regressed to zero fallback:\n{}", asm);
        assert!(asm.contains(".global widen"), "missing widened function symbol:\n{}", asm);
    }

    #[test]
    fn compile_guards_numeric_narrowing_casts() {
        let result = compile(CHECKED_NARROWING_CAST_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("li t1, 255"), "narrowing cast is missing its u8 upper bound:\n{}", asm);
        assert!(
            asm.contains("numeric-or-discriminant-invalid") || asm.contains("li a0, 20"),
            "narrowing cast is missing its fail-closed runtime path:\n{}",
            asm
        );
        assert!(
            result.metadata.typed_semantics.entries.iter().any(|entry| {
                entry.id == "action:narrow"
                    && entry.blocks.iter().any(|block| {
                        block.runtime_error.as_ref().is_some_and(|error| {
                            error.code == crate::runtime_errors::CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()
                                && error.name == crate::runtime_errors::CellScriptRuntimeError::NumericOrDiscriminantInvalid.name()
                        })
                    })
            }),
            "narrowing cast typed semantics must identify its fail-closed numeric error exit"
        );
    }

    #[test]
    fn compile_lowers_u64_division_as_unsigned_riscv() {
        let result = compile(U64_DIVISION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.matches("divu t0, t0, t1").count() >= 2,
            "u64 return and assertion divisions should lower to unsigned divu:\n{}",
            asm
        );
        assert!(!asm.contains("div t0, t0, t1"), "u64 return division regressed to signed div:\n{}", asm);
    }

    #[test]
    fn compile_accepts_lock_boundary_param_sources_and_require() {
        let result = compile(LOCK_BOUNDARY_CLASSIFICATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".global owner_guard"), "missing lock symbol:\n{}", asm);
        assert!(
            asm.contains("cellscript runtime error 5 assertion-failed"),
            "require failure path did not lower to the lock script failure path:\n{}",
            asm
        );

        let lock = result.metadata.locks.iter().find(|lock| lock.name == "owner_guard").expect("owner_guard metadata");
        assert_eq!(lock.params[0].name, "token");
        assert_eq!(lock.params[0].ty, "&Token");
        assert_eq!(lock.params[0].source, "protected");
        assert!(lock.params[0].protected_spend_surface);
        assert!(!lock.params[0].witness_data_source);

        assert_eq!(lock.params[1].name, "script_owner");
        assert_eq!(lock.params[1].ty, "Address");
        assert_eq!(lock.params[1].source, "lock_args");
        assert!(lock.params[1].lock_args_data_source);
        assert!(!lock.params[1].protected_spend_surface);
        assert!(!lock.params[1].witness_data_source);

        assert_eq!(lock.params[2].name, "claimed_owner");
        assert_eq!(lock.params[2].ty, "Address");
        assert_eq!(lock.params[2].source, "witness");
        assert!(lock.params[2].witness_data_source);
        assert!(!lock.params[2].protected_spend_surface);
        assert!(!lock.params[2].lock_args_data_source);
        assert!(lock.ckb_runtime_features.iter().any(|feature| feature == "ckb-lock-args"));
        assert!(lock.ckb_runtime_accesses.iter().any(|access| {
            access.operation == "lock-args"
                && access.syscall == "LOAD_SCRIPT_ARGS"
                && access.source == "ScriptArgs"
                && access.binding == "script_owner"
        }));
    }

    #[test]
    fn compile_rejects_lock_boundary_sources_outside_supported_scope() {
        let action_err = compile(ACTION_PROTECTED_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            action_err.message.contains("protected and lock_args are lock parameter source syntax"),
            "unexpected error: {}",
            action_err.message
        );

        compile(ACTION_REQUIRE_PROGRAM, CompileOptions::default()).expect("actions may use require for verifier guards");
        compile(ACTION_REQUIRE_MESSAGE_PROGRAM, CompileOptions::default()).expect("require may carry a static review message");

        let lock_args_err = compile(LOCK_ARGS_DYNAMIC_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            lock_args_err.message.contains("lock_args lock parameter 'owner' must use a fixed-width script-args type"),
            "unexpected error: {}",
            lock_args_err.message
        );

        let function_require_err = compile(FUNCTION_REQUIRE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            function_require_err.message.contains("require/preserve is verifier-boundary syntax"),
            "unexpected error: {}",
            function_require_err.message
        );
    }

    #[test]
    fn compile_rejects_dynamic_require_messages() {
        let err = compile(REQUIRE_DYNAMIC_MESSAGE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("require message must be a string literal"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_non_bool_assert_condition() {
        let err = compile(ASSERT_NON_BOOL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("require condition must be boolean"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_preserves_read_ref_instructions_in_assembly() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# read_ref Config"), "read_ref expression vanished from assembly:\n{}", asm);
    }

    #[test]
    fn compile_binds_read_action_schema_params_to_cell_deps() {
        let program = r#"
module entry_read_ref;

resource NFT has destroy {
    token_id: u64,
    owner: Address,
}

receipt Listing has destroy {
    token_id: u64,
    seller: Address,
    price: u64,
}

action create_listing(read nft: NFT, price: u64) -> listing: Listing {

        create listing = Listing {
            token_id: nft.token_id,
            seller: nft.owner,
            price: price,
        }
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref_param_dep source=CellDep index=0"),
            "explicit read schema entry parameter did not use the CellDep LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: bind read-only param nft to Input#0 cell data"),
            "explicit read schema entry parameter regressed to implicit Input binding:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Listing.token_id offset=0 size=8"),
            "created output did not verify the u64 field copied from the read-only input:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output bytes field Listing.seller offset=8 size=32"),
            "created output did not verify the fixed-byte field copied from the read-only input:\n{}",
            asm
        );
    }

    #[test]
    fn compile_rejects_read_ref_for_non_cell_backed_types() {
        let err = compile(READ_REF_STRUCT_TARGET_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("read_ref target type 'Snapshot' must be a resource, shared, or receipt cell type"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_binds_read_ref_entry_params_to_cell_deps() {
        let program = r#"
module test;

shared Config has store {
    admin: Address,
}

resource Token has store {
    amount: u64,
}

receipt Grant has store {
    admin: Address,
    amount: u64,
}

action grant(read config: Config, token: Token) -> Grant {

        consume token;
        create Grant {
            admin: config.admin,
            amount: token.amount,
        } with_lock(config.admin)
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: bind read-only param config to CellDep#0 cell data"),
            "read_ref entry parameter was not bound to a CellDep:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref_param_dep source=CellDep index=0"),
            "read_ref entry parameter did not use the CellDep LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: bind read-only param config to Input#"),
            "read_ref entry parameter regressed back to Input binding:\n{}",
            asm
        );
    }

    #[test]
    fn compile_binds_duplicate_read_refs_by_order_not_name() {
        let result = compile(DUPLICATE_READ_REF_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref source=CellDep index=0"),
            "first read_ref did not bind to CellDep index 0:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref source=CellDep index=1"),
            "second read_ref did not bind to CellDep index 1:\n{}",
            asm
        );
        assert!(
            asm.matches("# cellscript abi: exact size check Config expected=8").count() >= 2,
            "duplicate read_refs should each have an exact schema-size check:\n{}",
            asm
        );
    }

    #[test]
    fn compile_preserves_schema_backed_parameter_field_access_in_assembly() {
        let result = compile(PARAM_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: fixed-byte scalar field Named(\"Snapshot\").amount offset=0 size=8"),
            "schema field access did not expose the concrete layout contract:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: schema param snapshot pointer=a0 length=a1"),
            "schema parameter did not use pointer+length ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check fixed-byte scalar field access loaded bytes var0 expected=8"),
            "schema parameter field access did not enforce exact fixed schema size:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 0(t4)") && asm.contains("slli t2, t2, 56"),
            "u64 schema field access did not lower to an unaligned-safe byte load sequence:\n{}",
            asm
        );
        assert!(!asm.contains("field access '.amount' has no lowered schema-backed representation"));
    }

    #[test]
    fn compile_lowers_fixed_byte_schema_field_comparison() {
        let result = compile(FIXED_BYTE_FIELD_COMPARISON_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "same_symbol").unwrap();

        assert!(
            asm.contains("# cellscript abi: fixed-byte Eq comparison size=8"),
            "schema-backed fixed-byte equality did not lower to a byte comparison:\n{}",
            asm
        );
        assert!(
            !asm.contains("fixed-byte comparison fail-closed runtime path"),
            "verified fixed-byte equality should not use the fail-closed lowering path:\n{}",
            asm
        );
        assert!(
            !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "metadata should not report fixed-byte comparison when both operands are verifier-coverable: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_marks_addressable_runtime_hash_comparisons_as_checked() {
        let source = r#"
module runtime_hash_comparison;

struct Envelope {
    digest: Hash,
}

action verify_runtime_hashes(witness envelope: Envelope, witness expected: Hash) -> bool {

    let input = source::input(0);
    let out_point_hash = ckb::input_out_point_tx_hash(input);
    let local_envelope = new Envelope { digest: envelope.digest, };
    let packed_hash = ckb::hash_data_packed(local_envelope);
    require out_point_hash == expected;
    require packed_hash == expected;
    true
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "verify_runtime_hashes").unwrap();

        assert!(asm.contains("call __ckb_input_out_point_tx_hash"), "full OutPoint hash read must be emitted:\n{asm}");
        assert!(asm.contains("call __ckb_hash_blake2b_var"), "packed-data hash must be emitted:\n{asm}");
        assert!(asm.contains("call __cellscript_memcmp_fixed"), "runtime Hash equality must use the fixed-byte helper:\n{asm}");
        assert!(
            !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "addressable runtime Hash results must not be reported as unresolved: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_bip340_ipc_as_eighteen_checked_words() {
        let source = r#"
module bip340_ipc_words;

action verify(witness verifier_data_hash: Hash, witness message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> bool {

    let verifier_dep = source::cell_dep(3);
    ckb::require_cell_data_hash(verifier_dep, verifier_data_hash);
    verifier::btc::bip340::require_signature_from_cell_dep(3, message, pubkey, signature);
    true
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "verify").expect("verify metadata");

        assert_eq!(
            asm.matches("# cellscript abi: novaseal bip340 ipc word ").count(),
            18,
            "BIP340 IPC must expose all 18 frozen envelope words:\n{}",
            asm
        );
        assert_eq!(
            asm.matches("call __ckb_pipe_write").count(),
            18,
            "each BIP340 IPC word must use the checked VM2 write helper:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: novaseal bip340 ipc word 0")
                && asm.contains("# cellscript abi: novaseal bip340 ipc word 17")
                && asm.contains("cellscript runtime error 52 bip340-message-write-failed"),
            "BIP340 IPC word range and fail-closed write path must remain visible:\n{}",
            asm
        );
        for operation in ["bip340-pipe", "bip340-spawn", "bip340-pipe-write-18-words", "bip340-close", "bip340-wait"] {
            assert!(
                action.ckb_runtime_accesses.iter().any(|access| access.operation == operation),
                "BIP340 lowering metadata omitted {operation}: {:?}",
                action.ckb_runtime_accesses
            );
        }
        assert!(
            action.ckb_runtime_features.iter().any(|feature| feature == "btc-bip340-runtime-verifier")
                && action.ckb_runtime_features.iter().any(|feature| feature == "ckb-spawn-ipc"),
            "BIP340 lowering metadata omitted verifier or VM2 feature: {:?}",
            action.ckb_runtime_features
        );
        assert!(
            action
                .ckb_runtime_accesses
                .iter()
                .any(|access| access.operation == "bip340-spawn" && access.source == "CellDep" && access.index == 3),
            "explicit verifier CellDep index must remain visible in metadata: {:?}",
            action.ckb_runtime_accesses
        );
        assert!(
            action.proof_plan.iter().any(|plan| plan.feature.contains("bip340")),
            "BIP340 lowering must remain visible in ProofPlan: {:?}",
            action.proof_plan
        );
    }

    #[test]
    fn compile_lowers_bounded_cell_dep_data_hash_scan() {
        let source = r#"
module bounded_cell_dep_scan;

action verify(witness expected: Hash) -> bool {

    ckb::require_bounded_cell_dep_data_hash(8, expected);
    true
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "verify").expect("verify metadata");

        assert!(
            asm.contains("call __ckb_require_bounded_cell_dep_data_hash")
                && asm.contains("scan resolved CellDeps with LOAD_CELL_BY_FIELD")
                && asm.contains("cellscript runtime error 63 bounded-cell-dep-not-found"),
            "bounded CellDep scan and stable failure code must remain visible:\n{asm}"
        );
        assert!(
            action.ckb_runtime_features.iter().any(|feature| feature == "ckb-bounded-cell-dep-scan")
                && action.ckb_runtime_features.iter().any(|feature| feature == "ckb-cell-dep-data-hash-binding"),
            "bounded CellDep features missing from metadata: {:?}",
            action.ckb_runtime_features
        );
        assert!(
            action
                .ckb_runtime_accesses
                .iter()
                .any(|access| access.operation == "bounded-cell-dep-data-hash-scan" && access.source == "CellDep"),
            "bounded CellDep access missing from metadata: {:?}",
            action.ckb_runtime_accesses
        );
    }

    #[test]
    fn compile_verifies_fixed_named_aggregate_output_fields() {
        let source = r#"
module fixed_named_output_field;

struct OutPoint {
    tx_hash: Hash,
    index: u32,
}

receipt Receipt {
    old_cell: OutPoint,
    digest: Hash,
}

action issue(witness old_cell: OutPoint, witness digest: Hash) -> issued: Receipt {

    create issued = Receipt {
        old_cell: old_cell,
        digest: digest,
    }
}
"#;
        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "issue").expect("issue metadata");

        assert!(
            asm.contains("# cellscript abi: verify output bytes field Receipt.old_cell offset=0 size=36"),
            "fixed named aggregate output fields must be compared as packed bytes:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: ordered named output field verification incomplete")
                && !action.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "fixed named aggregate output verification must stay aligned across metadata and codegen:\n{}",
            asm
        );
    }

    #[test]
    fn compile_lowers_packed_bool_and_u32_schema_fields_without_aligned_loads() {
        let result = compile(PACKED_SCALAR_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: fixed-byte scalar field Ref(Named(\"Flags\")).enabled offset=0 size=1"),
            "bool schema field access did not expose concrete layout:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte scalar field Ref(Named(\"Flags\")).nonce offset=1 size=4"),
            "u32 schema field access did not expose packed schema offset:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check fixed-byte scalar field access loaded bytes var0 expected=5"),
            "packed u32 schema field access did not check full byte span:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 1(t4)") && asm.contains("slli t2, t2, 24"),
            "packed u32 field access did not lower to unaligned-safe little-endian byte loads:\n{}",
            asm
        );
    }

    #[test]
    fn compile_classifies_guarded_identity_field_merge_as_checked_runtime() {
        let result = compile(
            CONSUME_CREATE_IDENTITY_FIELD_MERGE_CONSERVATION_PROGRAM,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "merge").expect("merge metadata");
        assert!(
            action.verifier_obligations.iter().any(|obligation| {
                obligation.category == "transaction-invariant"
                    && obligation.feature == "resource-conservation:Token"
                    && obligation.status == "checked-runtime"
                    && obligation.detail.contains("verifier-recomputed u64 amount sum")
            }),
            "guarded amount+identity merge should be marked checked-runtime: {:?}",
            action.verifier_obligations
        );
        assert!(
            action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "resource-conservation:Token"
                    && requirement.component == "resource-conservation-proof"
                    && requirement.status == "checked-runtime"
                    && requirement.blocker_class.is_none()
            }),
            "guarded amount+identity merge should expose checked transaction input metadata: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_lowers_len_method_to_length_instruction() {
        let result = compile(LEN_METHOD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# length"), "len() call did not lower to length instruction:\n{}", asm);
        assert!(!asm.contains("# call len"), "len() call leaked through generic call path:\n{}", asm);
    }

    #[test]
    fn compile_rejects_forbidden_unwrap_helpers() {
        let unwrap_err = compile(FORBIDDEN_UNWRAP_CALL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unwrap_err.message.contains("unwrap is forbidden"), "unexpected error: {}", unwrap_err.message);

        let expect_err = compile(FORBIDDEN_EXPECT_METHOD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(expect_err.message.contains("expect is forbidden"), "unexpected error: {}", expect_err.message);

        let unwrap_or_err = compile(FORBIDDEN_NAMESPACED_UNWRAP_OR_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unwrap_or_err.message.contains("unwrap_or is forbidden"), "unexpected error: {}", unwrap_or_err.message);
    }

    #[test]
    fn compile_lowers_match_expression_into_branch_cfg() {
        let result = compile(MATCH_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("seqz t0, t0"), "match lowering missing equality check:\n{}", asm);
        assert!(asm.contains(".Lselect_block_1:"), "match lowering missing first arm block:\n{}", asm);
        assert!(asm.contains(".Lselect_block_3:"), "match lowering missing join block:\n{}", asm);
    }

    #[test]
    fn compile_rejects_unknown_or_reserved_named_types() {
        let unknown = compile(UNKNOWN_NAMED_TYPE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unknown.message.contains("unknown type 'MissingType'"), "unexpected error: {}", unknown.message);

        let option = compile(RESERVED_OPTION_TYPE_PROGRAM, CompileOptions::default()).unwrap();
        assert!(option.metadata.generic_instantiations.iter().any(|item| item.template == "Option"));

        let generic = compile(USER_GENERIC_TYPE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert_eq!(generic.code.as_deref(), Some("E2111"));
        assert!(generic.message.contains("has no local template available"), "unexpected error: {}", generic.message);
    }

    #[test]
    fn compile_rejects_duplicate_top_level_symbols() {
        let err = compile(DUPLICATE_TOP_LEVEL_SYMBOL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate symbol 'Token'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_materializes_local_fixed_byte_constants_into_rodata() {
        let result = compile(LOCAL_ZERO_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("__const_data"), "undefined const label leaked into assembly:\n{}", asm);
        assert!(asm.contains("__cellscript_const_data_0:"), "local fixed-byte constant should have a concrete rodata label:\n{}", asm);

        compile(LOCAL_ZERO_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
    }

    #[test]
    fn compile_lowers_u128_equality_as_fixed_byte_comparison() {
        let result = compile(U128_EQ_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: u128 compare high limb first"),
            "u128 equality should compare high and low 64-bit limbs:\n{}",
            asm
        );
        assert!(
            !asm.contains("unsupported u128 operand shape"),
            "u128 equality should not fall through to the generic fail-closed path:\n{}",
            asm
        );
    }

    #[test]
    fn compile_lowers_u128_modulo_without_fail_closed_debt() {
        let result = compile_with_executable_surface_policy(
            U128_MOD_PROGRAM,
            CompileOptions::default(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"u128-modulo".to_string()),
            "u128 modulo must not carry fail-closed metadata: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        assert!(asm.contains("checked u128 remainder by restoring long division"), "missing u128 remainder lowering:\n{asm}");
        assert!(!asm.contains("u128 Mod requires full-width lowering"), "legacy fail-closed modulo path remains:\n{asm}");
    }

    #[test]
    fn compile_lowers_integer_bitwise_and_shift_operations_to_elf() {
        let source = r#"
module bitwise_shift;

fn scalar(value: u64, mask: u64, count: u64) -> u64 {
    return ((value & mask) | (value ^ mask)) << count;
}

fn wide(value: u128, mask: u128, count: u64) -> u128 {
    return ((value & mask) | (value ^ mask)) >> count;
}

fn contextual_literals() -> u8 {
    return (1 << 7) | 3;
}
"#;
        for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let result =
                compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), diagnostics, ..CompileOptions::default() })
                    .unwrap();
            assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        }
    }

    #[test]
    fn compile_rejects_static_shift_amount_at_value_width() {
        let source = r#"
module bitwise_shift;

fn bad(value: u8) -> u8 {
    return value << 8;
}
"#;
        let error = compile(source, CompileOptions::default()).expect_err("static shift amount at width must be rejected");
        assert_eq!(error.code.as_deref(), Some("E2106"));
        assert!(error.message.contains("out of range for 8-bit u8"), "unexpected error: {}", error.message);
    }

    #[test]
    fn compile_verifies_large_output_field_requirements_without_partial_fallback() {
        let result = compile(LARGE_PRESERVED_MUTATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: fixed-byte Eq comparison size=600"),
            "large explicit output field requirements should use the fixed-byte verifier path:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "update").expect("update metadata");
        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "state_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "state_after"));
    }

    #[test]
    fn compile_result_writes_artifact_to_disk() {
        let result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        let dir = tempdir().unwrap();
        let output = Utf8Path::from_path(dir.path()).unwrap().join("out").join("program.s");

        result.write_to_path(&output).unwrap();

        let written = std::fs::read(&output).unwrap();
        assert_eq!(written, result.artifact_bytes);
    }

    #[test]
    fn compile_produces_non_empty_riscv_elf() {
        let result =
            compile(SIMPLE_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(!result.artifact_bytes.is_empty());
        // CKB profile does not embed VM ABI trailer
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
    }

    #[test]
    fn compile_riscv_elf_accepts_large_stack_offsets() {
        let mut program = String::from("module vm::large_stack;\n\naction main() -> u64 {\n    \n");
        for index in 0..260 {
            program.push_str(&format!("        let x{} = {};\n", index, index));
        }
        program.push_str("        return x259;\n}\n");

        let asm = compile(&program, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(asm.artifact_bytes).unwrap();
        assert!(!asm.contains("2048(sp)") && !asm.contains("2072(sp)"), "large stack offsets should be expanded:\n{}", asm);

        let elf = compile(&program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(elf.artifact_format, ArtifactFormat::RiscvElf);
        assert!(elf.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_riscv_elf_accepts_large_schema_field_offsets() {
        let scalar_program = r#"
module vm::large_schema_scalar;

struct Big {
    pad: [u8; 2048],
    amount: u64,
}

action inspect(snapshot: Big) -> u64 {

        return snapshot.amount;
}
"#;

        let scalar_asm =
            compile(scalar_program, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() }).unwrap();
        let scalar_asm = String::from_utf8(scalar_asm.artifact_bytes).unwrap();
        assert!(
            !scalar_asm.contains("lbu t2, 2048(t4)"),
            "large schema scalar field offset should be expanded before assembly:\n{}",
            scalar_asm
        );
        let scalar_elf =
            compile(scalar_program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(scalar_elf.artifact_format, ArtifactFormat::RiscvElf);
        assert!(scalar_elf.artifact_bytes.starts_with(b"\x7fELF"));

        let bytes_program = r#"
module vm::large_schema_bytes;

struct Big {
    pad: [u8; 2048],
    marker: [u8; 1],
}

action get_marker(snapshot: Big) -> [u8; 1] {

        return snapshot.marker;
}
"#;

        let bytes_asm =
            compile(bytes_program, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() }).unwrap();
        let bytes_asm = String::from_utf8(bytes_asm.artifact_bytes).unwrap();
        assert!(
            !bytes_asm.contains("addi t0, t4, 2048"),
            "large schema fixed-byte field pointer offset should be expanded before assembly:\n{}",
            bytes_asm
        );
        let bytes_elf =
            compile(bytes_program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(bytes_elf.artifact_format, ArtifactFormat::RiscvElf);
        assert!(bytes_elf.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_result_validation_rejects_hidden_raw_cell_data_codec() {
        let program = r#"
module test::raw_codec_tamper;

action inspect() -> u64 {

        let input = source::group_input(0);
        let amount = ckb::cell_data_u64_le(input, 4);
        if amount != 123456789 {
            return 91;
        }
        return 0;
}
"#;
        let mut result =
            compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        result.metadata.cell_data_codec_manifest.abi = "molecule".to_string();
        result.metadata.cell_data_codec_manifest.raw_bytes_required = false;
        result.metadata.cell_data_codec_manifest.raw_runtime_access_count = 0;
        result.metadata.cell_data_codec_manifest.raw_runtime_accesses.clear();
        result.metadata.cell_data_codec_manifest.manifest_hash =
            crate::cell_data_codec_manifest_hash(&result.metadata.cell_data_codec_manifest);

        let err = result.validate().unwrap_err();
        assert!(err.message.contains("cell_data_codec_manifest.abi"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_accepts_current_outputs() {
        let result =
            compile(SIMPLE_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.metadata.edition, crate::CURRENT_EDITION);
        assert_eq!(result.metadata.compatibility_profile.schema, crate::COMPATIBILITY_PROFILE_SCHEMA);
        assert_eq!(result.metadata.compatibility_profile.source_semantics, crate::CURRENT_EDITION.source_semantics());
        assert_eq!(result.metadata.compatibility_profile.metadata_schema_version, crate::METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.compatibility_profile.source_metadata_schema_version, crate::SOURCE_METADATA_SCHEMA_VERSION);
        assert_eq!(result.metadata.compatibility_profile.artifact_metadata_schema_version, crate::ARTIFACT_METADATA_SCHEMA_VERSION);
        assert_eq!(
            result.metadata.compatibility_profile.constraints_metadata_schema_version,
            crate::CONSTRAINTS_METADATA_SCHEMA_VERSION
        );
        assert_eq!(result.metadata.compatibility_profile.entry_witness_payload_abi, crate::ENTRY_WITNESS_ABI);
        assert_eq!(result.metadata.compatibility_profile.entry_witness_placement_abi, crate::ENTRY_WITNESS_PLACEMENT_ABI);
        assert_eq!(result.metadata.compatibility_profile.entry_witness_placement_field, crate::ENTRY_WITNESS_PLACEMENT_FIELD);
        assert_eq!(result.metadata.constraints.edition, crate::CURRENT_EDITION);
        assert_eq!(result.metadata.constraints.compatibility_profile, result.metadata.compatibility_profile.id);
        result.validate().unwrap();
    }

    #[test]
    fn compile_prefers_no_arg_main_for_entry_wrapper() {
        let result = compile(
            r#"
module test;

action needs_arg(value: u64) -> u64 {

        value

}
action main() -> u64 {

        0
}
"#,
            CompileOptions::default(),
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# cellscript entry abi: _cellscript_entry tail-calls no-arg main"), "missing entry wrapper:\n{}", asm);
        assert!(asm.contains("    j main"), "entry wrapper must tail-call main without clobbering ra:\n{}", asm);
        assert!(
            asm.find("_cellscript_entry:").unwrap() < asm.find("needs_arg:").unwrap(),
            "entry wrapper must precede actions:\n{}",
            asm
        );
    }

    #[test]
    fn compile_uses_unified_strategy_by_default() {
        let baseline = compile(
            OPTIMIZER_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let optimized = compile(
            OPTIMIZER_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::default(), ..CompileOptions::default() },
        )
        .unwrap();

        let baseline_asm = String::from_utf8(baseline.artifact_bytes).unwrap();
        let optimized_asm = String::from_utf8(optimized.artifact_bytes).unwrap();

        assert!(baseline_asm.contains("add t0, t0, t1"), "baseline assembly should still compute the expression");
        assert!(baseline_asm.contains("mul t0, t0, t1"), "baseline assembly should still compute the expression");
        assert!(optimized_asm.contains("li a0, 20"), "optimized assembly should fold the constant expression:\n{}", optimized_asm);
        assert!(!optimized_asm.contains("add t0, t0, t1"), "optimized assembly should not retain the folded add");
        assert!(!optimized_asm.contains("mul t0, t0, t1"), "optimized assembly should not retain the folded multiply");
    }

    #[test]
    fn compile_result_validation_rejects_tampered_artifact_hash() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.artifact_bytes.push(b'\n');

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("artifact_hash does not match artifact_bytes"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_assembly_with_vm_abi_trailer() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.artifact_bytes = crate::append_vm_abi_trailer(result.artifact_bytes, result.metadata.runtime.vm_abi.version);
        rebind_artifact_integrity_for_test(&mut result);

        let err = result.validate().unwrap_err();

        assert!(
            err.message.contains("RISC-V assembly artifacts must not embed a VM ABI trailer"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_result_validation_rejects_type_id_hash_mismatch() {
        let program = r#"
module audit::type_id;

#[type_id("cellscript::asset::Token:v1")]
resource Token has store {
    amount: u64,
}
"#;
        let mut result = compile(program, CompileOptions::default()).unwrap();
        let token = result.metadata.types.iter_mut().find(|ty| ty.name == "Token").expect("Token type metadata");
        token.type_id_hash = Some("00".repeat(32));

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("type_id_hash"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_compiler_version_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.compiler_version = "0.0.0-old".to_string();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("compiler_version"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_schema_backed_parameter_field_access_to_elf() {
        let result =
            compile(PARAM_FIELD_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    }

    #[test]
    fn compile_infers_and_validates_read_only_effects() {
        let result = compile(READ_ONLY_EFFECT_PROGRAM, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "inspect").expect("inspect metadata");

        assert_eq!(
            action.effects,
            ir::EffectFacts::read(
                cellscript_artifact_checker::effect_facts::ContextSource::CellDep,
                cellscript_artifact_checker::effect_facts::ContextAccess::CellData
            )
        );
        assert!(action.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(action.fail_closed_runtime_features.is_empty());
    }

    #[test]
    fn compile_infers_creation_without_an_effect_annotation() {
        let result = compile(CREATION_EFFECT_PROGRAM, CompileOptions::default()).unwrap();
        let entry = result.metadata.actions.iter().find(|action| action.name == "issue").unwrap();
        assert_eq!(
            entry.effects.lifecycle_ops(),
            ir::EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create).lifecycle_ops()
        );
    }

    #[test]
    fn compile_infers_read_only_helper_function_effects() {
        let result = compile(
            IMPURE_FN_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "helper").expect("helper metadata");
        assert!(!function.effects.context_reads().is_empty());
        assert!(function.effects.allows_observation());
        assert_eq!(function.effect_evidence_tier, crate::EvidenceTier::CheckedStatic);
    }

    #[test]
    fn compile_infers_helper_function_effects_through_action_calls() {
        let result = compile(
            INDIRECT_IMPURE_FN_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "helper").expect("helper metadata");

        assert_eq!(
            function.effects.lifecycle_ops().iter().copied().collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                cellscript_artifact_checker::effect_facts::LifecycleOperation::Create,
                cellscript_artifact_checker::effect_facts::LifecycleOperation::Consume,
                cellscript_artifact_checker::effect_facts::LifecycleOperation::Destroy
            ])
        );
        assert_eq!(function.effect_evidence_tier, crate::EvidenceTier::CheckedStatic);
    }

    #[test]
    fn compile_infers_read_only_effect_for_env_runtime_builtins() {
        let result = compile(
            FN_ENV_RUNTIME_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "helper").expect("helper metadata");
        assert!(!function.effects.context_reads().is_empty());
        assert!(function.effects.allows_observation());
        assert_eq!(function.effect_evidence_tier, crate::EvidenceTier::CheckedStatic);
    }

    #[test]
    fn compile_infers_read_only_effect_for_type_hash_runtime_builtins() {
        let result = compile(
            FN_TYPE_HASH_RUNTIME_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "helper").expect("helper metadata");
        assert!(function.effects.is_empty(), "a static local type identity does not observe transaction context");
        assert_eq!(function.effect_evidence_tier, crate::EvidenceTier::CheckedStatic);
    }

    #[test]
    fn compile_allows_actions_and_locks_to_call_pure_functions() {
        let action_result = compile(
            ACTION_CALLS_FN_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        assert!(action_result.metadata.functions.iter().any(|function| function.name == "add_one"));

        let lock_result = compile(
            LOCK_CALLS_FN_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
        )
        .unwrap();
        assert!(lock_result.metadata.functions.iter().any(|function| function.name == "yes"));
    }

    #[test]
    fn compile_normalizes_same_module_qualified_helper_calls() {
        let result = compile(QUALIFIED_ACTION_CALLS_FN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains("call add_one"), "qualified helper call was not normalized:\n{}", asm);
        assert!(!asm.contains("call test::add_one"), "qualified helper label leaked into assembly:\n{}", asm);
    }

    #[test]
    fn compile_rejects_function_call_argument_mismatches() {
        let err = compile(CALL_MISSING_ARGUMENT_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("function 'add' expects 2 arguments, found 1"), "unexpected error: {}", err.message);

        let err = compile(CALL_TYPE_MISMATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("function 'add' argument 2 type mismatch"), "unexpected error: {}", err.message);
        assert!(err.message.contains("expected u64, found bool"), "unexpected error: {}", err.message);

        let err = compile(QUALIFIED_CALL_EXTRA_ARGUMENT_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("function 'test::add_one' expects 1 argument, found 2"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_unstable_callable_parameter_names() {
        let err = compile(DUPLICATE_ACTION_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate parameter 'x' in action 'bad'"), "unexpected error: {}", err.message);

        let err = compile(WILDCARD_ACTION_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("action 'bad' parameter must have a stable name"), "unexpected error: {}", err.message);

        let err = compile(DUPLICATE_FN_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate parameter 'x' in function 'bad'"), "unexpected error: {}", err.message);

        let err = compile(WILDCARD_LOCK_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("lock 'owned' parameter must have a stable name"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_local_binding_name_reuse() {
        let err = compile(LOCAL_BINDING_REUSE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("binding 'x' already exists in this scope or an outer scope"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(TUPLE_BINDING_REUSE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("binding 'x' already exists in this scope or an outer scope"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(BLOCK_BINDING_SHADOW_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("binding 'x' already exists in this scope or an outer scope"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_builtin_call_argument_mismatches() {
        let err = compile(BUILTIN_WRONG_ARITY_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("env::current_timepoint expects 0 arguments, found 1"), "unexpected error: {}", err.message);

        let err = compile(NUMERIC_BUILTIN_TYPE_MISMATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("min argument 2 must be numeric, found bool"), "unexpected error: {}", err.message);

        let err = compile(UNKNOWN_NAMESPACED_CONSTRUCTOR_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown namespaced function 'Missing::new'"), "unexpected error: {}", err.message);

        let err = compile(METHOD_WRONG_ARITY_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("len expects 0 arguments, found 1"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ir_preserves_function_call_return_types() {
        let tokens = lexer::lex(BOOL_FN_CALL_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let ir = ir::generate(&module).unwrap();
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "run" => Some(action),
                _ => None,
            })
            .expect("run action");
        let call_dest = action
            .body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .find_map(|instruction| match instruction {
                ir::IrInstruction::Call { dest: Some(dest), func, .. } if func.function_symbol() == Some("ready") => Some(dest),
                _ => None,
            })
            .expect("ready call");

        assert_eq!(call_dest.ty, ir::IrType::Bool);
    }

    #[test]
    fn ir_lowers_unit_function_calls_without_result_destinations() {
        let tokens = lexer::lex(UNIT_FN_CALL_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let ir = ir::generate(&module).unwrap();
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "run" => Some(action),
                _ => None,
            })
            .expect("run action");
        let call = action
            .body
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .find(|instruction| matches!(instruction, ir::IrInstruction::Call { func, .. } if func.function_symbol() == Some("note")))
            .expect("note call");

        assert!(matches!(call, ir::IrInstruction::Call { dest: None, .. }));
    }

    #[test]
    fn ir_rejects_unknown_call_return_types_without_u64_fallback() {
        let tokens = lexer::lex(UNKNOWN_FUNCTION_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let err = ir::generate(&module).unwrap_err();

        assert!(
            err.message.contains("call 'missing' has no known return type during IR lowering"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_allows_unit_function_calls_as_statements() {
        let result = compile(UNIT_FN_CALL_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains("call note"), "unit helper call was not emitted:\n{}", asm);
    }

    #[test]
    fn compile_rejects_binding_unit_function_results() {
        let err = compile(BIND_UNIT_FN_CALL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert_eq!(err.code, Some(crate::error::DiagnosticCode::E2114));
    }

    #[test]
    fn compile_rejects_returning_unit_function_results() {
        let err = compile(RETURN_UNIT_FN_CALL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert_eq!(err.code, Some(crate::error::DiagnosticCode::E2114));
    }

    #[test]
    fn compile_rejects_return_values_from_unit_actions() {
        let err = compile(RETURN_VALUE_FROM_UNIT_ACTION_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("return value is not allowed in a function without a return type"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_bare_return_from_value_actions() {
        let err = compile(BARE_RETURN_FROM_VALUE_ACTION_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("return without value"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_missing_action_return_paths() {
        let err = compile(MISSING_ACTION_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("action 'bad' with a return type must return a value"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_missing_function_return_paths() {
        let err = compile(MISSING_FUNCTION_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("function 'bad' with a return type must return a value"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_tail_expr_as_action_return() {
        let result = compile(TAIL_EXPR_ACTION_RETURN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains("li a0, 1"), "tail expression was not lowered as the action return value:\n{}", asm);
    }

    #[test]
    fn compile_accepts_complete_branch_return_paths() {
        compile(BRANCH_COMPLETE_RETURN_PROGRAM, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_lowers_block_tail_if_expressions() {
        let result = compile(BLOCK_TAIL_IF_VALUE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains("li t0, 1"), "block tail-if then value was not lowered:\n{}", asm);
        assert!(asm.contains("li t0, 2"), "block tail-if else value was not lowered:\n{}", asm);
    }

    #[test]
    fn compile_lowers_tail_if_as_action_return() {
        let result = compile(TAIL_IF_ACTION_RETURN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains("li a0, 1"), "tail if then branch was not lowered as return:\n{}", asm);
        assert!(asm.contains("li a0, 2"), "tail if else branch was not lowered as return:\n{}", asm);
    }

    #[test]
    fn compile_rejects_incomplete_branch_return_paths() {
        let err = compile(BRANCH_INCOMPLETE_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("action 'bad' with a return type must return a value"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_unreachable_statements_after_return() {
        let err = compile(UNREACHABLE_AFTER_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("unreachable statement after guaranteed return"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_unreachable_statements_after_complete_branch_return() {
        let err = compile(UNREACHABLE_AFTER_BRANCH_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("unreachable statement after guaranteed return"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_pure_functions_that_call_locks() {
        let err = compile(FN_CALLS_LOCK_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("function cannot call lock 'guard'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_infers_creation_effects_through_calls() {
        let result = compile(INDIRECT_CREATION_EFFECT_PROGRAM, CompileOptions::default()).unwrap();
        let entry = result.metadata.actions.iter().find(|action| action.name == "wrapper").unwrap();
        assert_eq!(
            entry.effects.lifecycle_ops(),
            ir::EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create).lifecycle_ops()
        );
    }

    #[test]
    fn compile_infers_creation_effects_through_qualified_calls() {
        let result = compile(QUALIFIED_CREATION_EFFECT_PROGRAM, CompileOptions::default()).unwrap();
        let entry = result.metadata.actions.iter().find(|action| action.name == "wrapper").unwrap();
        assert_eq!(
            entry.effects.lifecycle_ops(),
            ir::EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create).lifecycle_ops()
        );
    }

    #[test]
    fn compile_rejects_input_source_outside_action_cell_params() {
        let function_source = r#"
module test;

resource Token has store {
    amount: u64,
}

fn helper(input token: Token) -> u64 {
    token.amount
}
"#;
        let function_err = compile(function_source, CompileOptions::default()).unwrap_err();
        assert!(function_err.message.contains("cannot use input source classification"), "unexpected error: {}", function_err.message);

        let scalar_source = r#"
module test;

action main(input amount: u64) -> u64 {

        amount
}
"#;
        let scalar_err = compile(scalar_source, CompileOptions::default()).unwrap_err();
        assert!(
            scalar_err.message.contains("must name a Cell-backed resource, shared cell, or receipt type"),
            "unexpected error: {}",
            scalar_err.message
        );
    }

    #[test]
    fn compile_accepts_prefix_read_params_as_cell_dep_bindings() {
        let source = r#"
module test;

shared Config has store {
    admin: Address,
}

resource Token has store {
    amount: u64,
}

receipt Grant has store {
    admin: Address,
    amount: u64,
}

action grant(read config: Config, token: Token) -> grant: Grant {

        consume token;
        create grant = Grant {
            admin: config.admin,
            amount: token.amount,
        } with_lock(config.admin)
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref_param_dep source=CellDep index=0"),
            "read prefix parameter did not bind to CellDep:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "grant").expect("grant metadata");
        let config_param = action.params.iter().find(|param| param.name == "config").expect("config param metadata");
        assert_eq!(config_param.source, "read");
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "grant"));
    }

    #[test]
    fn compile_rejects_undeclared_action_state_edge() {
        let source = r#"
module test;

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState,
    amount: u64,
}

flow Offer.state {
    Created -> Live;
}

action accept(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled;

        create output = Offer {
            state: OfferState::Filled,
            amount: 1,
        }
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("is not declared"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ir_summary_captures_cell_runtime_accesses() {
        let tokens = lexer::lex(SUMMARY_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let ir = ir::generate(&module).unwrap();
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "update" => Some(action),
                _ => None,
            })
            .expect("update action");

        assert_eq!(action.body.read_refs.len(), 1);
        assert_eq!(action.body.create_set.len(), 1);
        assert_eq!(action.body.consume_set.len(), 1);
        assert_eq!(action.body.read_refs[0].binding, "read_ref_Config");
        assert_eq!(action.body.create_set[0].ty, "Token");
        assert!(!action.scheduler_hints.touches_shared.is_empty());
        assert!(action.scheduler_hints.estimated_cycles > 32);
    }

    #[test]
    fn compile_rejects_non_bool_lock_definitions() {
        let err = compile(BAD_LOCK_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("lock definitions must return bool"));
    }

    #[test]
    fn dynamic_schema_fixed_field_access_is_table_decoded() {
        let source = r#"
module dynamic_schema_access;

resource Collection {
    name: String,
    creator: Address,
}

lock collection_creator(protected collection: Collection, witness claimed_creator: Address) -> bool {

        collection.creator == claimed_creator
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let collection = result.metadata.types.iter().find(|ty| ty.name == "Collection").expect("Collection metadata");
        let schema = collection.molecule_schema.as_ref().expect("dynamic Collection molecule schema");
        assert_eq!(collection.encoded_size, None);
        assert_eq!(schema.layout, "molecule-table-v1");
        assert_eq!(schema.fixed_size, 0);
        assert_eq!(schema.dynamic_fields, vec!["name".to_string()]);
        assert!(schema.schema.contains("vector CellScriptString <byte>;"));
        assert!(schema.schema.contains("table Collection"));
        assert!(schema.schema.contains("name: CellScriptString"));
        assert!(schema.schema.contains("creator: CellScriptAddress"));

        let lock = result.metadata.locks.iter().find(|lock| lock.name == "collection_creator").expect("lock metadata");
        assert!(
            !lock.fail_closed_runtime_features.contains(&"field-access".to_string()),
            "fixed field access through a Molecule table should be verifier-covered: {:?}",
            lock.fail_closed_runtime_features
        );

        let ckb = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        assert!(
            ckb.metadata.locks[0].fail_closed_runtime_features.is_empty(),
            "CKB table fixed-field lock should not require fail-closed runtime paths: {:?}",
            ckb.metadata.locks[0].fail_closed_runtime_features
        );
    }

    #[test]
    fn dynamic_mutable_schema_transitions_are_checked_after_table_decoding() {
        let source = r#"
module dynamic_mutation;

resource Collection {
    name: String,
    total_supply: u64,
    max_supply: u64,
}

action mint(collection_before: Collection) -> collection_after: Collection {

        require collection_before.total_supply < collection_before.max_supply, "max supply reached";
        require collection_after.name == collection_before.name;
        require collection_after.max_supply == collection_before.max_supply;
        require collection_after.total_supply == collection_before.total_supply + 1;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let collection = result.metadata.types.iter().find(|ty| ty.name == "Collection").expect("Collection metadata");
        let schema = collection.molecule_schema.as_ref().expect("dynamic Collection molecule schema");
        assert_eq!(schema.layout, "molecule-table-v1");
        assert_eq!(schema.dynamic_fields, vec!["name".to_string()]);

        let action = result.metadata.actions.iter().find(|action| action.name == "mint").expect("mint metadata");

        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "collection_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "collection_after"));
        assert!(!action.fail_closed_runtime_features.contains(&"field-access".to_string()));
        assert!(
            !action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "mutable-cell:Collection"
                    && matches!(requirement.component.as_str(), "mutate-field-equality" | "mutate-field-transition")
            }),
            "checked mutable table transitions should not leave runtime input blockers: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_result_validation_rejects_molecule_schema_hash_mismatch() {
        let mut result = compile(PARAM_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let snapshot = result.metadata.types.iter_mut().find(|ty| ty.name == "Snapshot").expect("Snapshot type metadata");
        let molecule_schema = snapshot.molecule_schema.as_mut().expect("Snapshot molecule schema metadata");
        molecule_schema.schema_hash = "00".repeat(32);

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("molecule_schema.schema_hash"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_result_validation_rejects_unsupported_template_layout_claim() {
        let program = r#"
module test;

resource Pool has store {
    reserve: u64,
}

action swap(input: Pool) -> output: Pool {

        require output.reserve == input.reserve;
}
"#;
        let mut result = compile(program, CompileOptions::default()).unwrap();
        let layout = result.metadata.template_layouts.first_mut().expect("TemplateLayout metadata");
        layout.consensus_checked = true;

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("consensus_checked=true"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_duplicate_stable_type_ids() {
        let program = r#"
module audit::type_id;

#[type_id("cellscript::asset::Token:v1")]
resource Token has store {
    amount: u64,
}

#[type_id("cellscript::asset::Token:v1")]
struct TokenSnapshot {
    amount: u64,
}
"#;
        let err = compile(program, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("duplicate type_id 'cellscript::asset::Token:v1'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_identity_none_is_default_and_hidden() {
        let program = r#"
module audit::identity_none;

resource Token has store {
    amount: u64,
}

action mint(amount: u64) -> Token {

        create Token { amount: amount, }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token type metadata");

        assert_eq!(token.identity_policy, None, "identity_policy should be None (default) and hidden in metadata");
    }

    #[test]
    fn compile_identity_field_emits_path() {
        let program = r#"
module audit::identity_field;

resource NFT has store
    identity(field(token_id))
{
    token_id: u64,
    owner: Address,
}

action mint(token_id: u64, owner: Address) -> NFT {

        create NFT { token_id: token_id, owner: owner, }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let nft = result.metadata.types.iter().find(|ty| ty.name == "NFT").expect("NFT type metadata");

        assert_eq!(nft.identity_policy.as_deref(), Some("field(token_id)"));
    }

    #[test]
    fn compile_replace_unique_field_identity_compares_input_and_output() {
        let program = r#"
module audit::replace_unique_field_runtime;

resource NFT has store, replace
    identity(field(token_id))
{
    token_id: u64,
    owner: Address,
}

action rotate(old: NFT, owner: Address) -> NFT {

        replace_unique::<NFT>(identity = field(token_id)) old { token_id: old.token_id, owner: owner }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let create = result.metadata.actions[0].create_set.first().expect("replace_unique metadata");

        assert_eq!(create.operation, "replace_unique");
        assert_eq!(create.identity_policy.as_deref(), Some("field(token_id)"));
        assert!(create.ckb_output_data.is_some(), "replace_unique output must carry CKB output-data index evidence");
        assert!(
            result
                .metadata
                .runtime
                .verifier_obligations
                .iter()
                .any(|obligation| obligation.feature == "replace-unique-identity:NFT:field(token_id)"
                    && obligation.status == "checked-runtime"),
            "missing replace_unique field identity proof obligation"
        );
        assert!(asm.contains("replace_unique identity policy field(token_id)"), "missing identity policy runtime marker:\n{}", asm);
        assert!(
            asm.contains("replace_unique identity field NFT.token_id Input == Output#0"),
            "missing input/output field identity comparison:\n{}",
            asm
        );
    }

    #[test]
    fn compile_rejects_dynamic_unique_identity_field() {
        let program = r#"
module audit::dynamic_identity_field;

resource Note has store
    identity(field(memo))
{
    memo: Vec<u8>,
}

action mint(memo: Vec<u8>) -> Note {

        create_unique::<Note>(identity = field(memo)) { memo: memo }
}
"#;

        let err =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap_err();
        assert!(err.message.contains("identity field 'Note.memo' must be fixed-width"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compiled_riscv_elf_contains_exit_trampoline() {
        let program = r#"
module vm::minimal;

action main() -> u64 {

        return 0;
}
"#;

        let result =
            compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        let artifact = crate::strip_vm_abi_trailer(&result.artifact_bytes);
        let (flags, file_size, memory_size, text_offset) = executable_elf_entry_segment_for_test(artifact);
        assert_eq!(flags & 0x2, 0, "CKB executable segment must not be writable");
        assert_eq!(file_size, memory_size, "CKB ELF must not fake stack memory through PT_LOAD");

        let first_instruction = read_u32_at(artifact, text_offset, "entry trampoline first instruction");
        assert_eq!(first_instruction & 0x7f, 0x17, "ELF trampoline should call the entrypoint, not initialise sp");
        assert_eq!((first_instruction >> 7) & 0x1f, 1, "ELF trampoline call should write ra");

        let exit_syscall_addi_a7 = [0x93, 0x88, 0xd8, 0x05];
        let ecall = [0x73, 0x00, 0x00, 0x00];
        assert!(
            artifact.windows(exit_syscall_addi_a7.len()).any(|window| window == exit_syscall_addi_a7),
            "expected exit syscall load in ELF trampoline"
        );
        assert!(artifact.windows(ecall.len()).any(|window| window == ecall), "expected ecall in ELF trampoline");
    }

    #[test]
    fn type_validity_records_checked_runtime_and_builder_evidence() {
        let source = r#"
module validity::evidence;

resource TimeLock has store, create {
    amount: u64,
    owner: Address,
    locktime: u64,

    validity
        require true;
        require amount > 0;
        require owner != Address::zero();
        require locktime > env::block_number();
}

action mint(amount: u64, owner: Address, locktime: u64) -> TimeLock {

        return create TimeLock { amount: amount, owner: owner, locktime: locktime, };
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let time_lock = result.metadata.types.iter().find(|ty| ty.name == "TimeLock").expect("TimeLock metadata");
        assert_eq!(time_lock.validity_predicates.len(), 4);

        let static_predicate = &time_lock.validity_predicates[0];
        assert_eq!(static_predicate.evidence_tier, crate::EvidenceTier::CheckedStatic);
        assert_eq!(static_predicate.create_path_status, "checked-static");
        assert!(!static_predicate.runtime_checked_on_create);

        for expression in ["amount > 0", "owner != Address::zero()"] {
            let predicate = time_lock
                .validity_predicates
                .iter()
                .find(|predicate| predicate.expression == expression)
                .unwrap_or_else(|| panic!("missing validity predicate {expression}"));
            assert_eq!(predicate.evidence_tier, crate::EvidenceTier::CheckedRuntime);
            assert_eq!(predicate.create_path_status, "checked-runtime");
            assert!(predicate.runtime_checked_on_create);
            assert_eq!(predicate.create_paths_selected, 1);
            assert_eq!(predicate.create_paths_checked, 1);
        }

        let environment = time_lock
            .validity_predicates
            .iter()
            .find(|predicate| predicate.expression.contains("env::block_number"))
            .expect("environment validity predicate");
        assert_eq!(environment.evidence_tier, crate::EvidenceTier::BuilderEvidenceRequired);
        assert_eq!(environment.create_path_status, "builder-header-evidence-required");
        assert_eq!(environment.create_paths_selected, 1);
        assert_eq!(environment.create_paths_checked, 0);
        assert!(environment.dependencies.contains(&"environment:env::block_number".to_string()));
        assert!(environment.dependencies.contains(&"builder:header-dep-block-number-evidence".to_string()));
        assert!(!environment.runtime_checked_on_create);

        let plans =
            result.metadata.runtime.proof_plan.iter().filter(|plan| plan.origin.starts_with("validity:TimeLock#")).collect::<Vec<_>>();
        assert_eq!(plans.len(), 4);
        assert!(plans.iter().any(|plan| plan.evidence_tier == crate::EvidenceTier::CheckedStatic));
        assert!(plans.iter().any(|plan| plan.evidence_tier == crate::EvidenceTier::CheckedRuntime));
        let builder = plans
            .iter()
            .find(|plan| plan.evidence_tier == crate::EvidenceTier::BuilderEvidenceRequired)
            .expect("builder validity ProofPlan");
        assert!(!builder.on_chain_checked);
        assert_eq!(builder.codegen_coverage_status, "gap:builder-evidence-required");

        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(!asm.contains("env::block_number"), "environment evidence must not be emitted as a fake syscall:\n{asm}");
    }

    #[test]
    fn type_validity_keeps_pure_helper_dependencies_through_optimization() {
        let source = r#"
module validity::helpers;

fn positive(value: u64) -> bool {
    let floor = 0;
    return (value > floor);
}

const MIN_AMOUNT: u64 = 1;

resource Token has store, create {
    amount: u64,

    validity
        require true;
        require positive(amount) && (amount >= MIN_AMOUNT);
}

action mint(amount: u64) -> Token {

        return create Token { amount: amount, };
}
"#;
        let result =
            compile(source, CompileOptions { diagnostics: crate::CompilerDiagnostics::default(), ..CompileOptions::default() })
                .unwrap();
        let token = result.metadata.types.iter().find(|ty| ty.name == "Token").expect("Token metadata");
        assert_eq!(token.validity_predicates[0].evidence_tier, crate::EvidenceTier::CheckedStatic);
        let helper = token
            .validity_predicates
            .iter()
            .find(|predicate| predicate.expression.contains("positive"))
            .expect("helper predicate metadata");
        assert_eq!(helper.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert!(helper.dependencies.contains(&"helper:positive".to_string()));
        assert!(helper.dependencies.contains(&"constant:MIN_AMOUNT".to_string()));
    }

    #[test]
    fn type_validity_rejects_deferred_or_impure_surfaces() {
        let cases = [
            (
                r#"
module bad;
struct Value { amount: u64 where amount > 0 }
"#,
                "field-level 'where' refinements are deferred",
            ),
            (
                r#"
module bad;
struct Value {
    amount: u64,
    validity
}
"#,
                "validity block must contain at least one require predicate",
            ),
            (
                r#"
module bad;
struct Value {
    amount: u64,
    validity
        require amount;
}
"#,
                "validity predicate for 'Value' must be boolean",
            ),
            (
                r#"
module bad;
struct Value {
    amount: u64,
    validity
        require amount > env::timestamp();
}
"#,
                "only env::block_number() is approved",
            ),
            (
                r#"
module bad;
struct Value {
    amount: u64,
    validity
        require amount > ckb::header_epoch_number();
}
"#,
                "validity predicate cannot read transaction view",
            ),
            (
                r#"
module bad;
resource Token has store, create { amount: u64, }
fn impure(amount: u64) -> bool { return amount > ckb::cell_capacity(source::input(0)); }
struct Value {
    amount: u64,
    validity
        require impure(amount);
}
"#,
                "validity",
            ),
        ];
        for (source, expected) in cases {
            let error = compile(source, CompileOptions::default()).unwrap_err();
            assert!(error.message.contains(expected), "expected '{expected}', got: {}", error.message);
        }
    }

    #[test]
    fn validity_remains_a_contextual_field_name() {
        let source = r#"
module validity::contextual;

struct Legacy {
    validity: u64,
}

action read() -> u64 {

        let value = new Legacy { validity: 7, };
        return value.validity;
}
"#;
        compile(source, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_result_validation_rejects_tampered_validity_evidence() {
        let source = r#"
module validity::tamper;

struct Value {
    amount: u64,
    validity
        require amount > 0;
}

action make(amount: u64) -> u64 {

        let value = new Value { amount: amount, };
        return value.amount;
}
"#;
        let mut result = compile(source, CompileOptions::default()).unwrap();
        let predicate =
            &mut result.metadata.types.iter_mut().find(|ty| ty.name == "Value").expect("Value metadata").validity_predicates[0];
        predicate.runtime_checked_on_create = false;
        let error = result.validate().unwrap_err();
        assert!(error.message.contains("checked-runtime evidence requires"), "unexpected error: {}", error.message);

        let mut result = compile(
            r#"
module validity::tamper_builder;
struct Value {
    height: u64,
    validity
        require height > env::block_number();
}
"#,
            CompileOptions::default(),
        )
        .unwrap();
        let predicate = &mut result.metadata.types[0].validity_predicates[0];
        predicate.dependencies.retain(|dependency| dependency != "builder:header-dep-block-number-evidence");
        let error = result.validate().unwrap_err();
        assert!(error.message.contains("header-dep block-number contract"), "unexpected error: {}", error.message);
    }
