use super::*;

#[test]
fn reassigned_copies_have_own_storage_and_conflicting_pointer_aliases_terminate() {
    for copied in [false, true] {
        let bytes = IrType::Array(Box::new(IrType::U8), 16);
        let ty = if copied { bytes } else { IrType::Ref(Box::new(bytes)) };
        let first = IrVar { id: 0, name: "first".into(), ty: ty.clone() };
        let second = IrVar { id: 1, name: "second".into(), ty: ty.clone() };
        let selected = IrVar { id: 2, name: "selected".into(), ty };
        let body = IrBody {
            cell_bindings: vec![],
            consume_set: vec![],
            read_refs: vec![],
            create_set: vec![],
            mutate_set: vec![],
            write_intents: vec![],
            bounded_collection_ops: vec![],
            borrow_regions: vec![],
            trusted_external_calls: vec![],
            enforced_claims: vec![],
            zk_origins: vec![],
            blocks: vec![crate::ir::IrBlock {
                id: crate::ir::BlockId(0),
                instructions: vec![
                    IrInstruction::Move { dest: selected.clone(), src: IrOperand::Var(first) },
                    IrInstruction::Move { dest: selected, src: IrOperand::Var(second) },
                ],
                terminator: IrTerminator::Return(None),
                runtime_error: None,
            }],
        };
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.fixed_byte_param_size_offsets.insert(0, 64);
        generator.fixed_byte_param_size_offsets.insert(1, 72);
        if copied {
            generator.fixed_byte_local_offsets.insert(2, 128);
            generator.set_pointer_aliases(&body).unwrap();
            assert!(!generator.fixed_byte_param_size_offsets.contains_key(&2));
        } else {
            let error = generator.set_pointer_aliases(&body).unwrap_err();
            assert!(error.to_string().contains("incompatible source/length bindings"));
        }
    }
}
