#[test]
fn typed_semantics_rejects_operator_and_constant_mutations_after_rebinding() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.opcode == "binary")
        .expect("fixture must contain a binary operation");
    binary.detail = TypedSemanticOperationDetail::BinaryOperator { operator: "and".to_string() };
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let constant = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .flat_map(|operation| &mut operation.operands)
        .find_map(|operand| operand.constant.as_mut())
        .expect("fixture must contain a constant operand");
    *constant = TypedSemanticConstant::U64("01".to_string());
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let constant = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .flat_map(|operation| &mut operation.operands)
        .find_map(|operand| match &mut operand.constant {
            Some(TypedSemanticConstant::U64(value)) => Some(value),
            _ => None,
        })
        .expect("fixture must contain a u64 constant operand");
    *constant = "2".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn typed_semantics_rejects_cross_domain_epoch_since_comparison() {
    let valid = Fixture::from_source(
        r#"
module checker::temporal;

action main() -> bool {

        let left = ckb::since_absolute_epoch(42, 3, 10);
        let right = ckb::since_absolute_epoch(43, 0, 10);
        return left < right;
}
"#,
    );
    let binary = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "AbsoluteEpochSince"));

    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    binary.operands[1].ty = "RelativeEpochSince".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let valid = Fixture::from_source(
        r#"
module checker::temporal_scalar;

action main() -> bool {

        let left = ckb::since_absolute_block(42);
        let right = ckb::since_absolute_block(43);
        return left < right;
}
"#,
    );
    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "lt"))
        .expect("fixture must contain the temporal comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "AbsoluteBlockSince"));
    binary.operands[1].ty = "AbsoluteTimestampSince".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let valid = Fixture::from_source(
        r#"
module checker::header_timestamp;

action main() -> bool {

        let header = ckb::header_dep(0);
        return header.timestamp <= header.timestamp;
}
"#,
    );
    let mut changed = valid.clone();
    let binary = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "le"))
        .expect("fixture must contain the timestamp comparison");
    assert!(binary.operands.iter().all(|operand| operand.ty == "TimestampMillis"));
    binary.operands[1].ty = "BlockNumber".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn typed_semantics_accepts_declared_vec_constructors_and_unsigned_widening() {
    let widened = Fixture::from_source(
        r#"
module checker::widening;

action multiply(witness amount: u64, witness basis_points: u16) -> u64 {

        return amount * basis_points;
}
"#,
    );
    assert!(widened.check().is_ok());

    let empty_array = Fixture::from_source(
        r#"
module checker::empty_array;

action empty() -> [u8; 0] {

        return [];
}
"#,
    );
    assert!(empty_array.check().is_ok());

    let script_tuple = Fixture::from_source(
        r#"
module checker::script_tuple;

action script_value() -> u64 {

        let args = script::args_empty();
        let value = script::new(Hash::zero(), 0, args);
        return 0;
}
"#,
    );
    assert!(script_tuple.check().is_ok());

    let collection = Fixture::from_source(include_str!("../../examples/language/collections/order_book.cell"));
    assert!(collection.check().is_ok());

    let mut changed = collection;
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match &mut operation.detail {
            TypedSemanticOperationDetail::Collection { declared_type } => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain a collection constructor");
    *declared_type = "Map".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn bounded_cell_runtime_contract_is_bound_to_typed_and_machine_evidence() {
    let valid = Fixture::from_source(
        r#"
module checker::bounded_cells;

resource Token has store, consume { amount: u64, }

action verify(input inputs: BoundedCellSet<Token, 2>) -> u64 {

        consume_each token in inputs {
            require token.amount > 0;
        }
        return 0;
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid.clone();
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match (&*operation.opcode, &mut operation.detail) {
            ("bounded-cell-load", TypedSemanticOperationDetail::Collection { declared_type }) => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain the bounded Cell load contract");
    *declared_type = "BoundedCellSet<Token, 0>".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let (entry_id, typed_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| block.operations.iter().any(|operation| operation.opcode == "bounded-cell-load"))
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind the bounded Cell load to a typed block");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == typed_block_id))
        .expect("fixture must contain a machine binding for the bounded Cell load");
    binding.machine_block_ids.pop().expect("bounded Cell load must map to machine code");
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let machine_block = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
            .expect("bounded Cell machine block")
    };
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let loaded = machine_block(".Lbounded_cell_loaded_");
    let out_of_bound = machine_block(".Lbounded_cell_out_of_bound_");
    let lock_distinct = machine_block(".Lbounded_cell_lock_is_distinct_");
    let data_site = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| site.address < loaded.range.start)
        .max_by_key(|site| site.address)
        .expect("bounded data syscall")
        .address;
    let identity_sites = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| loaded.range.start < site.address && site.address < out_of_bound.range.start)
        .map(|site| site.address)
        .collect::<Vec<_>>();
    assert_eq!(identity_sites.len(), 3);

    let mut changed = valid.clone();
    let count_address = machine_block(".Lbounded_cell_loaded_").range.start + 4;
    changed.replace_machine_word(count_address, word_at(count_address) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let decode_address = machine_block(".Lbounded_cell_count_ok_").range.start + 4;
    changed.replace_machine_word(decode_address, word_at(decode_address) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let identity_address = machine_block(".Lbounded_cell_type_word_ok_").range.start - 12;
    changed.replace_machine_word(identity_address, word_at(identity_address) ^ (3 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let data_index_address = elf
        .instructions
        .iter()
        .rev()
        .find(|instruction| instruction.address < data_site && instruction.word & 0x7f == 0x13 && (instruction.word >> 7) & 0x1f == 13)
        .expect("bounded data syscall a3 definition")
        .address;
    changed.replace_machine_word(data_index_address, encode_addi(13, 0, 7));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut type_word_blocks = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lbounded_cell_type_word_ok_")))
        .collect::<Vec<_>>();
    type_word_blocks.sort_by_key(|block| block.range.start);
    let repeated_type_word_address = type_word_blocks[1].range.start - 20;
    let repeated_type_word = word_at(repeated_type_word_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(
        repeated_type_word_address,
        replace_i_immediate(repeated_type_word, ((repeated_type_word as i32) >> 20) - 8),
    );
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(lock_distinct.range.start - 60, encode_addi(7, 0, 1));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(identity_sites[1] + 16, encode_addi(11, 0, 31));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let success_pointer_address = lock_distinct.range.start;
    let success_pointer = word_at(success_pointer_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(success_pointer_address, replace_i_immediate(success_pointer, ((success_pointer as i32) >> 20) + 8));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let absent_destination_address = out_of_bound.range.start;
    let absent_destination = word_at(absent_destination_address);
    let mut changed = valid.clone();
    changed.replace_machine_word(
        absent_destination_address,
        replace_s_immediate(absent_destination, decode_s_immediate(absent_destination) + 8),
    );
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid;
    let (entry_id, predicate_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| {
                    block.operations.iter().any(|operation| {
                        operation.opcode == "binary"
                            && matches!(
                                &operation.detail,
                                TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "gt"
                            )
                    })
                })
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind its per-element predicate");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == predicate_block_id))
        .expect("predicate must have a machine binding");
    binding.machine_block_ids.clear();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn bounded_output_plan_contract_is_bound_to_typed_and_machine_evidence() {
    let valid = Fixture::from_source(
        r#"
module checker::bounded_outputs;

struct Plan { owner: Address, amount: u64, }
resource Token has store, create
with_capacity_floor(10000000000)
{ amount: u64, }

action verify(witness plans: BoundedList<Plan, 2>) -> u64 {

        create_each plan in plans {
            require plan.amount > 0;
            create Token { amount: plan.amount, } with_lock(plan.owner)
        }
        return 0;
}
"#,
    );
    assert!(valid.check().is_ok());

    let mut changed = valid.clone();
    changed.record.compatibility_profile.metadata_schema_version = 71;
    changed.record.compatibility_profile.id = changed.record.compatibility_profile.id.replace("metadata-73-", "metadata-72-");
    changed.record.compatibility_profile_hash =
        canonical_hash("cellscript-compatibility-profile-identity-v1", &changed.record.compatibility_profile).unwrap();
    changed.metadata["metadata_schema_version"] = Value::from(71);
    changed.metadata["compatibility_profile"] = serde_json::to_value(&changed.record.compatibility_profile).unwrap();
    changed.metadata["public_interface"]["runtime_contract"]["compatibility_profile_id"] =
        Value::String(changed.record.compatibility_profile.id.clone());
    changed.metadata["constraints"]["compatibility_profile"] = Value::String(changed.record.compatibility_profile.id.clone());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let declared_type = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match (&*operation.opcode, &mut operation.detail) {
            ("bounded-plan-load", TypedSemanticOperationDetail::Collection { declared_type }) => Some(declared_type),
            _ => None,
        })
        .expect("fixture must contain the bounded output plan decoder");
    *declared_type = "BoundedList<Plan, 0>".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let (entry_id, typed_block_id) = changed
        .record
        .typed_semantics
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .blocks
                .iter()
                .find(|block| block.operations.iter().any(|operation| operation.opcode == "bounded-output-verify"))
                .map(|block| (entry.id.clone(), block.id))
        })
        .expect("fixture must bind output verification to a typed block");
    let binding = changed
        .record
        .entries
        .iter_mut()
        .find(|entry| entry.id == entry_id)
        .and_then(|entry| entry.typed_blocks.iter_mut().find(|binding| binding.id == typed_block_id))
        .expect("fixture must contain a machine binding for bounded output verification");
    binding.machine_block_ids.clear();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    for (pointer, replacement) in [
        ("/runtime/collection_instantiations/0/bounded_output_plan/ordering", Value::String("reverse-plan-order".to_string())),
        ("/runtime/collection_instantiations/0/bounded_output_plan/identity_policy", Value::String("content-hash".to_string())),
        ("/runtime/collection_instantiations/0/bounded_output_plan/field_bindings/0/plan_offset_bytes", Value::from(0)),
    ] {
        let mut changed = valid.clone();
        *changed.metadata.pointer_mut(pointer).expect("bounded output contract field") = replacement;
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    let machine_block = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
            .expect("bounded output machine block")
    };
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
    let exact = machine_block(".Lbounded_output_count_exact_");
    let type_only = machine_block(".Lbounded_output_type_only_");
    let capacity_ok = machine_block(".Lbounded_output_capacity_ok_");
    let output_sites = valid
        .record
        .syscall_sites
        .iter()
        .filter(|site| {
            valid
                .record
                .blocks
                .iter()
                .find(|block| block.id == site.block_id)
                .is_some_and(|block| block.owner_entry == "action:verify" && exact.range.end <= site.address)
        })
        .collect::<Vec<_>>();
    assert_eq!(output_sites.len(), 4);

    let mut changed = valid.clone();
    let first_magic = machine_block(".Lbounded_plan_magic_ok_").range.start;
    changed.replace_machine_word(first_magic - 16, word_at(first_magic - 16) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(exact.range.start - 8, word_at(exact.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let data_site = output_sites[0].address;
    let data_index_move = elf
        .instructions
        .iter()
        .filter(|instruction| instruction.address < data_site)
        .rev()
        .find(|instruction| {
            let word = instruction.word;
            word & 0x7f == 0x13 && (word >> 7) & 0x1f == 13 && (word >> 15) & 0x1f == 28
        })
        .expect("GroupOutput data ordinal move")
        .address;
    changed.replace_machine_word(data_index_move, word_at(data_index_move) ^ (1 << 15));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(data_site + 16, word_at(data_site + 16) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(type_only.range.start + 12, word_at(type_only.range.start + 12) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(type_only.range.start - 8, word_at(type_only.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    changed.replace_machine_word(capacity_ok.range.start - 8, word_at(capacity_ok.range.start - 8) ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}
