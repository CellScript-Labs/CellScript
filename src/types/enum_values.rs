use super::{expr_span, type_repr, TypeChecker, TypeEnv};
use crate::ast::{CallExpr, Expr, Type};
use crate::error::{CompileError, Result, Span};
use crate::resolve::TypeDef;

impl TypeChecker<'_> {
    pub(super) fn enum_variant_expr_type(&self, name: &str, span: Span) -> Result<Option<Type>> {
        let Some((enum_name, variant)) = name.rsplit_once("::") else {
            return Ok(None);
        };
        let Some(variants) = self.resolve_enum_variants(enum_name) else {
            return Ok(None);
        };
        if !variants.iter().any(|candidate| candidate == variant) {
            return Err(CompileError::new(format!("unknown enum variant '{}::{}'", enum_name, variant), span));
        }
        if self.enum_variant_has_payload(enum_name, variant) {
            return Err(CompileError::new(
                format!("enum payload variant '{}::{}' requires an explicit fixed-width payload constructor call", enum_name, variant),
                span,
            ));
        }
        Ok(Some(Type::Named(enum_name.to_string())))
    }

    pub(super) fn infer_enum_constructor_call(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        let Expr::Identifier(path) = call.func.as_ref() else {
            return Ok(None);
        };
        let Some((enum_name, variant_name)) = path.rsplit_once("::") else {
            return Ok(None);
        };
        let Some(variants) = self.resolve_enum_variant_fields(enum_name) else {
            return Ok(None);
        };
        let Some(fields) = variants.get(variant_name) else {
            return Err(CompileError::new(format!("unknown enum variant '{}::{}'", enum_name, variant_name), call.span));
        };
        if fields.is_empty() {
            return Err(CompileError::new(
                format!("enum variant '{}::{}' has no payload and must be written without parentheses", enum_name, variant_name),
                call.span,
            ));
        }
        if call.args.len() != fields.len() {
            return Err(CompileError::new(
                format!(
                    "enum variant '{}::{}' expects {} payload value(s), got {}",
                    enum_name,
                    variant_name,
                    fields.len(),
                    call.args.len()
                ),
                call.span,
            ));
        }
        for (argument, expected) in call.args.iter().zip(fields) {
            let actual = self.infer_expr_with_expected_type(env, argument, expected, expr_span(argument))?;
            if !self.types_equal(&actual, expected) {
                return Err(CompileError::new(
                    format!(
                        "enum variant '{}::{}' payload type mismatch: expected {}, found {}",
                        enum_name,
                        variant_name,
                        type_repr(expected),
                        type_repr(&actual)
                    ),
                    expr_span(argument),
                ));
            }
        }
        for argument in &call.args {
            self.mark_expr_as_moved(env, argument)?;
        }
        Ok(Some(Type::Named(enum_name.to_string())))
    }

    pub(super) fn enum_variant_has_payload(&self, enum_name: &str, variant: &str) -> bool {
        if self.enum_payload_variants.get(enum_name).is_some_and(|payloads| payloads.contains(variant)) {
            return true;
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.resolve_type(module, enum_name))
            .is_some_and(|ty| match ty {
                TypeDef::Enum(enum_def) => {
                    enum_def.variants.iter().any(|candidate| candidate.name == variant && !candidate.fields.is_empty())
                }
                _ => false,
            })
    }
}
