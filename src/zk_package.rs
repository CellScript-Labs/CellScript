//! Named package intent for the first exact ZK profile. Reuses #11 receipts.
use crate::error::{CompileError, Result};
use crate::ir::IrModule;
#[cfg(not(feature = "wasm"))]
use crate::ir::{IrConst, IrInstruction, IrItem, IrOperand};
use crate::package::CkbDeployConfig;
#[cfg(not(feature = "wasm"))]
use crate::script_handle::{exact_script_handle_value_from_receipt, exact_script_handle_value_hash, ExactScriptHandleReceipt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedZkVerifier {
    pub policy: String,
    pub dependency: String,
    pub verification_key_hash: String,
    #[cfg(not(feature = "wasm"))]
    pub receipt: ExactScriptHandleReceipt,
    #[cfg(feature = "wasm")]
    pub receipt: serde_json::Value,
}

fn error(message: &str) -> CompileError {
    CompileError::without_span(format!("named ZK dependency: {message}"))
}

/// Resolve a policy to the ordered direct CellDep declaration and its exact handle.
/// No Registry lookup, DepGroup expansion, RPC liveness or implicit upgrade occurs.
#[cfg(not(feature = "wasm"))]
pub fn resolve(deploy: &CkbDeployConfig, policy: &str) -> Result<(usize, crate::script_handle::ExactScriptHandleValue)> {
    if deploy.zk_verifiers.len() > 64 || deploy.cell_deps.len() > 64 {
        return Err(error("at most 64 policies and direct dependencies are supported"));
    }
    let mut names = std::collections::BTreeSet::new();
    for declaration in &deploy.zk_verifiers {
        if declaration.policy.is_empty()
            || declaration.policy.len() > 64
            || !declaration.policy.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !names.insert(&declaration.policy)
        {
            return Err(error("invalid or duplicate policy name"));
        }
    }
    let declaration = deploy
        .zk_verifiers
        .iter()
        .find(|item| item.policy == policy)
        .ok_or_else(|| error("source policy has no Cell.toml deploy.ckb.zk_verifiers declaration"))?;
    let matching: Vec<_> =
        deploy.cell_deps.iter().enumerate().filter(|(_, dep)| dep.name.as_deref() == Some(&declaration.dependency)).collect();
    let [(index, dependency)] = matching.as_slice() else {
        return Err(error("policy must resolve to exactly one named CellDep"));
    };
    // Direct deps keep source indexes identical to the resolved syscall indexes.
    if deploy.cell_deps.iter().any(|dep| dep.dep_type.as_deref().unwrap_or("code") != "code") {
        return Err(error("this exact named profile supports direct code CellDeps only"));
    }
    let receipt = &declaration.receipt;
    let profile = hex::encode(cellscript_artifact_checker::zk::PROFILE_ID);
    if receipt.script_role != crate::protocol_bundle::ProtocolScriptRole::SpawnedVerifier
        || receipt.interface_hash != profile
        || receipt.runtime_abi_hash != profile
    {
        return Err(error("receipt role or exact ZK profile/ABI differs"));
    }
    let handle = exact_script_handle_value_from_receipt(receipt)?;
    let (tx, output_index) = crate::parse_ckb_cell_dep_location(dependency)?;
    let expected = &receipt.deployment.code_cell_dep;
    if expected.dep_type != crate::protocol_bundle::ProtocolDepType::Code
        || tx.as_deref().map(|h| h.trim_start_matches("0x")) != Some(expected.out_point.tx_hash.trim_start_matches("0x"))
        || output_index != Some(expected.out_point.index)
        || dependency.data_hash.as_deref().map(|h| h.trim_start_matches("0x")) != Some(receipt.artifact_hash.as_str())
    {
        return Err(error("named deployment outpoint or data hash differs from exact receipt"));
    }
    if declaration.verification_key_hash.len() != 64 || hex::decode(&declaration.verification_key_hash).is_err() {
        return Err(error("VK commitment must be 32 bytes of unprefixed hex"));
    }
    Ok((*index, handle))
}

#[cfg(not(feature = "wasm"))]
pub(crate) fn validate(module: &IrModule, deploy: &CkbDeployConfig) -> Result<()> {
    for declaration in &deploy.zk_verifiers {
        resolve(deploy, &declaration.policy)?;
    }
    for item in &module.items {
        let IrItem::Action(action) = item else { continue };
        let instructions: Vec<_> = action.body.blocks.iter().flat_map(|b| &b.instructions).collect();
        for instruction in &instructions {
            let IrInstruction::Call { func, args, .. } = instruction else { continue };
            if func != crate::zk_contract::REQUIRE_HELPER {
                continue;
            }
            let Some(IrOperand::Const(IrConst::Array(bytes))) = args.first() else {
                return Err(error("missing policy"));
            };
            let policy: String = bytes
                .iter()
                .map(|b| match b {
                    IrConst::U8(b) => Ok(char::from(*b)),
                    _ => Err(error("policy encoding")),
                })
                .collect::<Result<_>>()?;
            let (index, handle) = resolve(deploy, &policy)?;
            let declaration = deploy.zk_verifiers.iter().find(|d| d.policy == policy).unwrap();
            let hash = |i: usize| match args.get(i) {
                Some(IrOperand::Const(IrConst::Hash(bytes))) => Ok(hex::encode(bytes)),
                _ => Err(error("missing compile-time identity")),
            };
            if hash(3)? != exact_script_handle_value_hash(&handle)? || hash(4)? != declaration.verification_key_hash {
                return Err(error("source handle or VK differs from named policy"));
            }
            let Some(IrOperand::Var(selected)) = args.get(2) else {
                return Err(error("missing dependency view"));
            };
            let matching = instructions.iter().any(|op| match op {
                IrInstruction::Call { dest: Some(dest), func, args } if dest.id == selected.id && func == "__ckb_source_cell_dep" => {
                    matches!(args.as_slice(), [IrOperand::Const(IrConst::U64(n))] if *n == index as u64)
                }
                _ => false,
            });
            if !matching {
                return Err(error("source CellDep index must match the named manifest dependency"));
            }
        }
    }
    Ok(())
}

/// Browser compilation is metadata-only and cannot resolve native package receipts.
#[cfg(feature = "wasm")]
pub(crate) fn validate(module: &IrModule, deploy: &CkbDeployConfig) -> Result<()> {
    if !deploy.zk_verifiers.is_empty() || !crate::zk_contract::browser_contracts(module).is_empty() {
        return Err(error("named deployment resolution requires the native package compiler"));
    }
    Ok(())
}
