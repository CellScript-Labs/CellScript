// Compiler integration lifecycle behavior; retains crate::tests identities.

    #[test]
    fn compile_rejects_linear_state_changes_hidden_inside_loops() {
        compile(LOOP_LOCAL_LINEAR_COMPLETE_PROGRAM, CompileOptions::default()).unwrap();

        let dropped = compile(LOOP_DROPPED_LOCAL_LINEAR_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(dropped.message.contains("linear resource 'out' was not consumed"), "unexpected error: {}", dropped.message);

        let for_err = compile(FOR_LOOP_PARENT_LINEAR_CHANGE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            for_err.message.contains("linear resource 'token' cannot change ownership state inside a loop body"),
            "unexpected error: {}",
            for_err.message
        );

        let while_err = compile(WHILE_LOOP_PARENT_LINEAR_CHANGE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            while_err.message.contains("linear resource 'token' cannot change ownership state inside a loop body"),
            "unexpected error: {}",
            while_err.message
        );
    }

    #[test]
    fn compile_rejects_owned_linear_field_assignment() {
        let err = compile(OWNED_LINEAR_FIELD_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("assignment target rooted at linear/resource value 'token' is not supported"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_preserves_create_instructions_in_assembly() {
        let result = compile(CREATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# create Token"), "create expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("#   field amount = 42"), "create fields were not preserved in assembly comments:\n{}", asm);
        assert!(asm.contains("#   with_lock <expr>"), "create lock binding vanished from assembly:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "locked create verifier should verify covered fields before checking lock binding:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript output abi: field verification incomplete for this create pattern"),
            "locked create with coverable fields should not conflate lock verification with field verification:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte param owner pointer=a0 length=a1 size=32"),
            "Address lock parameter should use pointer+length ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_BY_FIELD reason=output_lock_hash source=Output index=0 field=3"),
            "Address lock parameter should be checked with output LockHash syscall:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output bytes field output lock hash offset=0 size=32 against pointer var"),
            "Address lock parameter should be compared through the fixed-byte pointer source:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript output abi: lock verification incomplete for this create pattern"),
            "Address lock parameter should no longer fail closed as incomplete:\n{}",
            asm
        );
        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "coverable locked create fields should not be reflected as incomplete output fields: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-lock-verification-incomplete".to_string()),
            "coverable Address lock parameter should not be reflected as incomplete output lock verification: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "mint").expect("mint metadata");
        assert!(action.params[0].fixed_byte_pointer_abi);
        assert!(action.params[0].fixed_byte_length_abi);
        assert_eq!(action.params[0].fixed_byte_len, Some(32));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "create-output:Token:create_Token"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("create-output-fields=checked-runtime")
                && obligation.detail.contains("create-output-lock=checked-runtime")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:Token:create_Token"
                && requirement.component == "create-output-fields"
                && requirement.status == "checked-runtime"
                && requirement.binding == "create_Token"
                && requirement.source == "Output"
                && requirement.field.as_deref() == Some("fields")
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:Token:create_Token"
                && requirement.component == "create-output-lock"
                && requirement.status == "checked-runtime"
                && requirement.binding == "create_Token"
                && requirement.source == "Output"
                && requirement.field.as_deref() == Some("lock_hash")
                && requirement.abi == "create-output-lock-hash-32"
                && requirement.byte_len == Some(32)
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
    }

    #[test]
    fn compile_emits_create_output_field_verification_for_fixed_u64_fields() {
        let result = compile(CREATE_VERIFY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=create source=Output index=0"),
            "create output was not loaded from CKB Output source:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Token.amount required=8"),
            "create output verification missed field bounds check:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "create output field verification was not emitted:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript output abi: field verification incomplete for this create pattern"),
            "fully verified create output was incorrectly marked incomplete:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check Token expected=8"),
            "create output verification did not enforce exact fixed schema size:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 42"), "create output verification did not load expected constant:\n{}", asm);
        assert!(asm.contains("sub t2, t0, t1"), "create output verification did not compare actual and expected values:\n{}", asm);
        let action = result.metadata.actions.iter().find(|action| action.name == "issue").expect("issue metadata");
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "create-output:Token:create_Token"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("create-output-fields=checked-runtime")
                && obligation.detail.contains("create-output-lock=not-required")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "create-output:Token:create_Token"
                && requirement.component == "create-output-fields"
                && requirement.status == "checked-runtime"
                && requirement.binding == "create_Token"
                && requirement.source == "Output"
                && requirement.field.as_deref() == Some("fields")
                && requirement.abi == "create-output-field-verifier"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
        assert!(
            !action
                .transaction_runtime_input_requirements
                .iter()
                .any(|requirement| requirement.feature == "create-output:Token:create_Token"
                    && requirement.component == "create-output-lock"),
            "create without with_lock should not expose a lock component: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_verifies_create_output_against_computed_scalar_stack_value() {
        let result = compile(CREATE_UNSUPPORTED_OUTPUT_EXPR_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "computed create expected expression did not emit output field verification:\n{}",
            asm
        );
        assert!(asm.contains("mul t0, t0, t1"), "computed scalar field value did not lower before create verification:\n{}", asm);
        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "computed scalar create expression should not be marked as incomplete output verification: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "issue").expect("issue action");
        assert!(
            !action.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "computed scalar create expression should not reach action fail-closed metadata: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_rejects_invalid_create_field_initializers() {
        let duplicate = compile(CREATE_DUPLICATE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            duplicate.message.contains("duplicate field 'amount' in create for 'Token'"),
            "unexpected error: {}",
            duplicate.message
        );

        let missing = compile(CREATE_MISSING_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(missing.message.contains("create for 'Token' is missing field(s): owner"), "unexpected error: {}", missing.message);

        let unknown = compile(CREATE_UNKNOWN_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unknown.message.contains("unknown field 'owner' in create for 'Token'"), "unexpected error: {}", unknown.message);

        let mismatch = compile(CREATE_FIELD_TYPE_MISMATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            mismatch.message.contains("field 'amount' in create for 'Token' has type mismatch"),
            "unexpected error: {}",
            mismatch.message
        );

        let struct_target = compile(CREATE_STRUCT_TARGET_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            struct_target.message.contains("create target type 'Snapshot' must be a resource, shared, or receipt cell type"),
            "unexpected error: {}",
            struct_target.message
        );
    }

    #[test]
    fn compile_rejects_stateful_operations_without_named_linear_cell_operands() {
        let non_cell = compile(CONSUME_NON_CELL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(non_cell.message.contains("consume requires a cell-backed linear value"), "unexpected error: {}", non_cell.message);

        let non_named_consume = compile(CONSUME_NON_NAMED_CELL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            non_named_consume.message.contains("consume requires a named cell-backed value"),
            "unexpected error: {}",
            non_named_consume.message
        );
    }

    #[test]
    fn compile_merges_if_branch_linear_states_conservatively() {
        compile(IF_BOTH_BRANCHES_CONSUME_PROGRAM, CompileOptions::default()).unwrap();

        let partial = compile(IF_PARTIAL_BRANCH_CONSUME_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            partial.message.contains("linear resource 'token' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            partial.message
        );

        let mismatched = compile(IF_MISMATCHED_BRANCH_CONSUME_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            mismatched.message.contains("linear resource 'token' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            mismatched.message
        );
    }

    #[test]
    fn compile_preserves_consume_and_destroy_instructions_in_assembly() {
        let result = compile(CONSUME_DESTROY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# consume"), "consume expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("# destroy"), "destroy expression vanished from assembly:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: destroy output type-hash absence scan binding=b size=32"),
            "destroy did not emit an Output TypeHash absence scan:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_BY_FIELD reason=destroy_output_type_hash source=Output index=t6 field=5"),
            "destroy absence scan did not use Output LOAD_CELL_BY_FIELD:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=destroy source=Input index=1"),
            "destroy input data load did not use operation-specific Input LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: destroy fail-closed runtime path"),
            "destroy should no longer use the fail-closed lowering path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "burn").expect("burn metadata");
        assert_eq!(action.consume_set.len(), 2);
        assert!(action.consume_set.iter().any(|pattern| pattern.binding == "a"));
        assert!(action.consume_set.iter().any(|pattern| pattern.binding == "b" && pattern.operation == "destroy"));
        assert!(action
            .ckb_runtime_accesses
            .iter()
            .any(|access| access.source == "Input" && access.binding == "b" && access.operation == "destroy"));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "resource-operation"
                && obligation.feature == "destroy:Token"
                && obligation.status == "checked-static"
        }));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "consume-input:Token:a"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("consume-input-data=checked-runtime")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "consume-input:Token:a"
                && requirement.status == "checked-runtime"
                && requirement.component == "consume-input-data"
                && requirement.source == "Input"
                && requirement.binding == "a"
                && requirement.field.as_deref() == Some("data")
                && requirement.abi == "consume-load-cell-input"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "destroy-input:Token:b"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("destroy-input-data=checked-runtime")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "destroy-input:Token:b"
                && requirement.status == "checked-runtime"
                && requirement.component == "destroy-input-data"
                && requirement.source == "Input"
                && requirement.binding == "b"
                && requirement.field.as_deref() == Some("data")
                && requirement.abi == "destroy-load-cell-input"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "transaction-invariant"
                && obligation.feature == "destroy-output-scan:Token"
                && obligation.status == "checked-runtime"
                && obligation.detail.contains("destroy-output-absence=checked-runtime")
                && obligation.detail.contains("destroy-output-scan=checked-runtime")
        }));
        assert!(action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "destroy-output-scan:Token"
                && requirement.status == "checked-runtime"
                && requirement.component == "destroy-output-absence"
                && requirement.source == "Output"
                && requirement.field.as_deref() == Some("ckb_type_script_hash-absence")
                && requirement.abi == "destroy-output-scan-type-id"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
        assert!(result.metadata.runtime.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.scope == "action:burn"
                && requirement.feature == "destroy-output-scan:Token"
                && requirement.status == "checked-runtime"
                && requirement.component == "destroy-output-scan"
                && requirement.blocker.is_none()
                && requirement.blocker_class.is_none()
        }));
    }

    #[test]
    fn compile_verifies_created_output_bool_and_u32_fields() {
        let result = compile(CREATE_SCALAR_VERIFY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: verify output field Flags.enabled offset=0 size=1"),
            "created bool output field was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Flags.nonce offset=1 size=4"),
            "created u32 output field was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check Flags expected=5"),
            "created packed scalar output did not enforce exact fixed schema size:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Flags.nonce required=5"),
            "created packed u32 output field did not check full byte span:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 1"), "created bool expected value was not loaded as 1:\n{}", asm);
        assert!(
            asm.contains("lbu t2, 1(t4)") && asm.contains("slli t2, t2, 24"),
            "created packed u32 verifier did not use unaligned-safe byte loads:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_created_scalar_fields_against_consumed_input_aliases() {
        let result = compile(CONSUME_CREATE_SCALAR_ALIAS_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=consume source=Input index=0"),
            "scalar alias verifier did not load consumed input:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=create source=Output index=0"),
            "scalar alias verifier did not load created output:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Flags.enabled offset=0 size=1"),
            "created bool field was not compared against consumed input alias:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Flags.nonce offset=1 size=4"),
            "created u32 field was not compared against consumed input alias:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check Flags expected=5"),
            "scalar alias verifier did not enforce exact fixed schema size:\n{}",
            asm
        );
    }

    #[test]
    fn compile_lowers_consumed_input_field_access_through_loaded_cell_bytes() {
        let result = compile(CONSUME_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=consume source=Input index=0"),
            "consume summary did not load the consumed input cell:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check fixed-byte scalar field access loaded bytes var0 expected=8"),
            "consumed input field access did not enforce exact fixed schema size:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte scalar field Named(\"Token\").amount offset=0 size=8"),
            "consumed input field access did not expose concrete schema layout:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: consumed input pointer retained for verifier field checks"),
            "consume instruction destroyed the preloaded input pointer:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_create_output_against_consumed_input_field_alias() {
        let result = compile(CONSUME_CREATE_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=consume source=Input index=0"),
            "conservation prelude did not load consumed input:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=create source=Output index=0"),
            "conservation prelude did not load created output:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "created output amount was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Token.amount offset=0 size=8"),
            "created output amount was not compared against the consumed input amount:\n{}",
            asm
        );
        assert!(asm.contains("sub t2, t0, t1"), "created output amount and consumed input amount were not compared:\n{}", asm);
        let action = result.metadata.actions.iter().find(|action| action.name == "pass").expect("pass metadata");
        assert!(
            action.verifier_obligations.iter().any(|obligation| {
                obligation.category == "transaction-invariant"
                    && obligation.feature == "resource-conservation:Token"
                    && obligation.status == "checked-runtime"
                    && obligation.detail.contains("resource-conservation=checked-runtime")
                    && obligation.detail.contains("fields: amount")
            }),
            "direct field-for-field resource conservation should be marked checked-runtime: {:?}",
            action.verifier_obligations
        );
        assert!(
            action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "resource-conservation:Token"
                    && requirement.component == "resource-conservation-proof"
                    && requirement.status == "checked-runtime"
                    && requirement.source == "Transaction"
                    && requirement.binding == "Token"
                    && requirement.field.as_deref() == Some("input-output-conservation")
                    && requirement.abi == "resource-conservation-consume-create-accounting"
                    && requirement.blocker.is_none()
                    && requirement.blocker_class.is_none()
            }),
            "checked direct conservation should expose a checked transaction input component: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_classifies_resource_merge_amount_sum_as_checked_runtime() {
        let result = compile(CONSUME_CREATE_MERGE_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: expected expression u64 Add"),
            "created output amount was not compared against the merged input amount expression:\n{}",
            asm
        );
        assert!(
            asm.matches("# cellscript abi: expected field Token.amount offset=0 size=8").count() >= 2,
            "resource merge did not read both consumed input amounts:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "merged output amount was not verified:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "merge").expect("merge metadata");
        assert!(
            action.verifier_obligations.iter().any(|obligation| {
                obligation.category == "transaction-invariant"
                    && obligation.feature == "resource-conservation:Token"
                    && obligation.status == "checked-runtime"
                    && obligation.detail.contains("2 consumed 'Token' Inputs")
                    && obligation.detail.contains("verifier-recomputed u64 amount sum")
            }),
            "amount-sum resource merge should be marked checked-runtime: {:?}",
            action.verifier_obligations
        );
        assert!(
            action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "resource-conservation:Token"
                    && requirement.component == "resource-conservation-proof"
                    && requirement.status == "checked-runtime"
                    && requirement.field.as_deref() == Some("input-output-conservation")
                    && requirement.blocker_class.is_none()
            }),
            "checked merge conservation should expose a checked transaction input component: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_classifies_resource_split_amount_subtraction_as_checked_runtime() {
        let result = compile(CONSUME_CREATE_SPLIT_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: expected expression u64 Sub"),
            "split output did not recompute consumed amount minus fee:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Token.amount offset=0 size=8"),
            "split output did not read the consumed input amount:\n{}",
            asm
        );
        assert!(
            asm.matches("# cellscript abi: verify output field Token.amount offset=0 size=8").count() >= 2,
            "resource split did not verify both created output amounts:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "split").expect("split metadata");
        assert!(
            action.verifier_obligations.iter().any(|obligation| {
                obligation.category == "transaction-invariant"
                    && obligation.feature == "resource-conservation:Token"
                    && obligation.status == "checked-runtime"
                    && obligation.detail.contains("one consumed 'Token' Input is split across 2 created Outputs")
                    && obligation.detail.contains("verifier-recomputed u64 amount subtraction")
            }),
            "amount split resource conservation should be marked checked-runtime: {:?}",
            action.verifier_obligations
        );
        assert!(
            action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "resource-conservation:Token"
                    && requirement.component == "resource-conservation-proof"
                    && requirement.status == "checked-runtime"
                    && requirement.field.as_deref() == Some("input-output-conservation")
                    && requirement.blocker_class.is_none()
            }),
            "checked split conservation should expose a checked transaction input component: {:?}",
            action.transaction_runtime_input_requirements
        );
        assert!(
            !action.transaction_runtime_input_requirements.iter().any(|requirement| {
                requirement.feature == "resource-conservation:Token"
                    && requirement.component == "resource-conservation-proof"
                    && requirement.status == "runtime-required"
            }),
            "checked split conservation must not expose a runtime-required conservation blocker: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_merges_linear_transfers_inside_match_expressions() {
        compile(LINEAR_MATCH_EXPR_LET_MOVE_PROGRAM, CompileOptions::default()).unwrap();
        compile(LINEAR_MATCH_EXPR_STATEFUL_ARMS_PROGRAM, CompileOptions::default()).unwrap();

        let moved_err = compile(LINEAR_MATCH_EXPR_INCONSISTENT_LET_MOVE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            moved_err.message.contains("linear resource 'left' has inconsistent ownership state across match arms")
                || moved_err.message.contains("linear resource 'right' has inconsistent ownership state across match arms"),
            "unexpected error: {}",
            moved_err.message
        );

        let stateful_err = compile(LINEAR_MATCH_EXPR_INCONSISTENT_STATEFUL_ARMS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            stateful_err.message.contains("linear resource 'token' has inconsistent ownership state across match arms"),
            "unexpected error: {}",
            stateful_err.message
        );
    }

    #[test]
    fn optimization_preserves_checked_u128_constant_overflow_paths() {
        let source = r#"
module u128_constant_overflow

action verify() -> u64 {
    verification
        let add_overflow: u128 = 340282366920938463463374607431768211455 + 340282366920938463463374607431768211455
        let sub_overflow: u128 = 18446744073709551616 - 340282366920938463463374607431768211455
        let mul_overflow: u128 = 340282366920938463463374607431768211455 * 340282366920938463463374607431768211455
        require add_overflow == 0
        require sub_overflow == 0
        require mul_overflow == 0
        return 0
}
"#;

        let mut trap_counts = Vec::new();
        for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let result = compile(source, CompileOptions { diagnostics, ..CompileOptions::default() }).unwrap();
            let asm = String::from_utf8(result.artifact_bytes).unwrap();
            // Fatal handlers are shared now. Count the retained overflow paths,
            // not repeated copies of the same numeric-error constant.
            let trap_count = asm.matches("j .Lverify_fail_49").count();
            assert!(trap_count >= 3, "-{diagnostics} must retain all three checked u128 overflow paths:\n{asm}");
            assert!(asm.contains("li a0, 49\n    j __cellscript_abort"));
            trap_counts.push(trap_count);
        }
        assert!(trap_counts.iter().all(|count| *count == trap_counts[0]), "optimization must not erase checked u128 overflow traps");
    }

    #[test]
    fn compile_lowers_u128_mutate_delta_with_carry_arithmetic() {
        let result = compile(U128_MUTATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: u128 add with carry"),
            "u128 mutate expression should lower through the 128-bit carry path:\n{}",
            asm
        );
        assert!(
            !asm.contains("unsupported u128 operand shape"),
            "u128 mutate expression should not fail through generic u128 handling:\n{}",
            asm
        );
    }

    #[test]
    fn compile_rejects_retired_effect_declarations_on_lifecycle_helpers() {
        let source = INDIRECT_IMPURE_FN_PROGRAM.replace("fn helper", "#[effect(Pure)]\nfn helper");
        let err = compile(&source, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("effect declarations have been removed"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_accepts_create_field_shorthand() {
        let source = r#"
module cellscript::shorthand

resource Token has store {
    amount: u64
    symbol: [u8; 8]
}

action mint(amount: u64, symbol: [u8; 8]) -> Token {
    verification
        create Token {
            amount,
            symbol,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.actions.len(), 1);
        let action = &result.metadata.actions[0];
        assert_eq!(action.create_set.len(), 1);
        assert_eq!(action.create_set[0].fields, vec!["amount".to_string(), "symbol".to_string()]);
    }

    #[test]
    fn compile_tracks_linear_values_returned_from_complete_branches() {
        compile(LINEAR_BRANCH_RETURN_PROGRAM, CompileOptions::default()).unwrap();

        let err = compile(LINEAR_BRANCH_INCONSISTENT_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("linear resource 'token' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_tracks_linear_values_returned_from_tail_if_branches() {
        compile(LINEAR_TAIL_IF_RETURN_PROGRAM, CompileOptions::default()).unwrap();

        let err = compile(LINEAR_TAIL_IF_INCONSISTENT_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("linear resource 'left' has inconsistent ownership state across if branches")
                || err.message.contains("linear resource 'right' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_merges_linear_transfers_inside_if_expressions() {
        compile(LINEAR_IF_EXPR_LET_MOVE_PROGRAM, CompileOptions::default()).unwrap();
        compile(LINEAR_IF_EXPR_STATEFUL_BRANCHES_PROGRAM, CompileOptions::default()).unwrap();

        let err = compile(LINEAR_IF_EXPR_INCONSISTENT_LET_MOVE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("linear resource 'left' has inconsistent ownership state across if branches")
                || err.message.contains("linear resource 'right' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_tracks_linear_values_inside_aggregate_bindings() {
        compile(LINEAR_TUPLE_DESTRUCTURE_HANDLES_ITEMS_PROGRAM, CompileOptions::default()).unwrap();

        let tuple_err = compile(LINEAR_TUPLE_BINDING_DROPPED_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(tuple_err.message.contains("linear resource 'pair' was not consumed"), "unexpected error: {}", tuple_err.message);

        let array_err = compile(LINEAR_ARRAY_BINDING_DROPPED_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(array_err.message.contains("linear resource 'items' was not consumed"), "unexpected error: {}", array_err.message);

        let wildcard_err = compile(LINEAR_WILDCARD_DISCARD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            wildcard_err.message.contains("wildcard binding cannot discard a linear value"),
            "unexpected error: {}",
            wildcard_err.message
        );

        let tuple_wildcard_err = compile(LINEAR_TUPLE_WILDCARD_DISCARD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            tuple_wildcard_err.message.contains("wildcard binding cannot discard a linear value"),
            "unexpected error: {}",
            tuple_wildcard_err.message
        );

        let field_err = compile(LINEAR_TUPLE_FIELD_PROJECTION_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            field_err.message.contains("field access cannot move a linear value out of an aggregate"),
            "unexpected error: {}",
            field_err.message
        );

        let index_err = compile(LINEAR_ARRAY_INDEX_PROJECTION_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            index_err.message.contains("index access cannot move a linear value out of an aggregate"),
            "unexpected error: {}",
            index_err.message
        );
    }

    #[test]
    fn compile_merges_linear_transfers_inside_block_expressions() {
        compile(LINEAR_BLOCK_EXPR_LET_MOVE_PROGRAM, CompileOptions::default()).unwrap();
        compile(LINEAR_BLOCK_EXPR_PREFIX_MOVE_PROGRAM, CompileOptions::default()).unwrap();
        compile(LINEAR_BLOCK_EXPR_STATEFUL_PROGRAM, CompileOptions::default()).unwrap();

        let err = compile(LINEAR_BLOCK_EXPR_DROPPED_LOCAL_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("linear resource 'out' was not consumed"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_merges_linear_transfers_inside_block_tail_if_expressions() {
        compile(LINEAR_BLOCK_TAIL_IF_MOVE_PROGRAM, CompileOptions::default()).unwrap();

        let err = compile(LINEAR_BLOCK_TAIL_IF_INCONSISTENT_MOVE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("linear resource 'left' has inconsistent ownership state across if branches")
                || err.message.contains("linear resource 'right' has inconsistent ownership state across if branches"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_noop_flow_transition_on_main_path() {
        let err = compile(STATE_MACHINE_NOOP_TRANSITION_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("flow must mention at least two states"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_missing_flow_state_create_on_main_path() {
        let err = compile(FLOW_MISSING_STATE_CREATE_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("create for 'Ticket' is missing field(s): state"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_flow_receipt_without_state_field() {
        let err = compile(FLOW_MISSING_STATE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("flow target field 'Ticket.state' is not defined"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_bad_flow_state_field_type_on_main_path() {
        let err = compile(FLOW_BAD_STATE_TYPE_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("flow field 'Ticket.state' must be an unsigned integer or no-payload enum"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_out_of_range_flow_state_create_on_main_path() {
        let err = compile(FLOW_OUT_OF_RANGE_STATE_CREATE_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("flow state index 2 is out of range for 'Ticket' with 2 states"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_accepts_non_initial_flow_create_without_consumed_prior_state() {
        compile(FLOW_NON_INITIAL_CREATE_PROGRAM, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_rejects_dynamic_initial_flow_create_state() {
        let err = compile(FLOW_DYNAMIC_INITIAL_CREATE_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("initial create of flow type 'Ticket' must use a statically known declared state"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_allows_flow_update_to_declared_initial_state_at_type_check() {
        compile(FLOW_RESET_UPDATE_PROGRAM, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_accepts_flow_state_name_initializers() {
        let result = compile(FLOW_INITIAL_STATE_NAME_CREATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(
            asm.contains("# cellscript abi: verify output field Ticket.state offset=0 size=1"),
            "state-name initializer should still verify the explicit state field:\n{}",
            asm
        );
    }

    #[test]
    fn compile_accepts_qualified_flow_state_names() {
        let result = compile(FLOW_QUALIFIED_STATE_NAME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "activate").expect("activate metadata");

        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Ticket.state" && obligation.status == "checked-runtime"
        }));
        assert!(asm.contains("state_count=2"), "qualified flow state names should preserve verifier metadata:\n{}", asm);
    }

    #[test]
    fn compile_rejects_wrong_qualified_flow_state_field_initializer() {
        let err = compile(FLOW_WRONG_QUALIFIED_STATE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("flow field 'Ticket.state' cannot be initialized with 'OtherTicket::Live'"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_accepts_static_flow_update_to_non_initial_state() {
        let result = compile(FLOW_STATIC_UPDATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let ticket = result.metadata.types.iter().find(|ty| ty.name == "Ticket").expect("Ticket type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "activate").expect("activate metadata");

        assert_eq!(result.metadata.module, "test");
        assert_eq!(ticket.flow_states, vec!["Created".to_string(), "Active".to_string()]);
        assert_eq!(ticket.flow_transitions.len(), 1);
        assert_eq!(ticket.flow_transitions[0].from, "Created");
        assert_eq!(ticket.flow_transitions[0].to, "Active");
        assert_eq!(ticket.flow_transitions[0].from_index, 0);
        assert_eq!(ticket.flow_transitions[0].to_index, 1);
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Ticket.state" && obligation.status == "checked-runtime"
        }));
        assert!(result.metadata.runtime.verifier_obligations.iter().any(|obligation| {
            obligation.scope == "action:activate"
                && obligation.category == "state-transition"
                && obligation.feature == "Ticket.state"
                && obligation.status == "checked-runtime"
        }));
        assert!(
            asm.contains("# cellscript abi: state transition Ticket.state state_count=2"),
            "missing flow runtime transition verifier:\n{}",
            asm
        );
        assert!(asm.contains("li a0, 7"), "missing state transition failure code in verifier:\n{}", asm);
        assert!(asm.contains("state_count=2"), "missing state-count marker in verifier:\n{}", asm);
        assert!(asm.contains("li a0, 9"), "missing old-state range failure code:\n{}", asm);
        assert!(asm.contains("li t3, 2"), "missing output state range check:\n{}", asm);
        assert!(asm.contains("li a0, 8"), "missing output state range failure code:\n{}", asm);
    }

    #[test]
    fn compile_accepts_explicit_flow_self_loop_for_repeatable_updates() {
        let result = compile(FLOW_SELF_LOOP_UPDATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let ticket = result.metadata.types.iter().find(|ty| ty.name == "Ticket").expect("Ticket type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "claim").expect("claim metadata");
        let layout =
            result.metadata.template_layouts.iter().find(|layout| layout.type_name == "Ticket").expect("Ticket template layout");

        assert!(ticket.flow_transitions.iter().any(|transition| {
            transition.from == "Active" && transition.to == "Active" && transition.from_index == 0 && transition.to_index == 0
        }));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Ticket.state" && obligation.status == "checked-runtime"
        }));
        assert!(!layout.state_machine_acyclic);
        assert_eq!(layout.cycle_policy, "RootRequired");
        assert!(asm.contains("# cellscript abi: state transition Ticket.state state_count=2"));
        assert!(asm.contains("li a0, 7"), "missing self-loop transition failure code:\n{}", asm);
    }

    #[test]
    fn compile_accepts_explicit_flow_action_edges() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow OfferFlow for Offer.state {
    initial Created;
    terminal Filled, Cancelled;

    Created -> Live;
    Live -> Filled by accept;
    Live -> Cancelled;
}

action accept(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        let amount = input.amount
        create output = Offer {
            state: OfferState::Filled,
            amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let offer = result.metadata.types.iter().find(|ty| ty.name == "Offer").expect("Offer type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "accept").expect("accept metadata");

        assert_eq!(offer.flow_states, vec!["Created", "Live", "Filled", "Cancelled"]);
        assert_eq!(offer.flow_state_field.as_deref(), Some("state"));
        assert_eq!(offer.flow_initial_state.as_deref(), Some("Created"));
        assert_eq!(offer.flow_terminal_states, vec!["Filled", "Cancelled"]);
        assert_eq!(offer.flow_terminal_discharge.as_deref(), Some("terminal-by-output-state"));
        assert_eq!(offer.flow_terminal_evidence_tier, Some(crate::EvidenceTier::CheckedRuntime));
        assert_eq!(offer.flow_state_model.as_deref(), Some("enum-backed"));
        assert!(offer.flow_audit_warnings.is_empty());
        assert!(offer.flow_transitions.iter().any(|transition| transition.from == "Live" && transition.to == "Filled"));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Offer.state" && obligation.status == "checked-runtime"
        }));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "flow-terminal"
                && obligation.feature == "terminal-by-output-state"
                && obligation.status == "checked-runtime"
        }));
        assert!(action.proof_plan.iter().any(|plan| {
            plan.category == "flow-terminal"
                && plan.feature == "terminal-by-output-state"
                && plan.evidence_tier == crate::EvidenceTier::CheckedRuntime
                && plan.on_chain_checked
        }));
        assert!(
            asm.contains("# cellscript abi: state transition Offer.state state_count=4"),
            "missing explicit flow runtime transition marker:\n{}",
            asm
        );
        assert!(asm.contains("li t3, 1"), "action transition should check source state Live=1:\n{}", asm);
        assert!(asm.contains("li t3, 2"), "action transition should check target state Filled=2:\n{}", asm);
    }

    #[test]
    fn compile_rejects_invalid_terminal_flow_contracts() {
        let missing_initial = r#"
module test

enum State { Start, Done }
resource Ticket { state: State }
flow Ticket.state {
    terminal Done;
    Start -> Done;
}
"#;
        let error = compile(missing_initial, CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("must declare exactly one initial state; found 0"), "unexpected error: {}", error.message);

        let overlap = missing_initial.replace("terminal Done;", "initial Done;\n    terminal Done;");
        let error = compile(&overlap, CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("cannot be both initial and terminal"), "unexpected error: {}", error.message);

        let outgoing = missing_initial
            .replace("terminal Done;", "initial Start;\n    terminal Done;")
            .replace("Start -> Done;", "Start -> Done;\n    Done -> Start;");
        let error = compile(&outgoing, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("terminal state 'Done' cannot have an outgoing transition"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn compile_accepts_core_input_output_state_transition_edges() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled;
    Live -> Cancelled;
}

action accept(input input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        require input.amount == output.amount
        create output = Offer {
            state: OfferState::Filled,
            amount: input.amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "accept").expect("accept metadata");

        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "input"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "output"));
        assert!(action.params.iter().any(|param| param.name == "input" && param.source == "input"));
        assert!(action.params.iter().any(|param| param.name == "output" && param.source == "output"));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Offer.state" && obligation.status == "checked-runtime"
        }));
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=input source=Input index=0"),
            "core input parameter should bind deterministically to Input#0:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_DATA reason=output_param source=Output index=0"),
            "core output parameter should bind deterministically to Output#0:\n{}",
            asm
        );
        assert!(asm.contains("li t3, 0"), "core transition should check source state Live=0:\n{}", asm);
        assert!(asm.contains("li t3, 1"), "core transition should check target state Filled=1:\n{}", asm);
    }

    #[test]
    fn compile_rejects_core_state_transition_edge_not_in_graph() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action cancel(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Cancelled
    verification
        require input.amount == output.amount
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("is not declared"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_duplicate_flow_for_same_state_field() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow OfferFlow for Offer.state {
    Created -> Live;
}

flow Offer.state {
    Live -> Filled;
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate flow for 'Offer.state'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_reports_equivalent_state_transition_obligation_for_sugar_and_core_forms() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action accept_sugar(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        let amount = input.amount
        create output = Offer {
            state: OfferState::Filled,
            amount,
        }

}
action accept_core(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        require input.amount == output.amount
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let sugar = result.metadata.actions.iter().find(|action| action.name == "accept_sugar").expect("sugar metadata");
        let core = result.metadata.actions.iter().find(|action| action.name == "accept_core").expect("core metadata");
        let sugar_obligations = sugar
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.category == "state-transition" && obligation.feature == "Offer.state")
            .count();
        let core_obligations = core
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.category == "state-transition" && obligation.feature == "Offer.state")
            .count();

        assert_eq!(sugar_obligations, 1);
        assert_eq!(core_obligations, sugar_obligations);
        assert!(sugar.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.ty == "Offer"));
        assert!(core.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "output"));
    }

    #[test]
    fn compile_accepts_named_action_output_and_create_binding() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled by accept;
}

action accept(old: Offer) -> new: Offer {
    transition old.state: Live -> new.state: Filled
    verification
        create new = Offer {
            state: OfferState::Filled,
            amount: old.amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "accept").expect("accept metadata");
        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "old"));
        assert!(action.create_set.iter().any(|pattern| {
            pattern.operation == "output"
                && pattern.ty == "Offer"
                && pattern.binding == "new"
                && pattern.fields == vec!["state".to_string(), "amount".to_string()]
        }));
        assert!(action.params.iter().any(|param| param.name == "new" && param.source == "output"));
    }

    #[test]
    fn named_action_output_create_binding_reuses_declared_output_index() {
        let source = r#"
module test

resource Authority has store {
    minted: u64
}

resource Token has store {
    amount: u64
}

action mint(auth: Authority, amount: u64) -> (next_auth: Authority, token: Token) {
    verification
        require next_auth.minted == auth.minted + amount

        create token = Token {
            amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let assembly = String::from_utf8(result.artifact_bytes).expect("assembly is utf8");
        assert!(assembly.contains("LOAD_CELL_DATA reason=output_param source=Output index=0"));
        assert!(assembly.contains("LOAD_CELL_DATA reason=output_param source=Output index=1"));
        assert!(
            !assembly.contains("source=Output index=2"),
            "named output create must constrain its declared Output index instead of allocating a new output:\n{}",
            assembly
        );
        assert!(assembly.contains("# constrain named output Token"));
    }

    #[test]
    fn compile_rejects_flow_by_action_when_explicit_move_uses_different_edge() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled by accept;
    Live -> Cancelled;
}

action accept(old: Offer) -> new: Offer {
    transition old.state: Live -> new.state: Cancelled
    verification
            require old.amount == new.amount
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("must declare the exact field-to-field transition"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_accepts_flow_on_custom_state_field() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    status: OfferState
    amount: u64
}

flow OfferFlow for Offer.status {
    Created -> Live;
    Live -> Filled by accept;
}

action accept(input: Offer) -> output: Offer {
    transition input.status: Live -> output.status: Filled
    verification
        let amount = input.amount
        create output = Offer {
            status: OfferState::Filled,
            amount,
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let offer = result.metadata.types.iter().find(|ty| ty.name == "Offer").expect("Offer type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "accept").expect("accept metadata");

        assert_eq!(offer.flow_state_field.as_deref(), Some("status"));
        assert!(action.verifier_obligations.iter().any(|obligation| {
            obligation.category == "state-transition" && obligation.feature == "Offer.status" && obligation.status == "checked-runtime"
        }));
        assert!(
            asm.contains("# cellscript abi: state transition Offer.status state_count=3"),
            "custom state field should be used by runtime verifier:\n{}",
            asm
        );
    }

    #[test]
    fn compile_accepts_flow_initial_create_at_any_declared_state() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Created -> Live;
    Live -> Filled;
}

action seed_live() -> Offer {
    verification
        create Offer {
            state: OfferState::Live,
            amount: 1,
        }
}
"#;
        compile(source, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_accepts_flow_edge_returning_to_first_state() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Created;
}

action reset(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Created
    verification
        let amount = input.amount
        create output = Offer {
            state: OfferState::Created,
            amount,
        }
}
"#;
        compile(source, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_rejects_state_edge_that_does_not_consume_binding() {
        let source = r#"
module test

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action accept(input: &Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        create output = Offer {
            state: OfferState::Filled,
            amount: input.amount,
        }
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("cannot use bare `&T` at the verifier boundary"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_flow_by_action_without_exact_move_clause() {
        let source = r#"
module test

enum OfferState {
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    amount: u64
}

flow OfferFlow for Offer.state {
    Live -> Filled by accept;
    Live -> Cancelled;
}

action accept(input: Offer) {
    verification
        let amount = input.amount
        consume input
        create Offer {
            state: OfferState::Filled,
            amount,
        }
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("must declare the exact field-to-field transition"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_flow_on_plain_struct() {
        let source = r#"
module test

struct Offer {
    state: u8
    amount: u64
}

flow Offer.state {
    Created -> Live;
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("must be a resource, shared, or receipt Cell type"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ir_carries_flow_rules() {
        let tokens = lexer::lex(FLOW_STATIC_UPDATE_PROGRAM).unwrap();
        let ast = parser::parse(&tokens).unwrap();
        let module = ir::generate(&ast).unwrap();
        let ticket = module
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::TypeDef(type_def) if type_def.name == "Ticket" => Some(type_def),
                _ => None,
            })
            .expect("Ticket IR type");

        assert_eq!(ticket.flow_rules.len(), 1);
        assert_eq!(ticket.flow_rules[0].from, "Created");
        assert_eq!(ticket.flow_rules[0].to, "Active");
        assert_eq!(ticket.flow_rules[0].from_index, 0);
        assert_eq!(ticket.flow_rules[0].to_index, 1);
    }

    #[test]
    fn compile_rejects_state_transitions_inside_locks() {
        let err = compile(LOCK_CREATE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("lock cannot contain 'create' Cell state transition"), "unexpected error: {}", err.message);

        let err = compile(LOCK_DESTROY_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("lock cannot contain 'destroy' Cell state transition"), "unexpected error: {}", err.message);

        compile(LOCK_READ_REF_PROGRAM, CompileOptions::default()).unwrap();
    }

    #[test]
    fn compile_fungible_type_group_entry_is_structural_payload_free_and_checked() {
        let source = r#"
module test

invariant conserved_units {
    trigger: type_group
    scope: group
    reads: group_inputs<Coin>.units, group_outputs<Coin>.units
    assert_sum(group_outputs<Coin>.units) == assert_sum(group_inputs<Coin>.units)
}

resource Coin {
    units: u128,
}

action business_transfer(coin: Coin, to: Address) -> next: Coin {
    verification
    consume coin
    create next = Coin { units: coin.units } with_lock(to)
}
"#;

        let result = compile_fungible_type_group_entry(
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
        let contract = result.metadata.runtime.fungible_type_group_entry.as_ref().expect("structured fungible entry contract");

        assert_eq!(contract.contract, "fungible-type-group-v1");
        assert_eq!(contract.invariant, "conserved_units");
        assert_eq!(contract.type_name, "Coin");
        assert_eq!(contract.field, "units");
        assert_eq!(contract.data_length_bytes, 16);
        assert!(!contract.payload_required);
        assert_eq!(contract.owner_mode, "script-args-32-byte-owner-lock-hash");
        assert_eq!(contract.owner_args_length_bytes, 32);
        assert_eq!(contract.authority_modes, ["input-lock-hash", "tagged-input-type-script-hash"]);
        assert_eq!(contract.authority_args_lengths_bytes, [32, 33]);
        assert!(contract.owner_authorized_mint);
        assert!(contract.owner_authorized_burn);
        assert!(contract.non_owner_input_group_non_empty);
        assert!(contract.non_owner_output_group_non_empty);
        assert!(contract.non_owner_conservation_required);
        assert_eq!(result.metadata.actions.len(), 1);
        assert_eq!(result.metadata.actions[0].name, "__cellscript_fungible_type_group_v1");
        assert!(result.metadata.actions[0].params.is_empty());
        assert!(result.metadata.actions[0].consume_set.is_empty());
        assert!(result.metadata.actions[0].create_set.is_empty());
        assert!(asm.contains("__cellscript_fungible_type_group_v1:"), "{asm}");
        assert!(asm.contains("call __cellscript_require_fungible_type_group_v1"), "{asm}");
        assert!(asm.contains("32-byte input lock hash or 0x01-tagged 32-byte input Type Script hash"), "{asm}");
        assert!(asm.contains("beqz t3, .Lxudt_group_mismatch"), "non-empty group check missing:\n{asm}");
        assert!(!asm.contains("business_transfer:"), "ordinary business action leaked into invariant artifact:\n{asm}");
        assert!(result.metadata.runtime.proof_plan.iter().any(|plan| {
            plan.origin == "invariant:conserved_units" && plan.status == "checked-runtime" && plan.codegen_coverage_status == "covered"
        }));
    }

    #[test]
    fn compile_fungible_type_group_entry_rejects_ambiguous_or_wide_assets() {
        let ambiguous = r#"
module test

invariant first_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<First>.value, group_outputs<First>.value
    assert_sum(group_outputs<First>.value) == assert_sum(group_inputs<First>.value)
}

invariant second_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Second>.quantity, group_outputs<Second>.quantity
    assert_sum(group_outputs<Second>.quantity) == assert_sum(group_inputs<Second>.quantity)
}

resource First { value: u128 }
resource Second { quantity: u128 }
"#;
        let err = compile_fungible_type_group_entry(ambiguous, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("2 eligible invariants"), "{}", err.message);

        let wide = r#"
module test

invariant wide_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Wide>.value, group_outputs<Wide>.value
    assert_sum(group_outputs<Wide>.value) == assert_sum(group_inputs<Wide>.value)
}

resource Wide {
    value: u128,
    tag: u64,
}
"#;
        let err = compile_fungible_type_group_entry(wide, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("no eligible invariant"), "{}", err.message);
    }

    #[test]
    fn create_output_verifier_accepts_fixed_byte_params_and_consts() {
        let result = compile(FIXED_BYTE_PARAM_AND_CONST_OUTPUT_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "fixed-byte parameter and constant output fields should be verifier-coverable: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        assert!(
            asm.contains("# cellscript abi: verify output bytes field Config.symbol offset=0 size=8 against stack slot"),
            "fixed-byte stack parameter verification was not emitted:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output bytes field Fingerprint.digest offset=0 size=32 against const"),
            "fixed-byte constant verification was not emitted:\n{}",
            asm
        );

        for action_name in ["make_config", "make_fingerprint"] {
            let action = result.metadata.actions.iter().find(|action| action.name == action_name).expect("action metadata");
            assert!(
                !action.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
                "{} should not expose incomplete output verification for fixed-byte parameter/constant fields: {:?}",
                action_name,
                action.fail_closed_runtime_features
            );
        }
    }

    #[test]
    fn create_output_verifier_accepts_const_lock_hash() {
        let result = compile(CONST_LOCK_OUTPUT_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "constant lock create should keep output fields verifier-coverable: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        assert!(
            !result.metadata.runtime.fail_closed_runtime_features.contains(&"output-lock-verification-incomplete".to_string()),
            "constant lock create should verify output lock hash instead of reporting a lock obligation: {:?}",
            result.metadata.runtime.fail_closed_runtime_features
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL_BY_FIELD reason=output_lock_hash source=Output index=0 field=3"),
            "output lock hash syscall was not emitted:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output lock hash offset=0 size=32"),
            "output lock hash byte verification was not emitted:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript output abi: lock verification incomplete for this create pattern"),
            "constant output lock should not fail closed as incomplete:\n{}",
            asm
        );
    }

    #[test]
    fn compile_rejects_destroy_without_destroy_capability() {
        let err = compile(MISSING_DESTROY_CAPABILITY_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("required [consume, burn]"), "unexpected error: {}", err.message);
        assert!(err.message.contains("missing [consume, burn]"), "unexpected error: {}", err.message);
        assert!(err.message.contains("capability-set version 1"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_accepts_kernel_effect_capabilities_for_destroy() {
        let source = r#"
module test

resource Token has store, consume, burn {
    amount: u64,
}

action burn(token: Token) {
    verification
        destroy token
}
"#;

        let result = compile(source, CompileOptions { primitive_compat: Some("0.15".to_string()), ..Default::default() }).unwrap();
        assert_eq!(result.metadata.capability_registry.capabilities, crate::ast::Capability::canonical_names());
        assert_eq!(result.metadata.capability_registry.capability_set_version, crate::ast::Capability::REGISTRY_VERSION);
        assert_eq!(result.metadata.types[0].capability_set_version, crate::ast::Capability::REGISTRY_VERSION);
        let proof = result.metadata.runtime.capability_proofs.first().expect("destroy capability proof");
        assert_eq!(proof.operation, "destroy");
        assert_eq!(proof.required, ["consume", "burn"]);
        assert_eq!(proof.provided, ["store", "consume", "burn"]);
        assert_eq!(proof.entailed, ["consume", "burn"]);
        assert!(proof.missing.is_empty());
        assert_eq!(proof.capability_set_version, crate::ast::Capability::REGISTRY_VERSION);
        assert_eq!(proof.entailment_version, crate::ast::CapabilityOperation::ENTAILMENT_VERSION);
    }

    #[test]
    fn compile_rejects_replace_unique_without_capability_and_identity_authority() {
        let source = r#"
module test

resource Token has store {
    amount: u64,
}

action rotate(old: Token) -> Token {
    verification
        replace_unique<Token>(identity = ckb_type_id) old { amount: old.amount }
}
"#;

        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("required [replace, identity-preservation]"), "unexpected error: {}", err.message);
        assert!(err.message.contains("missing [replace, identity(ckb_type_id)]"), "unexpected error: {}", err.message);
        assert!(err.message.contains("provided [store]"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_does_not_inherit_container_capability_authority() {
        let source = r#"
module test

resource Token has store {
    amount: u64,
}

resource Container has consume, burn {
    token_id: u64,
}

action bad(token: Token, container: Container) {
    verification
        destroy token
        destroy container
}
"#;

        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("operation 'destroy' on type 'Token'"), "unexpected error: {}", err.message);
        assert!(err.message.contains("provided [store]"), "unexpected error: {}", err.message);
        assert!(err.message.contains("missing [consume, burn]"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_accepts_kernel_effect_capabilities_for_transfer() {
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

        compile(source, CompileOptions { primitive_compat: Some("0.15".to_string()), ..Default::default() }).unwrap();
    }

    #[test]
    fn u128_mutable_state_transition_with_u64_delta_is_checked() {
        let source = r#"
module test

shared Ledger has store {
    balance: u128,
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

        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "ledger_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "ledger_after"));
        assert!(!action.transaction_runtime_input_requirements.iter().any(|requirement| {
            requirement.feature == "shared-mutation:Ledger" && requirement.component == "mutate-field-transition"
        }));
    }

    #[test]
    fn anonymous_creates_after_named_outputs_get_distinct_output_indices() {
        let source = r#"
module test

resource Authority has store {
    symbol: [u8; 8],
    minted: u64,
}

resource Token has store {
    amount: u64,
    symbol: [u8; 8],
}

shared Pool has store {
    symbol: [u8; 8],
    reserve: u64,
}

receipt Receipt has store {
    pool_id: Hash,
    amount: u64,
}

action launch(symbol: [u8; 8], seed: Token, creator: Address, recipients: [(Address, u64); 2]) -> (auth: Authority, pool: Pool, receipt: Receipt) {
    verification
        create auth = Authority { symbol, minted: seed.amount }
        if recipients[0].1 > 0 {
            create Token { amount: recipients[0].1, symbol } with_lock(recipients[0].0)
        }
        if recipients[1].1 > 0 {
            create Token { amount: recipients[1].1, symbol } with_lock(recipients[1].0)
        }
        consume seed
        create pool = Pool { symbol, reserve: seed.amount }
        create receipt = Receipt { pool_id: pool.type_hash(), amount: seed.amount } with_lock(creator)
}
"#;

        let result = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(
            asm.contains("LOAD_CELL_DATA reason=create source=Output index=3")
                && asm.contains("LOAD_CELL_DATA reason=create source=Output index=4"),
            "distinct anonymous creates after three named outputs must bind Output#3 and Output#4:\n{}",
            asm
        );
        assert_eq!(
            asm.matches("LOAD_CELL_DATA reason=create source=Output index=3").count(),
            1,
            "anonymous creates must not all collapse to the first anonymous output index:\n{}",
            asm
        );
        assert!(
            asm.contains("LOAD_CELL_BY_FIELD reason=output_type_hash source=Output index=1 field=5"),
            "type_hash() of named output `pool` must read the signature-bound Pool output index:\n{}",
            asm
        );
    }

    #[test]
    fn compile_create_unique_field_identity_emits_runtime_anchor() {
        let program = r#"
module audit::create_unique_field_runtime

resource NFT has store
    identity(field(token_id))
{
    token_id: u64,
    owner: Address
}

action mint(token_id: u64, owner: Address) -> NFT {
    verification
        create_unique<NFT>(identity = field(token_id)) { token_id: token_id, owner: owner }
}
"#;

        let result =
            compile(program, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        let create = result.metadata.actions[0].create_set.first().expect("create_unique metadata");

        assert_eq!(create.operation, "create_unique");
        assert_eq!(create.identity_policy.as_deref(), Some("field(token_id)"));
        assert!(create.ckb_output_data.is_some(), "create_unique output must carry CKB output-data index evidence");
        assert!(
            result
                .metadata
                .runtime
                .verifier_obligations
                .iter()
                .any(|obligation| obligation.feature == "create-unique-identity:NFT:field(token_id)"
                    && obligation.detail.contains("global field uniqueness remains")),
            "missing create_unique field identity proof obligation"
        );
        assert!(asm.contains("create_unique identity policy field(token_id)"), "missing identity policy runtime marker:\n{}", asm);
        assert!(asm.contains("create_unique field identity anchored"), "missing field identity output anchor:\n{}", asm);
    }

    #[test]
    fn bounded_cell_consume_each_has_a_checked_type_group_runtime_contract() {
        let source = r#"
module bounded::consume_runtime

resource Token has store, consume { amount: u64 }

action verify(input inputs: BoundedCellSet<Token, 2>, witness minimum: u64) -> u64 {
    verification
        consume_each token in inputs {
            require token.amount >= minimum
        }
        return 0
}
"#;
        let result = compile_with_executable_surface_policy(
            source,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();

        let action = result.metadata.actions.iter().find(|action| action.name == "verify").unwrap();
        assert!(action.fail_closed_runtime_features.is_empty());
        assert!(action.ckb_runtime_features.contains(&"ckb-bounded-type-group-inputs-v1".to_string()));
        assert!(action
            .ckb_runtime_accesses
            .iter()
            .any(|access| access.operation == "bounded-type-group-input-data" && access.source == "GroupInput"));
        let plan =
            action.proof_plan.iter().find(|plan| plan.feature.starts_with("consume_each:")).expect("bounded consume proof plan");
        assert_eq!(plan.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert!(plan.on_chain_checked);
        assert_eq!(plan.codegen_coverage_status, "covered");
        assert!(plan.coverage.contains(&"predicate-executed-once-per-element:token.amount >= minimum".to_string()));

        let entry = result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_name == "verify").expect("verify entry ABI");
        assert!(!entry.unsupported);
        assert_eq!(entry.params[0].abi_kind, crate::ir::BOUNDED_TYPE_GROUP_INPUTS_V1);
        assert_eq!(entry.params[0].witness_bytes, 0);

        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains("source=GroupInput element=Token max=2 width=8"));
        assert!(!asm.contains(crate::ir::BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER));
    }

    #[test]
    fn bounded_create_each_has_a_checked_output_plan_runtime_contract() {
        let source = r#"
module bounded::create_runtime

struct Plan {
    owner: Address
    amount: u64
}

resource Token has store, create
with_capacity_floor(10000000000)
{
    amount: u64
}

action mint(witness plans: BoundedList<Plan, 2>) -> u64 {
    verification
        create_each plan in plans {
            require plan.amount > 0
            create Token { amount: plan.amount } with_lock(plan.owner)
        }
        return 0
}
"#;
        let result = compile_with_executable_surface_policy(
            source,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "mint").unwrap();
        assert!(action.fail_closed_runtime_features.is_empty());
        assert!(action.ckb_runtime_features.contains(&"ckb-bounded-output-plan-v1".to_string()));
        let plan = action.proof_plan.iter().find(|plan| plan.feature.starts_with("create_each:")).expect("bounded create proof plan");
        assert_eq!(plan.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert!(plan.on_chain_checked);
        assert_eq!(plan.input_output_relation_checks, ["plan_count=group_output_count<=2"]);
        assert!(plan.builder_assumptions.is_empty());

        let entry = result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_name == "mint").expect("mint entry ABI");
        assert!(!entry.unsupported);
        assert_eq!(entry.params[0].abi_kind, crate::ir::BOUNDED_OUTPUT_PLAN_V1);
        assert_eq!(entry.params[0].witness_bytes, 4);

        let collection = result
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .find(|collection| collection.scope_name == "mint" && collection.ownership == "bounded-output-plan")
            .expect("bounded output collection metadata");
        let contract = collection.bounded_output_plan.as_ref().expect("explicit bounded output plan contract");
        assert_eq!(contract.binding, "plans");
        assert_eq!(contract.codec_magic_hex, "0x435342504c763100");
        assert_eq!(contract.element_width_bytes, 40);
        assert_eq!(contract.max_elements, 2);
        assert_eq!(contract.output_ty, "Token");
        assert_eq!(contract.output_width_bytes, 8);
        assert_eq!(contract.capacity_floor_shannons, 10_000_000_000);
        assert_eq!(contract.identity_policy, "fresh-output-outpoint-plus-type-group-ordinal");
        assert_eq!(contract.field_bindings.len(), 1);
        assert_eq!(contract.field_bindings[0].output_field, "amount");
        assert_eq!(contract.field_bindings[0].plan_field, "amount");
        assert_eq!(contract.field_bindings[0].plan_offset_bytes, 32);
        assert_eq!(contract.lock_binding.plan_field, "owner");
        assert_eq!(contract.lock_binding.width_bytes, 32);

        let mut element = [0_u8; 40].to_vec();
        element[32..40].copy_from_slice(&7_u64.to_le_bytes());
        let encoded_plan = encode_bounded_output_plan_v1(&[element], 40, 2).unwrap();
        assert_eq!(&encoded_plan[..8], crate::ir::BOUNDED_OUTPUT_PLAN_MAGIC_V1);
        assert_eq!(&encoded_plan[8..12], &1_u32.to_le_bytes());
        let action_metadata = result.metadata.actions.iter().find(|action| action.name == "mint").unwrap();
        let outer = action_metadata.entry_witness_args(&[EntryWitnessArg::Bytes(encoded_plan.clone())]).unwrap();
        assert_eq!(&outer[..8], ENTRY_WITNESS_ABI_MAGIC);
        assert_eq!(&outer[8..12], &(encoded_plan.len() as u32).to_le_bytes());

        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains("codec=MoleculeFixVec magic=CSBPLv1 element=Plan max=2 width=40"));
        assert!(asm.contains("require GroupOutput count equals plan count"));
        assert!(!asm.contains(crate::ir::BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER));
    }

    #[test]
    fn bounded_create_each_keeps_computed_output_fields_fail_closed() {
        let source = r#"
module bounded::computed_output

struct Plan { owner: Address, amount: u64 }
resource Token has store, create
with_capacity_floor(10000000000)
{ amount: u64 }

action mint(witness plans: BoundedList<Plan, 2>) -> u64 {
    verification
        create_each plan in plans {
            require plan.amount > 0
            create Token { amount: plan.amount + 1 } with_lock(plan.owner)
        }
        return 0
}
"#;
        let compiled =
            compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let collection = compiled
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .find(|collection| collection.scope_name == "mint" && collection.ownership == "bounded-output-plan")
            .expect("bounded output collection metadata");
        assert_eq!(collection.status, "builder-evidence-required");
        assert!(collection.bounded_output_plan.is_none());
        let asm = String::from_utf8(compiled.artifact_bytes).unwrap();
        assert!(asm.contains(crate::ir::BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER));

        let error = compile_with_executable_surface_policy(
            source,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2105"));
        assert!(error.message.contains("create_each:plans:BoundedList<Plan, 2>"));
    }

    #[test]
    fn type_validity_lowers_field_predicate_before_create() {
        let source = r#"
module validity::lowering

resource Token has store, create {
    amount: u64

    validity
        require amount > 0, "amount must be positive"
}

action mint(amount: u64) -> Token {
    verification
        return create Token { amount: amount }
}
"#;
        let ast = parse_module_for_test(source);
        crate::types::check(&ast).unwrap();
        let module = ir::generate(&ast).unwrap();
        let action = module
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "mint" => Some(action),
                _ => None,
            })
            .expect("mint IR action");
        let create_block = action
            .body
            .blocks
            .iter()
            .find(|block| block.instructions.iter().any(|instruction| matches!(instruction, ir::IrInstruction::Create { .. })))
            .expect("create instruction block");
        assert!(action.body.blocks.iter().any(|block| {
            matches!(block.terminator, ir::IrTerminator::Branch { then_block, .. } if then_block == create_block.id)
        }));
    }

    #[test]
    fn type_validity_marks_signature_only_create_and_update_paths_as_runtime_gaps() {
        let source = r#"
module validity::gaps

resource Pool has store {
    reserve: u64

    validity
        require reserve > 0
}

action update(input: Pool) -> output: Pool {
    verification
        require output.reserve == input.reserve
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let pool = result.metadata.types.iter().find(|ty| ty.name == "Pool").expect("Pool metadata");
        let predicate = pool.validity_predicates.first().expect("Pool validity predicate");
        assert_eq!(predicate.evidence_tier, crate::EvidenceTier::RuntimeHelperRequired);
        assert_eq!(predicate.create_path_status, "selected-runtime-helper-required");
        assert_eq!(predicate.update_path_status, "runtime-helper-required");
        assert!(!predicate.runtime_checked_on_create);
        assert_eq!(predicate.create_paths_selected, 1);
        assert_eq!(predicate.create_paths_checked, 0);
        assert_eq!(predicate.update_paths_selected, 1);

        let update_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "validity:Pool#predicate:0#update")
            .expect("update validity ProofPlan gap");
        assert_eq!(update_plan.evidence_tier, crate::EvidenceTier::RuntimeHelperRequired);
        assert!(!update_plan.on_chain_checked);
        assert_eq!(update_plan.codegen_coverage_status, "gap:runtime-helper-required");
    }

    #[test]
    fn type_validity_never_promotes_partial_create_path_coverage() {
        let source = r#"
module validity::partial

resource Pool has store, create {
    reserve: u64

    validity
        require reserve > 0
}

action mint(reserve: u64) -> Pool {
    verification
        return create Pool { reserve: reserve }
}

action update(input: Pool) -> output: Pool {
    verification
        require output.reserve == input.reserve
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let pool = result.metadata.types.iter().find(|ty| ty.name == "Pool").expect("Pool metadata");
        let predicate = pool.validity_predicates.first().expect("Pool validity predicate");
        assert_eq!(predicate.evidence_tier, crate::EvidenceTier::RuntimeHelperRequired);
        assert_eq!(predicate.create_path_status, "partial-runtime-helper-required");
        assert_eq!(predicate.create_paths_selected, 2);
        assert_eq!(predicate.create_paths_checked, 1);
        assert!(!predicate.runtime_checked_on_create);
        let plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.origin == "validity:Pool#predicate:0")
            .expect("partial validity ProofPlan");
        assert_eq!(plan.evidence_tier, crate::EvidenceTier::RuntimeHelperRequired);
        assert!(plan.coverage.contains(&"create_paths_selected:2".to_string()));
        assert!(plan.coverage.contains(&"create_paths_checked:1".to_string()));
        assert!(!plan.on_chain_checked);
    }
