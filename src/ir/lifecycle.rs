//! Lower shared lifecycle statements at their actual CFG occurrence.

use super::{BlockId, IrBlock, IrGenerator, IrInstruction, IrLifecycleExecution, IrOperand, IrPoolAccounting, IrType, IrVar};
use crate::ast::LifecycleStmt;
use crate::error::CompileError;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_lifecycle_statement(
        &mut self,
        source: &LifecycleStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let occurrence = self.lifecycle_occurrence;
        self.lifecycle_occurrence += 1;
        let identity = match source {
            LifecycleStmt::Fresh(fresh) => {
                match self.resolve_source_operation_identity(&fresh.ty, fresh.identity.as_ref(), fresh.span) {
                    Ok(identity) => Some(identity),
                    Err(error) => {
                        self.errors.borrow_mut().push(error);
                        return None;
                    }
                }
            }
            _ => None,
        };
        let mut dispositions = Self::lower_source_disposition(&self.lifecycle_entry_name, source.resolved(), identity.as_ref());
        let starts = blocks.iter().map(|block| (block.id, block.instructions.len())).collect::<HashMap<_, _>>();
        let before = vars.clone();
        let kernel = match source.kernel(identity.as_ref()) {
            Ok(kernel) => kernel,
            Err(error) => {
                self.errors.borrow_mut().push(error);
                return None;
            }
        };
        let success_block = self.lower_stmts(&kernel, current, blocks, vars, return_type, false)?;
        let pool = if let LifecycleStmt::Pool(pool) = source {
            let local = |name: &String| before.get(name).or_else(|| vars.get(name)).map(|var| var.id);
            Some(IrPoolAccounting {
                inputs: pool.inputs.iter().map(local).collect::<Option<Vec<_>>>()?,
                outputs: pool.outputs.iter().map(local).collect::<Option<Vec<_>>>()?,
                conserved_fields: pool
                    .data_fields
                    .iter()
                    .filter_map(|field| {
                        matches!(field.treatment, crate::ast::NextPoolFieldTreatment::Conserve).then_some(field.field.clone())
                    })
                    .collect(),
            })
        } else {
            None
        };
        for disposition in &mut dispositions {
            let input = disposition.input_binding.as_ref();
            let binding = input.or(disposition.output_binding.as_ref()).expect("lifecycle role");
            let Some(local) = before.get(binding).or_else(|| vars.get(binding)) else {
                self.errors.borrow_mut().push(CompileError::new("lifecycle role has no resolved executable local", source.span()));
                return None;
            };
            let mut sites = Vec::new();
            for block in blocks.iter() {
                for instruction in block.instructions.iter().skip(starts.get(&block.id).copied().unwrap_or(0)) {
                    let matches = match instruction {
                        IrInstruction::Consume { operand: IrOperand::Var(value) }
                        | IrInstruction::Destroy { operand: IrOperand::Var(value), .. } => input.is_some() && value.id == local.id,
                        IrInstruction::Create { dest, .. } | IrInstruction::CreateUnique { dest, .. } => {
                            input.is_none() && dest.id == local.id
                        }
                        _ => false,
                    };
                    if matches {
                        sites.push(block.id);
                    }
                }
            }
            let [operation_block] = sites.as_slice() else {
                self.errors.borrow_mut().push(CompileError::new(
                    "lifecycle occurrence must bind exactly one executable operation for each declared role",
                    source.span(),
                ));
                return None;
            };
            disposition.execution = Some(IrLifecycleExecution {
                occurrence,
                entry_block: current,
                operation_block: *operation_block,
                success_block,
                local_id: local.id,
                pool: pool.clone(),
            });
        }
        self.source_dispositions.extend(dispositions);
        Some(success_block)
    }
}
