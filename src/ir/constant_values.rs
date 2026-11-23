use super::{BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrType, IrVar, LoweredExpr};
use crate::ast::Expr;
use crate::error::Span;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_constant(&mut self, name: &str, span: Span) -> Option<IrOperand> {
        let (value, expected_ty) = self.constants.get(name)?.clone();
        match value {
            Expr::Integer(n) => Self::integer_const_for_expected_type(n, &expected_ty).map(IrOperand::Const).or_else(|| {
                self.record_error(
                    format!("constant '{}' integer literal does not fit declared IR type {:?}", name, expected_ty),
                    span,
                );
                None
            }),
            Expr::Bool(b) => Some(IrOperand::Const(IrConst::Bool(b))),
            Expr::ByteString(bytes) => {
                let items = bytes.into_iter().map(IrConst::U8).collect::<Vec<_>>();
                Some(IrOperand::Const(IrConst::Array(items)))
            }
            _ => {
                self.record_error(format!("constant '{}' uses an expression IR lowering does not support", name), span);
                None
            }
        }
    }

    pub(super) fn lower_zero_value(&self, name: &str) -> Option<IrOperand> {
        match name {
            "Address::zero" => Some(IrOperand::Const(IrConst::Address([0; 32]))),
            "Hash::zero" => Some(IrOperand::Const(IrConst::Hash([0; 32]))),
            _ => None,
        }
    }

    pub(super) fn lower_enum_variant(&self, name: &str) -> Option<(IrType, IrOperand)> {
        let (enum_name, variant_name) = name.rsplit_once("::")?;
        let ordinal = self.enum_variants.get(enum_name)?.get(variant_name).copied()?;
        Some((IrType::Named(enum_name.to_string()), IrOperand::Const(IrConst::U64(ordinal))))
    }

    pub(super) fn try_lower_enum_constructor_path(
        &mut self,
        path: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let (enum_name, variant_name) = path.rsplit_once("::")?;
        let layout = self.module.enum_layouts.get(enum_name)?.clone();
        if !layout.has_payload() {
            return None;
        }
        let Some(variant) = layout.variants.iter().find(|variant| variant.name == variant_name).cloned() else {
            self.record_error(format!("unknown enum variant '{}::{}' during IR lowering", enum_name, variant_name), span);
            return Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) });
        };
        if variant.fields.len() != args.len() {
            self.record_error(
                format!(
                    "enum constructor '{}::{}' expects {} payload value(s), got {}",
                    enum_name,
                    variant_name,
                    variant.fields.len(),
                    args.len()
                ),
                span,
            );
            return Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) });
        }

        let mut active = current;
        let mut fields = Vec::with_capacity(args.len());
        for (argument, field) in args.iter().zip(&variant.fields) {
            let lowered = self.lower_expr_with_expected_type(argument, &field.ty, active, blocks, vars);
            let Some(next) = lowered.current else {
                return Some(lowered);
            };
            active = next;
            fields.push(lowered.operand);
        }
        let dest = self.new_var(format!("{}_{}_payload", enum_name, variant_name), IrType::Named(enum_name.to_string()));
        self.block_mut(blocks, active).instructions.push(IrInstruction::EnumConstruct {
            dest: dest.clone(),
            enum_name: enum_name.to_string(),
            variant: variant_name.to_string(),
            fields,
        });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }
}
