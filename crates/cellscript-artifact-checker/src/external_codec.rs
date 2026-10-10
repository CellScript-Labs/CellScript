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
    constants: Vec<ProvenConstant>,
}

#[derive(Debug, Serialize)]
struct ProvenConstant {
    declaration: String,
    ty: String,
    value: String,
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
fn unsigned_layout(layout: &crate::TypedSemanticType, typed: &crate::TypedSemanticRecord) -> Result<u32, CheckerError> {
    unsigned_layout_at(layout, typed, true, 0)
}

/// Entry-bound Cell layouts stay flat: their ELF-level field materialization
/// evidence (`fixed_cell_fields`) is certified per direct scalar field, so a
/// nested Cell field keeps failing closed under this profile.
fn unsigned_layout_flat(layout: &crate::TypedSemanticType, typed: &crate::TypedSemanticRecord) -> Result<u32, CheckerError> {
    unsigned_layout_at(layout, typed, false, 0)
}

/// Public layouts may compose nested ordinary struct layouts: every nested
/// member must itself resolve to exactly one retained concrete struct layout
/// and recursively satisfy the same unsigned obligations. Depth is bounded so
/// hostile nesting cannot amplify the traversal; total width stays ≤ 512.
fn unsigned_layout_at(
    layout: &crate::TypedSemanticType,
    typed: &crate::TypedSemanticRecord,
    nested_allowed: bool,
    depth: usize,
) -> Result<u32, CheckerError> {
    if !layout.variants.is_empty() || layout.fields.len() > 64 || layout.fields.is_empty() {
        return Err(invalid("tagged, empty or oversized public layout lacks this profile"));
    }
    if depth > 8 {
        return Err(invalid("nested public layout exceeds its depth bound"));
    }
    let mut width = 0u32;
    for field in &layout.fields {
        let bytes = match field.ty.as_str() {
            "u8" => 1,
            "u16" => 2,
            "u32" => 4,
            "u64" => 8,
            _ => {
                if !nested_allowed {
                    return Err(invalid("public layout requires flat unsigned bitpattern fields"));
                }
                let members = typed.types.iter().filter(|ty| ty.name == field.ty).collect::<Vec<_>>();
                if members.len() != 1 || members[0].kind != "struct" {
                    return Err(invalid("nested public member lacks its unique ordinary struct layout"));
                }
                unsigned_layout_at(members[0], typed, true, depth + 1)?
            }
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
        let width = unsigned_layout(layout, typed)?;
        public_layouts.push(PublicLayout {
            declaration: declaration.identity.clone(),
            lowered: Some(layout.name.clone()),
            width: Some(width),
            availability: "fixed-flat-unsigned-layout",
        });
    }
    // Public constants need independently re-evaluated value evidence; an
    // absent catalog, an incomplete set or an expression that folds to a
    // different value fails closed here.
    let mut constants = Vec::new();
    if !inspection.declared().constants.is_empty() {
        let declared = inspection
            .declared()
            .constants
            .iter()
            .map(|constant| (inspection.declared().module.clone(), constant.name.clone(), constant.r#type.clone()))
            .collect::<Vec<_>>();
        let proven = crate::constant_values::check_constant_values(typed, &declared)?;
        for constant in &inspection.declared().constants {
            let (ty, value) = proven
                .proven(&inspection.declared().module, &constant.name)
                .ok_or_else(|| invalid("declared constant lacks its proven value"))?;
            constants.push(ProvenConstant { declaration: constant.identity.clone(), ty: ty.clone(), value: value.clone() });
        }
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
            let width = unsigned_layout_flat(layout, typed)?;
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
        profile: "policy-unit-scalars-nested-unsigned-cell-v1",
        module_contract: parameters.module_projection().identity().into(),
        parameter_decoder: parameters.identity().into(),
        cell_fields: fields.as_ref().map(|fields| fields.identity().into()),
        callables,
        cells,
        public_layouts,
        constants,
    };
    let identity = crate::canonical_hash("cellscript-fixed-external-codec-id-v1", &record)?;
    Ok(CheckedFixedExternalCodec { parameters, fields, record, identity })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TypedSemanticField, TypedSemanticType};

    fn layout(name: &str, fields: &[(&str, &str, u32, u32)]) -> TypedSemanticType {
        TypedSemanticType {
            name: name.into(),
            kind: "struct".into(),
            encoded_size: Some(fields.iter().map(|(_, _, _, width)| width).sum()),
            fields: fields
                .iter()
                .map(|(name, ty, offset, width)| TypedSemanticField {
                    name: (*name).into(),
                    ty: (*ty).into(),
                    offset: *offset,
                    width_bytes: Some(*width),
                })
                .collect(),
            ..TypedSemanticType::default()
        }
    }

    fn record(types: Vec<TypedSemanticType>) -> crate::TypedSemanticRecord {
        crate::TypedSemanticRecord { types, ..crate::TypedSemanticRecord::default() }
    }

    /// Nested public layouts compose: a member naming another retained
    /// ordinary struct contributes its aggregate width at a contiguous
    /// offset, recursively, under the depth bound.
    #[test]
    fn nested_unsigned_layouts_compose_contiguously() {
        let typed = record(vec![
            layout("Leaf", &[("first", "u32", 0, 4), ("second", "u32", 4, 4)]),
            layout("Middle", &[("leaf", "Leaf", 0, 8), ("count", "u64", 8, 8)]),
            layout("Outer", &[("middle", "Middle", 0, 16), ("tag", "u16", 16, 2)]),
        ]);
        assert_eq!(unsigned_layout(&typed.types[2], &typed).unwrap(), 18);
        assert_eq!(unsigned_layout(&typed.types[1], &typed).unwrap(), 16);
        // The Cell-parameter path stays flat under the same record.
        assert!(unsigned_layout_flat(&typed.types[1], &typed).is_err());
    }

    #[test]
    fn nested_unsigned_layouts_reject_mutations_and_foreign_members() {
        let base = record(vec![
            layout("Leaf", &[("first", "u32", 0, 4), ("second", "u32", 4, 4)]),
            layout("Outer", &[("leaf", "Leaf", 0, 8), ("count", "u64", 8, 8)]),
        ]);
        assert_eq!(unsigned_layout(&base.types[1], &base).unwrap(), 16);
        // Width mutation inside the nested aggregate.
        let mut changed = base.clone();
        changed.types[1].fields[0].width_bytes = Some(4);
        assert!(unsigned_layout(&changed.types[1], &changed).is_err());
        // Offset drift across the nesting boundary.
        let mut changed = base.clone();
        changed.types[1].fields[1].offset = 12;
        assert!(unsigned_layout(&changed.types[1], &changed).is_err());
        // A nested member without its unique ordinary struct layout.
        let mut changed = base;
        changed.types.remove(0);
        assert!(unsigned_layout(&changed.types[0], &changed).is_err());
        // Depth bound: nine chained single-field layouts reject.
        let mut chain = Vec::new();
        for index in 0..10 {
            let spelled: Vec<(&str, &str, u32, u32)> =
                if index == 9 { vec![("value", "u64", 0, 8)] } else { vec![("inner", "placeholder", 0, 8)] };
            let spelled: Vec<(&str, String, u32, u32)> = if index == 9 {
                spelled.into_iter().map(|(name, ty, offset, width)| (name, ty.to_string(), offset, width)).collect()
            } else {
                vec![("inner", format!("N{}", index + 1), 0, 8)]
            };
            let fields: Vec<(&str, &str, u32, u32)> =
                spelled.iter().map(|(name, ty, offset, width)| (*name, ty.as_str(), *offset, *width)).collect();
            chain.push(layout(&format!("N{index}"), &fields));
        }
        let deep = record(chain);
        assert!(unsigned_layout(&deep.types[0], &deep).is_err());
        // Equal-width nominal substitution inside the nested member set is
        // structural for the codec check itself; declaration identities keep
        // owners distinct at the module-contract layer.
        let swapped = record(vec![
            layout("Leaf", &[("first", "u32", 0, 4), ("second", "u32", 4, 4)]),
            layout("Substitute", &[("first", "u32", 0, 4), ("second", "u32", 4, 4)]),
            layout("Outer", &[("leaf", "Substitute", 0, 8), ("count", "u64", 8, 8)]),
        ]);
        assert_eq!(unsigned_layout(&swapped.types[2], &swapped).unwrap(), 16);
    }
}
