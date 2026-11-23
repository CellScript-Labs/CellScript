//! Bind a child-owned proof policy to the entry witness ABI and the resolved
//! parent operations. Cryptography inside the hash-pinned child stays external.
use super::*;

fn call(op: &TypedSemanticOperation, target: &str) -> bool {
    op.call.as_ref().is_some_and(|c| c.contract == "versioned-runtime-helper" && c.target == target)
}

fn integer(arg: &TypedSemanticOperand, value: &str) -> bool {
    matches!(&arg.constant, Some(TypedSemanticConstant::U64(n)) if n == value)
}

fn feeds(producer: &TypedSemanticOperation, operand: &TypedSemanticOperand) -> bool {
    matches!(producer.destinations.as_slice(), [id] if operand.local == Some(*id))
}

fn dep_hash(source: &TypedSemanticOperation, check: &TypedSemanticOperation, index: &str, hash: &str) -> bool {
    call(source, "__ckb_source_cell_dep")
        && matches!(source.operands.as_slice(), [i] if integer(i, index))
        && call(check, "__ckb_require_cell_data_hash")
        && matches!(check.operands.as_slice(), [dep, digest] if feeds(source, dep)
            && matches!(&digest.constant, Some(TypedSemanticConstant::Hash(h)) if h == hash))
}

pub(super) fn validate(
    entry: &TypedSemanticEntry,
    verifier: &TrustedExternalVerifierRecord,
    policy: &ExternalProofPolicy,
) -> Result<(), CheckerError> {
    let canonical = |hash: &str| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if policy.name.is_empty()
        || policy.name.len() > 64
        || !policy.name.bytes().enumerate().all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
        || ![&policy.domain, &policy.action, &policy.verification_key_hash].into_iter().all(|s| canonical(s))
        || verifier.operation != "spawn-wait"
        || verifier.adapter != "hex4-v1"
        || entry.kind != "action"
        || !matches!(entry.params.as_slice(), [p] if p.source == "witness" && !p.mutable && !p.reference
            && p.ty == format!("[u8; {}]", policy.profile.proof_bytes()))
        || entry.blocks.iter().any(|b| b.successors.contains(&entry.entry_block))
    {
        return typed_error("external proof policy differs from its action, witness or adapter");
    }
    let block = entry.blocks.iter().find(|b| b.id == entry.entry_block);
    let Some([key, check_key, new, bind, append, child, check_child, spawn, ..]) = block.map(|b| b.operations.as_slice()) else {
        return typed_error("external proof policy has no unconditional parent verification sequence");
    };
    let context = match append.operands.as_slice() {
        [buffer, TypedSemanticOperand { constant: Some(TypedSemanticConstant::Array(bytes)), .. }]
            if feeds(bind, buffer) && bytes.len() == 64 =>
        {
            bytes
                .iter()
                .map(|b| match b {
                    TypedSemanticConstant::U8(s) => s.parse::<u8>().ok(),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
        }
        _ => None,
    };
    if !dep_hash(key, check_key, "1", &policy.verification_key_hash)
        || new.opcode != "collection-new"
        || !matches!(new.operands.as_slice(), [cap] if integer(cap, "64"))
        || bind.opcode != "move"
        || !matches!(bind.operands.as_slice(), [v] if feeds(new, v))
        || append.opcode != "collection-extend"
        || !context.is_some_and(|v| hex_encode(&v[..32]) == policy.domain && hex_encode(&v[32..]) == policy.action)
        || !dep_hash(child, check_child, "0", &verifier.code_hash)
        || !call(spawn, "__ckb_spawn_wait_cell_dep_hex4")
        || !matches!(spawn.operands.as_slice(), [i, buf, d, a, x, y] if integer(i,"0") && feeds(bind, buf)
            && integer(d,"32") && integer(a,"32") && integer(x,"0") && integer(y,"0"))
    {
        return typed_error("external proof policy differs from the checked VK, verifier or statement context");
    }
    Ok(())
}
