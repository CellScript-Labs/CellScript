//! Package elaboration for child-owned transaction statements. The only witness
//! parameter occupies the complete CSARGv1 payload. Both parent and child
//! decode the same field. Policy names never select a verifier by convention.
use crate::ast::{BindingPattern, CallExpr, Expr, FieldAccessExpr, Item, LetStmt, Module, ParamSource, Stmt, Type};
use crate::error::{CompileError, Result, Span};
use crate::package::CkbDeployConfig;

fn call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::Call(CallExpr { func: Box::new(Expr::Identifier(name.into())), type_args: vec![], args, span })
}

fn hash_bytes(value: &str, span: Span) -> Result<Vec<u8>> {
    crate::hex_decode(value)
        .filter(|bytes| hex::encode(bytes) == value)
        .map(|bytes| bytes.to_vec())
        .ok_or_else(|| CompileError::new("ZK proof policy requires a canonical lowercase 32-byte hash", span))
}

pub(crate) fn bind(module: &mut Module, deploy: Option<&CkbDeployConfig>) -> Result<()> {
    let declarations = deploy.map_or(&[][..], |ckb| ckb.trusted_external_verifiers.as_slice());
    let mut policies = std::collections::BTreeMap::new();
    for declaration in declarations {
        let Some(policy) = &declaration.proof_policy else { continue };
        if policy.name.len() > 64
            || policy.name.is_empty()
            || !policy.name.bytes().enumerate().all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
        {
            return Err(CompileError::new("ZkProof policy requires an ASCII identifier of at most 64 bytes", module.span));
        }
        if policies.insert(&policy.name, (declaration, policy)).is_some() {
            return Err(CompileError::new("duplicate ZkProof policy in the package", module.span));
        }
    }
    // Without a package policy, ordinary (including imported) generic types
    // retain their own meanings. The unary intrinsic then rejects unbound calls.
    if policies.is_empty() {
        return Ok(());
    }
    if module.items.iter().any(|item| item.name() == Some("ZkProof")) {
        return Err(CompileError::new("ZkProof conflicts with a declaration in this module", module.span));
    }
    for item in &mut module.items {
        let Item::Action(action) = item else { continue };
        if !action.params.iter().any(|p| p.ty.application().is_some_and(|a| a.constructor == "ZkProof")) {
            continue;
        }
        let [proof] = action.params.as_mut_slice() else {
            return Err(CompileError::new("the counter verifier profile requires a sole witness proof parameter", action.span));
        };
        let Some(Type::Named(name)) = proof.ty.application().and_then(|a| a.unary_type("ZkProof")) else {
            return Err(CompileError::new("expected ZkProof<Policy> with a named deployment policy", proof.span));
        };
        if proof.source != ParamSource::Witness || proof.is_ref || proof.is_read_ref || proof.is_mut {
            return Err(CompileError::new("ZkProof requires a direct immutable witness parameter", proof.span));
        }
        let (declaration, policy) = policies.remove(name).ok_or_else(|| {
            CompileError::new(format!("ZkProof<{name}> needs a matching Cell.toml verifier proof_policy"), proof.span)
        })?;
        if declaration.scope != format!("action:{}", action.name)
            || declaration.operation != "spawn-wait"
            || declaration.adapter != "hex4-v1"
            || declaration.hash_type != "data"
        {
            return Err(CompileError::new(
                "ZkProof policy requires this exact action and a hash-bound hex4-v1 Spawn/Wait verifier",
                proof.span,
            ));
        }
        let Some(Stmt::Discard(Expr::Call(require))) = action.body.first() else {
            return Err(CompileError::new("start the ZkProof action with zk::require_valid(proof);", action.span));
        };
        if !matches!(&*require.func, Expr::Identifier(f) if f == "zk::require_valid")
            || !require.type_args.is_empty()
            || !matches!(require.args.as_slice(), [Expr::Identifier(p)] if p == &proof.name)
        {
            return Err(CompileError::new("zk::require_valid must directly verify the declared witness parameter", require.span));
        }
        action.body[0] = verification_body(declaration, policy, require.span)?;
        proof.ty = Type::Array(Box::new(Type::U8), policy.profile.proof_bytes());
    }
    if let Some((name, _)) = policies.first_key_value() {
        return Err(CompileError::new(
            format!("unused verifier proof_policy '{name}': declare its ZkProof witness on the owning action"),
            module.span,
        ));
    }
    Ok(())
}

fn verification_body(
    declaration: &crate::package::CkbTrustedExternalVerifierConfig,
    policy: &cellscript_artifact_checker::ExternalProofPolicy,
    span: Span,
) -> Result<Stmt> {
    let hash = |value: &str| -> Result<Expr> { Ok(call("Hash::from_bytes", vec![Expr::ByteString(hash_bytes(value, span)?)], span)) };
    let key_dep = call("source::cell_dep", vec![Expr::Integer(1)], span);
    let key = call("ckb::require_cell_data_hash", vec![key_dep, hash(&policy.verification_key_hash)?], span);
    let mut context = hash_bytes(&policy.domain, span)?;
    context.extend(hash_bytes(&policy.action, span)?);
    let buffer = "__zk_context";
    let allocate = Stmt::Let(LetStmt {
        pattern: BindingPattern::Name(buffer.into()),
        ty: Some(Type::vector(Type::U8)),
        is_mut: true,
        value: call("Vec::with_capacity", vec![Expr::Integer(64)], span),
        span,
    });
    let append = Expr::Call(CallExpr {
        func: Box::new(Expr::FieldAccess(FieldAccessExpr {
            expr: Box::new(Expr::Identifier(buffer.into())),
            field: "extend_from_slice".into(),
            span,
        })),
        args: vec![Expr::ByteString(context)],
        type_args: vec![],
        span,
    });
    let delegate = call(
        "ckb::trusted_spawn_wait_cell_dep_hex4",
        vec![
            Expr::Integer(0),
            hash(&declaration.code_hash)?,
            Expr::Identifier(buffer.into()),
            Expr::Integer(32),
            Expr::Integer(32),
            Expr::Integer(0),
            Expr::Integer(0),
        ],
        span,
    );
    Ok(Stmt::Discard(Expr::Block(vec![Stmt::Discard(key), allocate, Stmt::Discard(append), Stmt::Discard(delegate)])))
}

/// The child reads the transaction entry witness. Calling its action as a
/// helper with ordinary arguments would verify a different value.
pub(crate) fn validate_calls(
    ir: &crate::ir::IrModule,
    declarations: &[crate::package::CkbTrustedExternalVerifierConfig],
) -> Result<()> {
    use crate::ir::{IrInstruction, IrItem};
    let owners: std::collections::BTreeSet<_> =
        declarations.iter().filter(|d| d.proof_policy.is_some()).filter_map(|d| d.scope.strip_prefix("action:")).collect();
    if owners.is_empty() {
        return Ok(());
    }
    for item in &ir.items {
        let body = match item {
            IrItem::Action(a) => &a.body,
            IrItem::Function(f) => &f.body,
            IrItem::Lock(l) => &l.body,
            _ => continue,
        };
        if body
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|op| matches!(op, IrInstruction::Call {func, ..} if func.function_symbol().is_some_and(|f| owners.contains(f))))
        {
            return Err(CompileError::without_span("enter the ZkProof action directly: its verifier reads the entry witness"));
        }
    }
    Ok(())
}
