//! Exact experimental Groth16 integration contract. No proving code lives here.

pub const PROOF_TYPE: &str = "ZkTransitionProof";
pub const PROOF_BYTES: usize = 128;
pub const REQUIRE_HELPER: &str = "__zk_require_transition";

pub fn fixed_width(name: &str) -> Option<usize> {
    (name == PROOF_TYPE).then_some(PROOF_BYTES)
}

/// First profile deliberately permits one unconditional call in an action's
/// entry block. Helpers, loops and alternative accepting paths need a separate
/// effect/call-count contract before admission.
pub(crate) fn validate(module: &crate::ir::IrModule) -> crate::error::Result<()> {
    use crate::ir::{IrInstruction, IrItem, IrOperand};
    for item in &module.items {
        if matches!(item, IrItem::TypeDef(definition) if matches!(definition.name.as_str(), "ZkTransitionProof" | "VerificationKeyCommitment"))
        {
            return Err(crate::error::CompileError::without_span(
                "ZK profile types are reserved nominal types and cannot be redefined",
            ));
        }
        let (body, action) = match item {
            IrItem::Action(entry) => (&entry.body, true),
            IrItem::Lock(entry) => (&entry.body, false),
            IrItem::PureFn(entry) => (&entry.body, false),
            _ => continue,
        };
        let calls: Vec<_> = body
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(index, block)| {
                block.instructions.iter().filter_map(move |instruction| match instruction {
                    IrInstruction::Call { func, args, .. } if func == REQUIRE_HELPER => Some((index, args)),
                    _ => None,
                })
            })
            .collect();
        if calls.is_empty() {
            continue;
        }
        if let IrItem::Action(entry) = item {
            if entry.entry_trigger.as_deref().is_some_and(|trigger| trigger != "type-group") {
                return Err(crate::error::CompileError::without_span("exact ZK transition profile requires a Type-group action"));
            }
            if !entry.params.iter().any(|parameter| {
                parameter.source == crate::ast::ParamSource::Witness
                    && matches!(&calls[0].1[1], IrOperand::Var(var) if var.id == parameter.binding.id)
            }) {
                return Err(crate::error::CompileError::without_span("ZkTransitionProof must be a direct explicit witness parameter"));
            }
        }
        if !action
            || calls.len() != 1
            || calls[0].0 != 0
            || body.blocks.iter().any(|block| match block.terminator {
                crate::ir::IrTerminator::Jump(target) => target == body.blocks[0].id,
                crate::ir::IrTerminator::Branch { then_block, else_block, .. } => {
                    then_block == body.blocks[0].id || else_block == body.blocks[0].id
                }
                _ => false,
            })
        {
            return Err(crate::error::CompileError::without_span("exact ZK profile requires one unconditional call in the action entry block; helper, loop and conditional verification are unsupported"));
        }
        if !matches!(&calls[0].1[1], IrOperand::Var(var) if var.ty == crate::ir::IrType::Named(PROOF_TYPE.to_string())) {
            return Err(crate::error::CompileError::without_span("exact ZK profile requires a nominal ZkTransitionProof witness"));
        }
    }
    Ok(())
}

/// Browser compilation retains the same exact-profile contract without linking
/// the native artifact/ELF semantic emitter into the WASM bundle.
#[cfg(feature = "wasm")]
pub(crate) fn browser_contracts(module: &crate::ir::IrModule) -> Vec<cellscript_artifact_checker::zk_profile::ZkVerifierContract> {
    use crate::ir::{IrConst, IrInstruction, IrItem, IrOperand};
    use cellscript_artifact_checker::{
        TypedSemanticBlock, TypedSemanticCall, TypedSemanticConstant as C, TypedSemanticEntry, TypedSemanticOperand,
        TypedSemanticOperation, TypedSemanticRecord,
    };
    let mut typed = TypedSemanticRecord::default();
    for item in &module.items {
        let IrItem::Action(action) = item else { continue };
        let mut operations = Vec::new();
        for instruction in action.body.blocks.iter().flat_map(|block| &block.instructions) {
            let IrInstruction::Call { func, args, .. } = instruction else { continue };
            if func != REQUIRE_HELPER {
                continue;
            }
            let operands = args
                .iter()
                .map(|arg| match arg {
                    IrOperand::Var(var) => TypedSemanticOperand { local: Some(var.id as u32), ..Default::default() },
                    IrOperand::Const(IrConst::Hash(hash)) => {
                        TypedSemanticOperand { constant: Some(C::Hash(hex::encode(hash))), ..Default::default() }
                    }
                    IrOperand::Const(IrConst::Array(bytes)) => TypedSemanticOperand {
                        constant: Some(C::Array(
                            bytes
                                .iter()
                                .filter_map(|byte| match byte {
                                    IrConst::U8(byte) => Some(C::U8(byte.to_string())),
                                    _ => None,
                                })
                                .collect(),
                        )),
                        ..Default::default()
                    },
                    _ => unreachable!("validated exact ZK operands"),
                })
                .collect();
            operations.push(TypedSemanticOperation {
                operands,
                call: Some(TypedSemanticCall { target: func.clone(), ..Default::default() }),
                ..Default::default()
            });
        }
        if !operations.is_empty() {
            typed.entries.push(TypedSemanticEntry {
                id: format!("action:{}", action.name),
                zk_origins: action.body.zk_origins.clone(),
                blocks: vec![TypedSemanticBlock { operations, ..Default::default() }],
                ..Default::default()
            });
        }
    }
    cellscript_artifact_checker::zk_profile::contracts(&typed).expect("validated ZK browser contract")
}
