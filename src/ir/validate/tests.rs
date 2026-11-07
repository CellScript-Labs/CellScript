//! Mutation tests exercise structural admission without treating mutable locals
//! as SSA values or treating this compiler check as a machine/source proof.
use super::*;
use crate::codegen::{CodeGenerator, CodegenOptions};
use crate::ArtifactFormat;

fn module() -> IrModule {
    let ast = crate::frontend::parse(
        "module structural_admission\nstruct Pair { left: u64, right: u64 }\naction verify() -> u64 { verification return 0 }",
        crate::CellScriptEdition::Edition2027,
    )
    .unwrap();
    crate::types::check(&ast).unwrap();
    super::super::generate(&ast).unwrap()
}

fn action(module: &mut IrModule) -> &mut IrAction {
    module
        .items
        .iter_mut()
        .find_map(|item| match item {
            IrItem::Action(value) => Some(value),
            _ => None,
        })
        .unwrap()
}

fn var(id: usize, ty: IrType) -> IrVar {
    IrVar { id, name: format!("local_{id}"), ty }
}

fn parameter(id: usize, ty: IrType) -> IrParam {
    IrParam {
        name: format!("argument_{id}"),
        ty: ty.clone(),
        is_mut: false,
        is_ref: false,
        is_read_ref: false,
        source: ParamSource::Witness,
        binding: var(id, ty),
    }
}

fn block(id: usize, instructions: Vec<IrInstruction>, terminator: IrTerminator) -> IrBlock {
    IrBlock { id: BlockId(id), instructions, terminator, runtime_error: None }
}

fn rejects(module: &IrModule, detail: &str) {
    let error = validate_structure(module).unwrap_err();
    assert_eq!(error.code, Some(DiagnosticCode::E2105), "{error:?}");
    assert!(error.message.contains(detail), "{error:?}");
}

#[test]
fn ir_structure_accepts_mutable_local_redefinitions_loops_and_noncontiguous_block_ids() {
    let mut module = module();
    let value = var(1, IrType::U64);
    let condition = var(2, IrType::Bool);
    let entry = action(&mut module);
    entry.params = vec![parameter(2, IrType::Bool)];
    entry.body.blocks = vec![
        block(7, vec![IrInstruction::LoadConst { dest: value.clone(), value: IrConst::U64(0) }], IrTerminator::Jump(BlockId(19))),
        block(
            19,
            vec![IrInstruction::Binary {
                dest: value.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(value.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            }],
            IrTerminator::Branch { cond: IrOperand::Var(condition), then_block: BlockId(19), else_block: BlockId(43) },
        ),
        block(43, Vec::new(), IrTerminator::Return(Some(IrOperand::Var(value)))),
    ];
    validate_structure(&module).unwrap();
}

#[test]
fn ir_structure_rejects_empty_duplicate_and_missing_blocks() {
    let original = module();
    let mut empty = original.clone();
    action(&mut empty).body.blocks.clear();
    rejects(&empty, "no entry block");

    let mut duplicate = original.clone();
    let first = action(&mut duplicate).body.blocks[0].clone();
    action(&mut duplicate).body.blocks.push(first);
    rejects(&duplicate, "repeats block");

    let mut missing_jump = original.clone();
    action(&mut missing_jump).body.blocks[0].terminator = IrTerminator::Jump(BlockId(usize::MAX));
    rejects(&missing_jump, "targets missing block");

    let mut missing_branch = original;
    let first = action(&mut missing_branch).body.blocks[0].id;
    action(&mut missing_branch).body.blocks[0].terminator =
        IrTerminator::Branch { cond: IrOperand::Const(IrConst::Bool(true)), then_block: first, else_block: BlockId(usize::MAX) };
    // Even the untaken literal branch must refer to a retained block.
    rejects(&missing_branch, "targets missing block");
}

#[test]
fn ir_structure_rejects_conflicting_local_types_and_non_boolean_branches() {
    let mut conflict = module();
    action(&mut conflict).body.blocks[0].instructions = vec![
        IrInstruction::LoadConst { dest: var(4, IrType::U64), value: IrConst::U64(1) },
        IrInstruction::Call {
            dest: None,
            func: IrCallTarget::Function("inspect".into()),
            args: vec![IrOperand::Var(var(4, IrType::Bool))],
        },
    ];
    rejects(&conflict, "conflicting types");

    let mut branch = module();
    let first = action(&mut branch).body.blocks[0].id;
    action(&mut branch).body.blocks[0].terminator =
        IrTerminator::Branch { cond: IrOperand::Const(IrConst::U64(1)), then_block: first, else_block: first };
    rejects(&branch, "non-boolean branch");

    let mut return_conflict = module();
    let entry = action(&mut return_conflict);
    entry.body.blocks[0].instructions = vec![IrInstruction::LoadConst { dest: var(4, IrType::U64), value: IrConst::U64(1) }];
    entry.body.blocks[0].terminator = IrTerminator::Return(Some(IrOperand::Var(var(4, IrType::Bool))));
    rejects(&return_conflict, "conflicting types");
}

#[test]
fn ir_structure_rejects_inconsistent_parameter_bindings() {
    let mut mismatched = module();
    let mut param = parameter(3, IrType::U64);
    param.binding.ty = IrType::Bool;
    action(&mut mismatched).params = vec![param];
    rejects(&mismatched, "inconsistent parameter binding");

    let mut repeated = module();
    let first = parameter(3, IrType::U64);
    let mut second = first.clone();
    second.name = "second".into();
    action(&mut repeated).params = vec![first, second];
    rejects(&repeated, "inconsistent parameter binding");
}

#[test]
fn ir_structure_accepts_borrow_backing_aliases_and_vector_inference_refinement() {
    for concrete_first in [false, true] {
        let mut module = module();
        let empty = var(5, IrType::Vector(None));
        let typed = var(5, IrType::vector(IrType::U64));
        let borrowed = var(5, IrType::Ref(Box::new(IrType::vector(IrType::U64))));
        let mutable = var(5, IrType::MutRef(Box::new(IrType::vector(IrType::U64))));
        let mut values = vec![empty, typed, borrowed, mutable];
        if concrete_first {
            values.reverse();
        }
        action(&mut module).body.blocks[0].instructions = vec![IrInstruction::Call {
            dest: None,
            func: IrCallTarget::Function("inspect".into()),
            args: values.into_iter().map(IrOperand::Var).collect(),
        }];
        validate_structure(&module).unwrap();

        action(&mut module).body.blocks[0]
            .instructions
            .push(IrInstruction::CollectionClear { collection: IrOperand::Var(var(5, IrType::vector(IrType::Bool))) });
        rejects(&module, "conflicting types");
    }
}

fn nominal(module_name: &str, name: &str) -> crate::resolve::ResolvedTypeIdentity {
    crate::resolve::ResolvedTypeIdentity::Nominal {
        declaration: DeclarationIdentity { module: module_name.into(), name: name.into() },
        arguments: Vec::new(),
    }
}

#[test]
fn ir_structure_uses_retained_nominal_alias_identity_and_rejects_ambiguous_repeated_locals() {
    let mut module = module();
    module.nominal_identities.insert("First".into(), Some(nominal("provider", "Pair")));
    module.nominal_identities.insert("Alias".into(), Some(nominal("provider", "Pair")));
    action(&mut module).body.blocks[0].instructions = vec![IrInstruction::Move {
        dest: var(8, IrType::Named("Alias".into())),
        src: IrOperand::Var(var(8, IrType::Named("First".into()))),
    }];
    validate_structure(&module).unwrap();
    module.nominal_identities.insert("Alias".into(), Some(nominal("other", "Pair")));
    rejects(&module, "conflicting types");

    module.nominal_identities.insert("First".into(), None);
    action(&mut module).body.blocks[0].instructions = vec![IrInstruction::Move {
        dest: var(8, IrType::Named("First".into())),
        src: IrOperand::Var(var(8, IrType::Named("First".into()))),
    }];
    rejects(&module, "conflicting types");
}

#[test]
fn ir_structure_public_codegen_rejects_builtin_declarations_external_definitions_and_nominal_aliases() {
    let original = module();
    let definition = original
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::TypeDef(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap();
    for kind in ["declaration", "external", "alias", "origin"] {
        let mut module = original.clone();
        match kind {
            "declaration" => {
                let mut conflicting = definition.clone();
                conflicting.name = "BlockNumber".into();
                module.items.push(IrItem::TypeDef(conflicting));
            }
            "external" => {
                let mut conflicting = definition.clone();
                conflicting.name = "ScriptHash".into();
                module.external_type_defs.push(conflicting);
            }
            "alias" => {
                module.nominal_identities.insert("ExactScriptHandle".into(), Some(nominal("provider", "Pair")));
            }
            "origin" => {
                module
                    .source_type_origins
                    .insert("EpochNumber".into(), Some(DeclarationIdentity { module: "provider".into(), name: "Pair".into() }));
            }
            _ => unreachable!(),
        }
        let error = CodeGenerator::new(CodegenOptions::default()).generate(&module, ArtifactFormat::RiscvAssembly).unwrap_err();
        assert_eq!(error.code, Some(DiagnosticCode::E2105), "{kind}: {error:?}");
        assert!(error.message.contains("reserved builtin representation"), "{kind}: {error:?}");
    }
}

#[test]
fn ir_structure_public_codegen_preserves_builtin_uses_and_ordinary_nominal_controls() {
    let ast = crate::frontend::parse(
        "module builtin_control\nstruct MyBlockNumber { left: u64, right: u64 }\naction verify(witness block: BlockNumber) -> u64 { verification require ckb::block_number_to_u64(block) == 42 return 0 }",
        crate::CellScriptEdition::Edition2027,
    ).unwrap();
    crate::types::check(&ast).unwrap();
    let mut module = super::super::generate(&ast).unwrap();
    module.nominal_identities.insert("provider::BlockNumber".into(), Some(nominal("provider", "BlockNumber")));
    validate_structure(&module).unwrap();
    assert!(!CodeGenerator::new(CodegenOptions::default()).generate(&module, ArtifactFormat::RiscvAssembly).unwrap().is_empty());
}
