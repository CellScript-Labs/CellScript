use super::error::{CompileError, Result};
use super::metadata_constraints::ckb_supported_hash_types;
use super::package::{CkbCellDepConfig, CkbTrustedExternalVerifierConfig, PackageManifest};
use super::proof_plan::{EvidenceTier, ProofPlanDiagnosticMetadata, ProofPlanMetadata};
use super::{
    hex_decode, hex_encode, ir, refresh_constraints_status, CkbCellDepMetadata, CkbScriptReferenceMetadata, CompileMetadata,
    TargetProfileMetadata, TransactionRuntimeInputRequirementMetadata, VerifierObligationMetadata, CKB_MINIMUM_VM_VERSION,
    CKB_RISCV_ISA,
};
#[cfg(not(feature = "wasm"))]
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn apply_manifest_deploy_metadata(metadata: &mut CompileMetadata, manifest: &PackageManifest) -> Result<()> {
    let Some(ckb_manifest) = manifest.deploy.ckb.as_ref() else {
        return Ok(());
    };
    let Some(ckb_constraints) = metadata.constraints.ckb.as_mut() else {
        return Ok(());
    };

    ckb_constraints.trusted_external_verifiers = metadata.runtime.trusted_external_verifiers.clone();
    for verifier in &metadata.runtime.trusted_external_verifiers {
        ckb_constraints.script_references.push(CkbScriptReferenceMetadata {
            scope: verifier.scope.clone(),
            purpose: format!("trusted-external-{}", verifier.operation),
            name: verifier.name.clone(),
            code_hash: Some(verifier.code_hash.clone()),
            hash_type: Some(verifier.hash_type.clone()),
            args: None,
            dep_source: "CellDep selected at runtime and bound by DATA_HASH".to_string(),
            profile: "ckb".to_string(),
            status: "trusted-external-identity-bound".to_string(),
        });
    }
    ckb_constraints
        .script_references
        .sort_by(|left, right| (&left.scope, &left.purpose, &left.name).cmp(&(&right.scope, &right.purpose, &right.name)));
    ckb_constraints
        .script_references
        .dedup_by(|left, right| left.scope == right.scope && left.purpose == right.purpose && left.name == right.name);

    if let Some(hash_type) = ckb_manifest.hash_type.as_deref() {
        validate_ckb_hash_type(hash_type)?;
        validate_ckb_generated_artifact_hash_type(hash_type, &metadata.target_profile)?;
        ckb_constraints.hash_type_policy.source = "Cell.toml deploy.ckb.hash_type".to_string();
        ckb_constraints.hash_type_policy.declared_hash_type = Some(hash_type.to_string());
        ckb_constraints.hash_type_policy.status = "manifest-declared-builder-must-match".to_string();
        ckb_constraints.hash_type_policy_surface =
            "manifest-declared-script-hash-type; builder/deployment output must match".to_string();
    }

    let mut declared_cell_deps = Vec::new();
    if ckb_manifest.out_point.is_some() || ckb_manifest.dep_type.is_some() || ckb_manifest.data_hash.is_some() {
        let dep_type = ckb_manifest.dep_type.as_deref().unwrap_or("code");
        validate_ckb_dep_type(dep_type)?;
        let (tx_hash, index) = ckb_manifest
            .out_point
            .as_deref()
            .map(parse_ckb_out_point)
            .transpose()?
            .map(|(tx_hash, index)| (Some(tx_hash), Some(index)))
            .unwrap_or((None, None));
        declared_cell_deps.push(CkbCellDepMetadata {
            name: "primary".to_string(),
            artifact_hash: ckb_manifest.artifact_hash.clone(),
            tx_hash,
            index,
            dep_type: dep_type.to_string(),
            data_hash: ckb_manifest.data_hash.clone(),
            hash_type: ckb_manifest.hash_type.clone(),
            type_id: ckb_manifest.type_id.clone(),
        });
    }
    for (index, dep) in ckb_manifest.cell_deps.iter().enumerate() {
        let dep_type = dep.dep_type.as_deref().unwrap_or("code");
        validate_ckb_dep_type(dep_type)?;
        if let Some(hash_type) = dep.hash_type.as_deref() {
            validate_ckb_hash_type(hash_type)?;
        }
        let (tx_hash, dep_index) = parse_ckb_cell_dep_location(dep)?;
        declared_cell_deps.push(CkbCellDepMetadata {
            name: dep.name.clone().unwrap_or_else(|| format!("cell_dep_{}", index)),
            artifact_hash: None,
            tx_hash,
            index: dep_index,
            dep_type: dep_type.to_string(),
            data_hash: dep.data_hash.clone(),
            hash_type: dep.hash_type.clone(),
            type_id: dep.type_id.clone(),
        });
    }

    if !declared_cell_deps.is_empty() {
        let has_dep_group = declared_cell_deps.iter().any(|dep| dep.dep_type == "dep_group");
        ckb_constraints.dep_group_manifest.source = "Cell.toml deploy.ckb".to_string();
        ckb_constraints.dep_group_manifest.production_manifest_required = true;
        ckb_constraints.dep_group_manifest.declared_cell_deps = declared_cell_deps;
        ckb_constraints.dep_group_manifest.status = if has_dep_group {
            "manifest-declares-dep-group-builder-must-expand-or-reference".to_string()
        } else {
            "manifest-declares-code-cell-deps".to_string()
        };
    }

    refresh_constraints_status(&mut metadata.constraints);
    Ok(())
}

pub(super) const TRUSTED_EXTERNAL_VERIFIER_SCHEMA: &str = "cellscript-trusted-external-verifier-v1";

#[derive(Debug, Clone)]
struct TrustedExternalInvocation {
    scope: String,
    operation: String,
    adapter: String,
    code_hash: String,
}

fn trusted_external_invocations(ir: &ir::IrModule) -> Result<Vec<TrustedExternalInvocation>> {
    let mut invocations = Vec::new();
    for item in &ir.items {
        let (scope, body) = match item {
            ir::IrItem::Action(item) => (format!("action:{}", item.name), &item.body),
            ir::IrItem::Function(item) => (format!("helper:{}", item.name), &item.body),
            ir::IrItem::Lock(item) => (format!("lock:{}", item.name), &item.body),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        for operation in ["exec", "spawn-wait"] {
            let trusted_count = body.trusted_external_calls.iter().filter(|call| call.operation == operation).count();
            let raw_count = body
                .blocks
                .iter()
                .flat_map(|block| block.instructions.iter())
                .filter(|instruction| {
                    matches!(
                        instruction,
                        ir::IrInstruction::Call { func, .. }
                            if (operation == "exec"
                                && matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepU8Args | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepHex4)))
                                || (operation == "spawn-wait" && func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSpawnWaitCellDepHex4))
                    )
                })
                .count();
            if trusted_count > 0 && raw_count != trusted_count {
                return Err(CompileError::without_span(format!(
                    "scope '{scope}' mixes trusted and undeclared {operation} delegation; every external verifier call in a trusted scope must use a trusted_* intrinsic"
                ))
                .with_code(crate::error::DiagnosticCode::E2113));
            }
        }
        invocations.extend(body.trusted_external_calls.iter().map(|call| TrustedExternalInvocation {
            scope: scope.clone(),
            operation: call.operation.clone(),
            adapter: call.adapter.clone(),
            code_hash: hex_encode(&call.code_hash),
        }));
    }
    if invocations.len() > 1_024 {
        return Err(CompileError::without_span("trusted external verifier call-site count exceeds 1024")
            .with_code(crate::error::DiagnosticCode::E2113));
    }

    Ok(invocations)
}

#[cfg(feature = "wasm")]
pub(super) fn apply_trusted_external_verifiers(
    _metadata: &mut CompileMetadata,
    ir: &ir::IrModule,
    declarations: &[CkbTrustedExternalVerifierConfig],
) -> Result<()> {
    // Browser source sets have no authenticated Cell.toml/deployment binding.
    // Keep the shared call inventory checks, then reject unbound delegation.
    if !declarations.is_empty() {
        return Err(CompileError::without_span("browser metadata cannot bind native external verifier declarations")
            .with_code(crate::error::DiagnosticCode::E2113));
    }
    if let Some(invocation) = trusted_external_invocations(ir)?.first() {
        return Err(CompileError::without_span(format!(
            "trusted external verifier call in '{}' has no exact Cell.toml binding for operation '{}', adapter '{}', and code hash '{}'",
            invocation.scope, invocation.operation, invocation.adapter, invocation.code_hash
        ))
        .with_code(crate::error::DiagnosticCode::E2113));
    }
    Ok(())
}

#[cfg(not(feature = "wasm"))]
pub(super) fn apply_trusted_external_verifiers(
    metadata: &mut CompileMetadata,
    ir: &ir::IrModule,
    declarations: &[CkbTrustedExternalVerifierConfig],
) -> Result<()> {
    if declarations.len() > 1_024 {
        return Err(CompileError::without_span("trusted external verifier declaration count exceeds 1024")
            .with_code(crate::error::DiagnosticCode::E2113));
    }
    let invocations = trusted_external_invocations(ir)?;

    let mut declaration_keys = BTreeSet::new();
    let mut declaration_names = BTreeSet::new();
    for declaration in declarations {
        validate_trusted_external_declaration(declaration)?;
        let key =
            (declaration.scope.clone(), declaration.operation.clone(), declaration.adapter.clone(), declaration.code_hash.clone());
        if !declaration_keys.insert(key) {
            return Err(CompileError::without_span(format!(
                "duplicate trusted external verifier binding for scope '{}', operation '{}', adapter '{}', code hash '{}'",
                declaration.scope, declaration.operation, declaration.adapter, declaration.code_hash
            ))
            .with_code(crate::error::DiagnosticCode::E2113));
        }
        if !declaration_names.insert(declaration.name.clone()) {
            return Err(CompileError::without_span(format!(
                "trusted external verifier name '{}' is declared more than once",
                declaration.name
            ))
            .with_code(crate::error::DiagnosticCode::E2113));
        }
    }

    let mut used = BTreeSet::new();
    let mut records = Vec::new();
    for invocation in &invocations {
        let Some((index, declaration)) = declarations.iter().enumerate().find(|(_, declaration)| {
            declaration.scope == invocation.scope
                && declaration.operation == invocation.operation
                && declaration.adapter == invocation.adapter
                && declaration.code_hash == invocation.code_hash
                && declaration.hash_type == "data"
        }) else {
            return Err(CompileError::without_span(format!(
                "trusted external verifier call in '{}' has no exact Cell.toml binding for operation '{}', adapter '{}', and code hash '{}'",
                invocation.scope, invocation.operation, invocation.adapter, invocation.code_hash
            ))
            .with_code(crate::error::DiagnosticCode::E2113));
        };
        used.insert(index);
        let mut guarantees = declaration.guarantees.clone();
        guarantees.sort();
        guarantees.dedup();
        records.push(cellscript_artifact_checker::TrustedExternalVerifierRecord {
            schema: TRUSTED_EXTERNAL_VERIFIER_SCHEMA.to_string(),
            version: 1,
            name: declaration.name.clone(),
            scope: declaration.scope.clone(),
            operation: declaration.operation.clone(),
            adapter: declaration.adapter.clone(),
            code_hash: declaration.code_hash.clone(),
            hash_type: declaration.hash_type.clone(),
            source_identity: declaration.source_identity.clone(),
            applicability: declaration.applicability.clone(),
            trust_basis: declaration.trust_basis.clone(),
            guarantees,
            identity_binding: "runtime-load-cell-data-hash-before-delegation-v1".to_string(),
            evidence_tier: EvidenceTier::TrustedExternal.as_str().to_string(),
            compiler_proves_internal_semantics: false,
        });
    }
    if let Some((_, unused)) = declarations.iter().enumerate().find(|(index, _)| !used.contains(index)) {
        return Err(CompileError::without_span(format!(
            "trusted external verifier declaration '{}' is unused; declarations must bind an exact trusted_* call site",
            unused.name
        ))
        .with_code(crate::error::DiagnosticCode::E2113));
    }
    records.sort_by(|left, right| {
        (&left.scope, &left.operation, &left.adapter, &left.code_hash, &left.name).cmp(&(
            &right.scope,
            &right.operation,
            &right.adapter,
            &right.code_hash,
            &right.name,
        ))
    });
    records.dedup_by(|left, right| {
        left.scope == right.scope
            && left.operation == right.operation
            && left.adapter == right.adapter
            && left.code_hash == right.code_hash
            && left.name == right.name
    });
    metadata.runtime.trusted_external_verifiers = records.clone();

    let grouped = records.iter().fold(BTreeMap::<(String, String), Vec<_>>::new(), |mut grouped, record| {
        grouped.entry((record.scope.clone(), record.operation.clone())).or_default().push(record);
        grouped
    });
    for ((scope, operation), bindings) in grouped {
        rewrite_trusted_external_evidence(metadata, &scope, &operation, &bindings);
    }
    metadata.runtime.proof_plan_soundness = crate::proof_plan::soundness::check_metadata(metadata, false);
    Ok(())
}

fn validate_trusted_external_declaration(declaration: &CkbTrustedExternalVerifierConfig) -> Result<()> {
    let canonical_hash = hex_decode(&declaration.code_hash).filter(|bytes| hex_encode(bytes) == declaration.code_hash).is_some();
    let required_text = [
        ("name", declaration.name.as_str()),
        ("scope", declaration.scope.as_str()),
        ("source_identity", declaration.source_identity.as_str()),
        ("applicability", declaration.applicability.as_str()),
        ("trust_basis", declaration.trust_basis.as_str()),
    ];
    let adapter_valid = matches!(
        (declaration.operation.as_str(), declaration.adapter.as_str()),
        ("exec", "u8-args-v1" | "hex4-v1") | ("spawn-wait", "hex4-v1")
    );
    if declaration.schema != TRUSTED_EXTERNAL_VERIFIER_SCHEMA
        || !adapter_valid
        || declaration.hash_type != "data"
        || declaration.code_hash.len() != 64
        || !canonical_hash
        || required_text.iter().any(|(_, value)| value.trim().is_empty() || value.len() > 4_096)
        || declaration.guarantees.is_empty()
        || declaration.guarantees.len() > 64
        || declaration.guarantees.iter().any(|guarantee| guarantee.trim().is_empty() || guarantee.len() > 4_096)
    {
        return Err(CompileError::without_span(format!(
            "trusted external verifier declaration '{}' is invalid: require schema '{}', operation/adapter exec+(u8-args-v1|hex4-v1) or spawn-wait+hex4-v1, hash_type=data, a canonical lowercase 32-byte code_hash, bounded non-empty identity/applicability/trust text, and 1..=64 bounded guarantees",
            declaration.name, TRUSTED_EXTERNAL_VERIFIER_SCHEMA
        ))
        .with_code(crate::error::DiagnosticCode::E2113));
    }
    Ok(())
}

fn rewrite_trusted_external_evidence(
    metadata: &mut CompileMetadata,
    scope: &str,
    operation: &str,
    bindings: &[&cellscript_artifact_checker::TrustedExternalVerifierRecord],
) {
    let proof_scope = scope.strip_prefix("helper:").map_or_else(|| scope.to_string(), |name| format!("fn:{name}"));
    let (category, feature) = if operation == "exec" {
        ("exec-delegation", "exec-target-and-replaced-continuation")
    } else {
        ("spawn-delegation", "spawn-target-and-checked-child-exit")
    };
    let identities = bindings
        .iter()
        .map(|binding| format!("{}:{}={}", binding.name, binding.adapter, binding.code_hash))
        .collect::<Vec<_>>()
        .join(",");
    let detail = format!(
        "external verifier identity is checked on-chain before {operation}; trusted declaration(s) [{identities}] are recorded claims and the compiler does not prove external code internals"
    );
    let rewrite = |plans: &mut Vec<ProofPlanMetadata>, obligations: &mut Vec<VerifierObligationMetadata>| {
        for obligation in
            obligations.iter_mut().filter(|item| item.scope == proof_scope && item.category == category && item.feature == feature)
        {
            obligation.status = "trusted-external".to_string();
            obligation.detail = detail.clone();
        }
        for plan in plans.iter_mut().filter(|item| item.origin == proof_scope && item.category == category && item.feature == feature)
        {
            plan.evidence_tier = EvidenceTier::TrustedExternal;
            plan.coverage.extend([
                "identity-binding:runtime-load-cell-data-hash-before-delegation-v1".to_string(),
                format!("trusted-external:{identities}"),
                "compiler-proves-external-internals:false".to_string(),
            ]);
            plan.coverage.sort();
            plan.coverage.dedup();
            plan.on_chain_checked = true;
            plan.on_chain_checked_obligations = vec![
                format!("{category}:{feature}=trusted-external"),
                "selected CellDep DATA_HASH equals the source-pinned verifier hash before delegation".to_string(),
                "EXEC process replacement or SPAWN/WAIT child-exit contract is enforced by emitted runtime code".to_string(),
            ];
            plan.builder_assumptions.clear();
            plan.codegen_coverage_status = "covered".to_string();
            plan.status = "trusted-external".to_string();
            plan.detail = detail.clone();
            plan.diagnostics = vec![ProofPlanDiagnosticMetadata {
                severity: "info".to_string(),
                message: "trusted-external discharges target identity and delegation binding only; external verifier semantics remain outside compiler proof".to_string(),
            }];
        }
    };
    rewrite(&mut metadata.runtime.proof_plan, &mut metadata.runtime.verifier_obligations);
    for action in &mut metadata.actions {
        rewrite(&mut action.proof_plan, &mut action.verifier_obligations);
        rewrite_trusted_external_requirements(&mut action.transaction_runtime_input_requirements, &proof_scope, feature);
    }
    for function in &mut metadata.functions {
        rewrite(&mut function.proof_plan, &mut function.verifier_obligations);
        rewrite_trusted_external_requirements(&mut function.transaction_runtime_input_requirements, &proof_scope, feature);
    }
    for lock in &mut metadata.locks {
        rewrite(&mut lock.proof_plan, &mut lock.verifier_obligations);
        rewrite_trusted_external_requirements(&mut lock.transaction_runtime_input_requirements, &proof_scope, feature);
    }
    rewrite_trusted_external_requirements(&mut metadata.runtime.transaction_runtime_input_requirements, &proof_scope, feature);
}

fn rewrite_trusted_external_requirements(requirements: &mut [TransactionRuntimeInputRequirementMetadata], scope: &str, feature: &str) {
    for requirement in requirements.iter_mut().filter(|item| item.scope == scope && item.feature == feature) {
        requirement.status = "trusted-external".to_string();
        requirement.blocker = None;
        requirement.blocker_class = None;
        requirement.abi = "runtime-load-cell-data-hash-before-delegation-v1".to_string();
    }
}

pub(super) fn validate_ckb_hash_type(hash_type: &str) -> Result<()> {
    if ckb_supported_hash_types().iter().any(|supported| supported == hash_type) {
        Ok(())
    } else {
        Err(CompileError::without_span(format!("unsupported CKB hash_type '{}'; expected one of data, type, data1, data2", hash_type)))
    }
}

fn validate_ckb_generated_artifact_hash_type(hash_type: &str, target_profile: &TargetProfileMetadata) -> Result<()> {
    let expected = target_profile.deployment_hash_types.as_slice();
    if expected == [hash_type] {
        Ok(())
    } else {
        Err(CompileError::without_span(format!(
            "CKB VM{} / {} target profile '{}' artifacts require deploy.ckb.hash_type = '{}'; got '{}'",
            CKB_MINIMUM_VM_VERSION,
            CKB_RISCV_ISA,
            target_profile.name,
            expected.first().map(String::as_str).unwrap_or("<missing>"),
            hash_type
        )))
    }
}

fn validate_ckb_dep_type(dep_type: &str) -> Result<()> {
    if matches!(dep_type, "code" | "dep_group") {
        Ok(())
    } else {
        Err(CompileError::without_span(format!("unsupported CKB dep_type '{}'; expected 'code' or 'dep_group'", dep_type)))
    }
}

pub(super) fn parse_ckb_cell_dep_location(dep: &CkbCellDepConfig) -> Result<(Option<String>, Option<u32>)> {
    if let Some(out_point) = dep.out_point.as_deref() {
        if dep.tx_hash.is_some() || dep.index.is_some() {
            return Err(CompileError::without_span("CKB cell_dep location must use either out_point or tx_hash/index, not both"));
        }
        let (tx_hash, index) = parse_ckb_out_point(out_point)?;
        return Ok((Some(tx_hash), Some(index)));
    }
    if let Some(tx_hash) = dep.tx_hash.as_deref() {
        validate_ckb_tx_hash(tx_hash)?;
    }
    if dep.tx_hash.is_some() != dep.index.is_some() {
        return Err(CompileError::without_span("CKB cell_dep split location must provide both tx_hash and index, or neither"));
    }
    Ok((dep.tx_hash.clone(), dep.index))
}

fn parse_ckb_out_point(out_point: &str) -> Result<(String, u32)> {
    let Some((tx_hash, index)) = out_point.rsplit_once(':') else {
        return Err(CompileError::without_span(format!(
            "invalid CKB out_point '{}'; expected 0x<32-byte-tx-hash>:<index>",
            out_point
        )));
    };
    validate_ckb_tx_hash(tx_hash)?;
    let index = index.parse::<u32>().map_err(|_| {
        CompileError::without_span(format!("invalid CKB out_point index '{}'; expected unsigned 32-bit integer", index))
    })?;
    Ok((tx_hash.to_string(), index))
}

fn validate_ckb_tx_hash(tx_hash: &str) -> Result<()> {
    let hex = tx_hash
        .strip_prefix("0x")
        .ok_or_else(|| CompileError::without_span(format!("invalid CKB tx_hash '{}'; expected 0x-prefixed 32-byte hash", tx_hash)))?;
    if hex.len() != 64 || !hex.as_bytes().iter().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CompileError::without_span(format!("invalid CKB tx_hash '{}'; expected 0x-prefixed 32-byte hash", tx_hash)));
    }
    Ok(())
}
