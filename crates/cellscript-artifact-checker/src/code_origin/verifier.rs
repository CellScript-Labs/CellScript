//! Bind a selected child ELF to a checked, unconditional spawn-and-wait call.
//! This proof records required execution; it never reports VM success itself.
use super::{CheckedDirectCodeDependency, CheckedProtocolTransaction};
use crate::fixed_policy_receipt::CheckedFixedPolicyReceipt;
use crate::interface::ModuleBundle;
use crate::{
    canonical_hash, CheckerBudgets, CheckerError, CheckerRejectionCode, EntryDispatchContract, TypedSemanticConstant,
    VerifiedLoweringRecord,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct CheckedOpenVerifierInvocation {
    evidence: OpenVerifierInvocation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenVerifierInvocation {
    pub schema: String,
    pub identity: String,
    pub consumer_entry: String,
    pub child_entry: String,
    pub dependency_index: u32,
    pub operation: String,
    pub context: String,
    pub vm_execution_claimed: bool,
}
impl CheckedOpenVerifierInvocation {
    pub fn evidence(&self) -> &OpenVerifierInvocation {
        &self.evidence
    }
}
fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("open verifier invocation: {message}"))
}

/// Recompute the compiler's on-chain target/exit claim before any artifact
/// receipt exists. This does not promote the child's interface or predicates.
pub(crate) fn validate_claims(record: &VerifiedLoweringRecord, elf: &crate::ParsedElf) -> Result<(), CheckerError> {
    for proof in record.proof_records.iter().filter(|p| {
        p.evidence_tier == "checked-runtime" && p.obligation == "spawn-target-and-checked-child-exit:spawn-delegation:checked-runtime"
    }) {
        let entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|e| e.id == proof.entry_id)
            .ok_or_else(|| invalid("checked spawn claim has no typed owner"))?;
        let block = entry.blocks.iter().find(|b| b.id == entry.entry_block).ok_or_else(|| invalid("missing spawn entry block"))?;
        let raw = entry
            .blocks
            .iter()
            .flat_map(|b| &b.operations)
            .filter(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_spawn_wait_cell_dep_hex4"))
            .count();
        if raw != 1 {
            return Err(invalid("checked open spawn claim needs exactly one entry-block invocation"));
        }
        let mut views = std::collections::BTreeMap::new();
        let mut authorized = std::collections::BTreeSet::new();
        let mut selected = None;
        for op in &block.operations {
            let Some(call) = &op.call else { continue };
            match call.target.as_str() {
                "__ckb_source_cell_dep" => {
                    if let ([dest], Some(TypedSemanticConstant::U64(index))) =
                        (op.destinations.as_slice(), op.operands.first().and_then(|o| o.constant.as_ref()))
                        && let Ok(index) = index.parse::<u64>()
                        && index < 64
                    {
                        views.insert(*dest, index);
                    }
                }
                "__ckb_require_cell_dep_open_verifier_handle_from_args" => {
                    if let [view, handle] = op.operands.as_slice()
                        && entry.params.iter().any(|p| Some(p.binding_id) == handle.local && p.ty.starts_with("VerifierHandle<"))
                        && let Some(index) = view.local.and_then(|local| views.get(&local))
                    {
                        authorized.insert(*index);
                    }
                }
                "__ckb_spawn_wait_cell_dep_hex4" => {
                    if let Some(TypedSemanticConstant::U64(index)) = op.operands.first().and_then(|o| o.constant.as_ref())
                        && let Ok(index) = index.parse::<u64>()
                        && authorized.contains(&index)
                    {
                        selected = Some(index);
                    }
                }
                _ => {}
            }
        }
        let index = selected.ok_or_else(|| invalid("checked spawn target is not bound to preceding nominal membership"))?;
        crate::entry_codec::verify_spawn(record, elf, &entry.id, index)?;
    }
    Ok(())
}

/// The initial child entry profile is parameterless: its complete interface
/// receipt is independently checked, and its transaction syscalls inherit the
/// caller's Script/group exactly as CKB spawn specifies. No child Script group,
/// standalone witness or argv decoder is invented. The bounded hex4 transport
/// may carry bytes, but those do not become positional entry parameters.
pub fn check_open_verifier_invocation(
    consumer: ModuleBundle<'_>,
    parameter: &str,
    receipt: &CheckedFixedPolicyReceipt,
    dependency: &CheckedDirectCodeDependency,
    transaction: &CheckedProtocolTransaction<'_>,
    budgets: &CheckerBudgets,
) -> Result<CheckedOpenVerifierInvocation, CheckerError> {
    if receipt.entry_contract().script_role != "type"
        || !receipt.target_origin().origin().codec().parameters().parameterless_single_entry()
        || dependency.target_identity() != receipt.target_origin().identity()
        || dependency.transaction_hash() != transaction.raw_hash()
    {
        return Err(invalid("requires the exact checked parameterless child receipt and dependency"));
    }
    let inspection = crate::interface::inspect_bundle(consumer[0], consumer[1], consumer[2], consumer[3], budgets)?;
    let lowering: VerifiedLoweringRecord = serde_json::from_slice(consumer[2]).map_err(|_| invalid("invalid lowering"))?;
    let typed = &lowering.typed_semantics;
    let contract = &typed.foundation.entry_contract;
    if !matches!(contract.dispatch, EntryDispatchContract::SingleEntry) || !matches!(contract.script_role.as_str(), "type" | "lock") {
        return Err(invalid("consumer must have a checked single Script entry"));
    }
    let entry = typed.entries.iter().find(|e| e.id == contract.exact_entry).ok_or_else(|| invalid("missing exact consumer entry"))?;
    let param = entry
        .params
        .iter()
        .find(|p| p.name == parameter && p.ty.starts_with("VerifierHandle<"))
        .ok_or_else(|| invalid("missing nominal verifier parameter"))?;
    let block = entry.blocks.iter().find(|b| b.id == entry.entry_block).ok_or_else(|| invalid("missing entry block"))?;
    let membership = block
        .operations
        .iter()
        .position(|op| {
            op.call.as_ref().is_some_and(|call| call.target == "__ckb_require_cell_dep_open_verifier_handle_from_args")
                && op.operands.get(1).is_some_and(|operand| operand.local == Some(param.binding_id))
        })
        .ok_or_else(|| invalid("entry must unconditionally verify its committed verifier selection"))?;
    let expected_index = dependency.raw_dependency_index() as u64;
    let invocations = block
        .operations
        .iter()
        .skip(membership + 1)
        .filter(|op| {
            op.call.as_ref().is_some_and(|call| call.target == "__ckb_spawn_wait_cell_dep_hex4")
                && matches!(op.operands.first().and_then(|operand| operand.constant.as_ref()),
                Some(TypedSemanticConstant::U64(value)) if value.parse::<u64>() == Ok(expected_index))
        })
        .collect::<Vec<_>>();
    if invocations.len() != 1 {
        return Err(invalid("requires exactly one unconditional spawn/wait of the selected direct dependency after membership"));
    }
    let elf = crate::parse_elf(consumer[0], budgets.instructions).map_err(|_| invalid("invalid consumer machine code"))?;
    crate::entry_codec::verify_spawn(&lowering, &elf, &entry.id, expected_index)?;
    let mut evidence = OpenVerifierInvocation {
        schema: "cellscript-open-verifier-invocation-v1".into(),
        identity: String::new(),
        consumer_entry: entry.name.clone(),
        child_entry: receipt.entry_contract().exact_entry.clone(),
        dependency_index: dependency.raw_dependency_index() as u32,
        operation: "spawn-wait-hex4-v1".into(),
        context: "inherited-consumer-script-group".into(),
        vm_execution_claimed: false,
    };
    evidence.identity = canonical_hash(
        "cellscript-open-verifier-invocation-id-v1",
        &serde_json::json!({
            "invocation": evidence, "consumer": inspection.report(), "receipt": receipt.identity(),
            "dependency": dependency.identity(), "transaction": transaction.full_hash(),
        }),
    )?;
    Ok(CheckedOpenVerifierInvocation { evidence })
}
