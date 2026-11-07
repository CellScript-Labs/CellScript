//! Contract tests for compiler-side value transport, independent of either
//! entry-witness decoder and of the assembler's text/ELF placement.
use super::*;

fn params(types: impl IntoIterator<Item = IrType>) -> Vec<IrParam> {
    types
        .into_iter()
        .enumerate()
        .map(|(id, ty)| {
            let name = format!("arg_{id}");
            IrParam {
                name: name.clone(),
                ty: ty.clone(),
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Default,
                binding: IrVar { id, name, ty },
            }
        })
        .collect()
}

fn fieldless_enum(generator: &mut CodeGenerator) -> IrType {
    let name = "Choice".to_string();
    generator.enum_layouts.insert(
        name.clone(),
        IrEnumLayout {
            name: name.clone(),
            value_abilities: Vec::new(),
            tag_width: 1,
            encoded_size: 1,
            variants: vec![IrEnumVariantLayout { name: "First".to_string(), tag: 0, payload_width: 0, fields: Vec::new() }],
            contains_linear_payload: false,
        },
    );
    IrType::Named(name)
}

#[test]
fn scalar_register_frontier_reserves_only_aligned_outgoing_words() {
    let generator = CodeGenerator::new(CodegenOptions::default());
    for (count, stack_bytes) in [(0, 0), (7, 0), (8, 0), (9, 16)] {
        let parameters = params(std::iter::repeat_n(IrType::U64, count));
        let layout = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::SingleWord).unwrap();
        assert_eq!(layout.input_words, count);
        assert_eq!(layout.outgoing_stack_bytes, stack_bytes);
        assert_eq!(layout.hidden_result_word(), None);
        assert_eq!(layout.parameters.iter().map(|param| param.first_word).collect::<Vec<_>>(), (0..count).collect::<Vec<_>>());
        assert!(layout.parameters.iter().all(|param| param.representation == ParameterRepresentation::Scalar));
    }
}

#[test]
fn index_and_five_u128_parameters_keep_the_split_word_seven_boundary() {
    let generator = CodeGenerator::new(CodegenOptions::default());
    let parameters = params([IrType::U64, IrType::U128, IrType::U128, IrType::U128, IrType::U128, IrType::U128]);
    let layout = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::Unit).unwrap();
    assert_eq!(layout.parameters.iter().map(|param| param.first_word).collect::<Vec<_>>(), [0, 1, 3, 5, 7, 9]);
    assert_eq!(layout.parameters[4].representation, ParameterRepresentation::FixedBytes { bytes: 16 });
    // This u128 starts in a7 and ends at the first outgoing stack word. Source
    // parameter count six must not be used as the machine argument count.
    assert_eq!(layout.parameters[4].representation.words(), 2);
    assert_eq!(layout.input_words, 11);
    assert_eq!(layout.outgoing_stack_bytes, 32);
    assert_eq!(layout.hidden_result_word(), None);
}

#[test]
fn caller_owned_result_follows_all_ordinary_words_and_changes_stack_rounding() {
    let generator = CodeGenerator::new(CodegenOptions::default());
    let parameters = params(std::iter::repeat_n(IrType::U128, 5));
    let ordinary = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::WidePair).unwrap();
    let buffered = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::CallerOwned { bytes: 32 }).unwrap();
    assert_eq!(ordinary.input_words, 10);
    assert_eq!(ordinary.outgoing_stack_bytes, 16);
    assert_eq!(ordinary.hidden_result_word(), None);
    assert_eq!(buffered.input_words, 10, "the hidden result is not an ordinary source argument");
    assert_eq!(buffered.hidden_result_word(), Some(10));
    assert_eq!(buffered.outgoing_stack_bytes, 32);
    assert_eq!(buffered.result.buffer_bytes(), Some(32));
    assert_eq!(buffered.parameters.iter().map(|param| param.first_word).collect::<Vec<_>>(), [0, 2, 4, 6, 8]);
}

#[test]
fn equal_wire_widths_do_not_merge_distinct_argument_objects_or_representations() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.type_fixed_sizes.insert("Cell32".to_string(), 32);
    generator.cell_type_names.insert("Cell32".to_string());
    generator.type_fixed_sizes.insert("Value32".to_string(), 32);
    generator.specialized_type_names.insert("Value32".to_string());
    let parameters = params([
        IrType::Hash,
        IrType::Commitment(Box::new(IrType::U64)),
        IrType::Named("Cell32".to_string()),
        IrType::Named("Value32".to_string()),
        IrType::Array(Box::new(IrType::U64), 4),
    ]);
    let layout = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::Unit).unwrap();
    assert_eq!(
        layout.parameters.iter().map(|param| param.representation).collect::<Vec<_>>(),
        [
            ParameterRepresentation::FixedBytes { bytes: 32 },
            ParameterRepresentation::Commitment,
            ParameterRepresentation::SchemaSpan { type_hash: false },
            ParameterRepresentation::FixedNamed { bytes: 32 },
            ParameterRepresentation::FixedAggregate { bytes: 32 },
        ]
    );
    assert_eq!(layout.parameters.iter().map(|param| param.first_word).collect::<Vec<_>>(), [0, 2, 4, 6, 8]);
    assert_eq!(layout.input_words, 10);
    assert_eq!(layout.outgoing_stack_bytes, 16);
}

#[test]
fn one_byte_fieldless_enum_keeps_its_reserved_two_word_transport() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    let choice = fieldless_enum(&mut generator);
    let parameters = params([choice, IrType::U64]);
    let layout = CallableLayout::new(&generator, &parameters, &BTreeSet::from([0]), ReturnLayout::SingleWord).unwrap();
    assert_eq!(layout.parameters[0].representation, ParameterRepresentation::FieldlessEnum { bytes: 1 });
    assert_eq!(layout.parameters[1].first_word, 2);
    assert_eq!(layout.input_words, 3);
    assert_eq!(layout.outgoing_stack_bytes, 0);
}

#[test]
fn type_hash_profile_adds_words_only_to_schema_span_parameters() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    let choice = fieldless_enum(&mut generator);
    let parameters = params([
        IrType::U64,
        IrType::Hash,
        IrType::Named("LiveCell".to_string()),
        IrType::vector(IrType::U64),
        IrType::Commitment(Box::new(IrType::U64)),
        IrType::U128,
        IrType::Named("BlockNumber".to_string()),
        choice,
    ]);
    let plain = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::Unit).unwrap();
    let profiled = CallableLayout::new(&generator, &parameters, &(0..parameters.len()).collect(), ReturnLayout::Unit).unwrap();
    assert_eq!(plain.input_words, 14);
    assert_eq!(profiled.input_words, 18);
    for (index, (plain, profiled)) in plain.parameters.iter().zip(&profiled.parameters).enumerate() {
        if [2, 3].contains(&index) {
            assert_eq!(plain.representation, ParameterRepresentation::SchemaSpan { type_hash: false });
            assert_eq!(profiled.representation, ParameterRepresentation::SchemaSpan { type_hash: true });
        } else {
            assert_eq!(plain.representation, profiled.representation, "parameter {index}");
        }
    }
    assert_eq!(profiled.parameters[6].representation, ParameterRepresentation::TemporalScalar);
}

#[test]
fn result_transport_retains_scalar_pair_tuple_and_owned_buffer_boundaries() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.type_layouts.insert("Record32".to_string(), HashMap::new());
    generator.type_fixed_sizes.insert("Record32".to_string(), 32);
    generator.type_layouts.insert("Cell32".to_string(), HashMap::new());
    generator.type_fixed_sizes.insert("Cell32".to_string(), 32);
    generator.cell_type_names.insert("Cell32".to_string());
    generator.enum_layouts.insert(
        "OptionalValue".to_string(),
        IrEnumLayout {
            name: "OptionalValue".to_string(),
            value_abilities: Vec::new(),
            tag_width: 1,
            encoded_size: 9,
            variants: vec![IrEnumVariantLayout {
                name: "Some".to_string(),
                tag: 0,
                payload_width: 8,
                fields: vec![IrEnumFieldLayout { index: 0, ty: IrType::U64, offset: 1, width: 8, linear: false }],
            }],
            contains_linear_payload: false,
        },
    );
    for (ty, expected) in [
        (None, ReturnLayout::Unit),
        (Some(IrType::Unit), ReturnLayout::Unit),
        (Some(IrType::U64), ReturnLayout::SingleWord),
        (Some(IrType::U128), ReturnLayout::WidePair),
        (Some(IrType::Hash), ReturnLayout::CallerOwned { bytes: 32 }),
        (Some(IrType::Address), ReturnLayout::CallerOwned { bytes: 32 }),
        (Some(IrType::Commitment(Box::new(IrType::U64))), ReturnLayout::CallerOwned { bytes: 32 }),
        (Some(IrType::Array(Box::new(IrType::U8), 8)), ReturnLayout::SingleWord),
        (Some(IrType::Array(Box::new(IrType::U8), 9)), ReturnLayout::CallerOwned { bytes: 9 }),
        (Some(IrType::Array(Box::new(IrType::U8), 64)), ReturnLayout::CallerOwned { bytes: 64 }),
        (Some(IrType::Named("Record32".to_string())), ReturnLayout::CallerOwned { bytes: 32 }),
        (Some(IrType::Named("Cell32".to_string())), ReturnLayout::SingleWord),
        (Some(IrType::Named("OptionalValue".to_string())), ReturnLayout::EncodedPair { bytes: 9 }),
        (Some(IrType::Tuple(vec![IrType::U64, IrType::U128])), ReturnLayout::Tuple { fields: 2 }),
    ] {
        assert_eq!(generator.function_return_layout(ty.as_ref()), expected, "{ty:?}");
    }
}

#[test]
fn outgoing_extent_checks_target_width_and_host_arithmetic_before_emission() {
    // The target extent is u32; sixteen-byte alignment makes its largest
    // representable outgoing region smaller than u32::MAX by fifteen bytes.
    assert_eq!(checked_outgoing_stack_bytes(536_870_918).unwrap(), 4_294_967_280);
    for words in [536_870_919, usize::MAX / 8 + 9, usize::MAX] {
        let error = checked_outgoing_stack_bytes(words).expect_err("no overflowing outgoing frame may reach emission");
        assert!(error.message.contains("fixed-value layout exceeds"), "{error}");
    }
}

#[test]
fn layout_mismatch_diagnostics_name_phase_callee_expected_and_actual_words() {
    let generator = CodeGenerator::new(CodegenOptions::default());
    let parameters = params([IrType::U64, IrType::U128, IrType::U128]);
    let layout = CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::Unit).unwrap();
    for phase in ["caller", "callee"] {
        layout.check_parameter_start(2, 3, "nested_values", phase).unwrap();
        layout.check_input_words(5, "nested_values", phase).unwrap();
        let error = layout.check_parameter_start(2, 2, "nested_values", phase).unwrap_err();
        for part in [phase, "nested_values", "parameter 2", "expected machine word 3", "reached 2"] {
            assert!(error.message.contains(part), "missing {part}: {error}");
        }
        let error = layout.check_input_words(4, "nested_values", phase).unwrap_err();
        for part in [phase, "nested_values", "expected 5 ordinary machine words", "reached 4"] {
            assert!(error.message.contains(part), "missing {part}: {error}");
        }
        let error = layout.check_parameter_start(3, 5, "nested_values", phase).unwrap_err();
        assert!(error.message.contains("unexpected source parameter 3"), "{error}");
    }
}

fn select_return_layout(generator: &mut CodeGenerator, result: ReturnLayout) {
    let layout = CallableLayout::new(generator, &[], &BTreeSet::new(), result).unwrap();
    generator.current_function = Some("classified_result".to_string());
    generator.callable_abis.insert(
        "classified_result".to_string(),
        CallableAbi {
            params: Vec::new(),
            checked_scalar_parameters: false,
            type_hash_param_indices: BTreeSet::new(),
            runtime_bound_param_indices: BTreeSet::new(),
            bounded_plan_param_indices: BTreeSet::new(),
            layout,
        },
    );
}

#[test]
fn return_plan_rejects_reverse_transport_mismatches_and_absent_required_values() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    let wide = IrOperand::Const(IrConst::U128(1));
    let scalar = IrOperand::Const(IrConst::U64(1));
    let invalid_array = IrOperand::Const(IrConst::Array(vec![IrConst::U64(1)]));
    let tuple = IrOperand::Var(IrVar { id: 0, name: "pair".to_string(), ty: IrType::Tuple(vec![IrType::U64, IrType::U64]) });
    for (expected, actual) in [
        (ReturnLayout::SingleWord, Some(&wide)),
        (ReturnLayout::SingleWord, Some(&tuple)),
        (ReturnLayout::SingleWord, None),
        (ReturnLayout::SingleWord, Some(&invalid_array)),
        (ReturnLayout::Unit, Some(&wide)),
        (ReturnLayout::WidePair, None),
        (ReturnLayout::WidePair, Some(&scalar)),
        (ReturnLayout::CallerOwned { bytes: 32 }, None),
        (ReturnLayout::CallerOwned { bytes: 32 }, Some(&wide)),
        (ReturnLayout::Tuple { fields: 1 }, Some(&tuple)),
        (ReturnLayout::EntryValue(Box::new(ReturnLayout::WidePair)), Some(&scalar)),
    ] {
        select_return_layout(&mut generator, expected);
        let error = generator.validate_return_transport(actual).expect_err("return shape must agree with its classified plan");
        assert!(error.message.contains("return-layout mismatch for classified_result"), "{error}");
    }
}

#[test]
fn valid_return_transports_and_annotated_entry_values_keep_the_existing_abi() {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    let unit = IrOperand::Const(IrConst::Unit);
    let scalar = IrOperand::Const(IrConst::U64(7));
    let wide = IrOperand::Const(IrConst::U128(1 << 100));
    let short_bytes = IrOperand::Const(IrConst::Array(vec![IrConst::U8(7); 3]));
    let buffered_bytes = IrOperand::Const(IrConst::Array(vec![IrConst::U8(7); 9]));
    let digest = IrOperand::Var(IrVar { id: 0, name: "digest".to_string(), ty: IrType::Hash });
    let tuple = IrOperand::Var(IrVar { id: 1, name: "pair".to_string(), ty: IrType::Tuple(vec![IrType::U64, IrType::U128]) });
    for (expected, actual) in [
        (ReturnLayout::Unit, None),
        (ReturnLayout::Unit, Some(&unit)),
        (ReturnLayout::EntryStatus, None),
        (ReturnLayout::EntryStatus, Some(&scalar)),
        (ReturnLayout::SingleWord, Some(&scalar)),
        (ReturnLayout::SingleWord, Some(&short_bytes)),
        (ReturnLayout::WidePair, Some(&wide)),
        (ReturnLayout::CallerOwned { bytes: 32 }, Some(&digest)),
        (ReturnLayout::CallerOwned { bytes: 9 }, Some(&buffered_bytes)),
        (ReturnLayout::Tuple { fields: 2 }, Some(&tuple)),
        (ReturnLayout::EntryValue(Box::new(ReturnLayout::WidePair)), Some(&wide)),
    ] {
        select_return_layout(&mut generator, expected);
        generator.validate_return_transport(actual).unwrap();
    }
    let parameters = params(std::iter::repeat_n(IrType::U128, 5));
    for result in [ReturnLayout::WidePair, ReturnLayout::CallerOwned { bytes: 32 }] {
        let entry =
            CallableLayout::new(&generator, &parameters, &BTreeSet::new(), ReturnLayout::EntryValue(Box::new(result))).unwrap();
        assert_eq!(entry.input_words, 10);
        assert_eq!(entry.result.buffer_bytes(), None, "annotated entries must not acquire ordinary-function hidden output storage");
        assert_eq!(entry.hidden_result_word(), None);
        assert_eq!(entry.outgoing_stack_bytes, 16);
    }
}
