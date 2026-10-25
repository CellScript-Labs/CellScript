//! Affine invocation-local collection owners. Member handles never become
//! ordinary values; reads copy fixed values, and mutation requires a live owner.
use super::*;

impl TypeChecker<'_> {
    /// Ordinary records are copied by value. Resolve every nested field rather
    /// than accepting a pointer-sized Cell, an opaque name or an advertised size.
    pub(crate) fn allocated_value_width(&self, ty: &Type, visiting: &mut HashSet<String>) -> Option<u32> {
        if let Some(width) = crate::allocated_values::fixed_width(ty) {
            return Some(width);
        }
        let Type::Named(name) = ty else { return None };
        if visiting.len() >= 32 || !visiting.insert(name.clone()) {
            return None;
        }
        let fields = self.nested_record_fields.get(name).cloned().or_else(|| {
            let (resolver, module) = self.resolver.zip(self.current_module.as_deref())?;
            match resolver.resolve_type(module, name)? {
                TypeDef::Struct(record) => Some(record.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
                _ => None,
            }
        });
        let width = fields.and_then(|fields| {
            fields.iter().try_fold(0u32, |total, (_, field)| total.checked_add(self.allocated_value_width(field, visiting)?))
        });
        visiting.remove(name);
        width.filter(|width| (1..=64).contains(width))
    }

    pub(super) fn collection_reference(&self, ty: &Type) -> Option<&CollectionsDef> {
        match ty {
            Type::Ref(inner) => self.collection_owner(inner),
            _ => None,
        }
    }

    /// Parameters borrow caller-owned storage for the entire helper invocation.
    /// Their active borrow carries liveness; they are not owned linear values
    /// whose state must be merged when a helper returns early.
    pub(super) fn bind_collection_helper_borrows(&mut self, env: &mut TypeEnv, params: &[Param]) {
        for param in params {
            if self.collection_reference(&param.ty).is_some() && env.lookup(&param.name) == Some(&param.ty) {
                let Type::Ref(inner) = &param.ty else { unreachable!() };
                self.active_borrows.push(ActiveBorrow {
                    root: param.name.clone(),
                    binding: param.name.clone(),
                    root_ty: inner.as_ref().clone(),
                });
            }
        }
    }

    /// A dedicated argument path keeps references out of ordinary value
    /// inference. Each evaluated reference stays borrowed until all arguments
    /// have been evaluated and the call has been checked, including on errors.
    pub(super) fn infer_collection_helper_call(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        let Expr::Identifier(name) = call.func.as_ref() else { return Ok(None) };
        let local_name =
            self.current_module.as_deref().and_then(|module| name.strip_prefix(module)?.strip_prefix("::")).unwrap_or(name);
        let Some(signature) = self.functions.get(local_name).cloned() else { return Ok(None) };
        if !signature.params.iter().any(|ty| self.contains_collection_owner(ty)) {
            return Ok(None);
        }
        if signature.kind != CallableKind::Function
            || !matches!(signature.effect, EffectClass::Pure | EffectClass::ReadOnly)
            || signature.params.iter().any(|ty| self.contains_collection_owner(ty) && self.collection_reference(ty).is_none())
        {
            return Err(CompileError::new("collection arguments require dedicated read-only &Owner helper parameters", call.span));
        }
        if signature.params.len() != call.args.len() {
            return Err(CompileError::new(format!("function '{name}' expects {} arguments", signature.params.len()), call.span));
        }
        if let Some(ty) = &signature.return_type {
            self.reject_collection_container(ty, call.span)?;
        }
        let borrow_depth = self.active_borrows.len();
        let result = (|| {
            let mut types = Vec::with_capacity(call.args.len());
            for (arg, expected) in call.args.iter().zip(&signature.params) {
                if self.collection_reference(expected).is_some() {
                    let (receiver, explicit_ref) = match arg {
                        Expr::Unary(unary) if unary.op == UnaryOp::Ref => (unary.expr.as_ref(), true),
                        Expr::Identifier(_) => (arg, false),
                        _ => {
                            return Err(CompileError::new(
                                "collection helper expects &owner or an active borrowed binding",
                                expr_span(arg),
                            ))
                        }
                    };
                    let Some((root, borrowed, def)) = self.collection_receiver(env, receiver)? else {
                        return Err(CompileError::new("collection helper expects a live collection owner borrow", expr_span(arg)));
                    };
                    if explicit_ref == borrowed {
                        return Err(CompileError::new(
                            "pass &owner or a borrowed binding directly, without nested references",
                            expr_span(arg),
                        ));
                    }
                    let owner_ty = Type::Named(def.name);
                    types.push(Type::Ref(Box::new(owner_ty.clone())));
                    self.active_borrows.push(ActiveBorrow {
                        root,
                        binding: format!("<collection call {} argument {}>", call.span.start, types.len()),
                        root_ty: owner_ty,
                    });
                } else {
                    let ty = self.infer_expr_with_expected_type(env, arg, expected, expr_span(arg))?;
                    self.reject_collection_container(&ty, expr_span(arg))?;
                    self.mark_expr_as_moved(env, arg)?;
                    types.push(ty);
                }
            }
            self.infer_call_type(env, call, &types).map(Some)
        })();
        self.active_borrows.truncate(borrow_depth);
        result
    }

    pub(super) fn collection_owner(&self, ty: &Type) -> Option<&CollectionsDef> {
        match ty {
            Type::Named(name) => self.collection_pools.get(name),
            _ => None,
        }
    }

    pub(super) fn contains_collection_owner(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(name) => {
                self.collection_pools.contains_key(name)
                    || crate::generics::applied_type(name).is_some_and(|(_, args)| {
                        args.iter()
                            .filter_map(|arg| crate::generics::parse_type_repr(arg))
                            .any(|arg| self.contains_collection_owner(&arg))
                    })
            }
            Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => self.contains_collection_owner(inner),
            Type::Tuple(items) => items.iter().any(|item| self.contains_collection_owner(item)),
            _ => false,
        }
    }

    pub(super) fn reject_collection_container(&self, ty: &Type, span: Span) -> Result<()> {
        if self.contains_collection_owner(ty) {
            return Err(CompileError::new(
                "collection owners cannot be stored, serialized, wrapped or returned; keep the owner in its invocation",
                span,
            ));
        }
        Ok(())
    }

    fn collection_receiver(&self, env: &TypeEnv, expr: &Expr) -> Result<Option<(String, bool, CollectionsDef)>> {
        let Expr::Identifier(name) = expr else { return Ok(None) };
        let Some(ty) = env.lookup(name) else { return Ok(None) };
        let (inner, borrowed) = match ty {
            Type::Ref(inner) => (inner.as_ref(), true),
            _ => (ty, false),
        };
        let Some(def) = self.collection_owner(inner) else { return Ok(None) };
        let root = if borrowed {
            self.active_borrows
                .iter()
                .rev()
                .find(|borrow| borrow.binding == *name)
                .ok_or_else(|| CompileError::new("collection references require an active lexical borrow", expr_span(expr)))?
                .root
                .clone()
        } else {
            name.clone()
        };
        let parameter_borrow = borrowed && env.lookup(&root).is_some_and(|ty| self.collection_reference(ty).is_some());
        if !parameter_borrow && env.linear_state(&root) != Some(LinearState::Available) {
            return Err(CompileError::new(format!("collection owner '{root}' was moved or dropped"), expr_span(expr)));
        }
        Ok(Some((root, borrowed, def.clone())))
    }

    fn collection_member(&self, env: &TypeEnv, expr: &Expr) -> Result<Option<(String, bool, CollectionMember)>> {
        let Expr::FieldAccess(field) = expr else { return Ok(None) };
        let Some((root, borrowed, def)) = self.collection_receiver(env, &field.expr)? else { return Ok(None) };
        let member = def
            .members
            .iter()
            .find(|member| member.name == field.field)
            .ok_or_else(|| CompileError::new(format!("unknown collection member '{}.{}'", def.name, field.field), field.span))?;
        Ok(Some((root, borrowed, member.clone())))
    }

    pub(super) fn reject_collection_member_escape(&self, env: &TypeEnv, expr: &Expr) -> Result<()> {
        if self.collection_member(env, expr)?.is_some() {
            return Err(CompileError::new(
                "collection members must stay with their owner; use a member method, copied index read or bounded iteration",
                expr_span(expr),
            ));
        }
        Ok(())
    }

    fn check_collection_mutation(&self, env: &TypeEnv, root: &str, borrowed: bool, span: Span) -> Result<()> {
        if self.collection_constraint {
            return Err(CompileError::new("collection mutation is not a pure constraint; perform it before require/assert", span));
        }
        if borrowed || !env.is_mutable(root) {
            return Err(CompileError::new(
                format!("collection owner '{root}' is read-only; mutation requires a mutable owned binding"),
                span,
            ));
        }
        self.reject_active_borrow_root_lifecycle(root, "collection mutation", span)
    }

    pub(super) fn infer_collection_call(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        if let Expr::Identifier(path) = call.func.as_ref()
            && let Some((owner, method)) = path.rsplit_once("::")
            && self.collection_pools.contains_key(owner)
        {
            if method != "new" || !call.args.is_empty() || !call.type_args.is_empty() {
                return Err(CompileError::new(
                    "collection construction uses Owner::new() with its declaration's named budgets",
                    call.span,
                ));
            }
            if self.collection_constraint
                || !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Lock | CallableKind::Function))
            {
                return Err(CompileError::new(
                    "construct collection owners in callable statements, not constants or constraints",
                    call.span,
                ));
            }
            return Ok(Some(Type::Named(owner.into())));
        }
        let Expr::FieldAccess(method) = call.func.as_ref() else { return Ok(None) };
        if let Some((root, borrowed, _)) = self.collection_receiver(env, &method.expr)? {
            if method.field != "drop" || !call.args.is_empty() || !call.type_args.is_empty() {
                return Err(CompileError::new("collection owners expose named members and drop()", call.span));
            }
            if borrowed || self.collection_constraint {
                return Err(CompileError::new("cannot drop a borrowed owner or drop inside a constraint", call.span));
            }
            self.reject_active_borrow_root_lifecycle(&root, "drop", call.span)?;
            env.destroy(&root)?;
            return Ok(Some(Type::Unit));
        }
        let Some((root, borrowed, member)) = self.collection_member(env, &method.expr)? else { return Ok(None) };
        if !call.type_args.is_empty() {
            return Err(CompileError::new("collection member operations infer types from their declaration", call.span));
        }
        if method.field == "load_sorted" {
            self.check_collection_mutation(env, &root, borrowed, call.span)?;
            let element = match &member.kind {
                ValueCollectionKind::OrderedMap { value: Type::Named(_), .. } => {
                    return Err(CompileError::new("record-valued sorted batches are deferred; use reserve and insert", call.span));
                }
                ValueCollectionKind::OrderedMap { key, value } => Type::Tuple(vec![key.clone(), value.clone()]),
                ValueCollectionKind::OrderedSet(key) => key.clone(),
                _ => return Err(CompileError::new("load_sorted requires an ordered Map or Set", call.span)),
            };
            if call.args.len() != 1 {
                return Err(CompileError::new("load_sorted expects one fixed array of entries", call.span));
            }
            let input = &call.args[0];
            let actual = if let Expr::Array(items) = input {
                self.infer_expr_with_expected_type(env, input, &Type::Array(Box::new(element.clone()), items.len()), call.span)?
            } else {
                self.infer_expr(env, input)?
            };
            if !matches!(&actual, Type::Array(inner, count) if **inner == element && *count <= 1024) {
                return Err(CompileError::new(
                    "load_sorted requires a fixed array of at most 1024 declared keys or (key, value) entries",
                    call.span,
                ));
            }
            self.collection_member(env, &method.expr)?;
            self.check_collection_mutation(env, &root, borrowed, call.span)?;
            return Ok(Some(Type::Unit));
        }
        let (expected, result, mutates) = match (method.field.as_str(), &member.kind) {
            ("len" | "capacity", _) => (vec![], Type::U64, false),
            ("is_empty", _) => (vec![], Type::Bool, false),
            ("reserve", _) => (vec![Type::U64], Type::Unit, true),
            ("push", ValueCollectionKind::Vec(item)) => (vec![item.clone()], Type::Unit, true),
            ("insert" | "set", ValueCollectionKind::Vec(item)) => (vec![Type::U64, item.clone()], Type::Unit, true),
            ("get", ValueCollectionKind::Vec(item)) => (vec![Type::U64], item.clone(), false),
            ("remove", ValueCollectionKind::Vec(item)) => (vec![Type::U64], item.clone(), true),
            ("pop", ValueCollectionKind::Vec(item)) => (vec![], item.clone(), true),
            ("insert", ValueCollectionKind::OrderedMap { key, value }) => (vec![key.clone(), value.clone()], Type::Bool, true),
            ("get", ValueCollectionKind::OrderedMap { key, value }) => (vec![key.clone()], value.clone(), false),
            ("get_or", ValueCollectionKind::OrderedMap { key, value }) => (vec![key.clone(), value.clone()], value.clone(), false),
            ("contains_key", ValueCollectionKind::OrderedMap { key, .. }) => (vec![key.clone()], Type::Bool, false),
            ("remove", ValueCollectionKind::OrderedMap { key, .. }) => (vec![key.clone()], Type::Bool, true),
            ("insert" | "remove", ValueCollectionKind::OrderedSet(key)) => (vec![key.clone()], Type::Bool, true),
            ("contains", ValueCollectionKind::OrderedSet(key)) => (vec![key.clone()], Type::Bool, false),
            _ => {
                return Err(CompileError::new(
                    format!("unsupported collection operation '{}.{}'", member.name, method.field),
                    call.span,
                ))
            }
        };
        if mutates {
            self.check_collection_mutation(env, &root, borrowed, call.span)?;
        }
        if expected.len() != call.args.len() {
            return Err(CompileError::new(
                format!("collection operation '{}' expects {} arguments", method.field, expected.len()),
                call.span,
            ));
        }
        for (arg, expected) in call.args.iter().zip(expected.iter()) {
            let actual = self.infer_expr_with_expected_type(env, arg, expected, expr_span(arg))?;
            if !self.types_equal(&actual, expected) {
                return Err(CompileError::new(
                    format!("collection argument expects {}, found {}", type_repr(expected), type_repr(&actual)),
                    expr_span(arg),
                ));
            }
        }
        // Argument evaluation may move/drop the receiver, so validate again.
        self.collection_member(env, &method.expr)?;
        if mutates {
            self.check_collection_mutation(env, &root, borrowed, call.span)?;
        }
        Ok(Some(result))
    }

    pub(super) fn infer_collection_index(&mut self, env: &mut TypeEnv, index: &IndexExpr) -> Result<Option<Type>> {
        let Some((_, _, member)) = self.collection_member(env, &index.expr)? else { return Ok(None) };
        let ValueCollectionKind::Vec(item) = member.kind else {
            return Err(CompileError::new(
                "only allocated Vec supports ordinal indexing; use ordered get/contains or iteration",
                index.span,
            ));
        };
        let ty = self.infer_expr_with_expected_type(env, &index.index, &Type::U64, index.span)?;
        if ty != Type::U64 {
            return Err(CompileError::new("collection index requires u64", index.span));
        }
        self.collection_member(env, &index.expr)?;
        Ok(Some(item))
    }

    pub(super) fn check_collection_for(&mut self, env: &mut TypeEnv, stmt: &ForStmt) -> Result<bool> {
        let Some((root, _, member)) = self.collection_member(env, &stmt.iterable)? else { return Ok(false) };
        let item = match member.kind {
            ValueCollectionKind::Vec(item) | ValueCollectionKind::OrderedSet(item) => item,
            ValueCollectionKind::OrderedMap { key, value } => Type::Tuple(vec![key, value]),
        };
        let owner = env.lookup(&root).unwrap().clone();
        let mut body_env = env.child();
        self.bind_pattern(&mut body_env, &stmt.pattern, &item, false, stmt.span)?;
        self.active_borrows.push(ActiveBorrow {
            root,
            binding: format!("<collection iteration {}>", stmt.span.start),
            root_ty: owner,
        });
        self.loop_labels.push(stmt.label.clone());
        let result = (|| {
            for statement in &stmt.body {
                self.check_stmt(&mut body_env, statement)?;
            }
            body_env.check_linear_complete()?;
            env.reject_loop_linear_state_changes(&body_env, stmt.span)
        })();
        self.loop_labels.pop();
        self.active_borrows.pop();
        result?;
        env.merge_existing_type_refinements_from(&body_env);
        Ok(true)
    }
}
