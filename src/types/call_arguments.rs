use super::{
    forbidden_consensus_call_name, function_def_kind, function_def_param_types, mutable_reference_root_names, type_repr, CallableKind,
    EffectRequirement, FunctionSignature, TypeChecker, TypeEnv,
};
use crate::ast::{CallExpr, Expr, Type};
use crate::error::{CompileError, Result, Span};
use crate::resolve::FunctionDef;
use std::collections::HashMap;

impl TypeChecker<'_> {
    pub(super) fn infer_commitment_open_call(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        let Expr::Identifier(function) = call.func.as_ref() else {
            return Ok(None);
        };
        if function != crate::commitment_contract::OPEN_FUNCTION {
            return Ok(None);
        }
        if !call.type_args.is_empty() {
            return Err(CompileError::new("commitment::open infers T and does not accept type arguments", call.span));
        }
        if call.args.len() != 2 {
            return Err(CompileError::new(format!("commitment::open expects 2 arguments, found {}", call.args.len()), call.span));
        }

        let expected_ty = self.infer_expr(env, &call.args[0])?;
        let Some(expected_inner) = expected_ty.commitment_value_type() else {
            return Err(CompileError::new("commitment::open first argument must be Commitment<T>", call.span));
        };
        let Expr::Identifier(opening_binding) = &call.args[1] else {
            return Err(CompileError::new("commitment::open second argument must be a direct Opening<T> witness binding", call.span));
        };
        let opening_ty = env
            .lookup(opening_binding)
            .cloned()
            .ok_or_else(|| CompileError::new(format!("undefined opening witness '{}'", opening_binding), call.span))?;
        let Some(opening_inner) = opening_ty.opening_value_type() else {
            return Err(CompileError::new("commitment::open second argument must be Opening<T>", call.span));
        };
        if expected_inner.source_text() != opening_inner.source_text() || !self.types_equal(expected_inner, opening_inner) {
            return Err(CompileError::new(
                format!(
                    "commitment::open type mismatch: Commitment<{}> and Opening<{}> must retain the same V0 packed type spelling and owner",
                    expected_inner.source_text(),
                    opening_inner.source_text()
                ),
                call.span,
            ).with_code(crate::error::DiagnosticCode::E2111));
        }
        let inner_ty = expected_inner.clone();
        self.validate_committed_value_type(&inner_ty, call.span)?;
        env.consume(opening_binding)?;
        Ok(Some(inner_ty))
    }

    /// The source binding takes precedence over identically spelled builtins.
    /// Keep its owner/signature rather than rediscovering a target after typing.
    pub(super) fn resolve_source_signature(&self, name: &str, _span: Span) -> Result<Option<FunctionSignature>> {
        if let Some(signature) = self.functions.get(name) {
            return Ok(Some(signature.clone()));
        }
        if let Some(function) = self.resolve_function(name) {
            return Ok(Some(FunctionSignature {
                params: function_def_param_types(&function),
                return_type: self.function_return_type(&function),
                kind: function_def_kind(&function),
            }));
        }
        if let Some((prefix, suffix)) = name.rsplit_once("::")
            && self.current_module.as_deref() == Some(prefix)
        {
            return Ok(self.functions.get(suffix).cloned());
        }
        Ok(None)
    }

    pub(super) fn validate_namespaced_type_constructor(&self, type_name: &str, constructor: &str, span: Span) -> Result<()> {
        if type_name == "Vec" {
            return Ok(());
        }
        self.validate_named_type(type_name)
            .map_err(|_| CompileError::new(format!("unknown namespaced function '{}::{}'", type_name, constructor), span))
    }

    pub(super) fn validate_call_args(
        &self,
        callee_name: &str,
        expected: &[Type],
        actual: &[Type],
        args: &[Expr],
        span: Span,
    ) -> Result<()> {
        if actual.len() != expected.len() {
            return Err(CompileError::new(
                format!(
                    "function '{}' expects {} argument{}, found {}",
                    callee_name,
                    expected.len(),
                    if expected.len() == 1 { "" } else { "s" },
                    actual.len()
                ),
                span,
            ));
        }

        for (index, ((expected_ty, actual_ty), arg)) in expected.iter().zip(actual.iter()).zip(args.iter()).enumerate() {
            if !self.call_argument_type_compatible(expected_ty, actual_ty)
                && !self.expr_type_compatible_with_expected(arg, actual_ty, expected_ty, span)?
            {
                return Err(CompileError::new(
                    format!(
                        "function '{}' argument {} type mismatch: expected {}, found {}",
                        callee_name,
                        index + 1,
                        type_repr(expected_ty),
                        type_repr(actual_ty)
                    ),
                    span,
                ));
            }
        }

        self.reject_duplicate_mutable_reference_call_roots(callee_name, expected, actual, args, span)?;

        Ok(())
    }

    fn reject_duplicate_mutable_reference_call_roots(
        &self,
        callee_name: &str,
        expected: &[Type],
        actual: &[Type],
        args: &[Expr],
        span: Span,
    ) -> Result<()> {
        let mut roots: HashMap<&str, bool> = HashMap::new();
        for (expected_ty, (actual_ty, arg)) in expected.iter().zip(actual.iter().zip(args.iter())) {
            if !matches!(actual_ty, Type::MutRef(_)) {
                continue;
            }
            let participates_in_mutable_alias = matches!(expected_ty, Type::MutRef(_));
            for root in mutable_reference_root_names(arg) {
                if let Some(prior_participated) = roots.get(root).copied() {
                    if participates_in_mutable_alias || prior_participated {
                        return Err(CompileError::new(
                            format!(
                                "function '{}' cannot receive mutable reference root '{}' more than once in one call; use signature-direction outputs for Cell updates",
                                callee_name, root
                            ),
                            span,
                        ));
                    }
                } else {
                    roots.insert(root, participates_in_mutable_alias);
                }
            }
        }
        Ok(())
    }

    fn call_argument_type_compatible(&self, expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Ref(expected_inner), Type::MutRef(actual_inner)) => self.types_equal(expected_inner, actual_inner),
            _ => self.types_equal(expected, actual),
        }
    }

    pub(super) fn validate_builtin_arity(&self, name: &str, expected: usize, actual: &[Type], span: Span) -> Result<()> {
        if actual.len() == expected {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("{} expects {} argument{}, found {}", name, expected, if expected == 1 { "" } else { "s" }, actual.len()),
                span,
            ))
        }
    }

    pub(super) fn validate_numeric_builtin_call(&self, name: &str, arg_types: &[Type], span: Span) -> Result<()> {
        let expected = if name == "isqrt" { 1 } else { 2 };
        self.validate_builtin_arity(name, expected, arg_types, span)?;
        for (index, arg_ty) in arg_types.iter().enumerate() {
            if !self.is_numeric_type(arg_ty) {
                return Err(CompileError::new(
                    format!("{} argument {} must be numeric, found {}", name, index + 1, type_repr(arg_ty)),
                    span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn reject_forbidden_consensus_call(&self, call: &CallExpr) -> Result<()> {
        if let Some(name) = forbidden_consensus_call_name(call.func.as_ref()) {
            return Err(CompileError::new(
                format!("{} is forbidden in consensus CellScript; use explicit control flow and checked error handling instead", name),
                call.span,
            ));
        }
        Ok(())
    }

    pub(super) fn resolve_function(&self, name: &str) -> Option<FunctionDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_function(module, name))
    }

    pub(super) fn resolve_constant(&self, name: &str) -> Option<crate::resolve::ConstantDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_constant(module, name))
    }

    fn function_return_type(&self, function: &FunctionDef) -> Option<Type> {
        match function {
            FunctionDef::Action(action) => action.return_type.clone(),
            FunctionDef::Function(function) => function.return_type.clone(),
            FunctionDef::Lock(_) => Some(Type::Bool),
        }
    }

    pub(super) fn validate_call_allowed(&self, callee_name: &str, callee_kind: CallableKind, span: Span) -> Result<()> {
        match (self.current_callable, callee_kind) {
            (Some(CallableKind::Invariant), CallableKind::Action | CallableKind::Lock) => {
                let context = if self.current_validity { "validity predicate" } else { "invariant" };
                Err(CompileError::new(
                    format!("{} cannot call stateful entry '{}'; call a pure helper instead", context, callee_name),
                    span,
                ))
            }
            (Some(CallableKind::Function), CallableKind::Lock) => {
                Err(CompileError::new(format!("function cannot call lock '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Action) => {
                Err(CompileError::new(format!("lock cannot call action '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Lock) => {
                Err(CompileError::new(format!("lock cannot call lock '{}'", callee_name), span))
            }
            (Some(CallableKind::Invariant), CallableKind::Function) => {
                self.require_effect(callee_name, true, span, "validity or invariant predicate: only transitively free of external operations helpers are allowed; incompatible callee");
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn require_effect(&self, target: &str, empty: bool, span: Span, reason: &'static str) {
        self.effect_requirements.borrow_mut().push(EffectRequirement { target: target.into(), empty, span, reason });
    }
}
