//! Finite external ABI closure, separate from deployment/authorization.
use crate::entry_codec::{check_fixed_policy_parameter_decoders, CheckedFixedPolicyParameterDecoders};
use crate::fixed_cell_fields::{check_fixed_cell_scalar_fields, CheckedFixedCellScalarFields};
use crate::interface::{inspect_bundle, ModuleBundle};
use crate::{CellBindingRole, CheckerBudgets, CheckerError, CheckerRejectionCode, EntryDispatchContract};
use serde::Serialize;
use std::collections::BTreeSet;
mod prologue;

/// Actual four-file checking is the only constructor. Exported evidence cannot
/// reconstruct a checked value. This proves the stated finite codec/availability
/// boundary, never source predicates, compatibility admission or deployment.
#[derive(Debug)]
pub struct CheckedFixedExternalCodec {
    parameters: CheckedFixedPolicyParameterDecoders,
    fields: Option<CheckedFixedCellScalarFields>,
    record: Record,
    identity: String,
}
impl CheckedFixedExternalCodec {
    pub fn parameters(&self) -> &CheckedFixedPolicyParameterDecoders {
        &self.parameters
    }
    pub fn fields(&self) -> Option<&CheckedFixedCellScalarFields> {
        self.fields.as_ref()
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    profile: &'static str,
    module_contract: String,
    parameter_decoder: String,
    cell_fields: Option<String>,
    callables: Vec<Callable>,
    cells: Vec<Cell>,
    public_layouts: Vec<PublicLayout>,
}
#[derive(Debug, Serialize)]
struct Callable {
    declaration: String,
    entry: Option<String>,
    tag: Option<u32>,
    availability: &'static str,
}
#[derive(Debug, Serialize)]
struct Cell {
    entry: String,
    parameter: String,
    ty: String,
    width: u32,
}
#[derive(Debug, Serialize)]
struct PublicLayout {
    declaration: String,
    lowered: Option<String>,
    width: Option<u32>,
    availability: &'static str,
}
fn unsigned_layout(layout: &crate::TypedSemanticType) -> Result<u32, CheckerError> {
    if !layout.variants.is_empty() || layout.fields.len() > 64 || layout.fields.is_empty() {
        return Err(invalid("tagged, empty or oversized public layout lacks this profile"));
    }
    let mut width = 0u32;
    for field in &layout.fields {
        let bytes = match field.ty.as_str() {
            "u8" => 1,
            "u16" => 2,
            "u32" => 4,
            "u64" => 8,
            _ => return Err(invalid("public layout requires flat unsigned bitpattern fields")),
        };
        if field.offset != width || field.width_bytes != Some(bytes) {
            return Err(invalid("public layout offsets or unsigned widths differ"));
        }
        width = width.checked_add(bytes).ok_or_else(|| invalid("public layout width overflow"))?;
    }
    if width > 512 || layout.encoded_size != Some(width) {
        return Err(invalid("public layout total width differs or exceeds the fixed profile"));
    }
    Ok(width)
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("fixed external codec: {}", message.into()))
}

/// The initial profile accepts policy-witness-v1, unit results, no output Cell
/// serialization, and at most one directly bound flat unsigned Cell per entry.
/// Every concrete public callable must actually be externally dispatched;
/// generic declarations remain explicitly non-executable. Missing, pruned or
/// retained-helper ABI evidence fails closed instead of completing a receipt.
pub fn check_fixed_external_codec(
    bundle: ModuleBundle<'_>,
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedExternalCodec, CheckerError> {
    let parameters = check_fixed_policy_parameter_decoders(bundle[0], bundle[1], bundle[2], bundle[3], budgets)?;
    let inspection = inspect_bundle(bundle[0], bundle[1], bundle[2], bundle[3], budgets)?;
    let typed = inspection.effective();
    let EntryDispatchContract::PolicyWitnessV1(policy) = &typed.foundation.entry_contract.dispatch else {
        return Err(invalid("requires independently checked policy-witness-v1 dispatch"));
    };
    if !policy.common_checks.is_empty() {
        return Err(invalid("common-check call ABI lacks this finite profile"));
    }
    let mut callables = Vec::new();
    let mut external = BTreeSet::new();
    for declaration in &inspection.declared().callables {
        if !declaration.type_parameters.is_empty() {
            if typed.instantiations.iter().any(|instance| {
                instance.kind == "function" && instance.module == inspection.declared().module && instance.template == declaration.name
            }) {
                return Err(invalid("a generic concrete instance needs its own external ABI evidence"));
            }
            callables.push(Callable {
                declaration: declaration.identity.clone(),
                entry: None,
                tag: None,
                availability: "declaration-only",
            });
            continue;
        }
        let entry = typed
            .entries
            .iter()
            .find(|entry| entry.name == declaration.name)
            .ok_or_else(|| invalid(format!("public callable is not executable: {}", declaration.identity)))?;
        let variants = policy.variants.iter().filter(|variant| variant.entry_id == entry.id).collect::<Vec<_>>();
        if variants.len() != 1
            || entry.return_type != "unit"
            || declaration.return_type.as_deref().is_some_and(|ty| ty != "unit")
            || !declaration.outputs.is_empty()
        {
            return Err(invalid("public callable requires exactly one dispatch tag, unit return and no serialized outputs"));
        }
        external.insert(entry.id.as_str());
        callables.push(Callable {
            declaration: declaration.identity.clone(),
            entry: Some(entry.id.clone()),
            tag: Some(variants[0].tag),
            availability: "external-policy-entry",
        });
    }
    if policy.variants.iter().any(|variant| !external.contains(variant.entry_id.as_str())) {
        return Err(invalid("dispatch contains an entry outside the checked public contract"));
    }
    let catalog = typed.nominal_declarations.as_ref().ok_or_else(|| invalid("missing checked source declaration catalog"))?;
    let mut public_layouts = Vec::new();
    for declaration in &inspection.declared().types {
        if !declaration.type_parameters.is_empty() {
            if typed.instantiations.iter().any(|instance| {
                instance.kind == declaration.kind
                    && instance.module == inspection.declared().module
                    && instance.template == declaration.name
            }) {
                return Err(invalid("generic concrete layout needs its own public codec profile"));
            }
            public_layouts.push(PublicLayout {
                declaration: declaration.identity.clone(),
                lowered: None,
                width: None,
                availability: "declaration-only",
            });
            continue;
        }
        let bindings = catalog
            .layout_bindings
            .iter()
            .filter(|binding| binding.owner_module == inspection.declared().module && binding.source_name == declaration.name)
            .collect::<Vec<_>>();
        if bindings.len() != 1 {
            return Err(invalid("concrete public type lacks unique independently checked layout availability"));
        }
        let layout = typed
            .types
            .iter()
            .find(|layout| layout.name == bindings[0].lowered_name)
            .ok_or_else(|| invalid("concrete public layout is absent"))?;
        let width = unsigned_layout(layout)?;
        public_layouts.push(PublicLayout {
            declaration: declaration.identity.clone(),
            lowered: Some(layout.name.clone()),
            width: Some(width),
            availability: "fixed-flat-unsigned-layout",
        });
    }
    let mut cells = Vec::new();
    for variant in &policy.variants {
        let entry =
            typed.entries.iter().find(|entry| entry.id == variant.entry_id).ok_or_else(|| invalid("missing actual dispatch entry"))?;
        if entry.cell_bindings.len() > 1 {
            return Err(invalid("this initial profile permits at most one directly bound Cell per entry"));
        }
        for binding in &entry.cell_bindings {
            if binding.role == CellBindingRole::Output {
                return Err(invalid("output Cell serialization lacks this profile"));
            }
            let local = binding.local_id.ok_or_else(|| invalid("unbound Cell observation lacks parameter codec evidence"))?;
            let parameter = entry
                .params
                .iter()
                .find(|parameter| parameter.binding_id == local)
                .ok_or_else(|| invalid("Cell observation is not a direct source parameter"))?;
            if parameter.reference {
                return Err(invalid("reference prefix decoding lacks this profile"));
            }
            let layout = typed
                .types
                .iter()
                .find(|layout| layout.name == parameter.ty)
                .ok_or_else(|| invalid("missing independently checked Cell layout"))?;
            let width = unsigned_layout(layout)?;
            cells.push(Cell { entry: entry.id.clone(), parameter: parameter.name.clone(), ty: parameter.ty.clone(), width });
        }
    }
    let fields = if cells.is_empty() {
        None
    } else {
        Some(check_fixed_cell_scalar_fields(bundle[0], bundle[1], bundle[2], bundle[3], budgets)?)
    };
    let lowering: crate::VerifiedLoweringRecord = serde_json::from_slice(bundle[2]).map_err(|error| invalid(error.to_string()))?;
    let elf = crate::parse_elf(bundle[0], budgets.instructions).map_err(|error| invalid(error.to_string()))?;
    for variant in &policy.variants {
        let entry =
            typed.entries.iter().find(|entry| entry.id == variant.entry_id).ok_or_else(|| invalid("missing dispatch owner"))?;
        prologue::check(entry, typed, &lowering, &elf, fields.as_ref())?;
    }
    let record = Record {
        schema: "cellscript-fixed-external-codec-v1",
        profile: "policy-unit-scalars-flat-unsigned-cell-v1",
        module_contract: parameters.module_projection().identity().into(),
        parameter_decoder: parameters.identity().into(),
        cell_fields: fields.as_ref().map(|fields| fields.identity().into()),
        callables,
        cells,
        public_layouts,
    };
    let identity = crate::canonical_hash("cellscript-fixed-external-codec-id-v1", &record)?;
    Ok(CheckedFixedExternalCodec { parameters, fields, record, identity })
}
