// Compiler integration values behavior; retains crate::tests identities.

    #[test]
    fn compile_lowers_local_struct_field_reads_and_writes() {
        let result = compile(STRUCT_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".global tweak"), "missing tweak symbol:\n{}", asm);
        assert!(asm.contains("li t0, 1"), "missing initial x field constant:\n{}", asm);
        assert!(asm.contains("li t0, 2"), "missing initial y field constant:\n{}", asm);
        assert!(asm.contains("sd t0, 8(sp)"), "missing field x storage slot writes:\n{}", asm);
        assert!(
            asm.contains("materialize fixed aggregate Point"),
            "local struct updates must retain complete value storage:\n{}",
            asm
        );
        assert!(!asm.contains("fixed aggregate field source is unavailable"), "local struct field source was unresolved:\n{}", asm);
    }

    #[test]
    fn compile_rejects_assignment_through_read_only_references() {
        let err = compile(READ_REF_BINDING_FIELD_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("assignment target rooted at 'cfg' is a read-only reference"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(READ_ONLY_REF_FIELD_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("assignment target rooted at 'view' is a read-only reference"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_local_mutable_reference_aliases() {
        let err = compile(MUT_REF_LOCAL_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("signature-direction outputs") || err.message.contains("`&mut` Cell parameters are not valid"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(MUT_REF_TUPLE_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("signature-direction outputs") || err.message.contains("`&mut` Cell parameters are not valid"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(MUT_REF_IF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("signature-direction outputs") || err.message.contains("`&mut` Cell parameters are not valid"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(MUT_REF_ASSIGN_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("signature-direction outputs") || err.message.contains("`&mut` Cell parameters are not valid"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_local_references_to_linear_roots() {
        let err = compile(LINEAR_LOCAL_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_FIELD_LOCAL_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_TUPLE_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_ARRAY_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_IF_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_ASSIGN_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_ASSIGN_TUPLE_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LINEAR_ASSIGN_TUPLE_FIELD_REF_ALIAS_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("local binding cannot store a read-only reference rooted at linear/resource value 'token'"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_lowers_explicit_borrow_regions_without_materializing_a_view() {
        let source = r#"
module test;

resource Token has destroy {
    amount: u64,
}

fn amount_is(token: &Token, expected: u64) -> bool {
    return (token.amount == expected);
}

fn positive(token: &Token) -> bool {
    return (token.amount > 0);
}

action inspect(token: Token, expected: u64) -> u64 {

        borrow token as view {
            require amount_is(view, expected);
            require positive(view);
            require (view.amount > 0);
        }
        destroy token;
        return expected;
}
"#;

        let result = compile(source, CompileOptions::default()).unwrap();
        let region = result.metadata.runtime.borrow_regions.first().expect("borrow region metadata");
        assert_eq!(region.view_type, "View<Token>");
        assert_eq!(region.storage, "none");
        assert_eq!(region.abi, "none");
        assert_eq!(region.evidence_tier, crate::EvidenceTier::CheckedStatic);
        let plan =
            result.metadata.runtime.proof_plan.iter().find(|plan| plan.category == "borrow-region").expect("borrow ProofPlan entry");
        assert_eq!(plan.evidence_tier, crate::EvidenceTier::CheckedStatic);
        assert_eq!(plan.codegen_coverage_status, "covered");
        assert!(plan.coverage.contains(&"abi:none".to_string()));

        let assembly = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(assembly.contains("call amount_is"), "missing pure borrow helper call:\n{}", assembly);
        assert!(assembly.contains("call positive"), "missing read-only borrow helper call:\n{}", assembly);
        assert!(!assembly.contains("View<Token>"), "borrow marker leaked into runtime assembly:\n{}", assembly);
    }

    #[test]
    fn compile_checks_field_borrows_and_read_only_reborrows_against_the_linear_root() {
        let source = r#"
module test;

resource Token has destroy { amount: u64, }

action inspect(token: Token) -> u64 {

        borrow token.amount as amount_view {
            borrow amount_view as again {
                require *again > 0;
            }
        }
        destroy token;
        return 0;
}
"#;

        let result = compile(source, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.runtime.borrow_regions.len(), 2);
        let field = &result.metadata.runtime.borrow_regions[0];
        assert_eq!(field.root, "token");
        assert_eq!(field.path, ["amount"]);
        assert_eq!(field.view_type, "View<u64>");
        let reborrow = &result.metadata.runtime.borrow_regions[1];
        assert_eq!(reborrow.root, "token");
        assert_eq!(reborrow.path, ["amount"]);
        assert_eq!(reborrow.view_type, "View<u64>");

        let invalid = r#"
module test;
resource Token has destroy { amount: u64, }
action bad(token: Token) {

        borrow token.amount as amount_view {
            borrow amount_view as again {
                destroy token;
                require *again > 0;
            }
        }
}
"#;
        let error = compile(invalid, CompileOptions::default()).unwrap_err();
        assert!(error.message.contains("cannot cross destroy of linear root 'token'"), "unexpected error: {}", error.message);
    }

    #[test]
    fn generic_read_only_calls_preserve_cell_borrow_authority() {
        let source = r#"
module test;

resource Token has destroy { amount: u64, }

fn visible<T: cell>(value: &T) -> bool {
    return true;
}

action inspect(token: Token) -> u64 {

        borrow token as view {
            require visible::<Token>(view);
        }
        destroy token;
        return 0;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        assert!(result
            .metadata
            .generic_instantiations
            .iter()
            .any(|entry| entry.template == "visible" && entry.type_arguments == ["Token"]));
    }

    #[test]
    fn compile_rejects_borrow_regions_that_cross_root_lifecycle_operations() {
        let direct = r#"
module test;
resource Token has destroy { amount: u64, }
action bad(token: Token) {

        borrow token as view {
            require view.amount > 0;
            destroy token;
        }
}
"#;
        let error = compile(direct, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("borrow 'view' cannot cross destroy of linear root 'token'"),
            "unexpected error: {}",
            error.message
        );

        let branch = r#"
module test;
resource Token has destroy { amount: u64, }
action bad(token: Token, flag: bool) {

        borrow token as view {
            if flag {
                destroy token;
            }
            require view.amount > 0;
        }
}
"#;
        let error = compile(branch, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("borrow 'view' cannot cross destroy of linear root 'token'"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn compile_rejects_borrow_escape_through_storage_and_return() {
        let stored = r#"
module test;
resource Token has destroy { amount: u64, }
action bad(token: Token) {

        borrow token as view {
            let escaped = (view, 1);
        }
        destroy token;
}
"#;
        let error = compile(stored, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("borrow 'view' cannot escape borrow block rooted at linear value 'token'"),
            "unexpected error: {}",
            error.message
        );

        let returned = r#"
module test;
resource Token has destroy { amount: u64, }
action bad(token: Token) -> u64 {

        borrow token as view {
            return view;
        }
}
"#;
        let error = compile(returned, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("borrow 'view' cannot escape borrow block rooted at linear value 'token'"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn compile_checks_callee_effects_for_borrowed_views() {
        let source = r#"
module test;
resource Token has create, destroy { amount: u64, }
action change() {  let value = create Token { amount: 1, }; destroy value; }

fn incompatible(token: &Token) -> bool {
    change();
    return (token.amount > 0);
}

action bad(token: Token) {

        borrow token as view {
            require incompatible(view);
        }
        destroy token;
}
"#;
        let error = compile(source, CompileOptions::default()).unwrap_err();
        assert!(
            error.message.contains("cannot receive borrowed linear view unless it is observation-only")
                && error.message.contains("incompatible")
                && error.message.contains("lifecycle="),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn compile_rejects_reference_escape_boundaries() {
        let err = compile(ACTION_RETURN_REF_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("action 'leak' cannot return reference type &Token"), "unexpected error: {}", err.message);

        let err = compile(FUNCTION_RETURN_REF_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("function 'leak' cannot return reference type &Point"), "unexpected error: {}", err.message);

        let err = compile(FUNCTION_CELL_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("function 'bad' parameter 'token' cannot use owned cell-backed type Token"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(FUNCTION_CELL_RETURN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("function 'bad' cannot return cell-backed type Token"), "unexpected error: {}", err.message);

        let err = compile(LOCK_CELL_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("lock 'bad' parameter 'token' cannot use owned cell-backed type Token"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(LOCK_BARE_REF_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("lock 'bad' parameter 'token' cannot use bare `&T` at the verifier boundary"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(CALLABLE_REFERENCE_TO_CELL_AGGREGATE_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains(
                "parameter 'pair' in function 'bad' cannot use reference to aggregate containing cell-backed values &(Token, u64)"
            ),
            "unexpected error: {}",
            err.message
        );

        let err = compile(CALLABLE_MUT_REFERENCE_TO_CELL_AGGREGATE_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("`&mut` Cell parameters are not valid")
                || err.message.contains(
                    "parameter 'pool' in action 'bad' cannot use reference to aggregate containing cell-backed values &mut (Pool, u64)"
                ),
            "unexpected error: {}",
            err.message
        );

        let err = compile(FUNCTION_VEC_CELL_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message
                .contains("type 'Vec<Token>' cannot store a cell-backed resource; use a source-aware BoundedCellSet<T, N> with explicit ownership"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(STRUCT_VEC_REFERENCE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("cannot contain reference type"), "unexpected error: {}", err.message);

        let err = compile(VEC_PUSH_REFERENCE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("Vec.push cannot store reference type &Point"), "unexpected error: {}", err.message);

        let err = compile(CALLABLE_TUPLE_REFERENCE_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("parameter 'pair' in function 'bad' cannot contain nested reference type (&Point, u64)"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(CALLABLE_ARRAY_MUT_REFERENCE_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("parameter 'pools' in action 'bad' cannot contain nested reference type [&mut Pool; 1]"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(CALLABLE_NESTED_REFERENCE_PARAM_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("`read_ref` is not a type qualifier"), "unexpected error: {}", err.message);

        let err = compile(ACTION_REF_PARAM_MODIFIER_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("parameter modifier 'ref' is reserved but unsupported"), "unexpected error: {}", err.message);

        let err = compile(FUNCTION_REF_PARAM_MODIFIER_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("parameter modifier 'ref' is reserved but unsupported"), "unexpected error: {}", err.message);

        let err = compile(LOCK_REF_PARAM_MODIFIER_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("parameter modifier 'ref' is reserved but unsupported"), "unexpected error: {}", err.message);

        let err = compile(SCHEMA_REFERENCE_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("struct 'Holder' field 'view' cannot use reference type &Token"),
            "unexpected error: {}",
            err.message
        );

        let err = compile(ENUM_REFERENCE_PAYLOAD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("enum variant 'MaybeToken::Some' payload cannot use reference type &Token"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_unknown_struct_fields() {
        let err = compile(UNKNOWN_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown field 'y'"));
        assert!(err.message.contains("Point"));
    }

    #[test]
    fn compile_rejects_string_literals_as_runtime_values() {
        let err = compile(STRING_VALUE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("string literals are only supported in metadata positions"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_preserves_index_and_tuple_projection_in_assembly() {
        let result = compile(INDEXED_TUPLE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# index access"), "array indexing vanished from assembly:\n{}", asm);
        assert!(asm.contains("# field access .1"), "tuple projection vanished from assembly:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: fixed aggregate index element_offset=0 element_size=40"),
            "fixed tuple-array indexing did not lower through the pointer+length ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed aggregate field tuple.1 offset=32 size=8"),
            "fixed tuple projection did not lower through aggregate field access:\n{}",
            asm
        );
        assert!(!asm.contains("fail-closed runtime path"), "fixed tuple-array index/projection should not fail closed:\n{}", asm);
    }

    #[test]
    fn compile_unrolls_fixed_param_array_foreach_with_pointer_abi() {
        let result = compile(FOREACH_ARRAY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# length"), "fixed parameter foreach should be unrolled without dynamic length lowering:\n{}", asm);
        assert!(!asm.contains("j .Lblock_1"), "fixed parameter foreach should be unrolled without a loop back edge:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: fixed-aggregate param items pointer=a0 length=a1 size=24"),
            "fixed array parameter did not use pointer+length ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed aggregate index element_offset=0 element_size=8")
                && asm.contains("# cellscript abi: fixed aggregate index element_offset=8 element_size=8")
                && asm.contains("# cellscript abi: fixed aggregate index element_offset=16 element_size=8"),
            "fixed parameter foreach did not unroll into exact aggregate indexes:\n{}",
            asm
        );
        assert!(
            !asm.contains("index access fail-closed runtime path"),
            "fixed parameter foreach should not use fail-closed index lowering:\n{}",
            asm
        );
    }

    #[test]
    fn compile_unrolls_local_fixed_array_foreach_without_runtime_indexing() {
        let result = compile(LOCAL_FOREACH_ARRAY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# length"), "local fixed-array foreach unnecessarily used length runtime:\n{}", asm);
        assert!(!asm.contains("# index access"), "local fixed-array foreach fell back to runtime indexing:\n{}", asm);
        assert!(
            !asm.contains("index access fail-closed runtime path"),
            "local fixed-array foreach incorrectly failed closed:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 1"), "unrolled foreach lost first element literal:\n{}", asm);
        assert!(asm.contains("li t0, 2"), "unrolled foreach lost second element literal:\n{}", asm);
        assert!(asm.contains("li t0, 3"), "unrolled foreach lost third element literal:\n{}", asm);
    }

    #[test]
    fn compile_unrolls_local_array_of_tuples_foreach_destructuring() {
        let result = compile(LOCAL_FOREACH_ARRAY_OF_TUPLES_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# index access"), "local array-of-tuples foreach fell back to runtime indexing:\n{}", asm);
        assert!(!asm.contains("# field access .1"), "tuple foreach destructuring fell back to schema field access:\n{}", asm);
        assert!(
            !asm.contains("fail-closed runtime path"),
            "local array-of-tuples foreach incorrectly required fail-closed runtime:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 2"), "tuple foreach lost first amount literal:\n{}", asm);
        assert!(asm.contains("li t0, 5"), "tuple foreach lost second amount literal:\n{}", asm);
    }

    #[test]
    fn compile_folds_local_fixed_array_len_to_constant() {
        let result = compile(LOCAL_ARRAY_LEN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# length"), "local fixed-array len should fold before runtime length lowering:\n{}", asm);
        assert!(
            !asm.contains("dynamic length fail-closed runtime path"),
            "local fixed-array len incorrectly required fail-closed runtime:\n{}",
            asm
        );
        assert!(asm.contains("li a0, 3"), "local fixed-array len did not return the static length:\n{}", asm);
    }

    #[test]
    fn compile_supports_typed_empty_array_literals() {
        let result = compile(TYPED_EMPTY_ARRAY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# length"), "typed empty fixed-array len should fold before runtime length lowering:\n{}", asm);
        assert!(asm.contains("li a0, 0"), "typed empty fixed-array len did not return zero:\n{}", asm);
    }

    #[test]
    fn compile_rejects_untyped_empty_array_literals() {
        let err = compile(UNTYPED_EMPTY_ARRAY_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(
            err.message.contains("empty array literal requires an explicit array type annotation"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_empty_array_length_mismatch() {
        let err = compile(WRONG_LENGTH_EMPTY_ARRAY_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("empty array literal cannot initialize non-empty array"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_empty_literal_in_non_vec_context() {
        let err = compile(EMPTY_LITERAL_NON_VEC_CONTEXT_PROGRAM, CompileOptions::default()).unwrap_err();

        assert!(err.message.contains("empty array literal cannot initialize non-empty array"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_local_fixed_array_static_index_reads_and_writes() {
        let result = compile(LOCAL_ARRAY_STATIC_INDEX_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("materialize fixed array"), "local fixed-array writes lost whole-value storage:\n{}", asm);
        assert!(
            !asm.contains("index access fail-closed runtime path"),
            "local fixed-array static indexes incorrectly failed closed:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 7"), "array element assignment did not preserve assigned constant:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "array element read/write result did not lower into arithmetic:\n{}", asm);
    }

    #[test]
    fn compile_rejects_assignment_to_immutable_array_element() {
        let err = compile(IMMUTABLE_ARRAY_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("assignment target rooted at 'items' is not mutable"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_heterogeneous_array_literals() {
        let err = compile(HETEROGENEOUS_ARRAY_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("array elements must have matching types"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_rejects_local_fixed_array_static_oob_read() {
        let err = compile(LOCAL_ARRAY_OOB_READ_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("array index 2 is out of bounds for local fixed array of length 2"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_local_fixed_array_static_oob_write() {
        let err = compile(LOCAL_ARRAY_OOB_WRITE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("array index 2 is out of bounds for local fixed array of length 2"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_lowers_local_tuple_static_field_reads_and_writes() {
        let result = compile(LOCAL_TUPLE_STATIC_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("materialize fixed tuple"), "local tuple writes lost whole-value storage:\n{}", asm);
        assert!(asm.contains("li t0, 7"), "tuple field assignment did not preserve assigned constant:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "tuple field reads did not lower into arithmetic:\n{}", asm);
    }

    #[test]
    fn compile_lowers_array_of_tuples_static_index_projection() {
        let result = compile(ARRAY_OF_TUPLES_STATIC_INDEX_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# index access"), "local array-of-tuples static index fell back to fail-closed runtime:\n{}", asm);
        assert!(!asm.contains("# field access .1"), "local tuple projection fell back to schema path:\n{}", asm);
        assert!(
            !asm.contains("fail-closed runtime path"),
            "local array-of-tuples projection incorrectly required fail-closed runtime:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 5"), "selected tuple amount did not preserve expected literal:\n{}", asm);
    }

    #[test]
    fn compile_rejects_assignment_to_immutable_tuple_field() {
        let err = compile(IMMUTABLE_TUPLE_ASSIGN_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("assignment target rooted at 'pair' is not mutable"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_local_tuple_destructuring_to_field_slots() {
        let result = compile(LOCAL_TUPLE_DESTRUCTURE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# field access .0"), "tuple destructuring fell back to schema field access:\n{}", asm);
        assert!(!asm.contains("# field access .1"), "tuple destructuring fell back to schema field access:\n{}", asm);
        assert!(asm.contains("li t0, 1"), "tuple destructuring lost first literal:\n{}", asm);
        assert!(asm.contains("li t0, 2"), "tuple destructuring lost second literal:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "tuple destructuring did not feed arithmetic:\n{}", asm);
    }

    #[test]
    fn compile_lowers_exhaustive_enum_match_without_wildcard() {
        let result = compile(EXHAUSTIVE_MATCH_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("seqz t0, t0"), "exhaustive match lowering missing equality check:\n{}", asm);
        assert!(asm.contains("li a0, 8"), "exhaustive match did not retain invalid-discriminant fail-closed branch:\n{}", asm);
    }

    #[test]
    fn compile_rejects_invalid_enum_match_patterns() {
        let unknown = compile(UNKNOWN_MATCH_VARIANT_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unknown.message.contains("unknown enum variant 'Flag::Maybe'"), "unexpected error: {}", unknown.message);

        let duplicate = compile(DUPLICATE_MATCH_VARIANT_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            duplicate.message.contains("duplicate exhaustive match coverage for enum variant 'On'"),
            "unexpected error: {}",
            duplicate.message
        );

        let non_exhaustive = compile(NON_EXHAUSTIVE_MATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            non_exhaustive.message.contains("non-exhaustive match for enum 'Flag'"),
            "unexpected error: {}",
            non_exhaustive.message
        );
        assert!(non_exhaustive.message.contains("Off"), "unexpected error: {}", non_exhaustive.message);
    }

    #[test]
    fn compile_rejects_payload_enum_pattern_arity_mismatches() {
        let err = compile(ENUM_PAYLOAD_VARIANT_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains("match pattern 'MaybeAmount::Some' expects 1 payload pattern(s), got 0"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn compile_rejects_malformed_or_unknown_enum_variant_values() {
        let payload = compile(ENUM_PAYLOAD_VALUE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            payload
                .message
                .contains("enum payload variant 'AssetType::Token' requires an explicit fixed-width payload constructor call"),
            "unexpected error: {}",
            payload.message
        );

        let unknown = compile(UNKNOWN_ENUM_VALUE_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(unknown.message.contains("unknown enum variant 'Flag::Maybe'"), "unexpected error: {}", unknown.message);
    }

    #[test]
    fn compile_lowers_vec_builtins_without_generic_calls() {
        let result = compile(VEC_BUILTIN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# collection new Vec"), "Vec::new() did not lower into collection instruction:\n{}", asm);
        assert!(asm.contains("# collection push"), "push() did not lower into collection instruction:\n{}", asm);
        assert!(
            asm.contains("# collection extend_from_slice"),
            "extend_from_slice() did not lower into collection instruction:\n{}",
            asm
        );
        assert!(asm.contains("# length"), "len() did not stay on builtin length path:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=1")
                && asm.contains("# cellscript abi: stack collection extend bytes=3 elements=3 element_size=1"),
            "collection push/extend did not use stack-backed Vec<u8> paths:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection push is not needed for verifier execution")
                && !asm.contains("# cellscript abi: collection extend is not needed for verifier execution"),
            "collection push/extend should not hit old fail-closed comments:\n{}",
            asm
        );
        assert!(!asm.contains("# call push"), "push() leaked through generic call path:\n{}", asm);
        assert!(!asm.contains("# call extend_from_slice"), "extend_from_slice() leaked through generic call path:\n{}", asm);
        assert!(!asm.contains("# call len"), "len() leaked through generic call path:\n{}", asm);
    }

    #[test]
    fn compile_rejects_unsupported_vec_helper_type_combinations() {
        let cases = [
            (
                r#"
module test;

action bad() -> u64 {

        let value = 7;
        return value.len();
}
"#,
                "len is only supported on array or Vec values",
            ),
            (
                r#"
module test;

action bad() -> bool {

        let value = 7;
        return value.is_empty();
}
"#,
                "is_empty is only supported on array or Vec values",
            ),
            (
                r#"
module test;

action bad() -> u64 {

        let value = 7;
        value.extend_from_slice([1, 2]);
        return value;
}
"#,
                "extend_from_slice is only supported on Vec values",
            ),
            (
                r#"
module test;

action bad() -> u64 {

        let mut values: Vec<u64> = [];
        values.extend_from_slice([true]);
        return values.len();
}
"#,
                "array literal type mismatch: expected U64, found Bool",
            ),
            (
                r#"
module test;

struct Point {
    x: u64,
}

action bad(point: Point) -> u64 {

        let mut points = Vec::new();
        points.extend_from_slice([&point]);
        return 0;
}
"#,
                "Vec.extend_from_slice cannot store reference type &Point",
            ),
            (
                r#"
module test;

action bad() -> u64 {

        let values = Vec::new();
        return values.capacity();
}
"#,
                "Vec.capacity requires a typed Vec<T>",
            ),
            (
                r#"
module test;

action bad() -> u64 {

        let values = Vec::new();
        return values.remove(0);
}
"#,
                "Vec.remove requires a typed Vec<T>",
            ),
            (
                r#"
module test;

action bad() -> u64 {

        let values = Vec::new();
        return values.first();
}
"#,
                "Vec.first requires a typed Vec<T>",
            ),
            (
                r#"
module test;

action bad(owner: Address) -> bool {

        let mut values = Vec::new();
        values.push(1);
        return values.contains(owner);
}
"#,
                "Vec.contains type mismatch",
            ),
            (
                r#"
module test;

action bad(owner: Address) -> u64 {

        let mut values = Vec::new();
        values.push(1);
        values.insert(0, owner);
        return 0;
}
"#,
                "Vec.insert type mismatch",
            ),
            (
                r#"
module test;

action bad(owner: Address) -> u64 {

        let mut values = Vec::new();
        values.push(1);
        values.set(0, owner);
        return 0;
}
"#,
                "Vec.set type mismatch",
            ),
            (
                r#"
module test;

struct Point {
    x: u64,
}

action bad(point: Point) -> u64 {

        let mut points = Vec::new();
        points.insert(0, &point);
        return 0;
}
"#,
                "Vec.insert cannot store reference type &Point",
            ),
            (
                r#"
module test;

struct Point {
    x: u64,
}

action bad(point: Point) -> u64 {

        let mut points = Vec::new();
        points.set(0, &point);
        return 0;
}
"#,
                "Vec.set cannot store reference type &Point",
            ),
        ];

        for (source, expected) in cases {
            let err = compile(source, CompileOptions::default()).unwrap_err();
            assert!(err.message.contains(expected), "expected '{expected}', got: {}", err.message);
        }
    }

    #[test]
    fn collection_fail_closed_feature_names_are_stable() {
        assert_eq!(
            super::COLLECTION_FAIL_CLOSED_FEATURE_NAMES,
            &[
                "collection-new",
                "collection-capacity",
                "collection-push",
                "collection-extend",
                "collection-clear",
                "collection-reverse",
                "collection-truncate",
                "collection-swap",
                "collection-contains",
                "collection-remove",
                "collection-insert",
                "collection-set",
                "collection-pop",
            ]
        );
        assert_eq!(
            super::CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_NAMES,
            &[
                "cell-backed-collection-capacity",
                "cell-backed-collection-push",
                "cell-backed-collection-extend",
                "cell-backed-collection-clear",
                "cell-backed-collection-reverse",
                "cell-backed-collection-truncate",
                "cell-backed-collection-swap",
                "cell-backed-collection-contains",
                "cell-backed-collection-remove",
                "cell-backed-collection-insert",
                "cell-backed-collection-set",
                "cell-backed-collection-pop",
                "cell-backed-collection-return",
            ]
        );

        let mut seen = std::collections::BTreeSet::new();
        for feature in
            super::COLLECTION_FAIL_CLOSED_FEATURE_NAMES.iter().chain(super::CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_NAMES.iter())
        {
            assert!(seen.insert(*feature), "duplicate collection fail-closed feature name: {feature}");
            assert!(
                feature.starts_with("collection-") || feature.starts_with("cell-backed-collection-"),
                "unexpected collection fail-closed feature prefix: {feature}"
            );
            assert!(
                feature.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "collection fail-closed feature names must remain lowercase kebab-case: {feature}"
            );
        }
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_runtime_push_len_index() {
        let result = compile(STACK_VEC_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=8"),
            "scalar Vec<u64> push should execute against the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection length"),
            "scalar Vec<u64> len should read the stack collection length word:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection index element_size=8"),
            "scalar Vec<u64> index should read from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection push is not needed for verifier execution")
                && !asm.contains("# cellscript abi: fail closed because dynamic length is not available")
                && !asm.contains("# cellscript abi: fail closed because element layout is not statically computable"),
            "stack-backed scalar Vec runtime should not hit the old fail-closed collection paths:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed scalar Vec runtime should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_vec_with_capacity_to_stack_collection_new() {
        let result = compile(STACK_VEC_WITH_CAPACITY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection with_capacity uses fixed backing buffer"),
            "Vec::with_capacity should lower to stack collection construction with capacity metadata:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=8"),
            "Vec::with_capacity result should use normal stack collection push:\n{}",
            asm
        );
        assert!(!asm.contains("# call Vec::with_capacity"), "Vec::with_capacity leaked through generic call path:\n{}", asm);

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_with_capacity_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "Vec::with_capacity stack runtime should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_bounded_vec_literal_to_stack_collection() {
        let result = compile(BOUNDED_VEC_LITERAL_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection with_capacity uses fixed backing buffer"),
            "bounded Vec literal should lower through the existing stack Vec constructor:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=8"),
            "bounded Vec literal elements should lower through existing push instructions:\n{}",
            asm
        );
        assert!(
            !asm.contains("# call Vec::with_capacity") && !asm.contains("# call push"),
            "bounded Vec literal leaked through generic call lowering:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "bounded_vec_literal_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "bounded Vec literal should stay on the checked stack collection path: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_accepts_empty_vec_literal_with_declared_type() {
        let result = compile(EMPTY_VEC_LITERAL_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# collection new Vec"),
            "empty Vec literal should lower through the existing stack Vec constructor:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: stack collection with_capacity uses fixed backing buffer"),
            "empty Vec literal should be Vec::new() sugar, not Vec::with_capacity(0):\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection capacity element_size=32"),
            "typed empty Vec literal should retain its declared item type:\n{}",
            asm
        );
    }

    #[test]
    fn compile_accepts_vec_literals_in_create_fields() {
        let result = compile(CREATE_VEC_LITERAL_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# collection new Vec"),
            "Vec literal create fields should lower through existing collection construction:\n{}",
            asm
        );
        let action = result.metadata.actions.iter().find(|action| action.name == "create_group").unwrap();
        let create = action.create_set.iter().find(|pattern| pattern.ty == "Group").expect("Group create metadata");
        assert!(
            create.fields.iter().any(|field| field == "members") && create.fields.iter().any(|field| field == "labels"),
            "Vec literal fields should remain visible in create metadata: {:?}",
            create.fields
        );
    }

    #[test]
    fn compile_rejects_bounded_vec_literal_type_mismatch() {
        let err = compile(
            r#"
module test;

action bad() -> u64 {

        let values: Vec<Address> = [1];
        return values.len();
}
"#,
            CompileOptions::default(),
        )
        .unwrap_err();
        assert!(err.message.contains("Vec literal type mismatch"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_capacity() {
        let result = compile(STACK_VEC_CAPACITY_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection capacity element_size=8"),
            "Vec<u64>.capacity should use the fixed stack collection backing size:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 32"), "Vec<u64>.capacity should report 256 bytes / 8-byte elements:\n{}", asm);
        assert!(
            !asm.contains("# cellscript abi: collection capacity is not available for this collection"),
            "stack-backed Vec<u64>.capacity should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_capacity_value").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-capacity".to_string()),
            "stack-backed Vec<u64>.capacity should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_capacity() {
        let result = compile(FIXED_BYTE_STACK_VEC_CAPACITY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection capacity element_size=32"),
            "Vec<Address>.capacity should use the fixed stack collection backing size:\n{}",
            asm
        );
        assert!(asm.contains("li t0, 8"), "Vec<Address>.capacity should report 256 bytes / 32-byte elements:\n{}", asm);
        assert!(
            !asm.contains("# cellscript abi: collection capacity is not available for this collection"),
            "stack-backed Vec<Address>.capacity should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_capacity").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-capacity".to_string()),
            "stack-backed Vec<Address>.capacity should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_remove() {
        let result = compile(STACK_VEC_REMOVE_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection remove element_size=8"),
            "Vec<u64>.remove should remove from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection remove shift element_size=8"),
            "Vec<u64>.remove should shift following stack collection elements left:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection remove is not available for this collection"),
            "stack-backed Vec<u64>.remove should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_remove_middle").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-remove".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.remove should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_remove() {
        let result = compile(FIXED_BYTE_STACK_VEC_REMOVE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection remove element_size=32"),
            "Vec<Address>.remove should remove from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection remove snapshot fixed bytes size=32"),
            "Vec<Address>.remove should snapshot the removed fixed-byte element before shifting:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection remove shift element_size=32"),
            "Vec<Address>.remove should shift following stack collection elements left:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection remove is not available for this collection"),
            "stack-backed Vec<Address>.remove should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_remove").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-remove".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.remove should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_pop() {
        let result = compile(STACK_VEC_POP_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection pop element_size=8"),
            "Vec<u64>.pop should pop from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection pop is not available for this collection"),
            "stack-backed Vec<u64>.pop should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_pop_last").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-pop".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.pop should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_pop() {
        let result = compile(FIXED_BYTE_STACK_VEC_POP_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection pop element_size=32"),
            "Vec<Address>.pop should pop from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection pop snapshot fixed bytes size=32"),
            "Vec<Address>.pop must snapshot the popped element into owned storage:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection pop is not available for this collection"),
            "stack-backed Vec<Address>.pop should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_pop").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-pop".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.pop should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_insert() {
        let result = compile(STACK_VEC_INSERT_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection insert element_size=8"),
            "Vec<u64>.insert should insert into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection insert shift element_size=8"),
            "Vec<u64>.insert should shift existing stack collection elements right:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection insert is not available for this collection"),
            "stack-backed Vec<u64>.insert should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_insert_middle").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-insert".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.insert should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_insert() {
        let result = compile(FIXED_BYTE_STACK_VEC_INSERT_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection insert element_size=32"),
            "Vec<Address>.insert should insert into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection insert snapshot fixed bytes size=32")
                && asm.contains("# cellscript abi: stack collection insert copy fixed bytes size=32"),
            "Vec<Address>.insert should snapshot and copy fixed-byte values:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection insert is not available for this collection"),
            "stack-backed Vec<Address>.insert should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_insert").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-insert".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.insert should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_set() {
        let result = compile(STACK_VEC_SET_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection set element_size=8"),
            "Vec<u64>.set should write into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection set is not available for this collection"),
            "stack-backed Vec<u64>.set should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_set_middle").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-set".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.set should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_set() {
        let result = compile(FIXED_BYTE_STACK_VEC_SET_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection set element_size=32"),
            "Vec<Address>.set should write into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection set copy fixed bytes size=32"),
            "Vec<Address>.set should copy fixed-byte elements:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection set is not available for this collection"),
            "stack-backed Vec<Address>.set should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_set").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-set".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.set should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_reverse() {
        let result = compile(STACK_VEC_REVERSE_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection reverse element_size=8"),
            "Vec<u64>.reverse should reverse the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection reverse swap element_size=8"),
            "Vec<u64>.reverse should swap stack collection elements:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection reverse is not available for this collection"),
            "stack-backed Vec<u64>.reverse should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_reverse_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-reverse".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.reverse should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_reverse() {
        let result = compile(FIXED_BYTE_STACK_VEC_REVERSE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection reverse element_size=32"),
            "Vec<Address>.reverse should reverse the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection reverse swap element_size=32"),
            "Vec<Address>.reverse should swap fixed-byte elements:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection reverse is not available for this collection"),
            "stack-backed Vec<Address>.reverse should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_reverse").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-reverse".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.reverse should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_swap() {
        let result = compile(STACK_VEC_SWAP_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection swap element_size=8"),
            "Vec<u64>.swap should swap the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection swap bytes element_size=8"),
            "Vec<u64>.swap should swap stack collection element bytes:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection swap is not available for this collection"),
            "stack-backed Vec<u64>.swap should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_swap_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-swap".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.swap should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_swap() {
        let result = compile(FIXED_BYTE_STACK_VEC_SWAP_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection swap element_size=32"),
            "Vec<Address>.swap should swap the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection swap bytes element_size=32"),
            "Vec<Address>.swap should swap fixed-byte elements:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection swap is not available for this collection"),
            "stack-backed Vec<Address>.swap should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_swap").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-swap".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.swap should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_first_last() {
        let result = compile(STACK_VEC_FIRST_LAST_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection index element_size=8"),
            "Vec<u64>.first/last should reuse stack collection indexing:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection length"),
            "Vec<u64>.last should compute the stack collection length:\n{}",
            asm
        );
        assert!(
            !asm.contains("# index access (unresolved)"),
            "stack-backed Vec<u64>.first/last should not hit unresolved index access:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_first_last_sum").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.first/last should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_first_last() {
        let result = compile(FIXED_BYTE_STACK_VEC_FIRST_LAST_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection index element_size=32"),
            "Vec<Address>.first/last should reuse stack collection indexing:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection length"),
            "Vec<Address>.last should compute the stack collection length:\n{}",
            asm
        );
        assert!(
            !asm.contains("# index access (unresolved)"),
            "stack-backed Vec<Address>.first/last should not hit unresolved index access:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_first_last").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.first/last should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_truncate() {
        let result = compile(STACK_VEC_TRUNCATE_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection truncate"),
            "Vec<u64>.truncate should update the stack collection length word:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection truncate is not available for this collection"),
            "stack-backed Vec<u64>.truncate should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_truncate_len").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-truncate".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "stack-backed Vec<u64>.truncate should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_truncate() {
        let result = compile(FIXED_BYTE_STACK_VEC_TRUNCATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection truncate"),
            "Vec<Address>.truncate should update the stack collection length word:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection truncate is not available for this collection"),
            "stack-backed Vec<Address>.truncate should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_truncate").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-truncate".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address>.truncate should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_runtime_push_index() {
        let result = compile(FIXED_BYTE_STACK_VEC_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=32"),
            "Vec<Address> push should execute against the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection copy fixed bytes size=32"),
            "Vec<Address> push should copy fixed-byte elements into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection index element_size=32"),
            "Vec<Address> index should return a pointer into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection push is not needed for verifier execution")
                && !asm.contains("# cellscript abi: fail closed because element layout is not statically computable"),
            "stack-backed Vec<Address> runtime should not hit the old fail-closed collection paths:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_roundtrip").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"index-access".to_string())
                && !action.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "stack-backed Vec<Address> runtime should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_extend_from_fixed_bytes() {
        let result = compile(STACK_VEC_EXTEND_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection extend bytes=3 elements=3 element_size=1"),
            "Vec<u8> extend_from_slice should execute against the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection extend copy fixed bytes size=3"),
            "Vec<u8> extend_from_slice should copy fixed bytes into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection length"),
            "Vec<u8> len after extend_from_slice should read the stack collection length word:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection extend is not needed for verifier execution")
                && !asm.contains("# cellscript abi: fail closed because dynamic length is not available"),
            "stack-backed Vec<u8> extend_from_slice should not hit the old fail-closed collection paths:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_extend_len").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-extend".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string()),
            "stack-backed Vec<u8> extend_from_slice should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_clear_and_is_empty() {
        let result = compile(STACK_VEC_CLEAR_IS_EMPTY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection clear"),
            "Vec.clear should reset the stack collection length word:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection length"),
            "Vec.is_empty should read stack collection length through the builtin length path:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection clear is not needed for verifier execution")
                && !asm.contains("# cellscript abi: fail closed because dynamic length is not available"),
            "stack-backed Vec.clear/is_empty should not hit fail-closed collection paths:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_clear_len").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-clear".to_string())
                && !action.fail_closed_runtime_features.contains(&"dynamic-length".to_string()),
            "stack-backed Vec.clear/is_empty should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_scalar_contains() {
        let result = compile(STACK_VEC_CONTAINS_RUNTIME_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection contains element_size=8"),
            "Vec<u64>.contains should scan the stack collection buffer:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection contains is not available for this collection"),
            "stack-backed Vec<u64>.contains should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_contains_hit").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-contains".to_string()),
            "stack-backed Vec<u64>.contains should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_byte_contains() {
        let result = compile(FIXED_BYTE_STACK_VEC_CONTAINS_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection contains element_size=32"),
            "Vec<Address>.contains should scan fixed-byte stack collection elements:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection contains is not available for this collection"),
            "stack-backed Vec<Address>.contains should not hit fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "stack_vec_address_contains").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-contains".to_string()),
            "stack-backed Vec<Address>.contains should not be reported as fail-closed: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_lowers_stack_vec_fixed_schema_values() {
        let result = compile(FIXED_SCHEMA_STACK_VEC_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: stack collection push element_size=40"),
            "Vec<Snapshot>.push should copy fixed-width schema values into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection with_capacity uses fixed backing buffer"),
            "Vec<Snapshot>::with_capacity should use the stack collection backing buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection capacity element_size=40"),
            "Vec<Snapshot>.capacity should report fixed backing capacity:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection insert element_size=40"),
            "Vec<Snapshot>.insert should shift fixed-width schema values in the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection set element_size=40"),
            "Vec<Snapshot>.set should write fixed-width schema values into the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection reverse element_size=40"),
            "Vec<Snapshot>.reverse should reverse fixed-width schema values in the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection swap element_size=40"),
            "Vec<Snapshot>.swap should swap fixed-width schema values in the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection remove element_size=40"),
            "Vec<Snapshot>.remove should return fixed-width schema values from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection truncate"),
            "Vec<Snapshot>.truncate should update stack collection length:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection contains element_size=40"),
            "Vec<Snapshot>.contains should compare fixed-width schema values in the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection pop element_size=40"),
            "Vec<Snapshot>.pop should return a fixed-width schema value pointer from the stack collection buffer:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: stack collection index element_size=40")
                && asm.contains("# cellscript abi: stack collection clear")
                && asm.contains("# cellscript abi: stack collection length"),
            "Vec<Snapshot>.first/last/clear/is_empty/len should lower through stack collection paths:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: fixed-byte field Named(\"Snapshot\").owner offset=0 size=32")
                && asm.contains("# cellscript abi: fixed-byte scalar field Named(\"Snapshot\").amount offset=32 size=8"),
            "Vec<Snapshot> elements should remain field-readable after pop:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: collection push is not needed for verifier execution")
                && !asm.contains("# cellscript abi: collection contains is not available for this collection")
                && !asm.contains("# cellscript abi: collection pop is not available for this collection")
                && !asm.contains("# cellscript abi: collection insert is not available for this collection")
                && !asm.contains("# cellscript abi: collection remove is not available for this collection")
                && !asm.contains("# cellscript abi: collection set is not available for this collection")
                && !asm.contains("# cellscript abi: collection reverse is not available for this collection")
                && !asm.contains("# cellscript abi: collection swap is not available for this collection")
                && !asm.contains("# cellscript abi: collection truncate is not available for this collection")
                && !asm.contains("# index access (unresolved)")
                && !asm.contains("# cellscript abi: fail closed because element layout is not statically computable"),
            "stack-backed Vec<Snapshot> should not hit old fail-closed collection paths:\n{}",
            asm
        );

        for action_name in
            ["stack_vec_snapshot_roundtrip", "stack_vec_snapshot_first_last", "stack_vec_snapshot_clear", "stack_vec_snapshot_pop"]
        {
            let action = result.metadata.actions.iter().find(|action| action.name == action_name).unwrap();
            for feature in [
                "collection-new",
                "collection-push",
                "collection-capacity",
                "collection-insert",
                "collection-remove",
                "collection-set",
                "collection-reverse",
                "collection-swap",
                "collection-truncate",
                "collection-contains",
                "collection-pop",
                "collection-clear",
                "dynamic-length",
                "index-access",
                "field-access",
                "fixed-byte-comparison",
            ] {
                assert!(
                    !action.fail_closed_runtime_features.contains(&feature.to_string()),
                    "{action_name} should not report {feature}: {:?}",
                    action.fail_closed_runtime_features
                );
            }
        }

        let instantiations = result
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .filter(|instantiation| instantiation.collection_ty == "Vec<Snapshot>")
            .collect::<Vec<_>>();
        assert!(!instantiations.is_empty(), "Vec<Snapshot> instantiation should be metadata-visible");
        for instantiation in &instantiations {
            assert_eq!(instantiation.element_ty, "Snapshot");
            assert_eq!(instantiation.element_width_bytes, 40);
            assert_eq!(instantiation.max_elements, 6);
            assert_eq!(instantiation.backing, "stack-fixed-buffer:256");
        }
        for helper in [
            "capacity",
            "clear",
            "contains",
            "index",
            "insert",
            "len",
            "new",
            "pop",
            "push",
            "remove",
            "reverse",
            "set",
            "swap",
            "truncate",
            "with_capacity",
        ] {
            assert!(
                instantiations.iter().any(|instantiation| instantiation.helpers.iter().any(|value| value == helper)),
                "missing helper {helper}: {instantiations:?}"
            );
        }
    }

    #[test]
    fn compile_verifies_constructed_fixed_width_vec_output() {
        let result = compile(FIXED_WIDTH_VEC_CREATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains(
                "# cellscript abi: verify output dynamic field Group.members as constructed Molecule vector elements=1 bytes=32 element_size=32"
            ),
            "Group.members Vec<Address> should be verifier-checked as a fixed-width Molecule vector:\n{}",
            asm
        );
        assert!(
            asm.contains(
                "# cellscript abi: verify output dynamic field Group.anchors as constructed Molecule vector elements=1 bytes=32 element_size=32"
            ),
            "Group.anchors Vec<Hash> should be verifier-checked as a fixed-width Molecule vector:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: collection push is covered by create-output vector verifier"),
            "fixed-width Vec pushes used for create-output fields should not execute the fail-closed collection path:\n{}",
            asm
        );

        let action = result.metadata.actions.iter().find(|action| action.name == "create_group").unwrap();
        assert!(
            !action.fail_closed_runtime_features.contains(&"collection-new".to_string())
                && !action.fail_closed_runtime_features.contains(&"collection-push".to_string())
                && !action.fail_closed_runtime_features.contains(&"output-verification-incomplete".to_string()),
            "fixed-width Vec create-output verifier should leave no collection fail-closed debt: {:?}",
            action.fail_closed_runtime_features
        );
    }

    #[test]
    fn compile_rejects_cell_backed_vec_with_source_aware_guidance() {
        let err = compile(CELL_BACKED_VEC_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(
            err.message.contains(
                "type 'Vec<NFT>' cannot store a cell-backed resource; use a source-aware BoundedCellSet<T, N> with explicit ownership"
            ),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn ckb_dynamic_vector_len_can_drive_mutate_transition() {
        let source = r#"
module dynamic_len_transition;

resource Collection {
    total_supply: u64,
    name: Vec<u8>,
}

action batch(collection_before: Collection, recipients: Vec<Address>) -> collection_after: Collection {

        require collection_after.name == collection_before.name;
        require collection_after.total_supply == collection_before.total_supply + recipients.len() as u64;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let action = result.metadata.actions.iter().find(|action| action.name == "batch").expect("batch metadata");

        assert!(action.consume_set.iter().any(|pattern| pattern.operation == "input" && pattern.binding == "collection_before"));
        assert!(action.create_set.iter().any(|pattern| pattern.operation == "output" && pattern.binding == "collection_after"));
        assert!(
            !action
                .transaction_runtime_input_requirements
                .iter()
                .any(|requirement| requirement.feature == "mutable-cell:Collection"
                    && requirement.component == "mutate-field-transition"),
            "dynamic vector len transition should not leave a runtime-required mutable-cell transition blocker: {:?}",
            action.transaction_runtime_input_requirements
        );
    }

    #[test]
    fn compile_lowers_type_hash_without_generic_call() {
        let result = compile(TYPE_HASH_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# type_hash"), "type_hash() did not lower into builtin instruction:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: schema param pool type_hash pointer=a2 length=a3 size=32"),
            "schema parameter type_hash() should be backed by an explicit pointer+length ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: exact size check param type hash expected=32"),
            "schema parameter type_hash() should reject non-32-byte ABI payloads:\n{}",
            asm
        );
        assert!(
            !asm.contains("# cellscript abi: type_hash fail-closed runtime path"),
            "schema parameter type_hash() should not use the fail-closed lowering path:\n{}",
            asm
        );
        assert!(!asm.contains("# call type_hash"), "type_hash() leaked through generic call path:\n{}", asm);
        let action = result.metadata.actions.iter().find(|action| action.name == "pool_id").unwrap();
        assert!(
            action.fail_closed_runtime_features.is_empty(),
            "schema parameter type_hash() should be verifier-coverable: {:?}",
            action.fail_closed_runtime_features
        );
        let param = action.params.iter().find(|param| param.name == "pool").unwrap();
        assert!(param.type_hash_pointer_abi);
        assert!(param.type_hash_length_abi);
        assert_eq!(param.type_hash_len, Some(32));
    }

    #[test]
    fn compile_lowers_zero_builtin_without_generic_call() {
        let result = compile(ZERO_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# call zero"), "Address::zero() leaked through generic call path:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: fixed-byte Eq comparison size=32"),
            "Address::zero() comparison should lower to a full-width byte comparison:\n{}",
            asm
        );
        assert!(
            !asm.contains("fixed-byte comparison fail-closed runtime path"),
            "Address::zero() comparison should not use the fail-closed lowering path:\n{}",
            asm
        );
    }

    #[test]
    fn compile_preserves_full_width_u128_decimal_literals() {
        let result = compile_with_executable_surface_policy(
            U128_LITERAL_PROGRAM,
            CompileOptions { diagnostics: crate::CompilerDiagnostics::without_optimizations(), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();

        assert!(asm.contains("# cellscript abi: u128 compare high limb first"));
        let max_value_body = asm
            .split(".global max_value")
            .nth(1)
            .and_then(|body| body.split(".section .text").next())
            .expect("max_value assembly body");
        assert!(max_value_body.contains("# cellscript abi: materialize u128 const at stack+"));
        assert_eq!(
            max_value_body.matches("sb t0, ").count(),
            16,
            "u128::MAX must materialize all sixteen 0xff bytes:\n{max_value_body}"
        );
    }

    #[test]
    fn compile_rejects_u128_decimal_literal_in_u64_context() {
        let source = r#"
module test;

fn too_narrow() -> u64 {
    return 18446744073709551616;
}
"#;
        let error = compile(source, CompileOptions::default()).expect_err("a literal wider than u64 must not be truncated");
        assert!(
            error.message.contains("integer literal 18446744073709551616 does not fit expected type u64"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn compile_riscv_elf_accepts_full_width_u64_literals() {
        let program = r#"
module vm::u64_literals;

action high_bit() -> u64 {

        return 9223372036854775808;

}
action max_value() -> u64 {

        return 18446744073709551615;
}
"#;

        let result =
            compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_rejects_flow_payload_enum_state_field() {
        let source = r#"
module test;

enum OfferState {
    Created,
    Live(u64),
}

resource Offer has store {
    state: OfferState,
    amount: u64,
}

flow Offer.state {
    Created -> Live;
}
"#;
        let err = compile(source, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("must not have payload variants"), "unexpected error: {}", err.message);
    }

    #[test]
    fn ckb_entry_scope_keeps_vec_element_schema_dependencies() {
        let source = r#"
module scoped_vec_dependency;

struct Signature {
    signer: Address,
}

receipt Proposal {
    signatures: Vec<Signature>,
    expires_at: u64,
}

lock not_expired(protected proposal: Proposal, witness now: u64) -> bool {

        now < proposal.expires_at
}
"#;
        let temp = tempdir().unwrap();
        let entry = Utf8Path::from_path(temp.path()).unwrap().join("scoped_vec_dependency.cell");
        std::fs::write(&entry, source).unwrap();

        let scoped = compile_file_with_entry_lock(
            &entry,
            CompileOptions {
                target_profile: Some("ckb".to_string()),
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
            "not_expired",
        )
        .unwrap();
        let proposal = scoped.metadata.types.iter().find(|ty| ty.name == "Proposal").expect("Proposal metadata");
        let schema = proposal.molecule_schema.as_ref().expect("Proposal schema should be generated in scoped CKB compile");
        assert!(schema.schema.contains("struct Signature"), "Vec<Signature> dependency was not retained:\n{}", schema.schema);
        assert!(scoped.metadata.types.iter().any(|ty| ty.name == "Signature"));
    }

    #[test]
    fn dynamic_schema_fixed_vec_length_is_table_decoded() {
        let source = r#"
module dynamic_vec_len;

receipt Emergency {
    lock_hash: Hash,
    approvers: Vec<Address>,
}

lock enough(protected emergency: Emergency, witness required: u8) -> bool {

        emergency.approvers.len() >= required as usize
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let emergency = result.metadata.types.iter().find(|ty| ty.name == "Emergency").expect("Emergency metadata");
        let schema = emergency.molecule_schema.as_ref().expect("dynamic Emergency molecule schema");
        assert_eq!(schema.layout, "molecule-table-v1");
        assert_eq!(schema.dynamic_fields, vec!["approvers".to_string()]);

        let lock = result.metadata.locks.iter().find(|lock| lock.name == "enough").expect("lock metadata");
        assert!(
            !lock.fail_closed_runtime_features.contains(&"field-access".to_string()),
            "dynamic Molecule vector field access should be verifier-covered: {:?}",
            lock.fail_closed_runtime_features
        );
        assert!(
            !lock.fail_closed_runtime_features.contains(&"dynamic-length".to_string()),
            "fixed-element Molecule vector length should be verifier-covered: {:?}",
            lock.fail_closed_runtime_features
        );

        let ckb = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        assert!(
            ckb.metadata.locks[0].fail_closed_runtime_features.is_empty(),
            "CKB fixed-element vector length lock should not require fail-closed runtime paths: {:?}",
            ckb.metadata.locks[0].fail_closed_runtime_features
        );
    }

    #[test]
    fn dynamic_schema_fixed_vec_iteration_is_table_decoded() {
        let source = r#"
module dynamic_vec_iter;

resource Wallet {
    signers: Vec<Address>,
    threshold: u8,
}

fn is_signer(wallet: &Wallet, addr: Address) -> bool {
    for signer in &wallet.signers {
        if (*signer == addr) {
            return true;
        }
    }
    false
}

lock signer(protected wallet: Wallet, witness addr: Address) -> bool {

        is_signer(wallet, addr)
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let function = result.metadata.functions.iter().find(|function| function.name == "is_signer").expect("function metadata");
        assert!(
            !function.fail_closed_runtime_features.contains(&"dynamic-length".to_string()),
            "fixed-element Molecule vector length should be verifier-covered: {:?}",
            function.fail_closed_runtime_features
        );
        assert!(
            !function.fail_closed_runtime_features.contains(&"index-access".to_string()),
            "fixed-element Molecule vector index should be verifier-covered: {:?}",
            function.fail_closed_runtime_features
        );
        assert!(
            !function.fail_closed_runtime_features.contains(&"fixed-byte-comparison".to_string()),
            "fixed-element Molecule vector comparison should be verifier-covered: {:?}",
            function.fail_closed_runtime_features
        );

        let ckb = compile(source, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() }).unwrap();
        let function = ckb.metadata.functions.iter().find(|function| function.name == "is_signer").expect("CKB function metadata");
        assert!(
            function.fail_closed_runtime_features.is_empty(),
            "CKB fixed-element vector iteration should not require fail-closed runtime paths: {:?}",
            function.fail_closed_runtime_features
        );
    }

    #[test]
    fn nested_helper_stack_args_preserve_entry_witness_frame() {
        let program = r#"
module vm::nested_stack_abi;

fn leaf(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, i: u64) -> u64 {
    return a + i;
}

fn middle(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, i: u64) -> u64 {
    return leaf(a, b, c, d, e, f, g, h, i);
}

action nested(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, i: u64) -> u64 {

        return middle(a, b, c, d, e, f, g, h, i);
}
"#;

        let result = compile(program, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains("# cellscript entry abi: stage stack arg8 at pre-call sp-16"), "entry stack arg was not staged:\n{asm}");
        assert_eq!(
            asm.matches("# cellscript abi: stage outgoing stack arg8 at pre-call sp-16").count(),
            2,
            "both nested helper calls must stage their ninth argument below the caller frame:\n{asm}"
        );
        assert!(
            asm.matches("# cellscript abi: arg8 loaded from caller stack +0").count() >= 3,
            "action and helper callees must load the ninth argument from the incoming caller area:\n{asm}"
        );
        assert!(asm.contains("li t6, 5368"), "entry return address must remain at the derived frame offset:\n{asm}");
        assert!(asm.contains("li t6, -5376"), "entry trampoline must reserve the complete derived frame:\n{asm}");
    }

    #[test]
    fn bounded_cell_collections_record_source_ownership_and_builder_obligations() {
        let source = r#"
module bounded::collections;

struct Plan {
    owner: Address,
    amount: u64,
}

resource Token has store, create, consume {
    owner: Address,
    amount: u64,
}

action batch(input inputs: BoundedCellSet<Token, 16>, witness plans: BoundedList<Plan, 16>) -> u64 {

        consume_each token in inputs {
            require token.amount > 0;
        }
        create_each plan in plans {
            require plan.amount > 0;
            create Token { owner: plan.owner, amount: plan.amount, }
        }
        return 0;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let consumed = result
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .find(|collection| collection.ownership == "linear-cell-set")
            .expect("bounded input Cell set metadata");
        assert_eq!(consumed.collection_ty, "BoundedCellSet<Token, 16>");
        assert_eq!(consumed.source, "input");
        assert_eq!(consumed.max_elements, 16);
        assert!(consumed.vacuous_possible);
        assert_eq!(consumed.status, "checked-runtime");
        assert_eq!(consumed.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert!(consumed.helpers.contains(&crate::ir::BOUNDED_TYPE_GROUP_INPUTS_V1.to_string()));

        let created = result
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .find(|collection| collection.ownership == "bounded-output-plan")
            .expect("bounded output plan metadata");
        assert_eq!(created.collection_ty, "BoundedList<Plan, 16>");
        assert_eq!(created.source, "witness");
        assert_eq!(created.output_cardinality_max, Some(16));
        assert!(created.capacity_builder_evidence_required);
        assert_eq!(created.evidence_tier, crate::EvidenceTier::BuilderEvidenceRequired);

        let consume_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.category == "bounded-cell-collection" && plan.feature.starts_with("consume_each:"))
            .expect("bounded consume ProofPlan");
        assert_eq!(consume_plan.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert!(consume_plan.coverage.contains(&"maximum_cardinality:16".to_string()));
        assert!(consume_plan.coverage.contains(&"vacuous:true-when-cardinality-zero".to_string()));
        assert!(consume_plan.coverage.contains(&"actual_scanned_cardinality:runtime-observed".to_string()));
        assert!(consume_plan.coverage.iter().any(|item| item == "predicate-executed-once-per-element:token.amount > 0"));
        assert_eq!(consume_plan.input_output_relation_checks, ["group_input_count<=16"]);
        assert!(consume_plan.on_chain_checked);

        let create_plan = result
            .metadata
            .runtime
            .proof_plan
            .iter()
            .find(|plan| plan.category == "bounded-cell-collection" && plan.feature.starts_with("create_each:"))
            .expect("bounded create ProofPlan");
        assert_eq!(create_plan.evidence_tier, crate::EvidenceTier::BuilderEvidenceRequired);
        assert!(create_plan.coverage.contains(&"output_cardinality_max:16".to_string()));
        assert!(create_plan.coverage.contains(&"capacity_builder_evidence_required:true".to_string()));
        assert!(create_plan.coverage.iter().any(|item| item == "create_template_output_type:Token"));
        assert!(create_plan.coverage.iter().any(|item| item == "create_template_field:amount=plan.amount"));
        assert!(create_plan.input_output_relation_checks.is_empty());
        assert!(!create_plan.on_chain_checked);

        let action = result.metadata.actions.iter().find(|action| action.name == "batch").expect("batch action metadata");
        assert!(!action.fail_closed_runtime_features.contains(&"bounded-consume-each-runtime".to_string()));
        assert!(action.fail_closed_runtime_features.contains(&"bounded-create-each-runtime".to_string()));
        let entry = result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_name == "batch").expect("batch entry ABI");
        assert!(entry.unsupported);
        assert!(entry.params[0].supported);
        assert_eq!(entry.params[0].abi_kind, crate::ir::BOUNDED_TYPE_GROUP_INPUTS_V1);
        assert!(!entry.params[1].supported);
        assert_eq!(entry.params[1].abi_kind, "unsupported");

        let asm = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(asm.contains(crate::ir::BOUNDED_TYPE_GROUP_INPUTS_V1));
        assert!(!asm.contains(crate::ir::BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER));
        assert!(asm.contains(crate::ir::BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER));
        assert!(asm.contains("cellscript runtime error 24 collection-runtime-unsupported"));
    }

    #[test]
    fn bounded_collection_ir_is_explicitly_fail_closed_and_production_is_rejected() {
        let source = r#"
module bounded::fail_closed;

struct Plan { amount: u64, }
resource DynamicToken has store, consume {
    amount: u64,
    memo: String,
}
resource Token has store, create { amount: u64, }

action batch(input inputs: BoundedCellSet<DynamicToken, 2>, witness plans: BoundedList<Plan, 2>) -> u64 {

        consume_each token in inputs {
            require false;
            require token.amount > 100;
        }
        create_each plan in plans {
            require false;
            create Token { amount: plan.amount, }
        }
        return 0;
}
"#;
        let ast = parse_module_for_test(source);
        crate::types::check(&ast).unwrap();
        let module = ir::generate(&ast).unwrap();
        let action = module
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "batch" => Some(action),
                _ => None,
            })
            .expect("batch IR action");
        for helper in [crate::ir::BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER, crate::ir::BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER] {
            assert!(action.body.blocks.iter().any(|block| {
                block
                    .instructions
                    .iter()
                    .any(|instruction| matches!(instruction, ir::IrInstruction::Call { func, .. } if *func == ir::IrCallTarget::runtime_symbol(helper)))
            }));
        }
        assert_eq!(action.body.bounded_collection_ops.len(), 2);
        assert_eq!(action.body.bounded_collection_ops[0].predicates, ["false", "token.amount > 100"]);
        assert!(action.body.bounded_collection_ops[1].create_template.is_some());

        let error = compile_with_executable_surface_policy(
            source,
            CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2105"));
        assert!(error.message.contains("consume_each:inputs:BoundedCellSet<DynamicToken, 2>"));
        assert!(error.message.contains("action:batch#bounded-collection:0"));
        assert!(error.message.contains("gap:runtime-helper-required"));
        assert!(error.message.contains("Remediation:"));
        let details = error.details.as_ref().expect("E2105 details");
        assert_eq!(details["consensus_gaps"].as_array().map(Vec::len), Some(2));
        assert_eq!(details["consensus_gaps"][1]["missing_enforcement"], "gap:builder-evidence-required");
    }

    #[test]
    fn bounded_cell_collections_reject_unbounded_or_wrong_ownership_surfaces() {
        let cases = [
            (
                r#"
module bad;
resource Token has store, consume { amount: u64, }
action run(inputs: BoundedCellSet<Token, 16>) -> u64 {

        consume_each token in inputs { require token.amount > 0; }
        return 0;
}
"#,
                "requires an explicit source classification",
            ),
            (
                r#"
module bad;
resource Token has store, consume { amount: u64, }
action run(input inputs: Vec<Token>) -> u64 {

        return 0;
}
"#,
                "cannot store a cell-backed resource",
            ),
            (
                r#"
module bad;
resource Token has store, consume { amount: u64, }
action run(input inputs: BoundedCellSet<Token, 0>) -> u64 {

        consume_each token in inputs { require token.amount > 0; }
        return 0;
}
"#,
                "maximum cardinality in 1..=1024",
            ),
            (
                r#"
module bad;
resource Token has store, consume { amount: u64, }
action run(input inputs: BoundedCellSet<Token>) -> u64 {

        return 0;
}
"#,
                "explicit maximum cardinality",
            ),
            (
                r#"
module bad;
struct Plan { amount: u64, }
action run(witness plans: BoundedList<Plan, 2>) -> u64 {

        consume_each plan in plans { require plan.amount > 0; }
        return 0;
}
"#,
                "consume_each expects BoundedCellSet",
            ),
            (
                r#"
module bad;
struct Plan { amount: u64, }
resource Token has store, create { amount: u64, }
action run(witness plans: BoundedList<Plan, 2>) -> u64 {

        create_each plan in plans { require plan.amount > 0; }
        return 0;
}
"#,
                "exactly one create template",
            ),
            (
                r#"
module bad;
resource Token has store, consume { amount: u64, }
action run(input inputs: BoundedCellSet<Token, 2>) -> u64 {

        consume_each token in inputs { require token.amount > 0; }
        consume_each token in inputs { require token.amount > 0; }
        return 0;
}
"#,
                "already Consumed",
            ),
            (
                r#"
module bad;
struct Plan { memo: String, }
action run(witness plans: BoundedList<Plan, 2>) -> u64 {

        return 0;
}
"#,
                "fixed-width element type",
            ),
            (
                r#"
module bad;
shared Config has store { value: u64, }
action run(input configs: BoundedCellSet<Config, 2>) -> u64 {

        return 0;
}
"#,
                "requires a linear resource element",
            ),
            (
                r#"
module bad;
struct Plan { amount: u64, }
resource Token has store, create { amount: u64, }
action run(witness plans: BoundedList<Plan, 2>) -> u64 {

        create_each plan in plans {
            create Token { amount: plan.amount, }
            create Token { amount: plan.amount, }
        }
        return 0;
}
"#,
                "exactly one create template per bounded plan element, found 2",
            ),
        ];
        for (source, expected) in cases {
            let error = compile(source, CompileOptions::default()).unwrap_err();
            assert!(error.message.contains(expected), "expected '{expected}', got: {}", error.message);
        }
    }

    #[test]
    fn concrete_payload_enum_compiles_with_tagged_union_lowering() {
        let source = r#"
module payload::limit;

enum Limit {
    None,
    Some(u64),
}

fn get_limit(enabled: bool) -> Limit {
    if enabled {
        Limit::Some(100)
    } else {
        Limit::None
    }
}

action spend(amount: u64, enabled: bool) {

        match get_limit(enabled) {
            Limit::Some(value) => require amount <= value,
            Limit::None => require false, "missing limit",
        }
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let layout = result.metadata.enum_layouts.iter().find(|layout| layout.name == "Limit").expect("Limit enum metadata");
        assert_eq!(layout.layout, "packed-tagged-union-v1");
        assert_eq!(layout.abi, "fixed-bytes-pointer-v1+register-pair-return-v1");
        assert_eq!(layout.encoded_size_bytes, 9);
        assert_eq!(layout.variants[1].fields[0].offset_bytes, 1);
        assert_eq!(layout.variants[1].fields[0].width_bytes, 8);
        let mut docs = crate::docgen::DocGenerator::new(crate::docgen::OutputFormat::Markdown);
        docs.add_module(&result.ast);
        docs.set_compile_metadata(&result.metadata);
        let docs = docs.generate().unwrap();
        assert!(docs.contains("### Concrete Enum Layouts"));
        assert!(docs.contains("Limit"));
        assert!(docs.contains("fixed-bytes-pointer-v1+register-pair-return-v1"));
        let assembly = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(assembly.contains("construct payload enum Limit::Some"), "missing payload constructor:\n{assembly}");
        assert!(assembly.contains("return payload enum Limit size=9 via a0/a1 register pair"));
        assert!(assembly.contains("receive payload enum Limit size=9 from a0/a1 register pair"));
    }

    #[test]
    fn no_payload_enum_field_comparisons_load_the_discriminant_value() {
        let source = r#"
module payload::state;

enum State {
    Open,
    Closed,
}

resource Item has store, consume {
    state: State,
}

action close(item: Item) {

        require item.state != State::Closed;
        consume item;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let assembly = String::from_utf8(result.artifact_bytes).unwrap();
        assert!(
            assembly.contains("# cellscript abi: fixed-byte scalar field Named(\"Item\").state offset=0 size=1")
                && assembly.contains("lbu t2, 0(t4)"),
            "one-byte no-payload enum comparisons must load the field discriminant, not the pointer/length representation:\n{assembly}"
        );
    }

    #[test]
    fn concrete_payload_enum_rejects_nonexhaustive_dynamic_generic_and_linear_drop_cases() {
        let cases = [
            (
                r#"
module payload::nonexhaustive;
enum Limit { None, Some(u64), }
action spend(limit: Limit) {

        match limit { Limit::Some(value) => require value > 0 }
}
"#,
                "non-exhaustive match",
            ),
            (
                r#"
module payload::dynamic;
enum Bytes { None, Some(Vec<u8>), }
"#,
                "variable-width or recursive",
            ),
            (
                r#"
module payload::generic;
resource Token has consume { amount: u64, }
private enum Boxed<T> { Empty, Some(T), }
fn hide(input: Token) -> Boxed<Token> { Boxed::Some::<Token>(input) }
"#,
                "cannot be hidden in ordinary generic layout",
            ),
            (
                r#"
module payload::linear_drop;
resource Token has consume { amount: u64, }
enum MaybeToken { None, Some(Token), }
action discard(input: Token) {

        let wrapped = MaybeToken::Some(input);
        match wrapped {
            MaybeToken::Some(token) => require true,
            MaybeToken::None => require true,
        }
}
"#,
                "linear resource 'token' was not consumed",
            ),
        ];
        for (source, expected) in cases {
            let error = compile(source, CompileOptions::default()).unwrap_err();
            assert!(error.message.contains(expected), "expected '{expected}', got: {}", error.message);
        }
    }

    #[test]
    fn compile_result_validation_rejects_tampered_payload_enum_layout() {
        let source = r#"
module payload::tamper;
enum Limit { None, Some(u64), }
fn limit() -> Limit { Limit::Some(7) }
"#;
        let mut result = compile(source, CompileOptions::default()).unwrap();
        result.metadata.enum_layouts[0].variants[1].fields[0].offset_bytes = 2;
        let error = result.validate().unwrap_err();
        assert!(error.message.contains("canonical indices and packed offsets"), "unexpected error: {}", error.message);
    }

    #[test]
    fn generic_value_struct_and_function_monomorphize_to_executable_elf() {
        let source = r#"
module generics::pair;

struct Pair<T: copy + drop + store + fixed + serializable + non_linear>
    has copy, drop, store, fixed, serializable, non_linear
{
    left: T,
    right: T,
}

fn choose_first<T: copy + drop + store + fixed + serializable + non_linear>(left: T, right: T) -> T {
    return left;
}

action verify() -> u64 {

        let pair: Pair<u64> = new Pair<u64> { left: 42, right: 7, };
        return choose_first(pair.left, pair.right);
}
"#;
        let result = compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(&result.artifact_bytes[..4], b"\x7fELF");
        assert!(result.metadata.generic_instantiations.iter().any(|entry| {
            entry.kind == "struct"
                && entry.template == "Pair"
                && entry.type_arguments == ["u64"]
                && entry.identity == "generics::pair::Pair<u64>"
        }));
        assert!(result
            .metadata
            .generic_instantiations
            .iter()
            .any(|entry| { entry.kind == "function" && entry.template == "choose_first" && entry.type_arguments == ["u64"] }));
    }

    #[test]
    fn generic_value_enum_preserves_payload_layout_and_match_semantics() {
        let source = r#"
module generics::option;

enum Maybe<T: copy + drop + store + fixed + serializable + non_linear>
    has copy, drop, store, fixed, serializable, non_linear
{
    None,
    Some(T),
}

fn unwrap_or<T: copy + drop + store + fixed + serializable + non_linear>(value: Maybe<T>, fallback: T) -> T {
    return match value {
        Maybe::Some(inner) => { inner }
        Maybe::None => { fallback }
    };
}

action verify() -> u64 {

        let value: Maybe<u64> = Maybe::Some::<u64>(42);
        return unwrap_or::<u64>(value, 0);
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        assert!(result.metadata.enum_layouts.iter().any(|layout| layout.variants.iter().any(|variant| variant.name == "Some")));
        assert!(result
            .metadata
            .generic_instantiations
            .iter()
            .any(|entry| { entry.kind == "enum" && entry.template == "Maybe" && entry.type_arguments == ["u64"] }));
    }

    #[test]
    fn nested_generic_values_preserve_derived_abilities() {
        let source = r#"
module generics::nested;

struct Pair<T: copy + drop + store + fixed + serializable + non_linear>
    has copy, drop, store, fixed, serializable, non_linear
{
    left: T,
    right: T,
}

struct Envelope<T: copy + drop + store + fixed + serializable + non_linear>
    has copy, drop, store, fixed, serializable, non_linear
{
    value: T,
}

action verify() -> u64 {

        let pair: Pair<u64> = new Pair<u64> { left: 20, right: 22, };
        let envelope: Envelope<Pair<u64>> = new Envelope<Pair<u64>> { value: pair, };
        return envelope.value.left + envelope.value.right;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        assert!(result.metadata.generic_instantiations.iter().any(|entry| entry.identity == "generics::nested::Envelope<Pair<u64>>"));
    }

    #[test]
    fn nested_tuple_struct_enum_and_or_patterns_lower_completely() {
        let source = r#"
module patterns::complete;

struct Point { x: u64, y: u64, }
enum Inner { None, Some((u64, u64)), }
enum Outer { Empty, Wrapped(Inner), }
enum Switch { Off, On, Unknown, }

action verify() -> u64 {

        let point: Point = new Point { x: 1, y: 2, };
        let point_sum = match point {
            Point { x, y } => { x + y }
        };
        let inner: Inner = Inner::Some((20, 22));
        let outer: Outer = Outer::Wrapped(inner);
        let payload_sum = match outer {
            Outer::Wrapped(Inner::Some((left, right))) => { left + right }
            _ => { 0 }
        };
        let switch: Switch = Switch::Unknown;
        let switched = match switch {
            Switch::On | Switch::Unknown => { 1 }
            Switch::Off => { 0 }
        };
        return point_sum + payload_sum + switched;
}
"#;
        let result = compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(&result.artifact_bytes[..4], b"\x7fELF");
        result.validate().unwrap();
    }

    #[test]
    fn nested_payload_patterns_merge_exhaustive_coverage_across_arms_and_or_patterns() {
        for source in [
            r#"
module patterns::nested_arms;

enum Inner { None, Some((u64, u64)), }
enum Outer { Empty, Wrapped(Inner), }

action verify() -> u64 {

        let outer: Outer = Outer::Wrapped(Inner::Some((20, 22)));
        return match outer {
            Outer::Empty => { 0 }
            Outer::Wrapped(Inner::None) => { 1 }
            Outer::Wrapped(Inner::Some((left, right))) => { left + right }
        };
}
"#,
            r#"
module patterns::payload_or;

enum Inner { None, Some((u64, u64)), }
enum Outer { Empty, Wrapped(Inner), }

action verify() -> u64 {

        let outer: Outer = Outer::Wrapped(Inner::None);
        return match outer {
            Outer::Empty => { 0 }
            Outer::Wrapped(Inner::None | Inner::Some(_)) => { 1 }
        };
}
"#,
        ] {
            let result = compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .expect("nested exhaustive coverage must compile");
            assert_eq!(&result.artifact_bytes[..4], b"\x7fELF");
            result.validate().unwrap();
        }
    }

    #[test]
    fn builtin_option_and_generic_fixed_vector_use_the_same_kernel() {
        let source = r#"
module generics::prelude;

fn duplicate<T: copy + drop + store + fixed + serializable + non_linear>(value: T) -> [T; 2] {
    return [value, value];
}

action verify() -> u64 {

        let values: [u64; 2] = duplicate::<u64>(21);
        let optional: Option<u64> = Option::Some::<u64>(values[1]);
        return match optional {
            Option::Some(value) => { value }
            Option::None => { 0 }
        };
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        assert!(result.metadata.generic_instantiations.iter().any(|entry| entry.identity == "generics::prelude::Option<u64>"));
        assert!(result.metadata.generic_instantiations.iter().any(|entry| entry.identity == "generics::prelude::duplicate<u64>"));
    }

    #[test]
    fn phantom_arguments_change_identity_without_hiding_cell_layout() {
        let source = r#"
module generics::phantom;

resource AssetA has consume { amount: u64, }
resource AssetB has consume { amount: u64, }

struct Tagged<phantom T> has copy, drop, store, fixed, serializable, non_linear {
    value: u64,
}

action verify() -> u64 {

        let left: Tagged<AssetA> = new Tagged<AssetA> { value: 20, };
        let right: Tagged<AssetB> = new Tagged<AssetB> { value: 22, };
        return left.value + right.value;
}
"#;
        let result = compile(source, CompileOptions::default()).unwrap();
        let identities = result
            .metadata
            .generic_instantiations
            .iter()
            .filter(|entry| entry.template == "Tagged")
            .map(|entry| entry.identity.as_str())
            .collect::<Vec<_>>();
        assert_eq!(identities, ["generics::phantom::Tagged<AssetA>", "generics::phantom::Tagged<AssetB>"]);
    }

    #[test]
    fn generic_kernel_rejects_phantom_layout_use_and_hidden_cells() {
        let unsafe_public = compile(
            r#"
module generics::unsafe_public;
public struct Box<T: copy + drop> { value: T, }
"#,
            CompileOptions { edition: crate::CURRENT_EDITION, ..Default::default() },
        )
        .unwrap_err();
        assert_eq!(unsafe_public.code.as_deref(), Some("E2110"));
        assert!(unsafe_public.message.contains("add 'fixed_value'"));

        let phantom = compile(
            r#"
module generics::bad_phantom;
struct Bad<phantom T> { value: T, }
"#,
            CompileOptions::default(),
        )
        .unwrap_err();
        assert_eq!(phantom.code.as_deref(), Some("E2110"));
        assert!(phantom.message.contains("appears in serialized layout"));

        let hidden = compile(
            r#"
module generics::hidden_cell;
resource Token has consume { amount: u64, }
private struct Box<T> { value: T, }
fn hide(value: Token) -> Box<Token> { new Box<Token> { value, } }
"#,
            CompileOptions::default(),
        )
        .unwrap_err();
        assert_eq!(hidden.code.as_deref(), Some("E2111"));
        assert!(hidden.message.contains("cannot be hidden in ordinary generic layout"));

        let transitively_hidden = compile(
            r#"
module generics::transitively_hidden_cell;
resource Token has store, consume { amount: u64, }
struct Wrapper has fixed, serializable { token: Token, }
struct Box<T: fixed + serializable + non_linear>
    has fixed, serializable, non_linear
{
    value: T,
}
action launder(token: Token) {

        let wrapper: Wrapper = new Wrapper { token, };
        consume token;
        let boxed: Box<Wrapper> = new Box<Wrapper> { value: wrapper, };
}
"#,
            CompileOptions::default(),
        )
        .unwrap_err();
        assert_eq!(transitively_hidden.code.as_deref(), Some("E2111"));
        assert!(transitively_hidden.message.contains("cannot be hidden in ordinary generic layout"));

        let reserved = compile(
            r#"
module generics::reserved;
enum Option<T> { None, Some(T), }
"#,
            CompileOptions::default(),
        )
        .unwrap_err();
        assert_eq!(reserved.code.as_deref(), Some("E2110"));
        assert!(reserved.message.contains("reserved by the CellScript prelude"));
    }
