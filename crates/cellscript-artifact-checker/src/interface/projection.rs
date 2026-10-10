//! Checked module API coherence, separate from package/deployment admission.
use super::{layouts, nominals, source_types::SourceArgument, InterfaceInspection, SourceType};
use crate::{
    CheckerBudgets, CheckerError, CheckerRejectionCode, CheckerReport, EntryDispatchContract, TypedSemanticGenericDeclaration,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const SCHEMA: &str = "cellscript-checked-module-projection-v1";
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;

/// Only an actual independently inspected bundle can construct this value.
/// Directional matching proves these projected contracts agree. It does not
/// prove source equivalence, behavior, package ownership, codecs, deployment,
/// catalog admission or successful execution of a peer Script.
#[derive(Debug)]
pub struct CheckedModuleProjection {
    record: ProjectionRecord,
    identity: String,
    report: CheckerReport,
}

#[derive(Debug, Serialize)]
struct ProjectionRecord {
    schema: &'static str,
    module: String,
    runtime: super::InterfaceRuntimeContract,
    contracts: BTreeMap<String, Value>,
}

impl CheckedModuleProjection {
    pub fn module(&self) -> &str {
        &self.record.module
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Runtime axes derived from the independently inspected actual bundle.
    pub fn runtime_contract(&self) -> &super::InterfaceRuntimeContract {
        &self.record.runtime
    }

    /// The bound artifact identities remain separate from the API identity.
    pub fn artifact_report(&self) -> &CheckerReport {
        &self.report
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }

    /// Candidate additions are permitted. Every required declaration, nested
    /// nominal, retained layout and effective callable binding stays bound
    /// under the directional relation: inferred and declared effects may only
    /// become more restrictive, and binder constraints may only relax (the
    /// candidate may not demand an ability the baseline did not demand).
    /// Binder names, phantom flags, abilities, codecs and every other field
    /// remain exact.
    pub fn check_required_contracts(&self, candidate: &Self) -> Result<(), CheckerError> {
        if self.record.module != candidate.record.module || self.record.runtime != candidate.record.runtime {
            return Err(invalid("required module or runtime profile differs"));
        }
        for (key, required) in &self.record.contracts {
            let Some(offered) = candidate.record.contracts.get(key) else {
                return Err(invalid(&format!("required checked module contract is absent: {key}")));
            };
            if required != offered && !contract_satisfies(key, required, offered) {
                return Err(invalid(&format!("required checked module contract differs: {key}")));
            }
        }
        Ok(())
    }
}

/// The enumerated effect-weakening relation. Pure does nothing observable,
/// ReadOnly observes without changing state, and Mutating/Creating/Destroying
/// change state in pairwise-incomparable ways: each class admits only itself
/// and the strictly smaller classes as a compatible candidate effect.
fn effect_weakens(required: &str, candidate: &str) -> bool {
    matches!(
        (required, candidate),
        ("Pure", "Pure")
            | ("ReadOnly", "Pure" | "ReadOnly")
            | ("Mutating", "Pure" | "ReadOnly" | "Mutating")
            | ("Creating", "Pure" | "ReadOnly" | "Creating")
            | ("Destroying", "Pure" | "ReadOnly" | "Destroying")
    )
}

/// Every ability the candidate demands must already be demanded by the
/// baseline, so arguments admitted under the required contract still satisfy
/// the candidate. Binder names and phantom flags stay identical.
fn parameters_relax(required: &[Value], candidate: &[Value]) -> bool {
    required.len() == candidate.len()
        && required.iter().zip(candidate).all(|(required, candidate)| {
            let (Some(required), Some(candidate)) = (required.as_object(), candidate.as_object()) else {
                return false;
            };
            if required.get("name") != candidate.get("name") || required.get("phantom") != candidate.get("phantom") {
                return false;
            }
            match (required.get("constraints").and_then(Value::as_array), candidate.get("constraints").and_then(Value::as_array)) {
                (Some(required), Some(candidate)) => candidate.iter().all(|ability| required.contains(ability)),
                (None, None) => true,
                _ => false,
            }
        })
}

/// One admitted directional difference: the field name and the predicate
/// deciding whether the candidate's value satisfies the required value.
type RelaxedField = (&'static str, fn(&Value, &Value) -> bool);

/// Compare contract objects field-wise. The field sets must match exactly;
/// only the named fields may differ and only in the admitted direction.
fn directional_object(required: &Value, candidate: &Value, relaxed: &[RelaxedField]) -> bool {
    let (Some(required), Some(candidate)) = (required.as_object(), candidate.as_object()) else {
        return false;
    };
    if required.len() != candidate.len() {
        return false;
    }
    required.iter().all(|(field, value)| match candidate.get(field) {
        Some(other) if value == other => true,
        Some(other) => relaxed.iter().any(|(name, admissible)| field == name && admissible(value, other)),
        None => false,
    })
}

fn contract_satisfies(key: &str, required: &Value, candidate: &Value) -> bool {
    let effect: fn(&Value, &Value) -> bool = |required, candidate| matches!((required.as_str(), candidate.as_str()), (Some(required), Some(candidate)) if effect_weakens(required, candidate));
    let parameters: fn(&Value, &Value) -> bool = |required, candidate| matches!((required.as_array(), candidate.as_array()), (Some(required), Some(candidate)) if parameters_relax(required, candidate));
    if key.starts_with("callable:")
        && directional_object(required, candidate, &[("declared_effect", effect), ("type_parameters", parameters)])
    {
        return true;
    }
    if key.starts_with("effective:") && directional_object(required, candidate, &[("effect", effect)]) {
        return true;
    }
    if key.starts_with("nominal:") && directional_object(required, candidate, &[("parameters", parameters)]) {
        return true;
    }
    false
}

fn check_sizes(lengths: [usize; 4]) -> Result<(), CheckerError> {
    let total = lengths.iter().try_fold(0usize, |total, size| total.checked_add(*size));
    if lengths.iter().any(|size| *size > MAX_FILE_BYTES) || total.is_none_or(|size| size > MAX_BUNDLE_BYTES) {
        return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "checked module projection exceeds byte budget"));
    }
    Ok(())
}

/// Apply fixed projection ceilings before parsing any supplied bundle bytes.
/// Caller checker budgets can narrow these ceilings, never enlarge them.
pub fn project_bundle(
    artifact: &[u8],
    metadata: &[u8],
    lowering: &[u8],
    source_map: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedModuleProjection, CheckerError> {
    check_sizes([artifact.len(), metadata.len(), lowering.len(), source_map.len()])?;
    super::inspect_bundle(artifact, metadata, lowering, source_map, budgets)?.project_module_contract()
}

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

struct Projector<'a> {
    inspection: &'a InterfaceInspection,
    contracts: BTreeMap<String, Value>,
    pending: VecDeque<String>,
    visited: BTreeSet<String>,
    visits: usize,
}

impl Projector<'_> {
    fn qualify(&mut self, ty: &str, module: &str, parameters: &[&str]) -> Result<String, CheckerError> {
        let catalog = self.inspection.effective.nominal_declarations.as_ref().expect("checked prerequisite");
        let scope = nominals::scope(catalog, module)?;
        let qualified = super::qualified_source_type_with_parameters(ty, scope, parameters)?;
        self.references(&super::parse_source_type(&qualified)?, parameters)?;
        Ok(qualified)
    }

    fn references(&mut self, ty: &SourceType, parameters: &[&str]) -> Result<(), CheckerError> {
        self.visits += 1;
        if self.visits > 4096 {
            return Err(invalid("module projection exceeds its traversal bound"));
        }
        match ty {
            SourceType::Named(name, arguments) => {
                if name.contains("::") && !parameters.contains(&name.as_str()) {
                    self.pending.push_back(name.clone());
                }
                for argument in arguments {
                    if let SourceArgument::Type(ty) = argument {
                        self.references(ty, parameters)?;
                    }
                }
            }
            SourceType::Array(inner, _) | SourceType::Reference { inner, .. } => self.references(inner, parameters)?,
            SourceType::Tuple(fields) => {
                for field in fields {
                    self.references(field, parameters)?;
                }
            }
        }
        Ok(())
    }

    fn nominal(&mut self, identity: &str) -> Result<(), CheckerError> {
        let (module, name) = identity.rsplit_once("::").ok_or_else(|| invalid("unqualified nominal projection"))?;
        let record = &self.inspection.effective;
        let catalog = record.nominal_declarations.as_ref().expect("checked prerequisite");
        if let Some(declaration) = catalog.declarations.iter().find(|item| item.module == module && item.name == name) {
            let mut fields = Vec::new();
            for field in &declaration.fields {
                fields.push(json!({"name":field.name,"type":self.qualify(&field.ty, module, &[])?}));
            }
            let mut variants = Vec::new();
            for variant in &declaration.variants {
                let fields = variant.fields.iter().map(|ty| self.qualify(ty, module, &[])).collect::<Result<Vec<_>, _>>()?;
                variants.push(json!({"name":variant.name,"fields":fields}));
            }
            let mut capabilities = declaration.capabilities.clone();
            capabilities.sort();
            self.contracts.insert(
                format!("nominal:{identity}"),
                json!({
                    "kind":declaration.kind,"visibility":declaration.visibility,
                    "fields":fields,"variants":variants,"abilities":declaration.abilities,
                    "capabilities":capabilities,"identity_policy":declaration.identity_policy,
                    "source_type_identity":declaration.type_identity
                }),
            );
            return Ok(());
        }
        let generic = record
            .generic_declarations
            .as_ref()
            .and_then(|catalog| {
                catalog.declarations.iter().find(|item| item.module == module && item.name == name && item.kind != "function")
            })
            .ok_or_else(|| invalid("nested nominal projection lacks a checked defining declaration"))?;
        let parameters = generic.parameters.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>();
        let shape = match &generic.declaration {
            TypedSemanticGenericDeclaration::Struct { fields, abilities } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        Ok(json!({
                            "name":field.name,"type":self.qualify(&field.ty, module, &parameters)?
                        }))
                    })
                    .collect::<Result<Vec<Value>, CheckerError>>()?;
                json!({"kind":"struct","fields":fields,"abilities":abilities})
            }
            TypedSemanticGenericDeclaration::Enum { variants, abilities } => {
                let variants = variants
                    .iter()
                    .map(|variant| {
                        let fields =
                            variant.fields.iter().map(|ty| self.qualify(ty, module, &parameters)).collect::<Result<Vec<_>, _>>()?;
                        Ok(json!({"name":variant.name,"fields":fields}))
                    })
                    .collect::<Result<Vec<Value>, CheckerError>>()?;
                json!({"kind":"enum","variants":variants,"abilities":abilities})
            }
            _ => return Err(invalid("nested nominal projection has no symbolic shape")),
        };
        self.contracts.insert(
            format!("nominal:{identity}"),
            json!({
                "parameters":generic.parameters,"visibility":generic.visibility,"shape":shape
            }),
        );
        Ok(())
    }

    fn layouts(&mut self) -> Result<(), CheckerError> {
        let record = &self.inspection.effective;
        let catalog = record.nominal_declarations.as_ref().expect("checked prerequisite");
        for ty in &record.types {
            let identity = layouts::checked_type(&ty.name, catalog, record)?;
            let owner = identity.split('<').next().unwrap_or(&identity);
            if !self.visited.contains(owner) {
                continue;
            }
            let fields = ty
                .fields
                .iter()
                .map(|field| {
                    Ok(json!({
                        "name":field.name,"type":layouts::checked_type(&field.ty,catalog,record)?,
                        "offset":field.offset,"width_bytes":field.width_bytes
                    }))
                })
                .collect::<Result<Vec<Value>, CheckerError>>()?;
            let variants = ty.variants.iter().map(|variant| {
                let fields = variant.fields.iter().map(|field| Ok(json!({
                    "index":field.index,"type":layouts::checked_type(&field.ty,catalog,record)?,
                    "offset":field.offset,"width_bytes":field.width_bytes,"linear":field.linear
                }))).collect::<Result<Vec<Value>, CheckerError>>()?;
                Ok(json!({"name":variant.name,"tag":variant.tag,"payload_width_bytes":variant.payload_width_bytes,"fields":fields}))
            }).collect::<Result<Vec<Value>, CheckerError>>()?;
            let value = json!({"kind":ty.kind,"encoded_size":ty.encoded_size,"fields":fields,
                "tag_width_bytes":ty.tag_width_bytes,"variants":variants,"capabilities":ty.capabilities,
                "value_abilities":ty.value_abilities,"identity_policy":ty.identity_policy});
            let key = format!("layout:{identity}");
            if self.contracts.insert(key, value.clone()).is_some_and(|previous| previous != value) {
                return Err(invalid("one qualified nominal has conflicting retained layouts"));
            }
        }
        Ok(())
    }

    fn callables(&mut self) -> Result<(), CheckerError> {
        for callable in &self.inspection.declared.callables {
            let parameters = callable.type_parameters.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>();
            let mut params = callable.params.clone();
            let mut outputs = callable.outputs.clone();
            for param in params.iter_mut().chain(&mut outputs) {
                param.r#type = self.qualify(&param.r#type, &self.inspection.declared.module, &parameters)?;
            }
            let result = callable.return_type.as_deref().unwrap_or("unit");
            let result = self.qualify(result, &self.inspection.declared.module, &parameters)?;
            self.contracts.insert(
                format!("callable:{}", callable.identity),
                json!({
                    "kind":callable.kind,"visibility":callable.visibility,"type_parameters":callable.type_parameters,
                    "params":params,"outputs":outputs,"return_type":result,"declared_effect":callable.effect,
                    "entry_witness_abi":callable.entry_witness_abi
                }),
            );
            if let Some(entry) = self.inspection.effective.entries.iter().find(|entry| entry.name == callable.name) {
                let catalog = self.inspection.effective.nominal_declarations.as_ref().expect("checked prerequisite");
                let record = &self.inspection.effective;
                let mut bindings = Vec::new();
                for binding in &entry.cell_bindings {
                    bindings.push(json!({
                        "binding":binding.binding,"role":binding.role,
                        "type":layouts::checked_type(&binding.ty,catalog,record)?,
                        "source":binding.source,"ordinal":binding.ordinal,"membership":binding.membership
                    }));
                }
                let params = entry
                    .params
                    .iter()
                    .map(|param| {
                        Ok(json!({
                            "index":param.index,"name":param.name,"type":layouts::checked_type(&param.ty,catalog,record)?,
                            "source":param.source,"mutable":param.mutable,"reference":param.reference
                        }))
                    })
                    .collect::<Result<Vec<Value>, CheckerError>>()?;
                let contract = &record.foundation.entry_contract;
                let dispatch = match &contract.dispatch {
                    EntryDispatchContract::SingleEntry if entry.id == contract.exact_entry => json!({"kind":"single-entry"}),
                    EntryDispatchContract::PolicyWitnessV1(policy) => {
                        match policy.variants.iter().find(|variant| variant.entry_id == entry.id) {
                            Some(variant) => json!({"kind":"policy-variant","tag":variant.tag,
                                "input_count":variant.input_count,"output_count":variant.output_count,
                                "max_records":policy.max_records,"max_witness_bytes":policy.max_witness_bytes,
                                "common_checks":policy.common_checks,"unknown_selector":policy.unknown_selector}),
                            None => json!({"kind":"retained-helper"}),
                        }
                    }
                    EntryDispatchContract::ExplicitVersionedDispatch { selector_type, variants, unknown_selector, .. } => {
                        let tags = variants
                            .iter()
                            .filter(|variant| variant.entry_id == entry.id)
                            .map(|variant| &variant.tag)
                            .collect::<Vec<_>>();
                        if tags.is_empty() {
                            json!({"kind":"retained-helper"})
                        } else {
                            json!({"kind":"explicit-dispatch","tags":tags,"selector_type":selector_type,"unknown_selector":unknown_selector})
                        }
                    }
                    _ => json!({"kind":"retained-helper"}),
                };
                self.contracts.insert(format!("effective:{}", callable.identity), json!({
                    "kind":entry.kind,"params":params,"return_type":layouts::checked_type(&entry.return_type,catalog,record)?,
                    "effect":entry.effect,"cell_bindings":bindings,"dispatch":dispatch,
                    "script_role":contract.script_role,"entry_payload_abi":contract.entry_payload_abi,
                    "witness_placement_abi":contract.witness_placement_abi,
                    "witness_placement_field":contract.witness_placement_field,"witness_placement_source":contract.witness_placement_source
                }));
            }
        }
        Ok(())
    }
}

pub(super) fn project(inspection: &InterfaceInspection) -> Result<CheckedModuleProjection, CheckerError> {
    check_sizes(inspection.bundle_bytes)?;
    inspection.validate_symbolic_declarations()?;
    let mut projector =
        Projector { inspection, contracts: BTreeMap::new(), pending: VecDeque::new(), visited: BTreeSet::new(), visits: 0 };
    for ty in &inspection.declared.types {
        projector.pending.push_back(ty.identity.clone());
    }
    // Every retained concrete instance is an identity dependency of the
    // checked module even when no public spelling names it, for example a
    // nominal referenced only through a phantom type argument or a private
    // template instantiated by an internal helper. Its template and qualified
    // arguments therefore join the nominal closure; unrelated types were
    // already pruned from the retained bundle before this projection runs.
    for instance in inspection.effective.instantiations.iter().filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum")) {
        let catalog = inspection.effective.nominal_declarations.as_ref().expect("checked prerequisite");
        let scope = nominals::scope(catalog, &inspection.effective.module)?;
        projector.pending.push_back(format!("{}::{}", instance.module, instance.template));
        for argument in &instance.type_arguments {
            let qualified = super::qualified_source_type(argument, scope)?;
            projector.references(&super::parse_source_type(&qualified)?, &[])?;
        }
    }
    for constant in &inspection.declared.constants {
        let ty = projector.qualify(&constant.r#type, &inspection.declared.module, &[])?;
        projector.contracts.insert(format!("constant:{}", constant.identity), json!({"visibility":constant.visibility,"type":ty}));
    }
    projector.callables()?;
    while let Some(identity) = projector.pending.pop_front() {
        if projector.visited.insert(identity.clone()) {
            if projector.visited.len() > 256 {
                return Err(invalid("module projection exceeds its nominal closure bound"));
            }
            projector.nominal(&identity)?;
        }
    }
    projector.layouts()?;
    let record = ProjectionRecord {
        schema: SCHEMA,
        module: inspection.declared.module.clone(),
        runtime: inspection.declared.runtime_contract.clone(),
        contracts: projector.contracts,
    };
    let identity = crate::canonical_hash("cellscript-checked-module-projection-id-v1", &record)?;
    Ok(CheckedModuleProjection { record, identity, report: inspection.report.clone() })
}
