use crate::ir::{BlockId, IrBlock, IrBody, IrCallTarget, IrInstruction, IrTerminator, IrType, IrVar};
use cellscript_artifact_checker::runtime_contract::RuntimeHelper;

#[test]
fn open_verifier_delegation_evidence_uses_the_structured_handle_class() {
    use crate::ast::{OpenHandleClass, OpenHandleType};
    use crate::ir::{IrConst, IrOperand};
    let mut body = IrBody {
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
        blocks: vec![],
    };
    let view = IrVar { id: 1, name: "dep".into(), ty: IrType::U64 };
    let bytes = IrVar { id: 3, name: "bytes".into(), ty: IrType::vector(IrType::U8) };
    let verifier = IrType::OpenHandle(Box::new(OpenHandleType { class: OpenHandleClass::Verifier, interface: "foreign".into() }));
    let cases = [
        (verifier, true),
        (IrType::OpenHandle(Box::new(OpenHandleType { class: OpenHandleClass::Script, interface: "foreign".into() })), false),
        (IrType::OpenHandle(Box::new(OpenHandleType { class: OpenHandleClass::Instance, interface: "foreign".into() })), false),
        (IrType::OpenHandle(Box::new(OpenHandleType { class: OpenHandleClass::Verifier, interface: "foreign::".into() })), false),
        (IrType::Named("VerifierHandle<foreign>".into()), false),
    ];
    for (ty, expected) in cases {
        let handle = IrVar { id: 2, name: "handle".into(), ty };
        body.blocks = vec![IrBlock {
            id: BlockId(0),
            instructions: vec![
                IrInstruction::Call {
                    dest: Some(view.clone()),
                    func: IrCallTarget::Runtime(RuntimeHelper::CkbSourceCellDep),
                    args: vec![IrOperand::Const(IrConst::U64(0))],
                },
                IrInstruction::Call {
                    dest: None,
                    func: IrCallTarget::Runtime(RuntimeHelper::CkbRequireCellDepOpenVerifierHandleFromArgs),
                    args: vec![IrOperand::Var(view.clone()), IrOperand::Var(handle)],
                },
                IrInstruction::Call {
                    dest: None,
                    func: IrCallTarget::Runtime(RuntimeHelper::CkbSpawnWaitCellDepHex4),
                    args: vec![
                        IrOperand::Const(IrConst::U64(0)),
                        IrOperand::Var(bytes.clone()),
                        IrOperand::Const(IrConst::U64(1)),
                        IrOperand::Const(IrConst::U64(0)),
                        IrOperand::Const(IrConst::U64(0)),
                        IrOperand::Const(IrConst::U64(0)),
                    ],
                },
            ],
            terminator: IrTerminator::Return(None),
            runtime_error: None,
        }];
        assert_eq!(crate::body_open_verifier_spawn_is_bound(&body), expected);
    }
}

#[test]
fn ordinary_function_labels_cannot_acquire_runtime_hash_storage_evidence() {
    for helper in [RuntimeHelper::CkbCurrentScriptHash, RuntimeHelper::CkbTransactionHash] {
        for runtime in [false, true] {
            let destination = IrVar { id: 7, name: "hash".into(), ty: IrType::Hash };
            let target = if runtime { IrCallTarget::Runtime(helper) } else { IrCallTarget::Function(helper.symbol().into()) };
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
                blocks: vec![IrBlock {
                    id: BlockId(0),
                    instructions: vec![IrInstruction::Call { dest: Some(destination.clone()), func: target, args: vec![] }],
                    terminator: IrTerminator::Return(None),
                    runtime_error: None,
                }],
            };
            let availability =
                crate::metadata_prelude_availability(&body, &Default::default(), &Default::default(), &[], &Default::default());
            assert_eq!(availability.fixed_value_vars.contains(&destination.id), runtime, "{helper:?}, runtime={runtime}");
            assert_eq!(availability.aggregate_pointer_vars.contains_key(&destination.id), runtime, "{helper:?}, runtime={runtime}");
        }
    }
}
