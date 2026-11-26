//! Human storage facts from resolved IR. These are layout and source bounds,
//! not execution-path, allocation-fit, authorization or whole-group proofs.
use crate::ast::{BoundedCollectionKind, ParamSource, ValueCollectionKind};
use crate::error::{CompileError, Result, Span};
use crate::ir::{self, IrBody, IrItem, IrParam, IrType};
use crate::metadata_layout::metadata_type_layouts;
use crate::nested_values::{Element, NestedType};
use crate::runtime_availability::{
    metadata_ir_type_fixed_width, metadata_molecule_vector_element_fixed_width, metadata_prelude_availability,
    metadata_pure_const_returns, MetadataPreludeAvailability,
};
use crate::runtime_metadata::schema_pointer_var_ids;
use crate::{MetadataTypeLayouts, STACK_COLLECTION_BACKING_BYTES};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "model", rename_all = "kebab-case")]
pub(crate) enum Storage {
    StackVector {
        backing_bytes: usize,
        element_width_bytes: Option<usize>,
        capacity_elements: Option<usize>,
        with_capacity: bool,
    },
    VectorParameter {
        element_width_bytes: Option<usize>,
        source: String,
    },
    EncodedVectorView {
        element_width_bytes: Option<usize>,
    },
    EncodedRecord {
        fields: Vec<SchemaField>,
    },
    UnsupportedVector {
        element_width_bytes: Option<usize>,
    },
    ArenaOwner {
        owner: String,
        read_only: bool,
        bytes: u32,
        allocations: u32,
        members: Vec<ArenaMember>,
    },
    BoundedCellSet {
        maximum_cells: usize,
        element_width_bytes: Option<usize>,
        source: String,
        runtime_contracts: Vec<String>,
    },
    BoundedOutputPlan {
        maximum_items: usize,
        element_width_bytes: Option<usize>,
        maximum_plan_bytes: Option<usize>,
        source: String,
        runtime_contracts: Vec<String>,
        capacity_floors_shannons: Vec<u64>,
    },
    ReadOnlyCodec {
        family: &'static str,
        element: String,
        maximum_items: Option<u32>,
        maximum_payload_bytes: u32,
        maximum_encoded_bytes: Option<u32>,
        fixed_identity_bytes: Option<u32>,
        payload_field: Option<String>,
        element_view: Option<Box<Storage>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ArenaMember {
    pub name: String,
    pub kind: &'static str,
    pub element_width_bytes: Option<usize>,
    pub maximum_items: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Value {
    pub module: String,
    pub entry: String,
    pub binding: String,
    pub ty: String,
    pub line: usize,
    pub storage: Storage,
    #[serde(skip)]
    pub declaration: Span,
    #[serde(skip)]
    pub access_prefix: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SchemaField {
    pub module: String,
    pub schema: String,
    pub source_module: Option<String>,
    pub source_schema: Option<String>,
    pub field: String,
    pub ty: String,
    pub element_width_bytes: Option<usize>,
    pub storage: Storage,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Report {
    pub values: Vec<Value>,
    pub schema_fields: Vec<SchemaField>,
}

pub(crate) fn analyze(module: &ir::IrModule) -> Result<Report> {
    let layouts = metadata_type_layouts(module);
    let constants = metadata_pure_const_returns(module);
    let mut report = Report::default();
    let mut arenas = BTreeMap::new();
    let mut arena_contracts = BTreeMap::new();
    for (_, _, _, body) in module.items.iter().filter_map(callable) {
        for operation in body.blocks.iter().flat_map(|block| &block.instructions).filter_map(|instruction| {
            if let ir::IrInstruction::AllocatedValue(operation) = instruction {
                Some(operation)
            } else {
                None
            }
        }) {
            let name = operation.declaration.name.as_str();
            let contract = operation.contract();
            if arena_contracts.get(name).is_some_and(|old| old != &contract) {
                return Err(CompileError::without_span(format!(
                    "collection storage has conflicting resolved owner contracts for '{name}'"
                )));
            }
            arena_contracts.insert(name, contract);
            arenas.insert(name, &operation.declaration);
        }
    }
    for definition in module.items.iter().filter_map(|item| if let IrItem::TypeDef(def) = item { Some(def) } else { None }) {
        for field in &definition.fields {
            if let Some(field) = schema_field(module, &definition.name, &field.name, &field.ty, &layouts) {
                report.schema_fields.push(field);
            }
        }
    }
    for (kind, name, params, body) in module.items.iter().filter_map(callable) {
        let schema_params = schema_pointer_var_ids(body, params);
        let availability = metadata_prelude_availability(body, &schema_params, &layouts, params, &constants);
        let mut variables = BTreeMap::<usize, ir::IrVar>::new();
        let mut constructors = BTreeMap::new();
        for param in params {
            variables.insert(param.binding.id, param.binding.clone());
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                ir::visit_instruction_variables(instruction, |var| -> Result<()> {
                    if !matches!(var.ty, IrType::Vector(None)) || !variables.contains_key(&var.id) {
                        variables.insert(var.id, var.clone());
                    }
                    Ok(())
                })?;
                if let ir::IrInstruction::CollectionNew { dest, capacity, .. } = instruction {
                    constructors.insert(dest.id, capacity.is_some());
                }
            }
        }
        for binding in &body.source_bindings {
            let value = variables.get(&binding.value.id).unwrap_or(&binding.value);
            let context = Context {
                ir: module,
                body,
                params,
                layouts: &layouts,
                availability: &availability,
                arenas: &arenas,
                constructors: &constructors,
            };
            if let Some(storage) =
                context.storage(value, &binding.name, matches!(binding.value.ty, IrType::Ref(_) | IrType::MutRef(_)))
            {
                let row = Value {
                    module: module.name.clone(),
                    entry: format!("{kind}:{name}"),
                    binding: binding.name.clone(),
                    ty: value.ty.source_text(),
                    line: binding.declaration.line,
                    storage,
                    declaration: binding.declaration,
                    access_prefix: view_prefix(&value.ty),
                };
                // A statically unrolled loop can have multiple IR locals for one
                // source binding. Keep distinct layouts, coalesce identical facts.
                if !report.values.iter().any(|old| {
                    old.entry == row.entry
                        && old.binding == row.binding
                        && old.declaration == row.declaration
                        && old.storage == row.storage
                }) {
                    report.values.push(row);
                }
            }
        }
    }
    report.values.sort_by(|a, b| {
        (&a.module, &a.entry, a.declaration.start, &a.binding).cmp(&(&b.module, &b.entry, b.declaration.start, &b.binding))
    });
    report.schema_fields.sort_by(|a, b| (&a.module, &a.schema, &a.field).cmp(&(&b.module, &b.schema, &b.field)));
    Ok(report)
}

fn callable(item: &IrItem) -> Option<(&'static str, &str, &[IrParam], &IrBody)> {
    Some(match item {
        IrItem::Action(value) => ("action", &value.name, &value.params, &value.body),
        IrItem::Function(value) => ("helper", &value.name, &value.params, &value.body),
        IrItem::Lock(value) => ("lock", &value.name, &value.params, &value.body),
        _ => return None,
    })
}

struct Context<'a> {
    ir: &'a ir::IrModule,
    body: &'a IrBody,
    params: &'a [IrParam],
    layouts: &'a MetadataTypeLayouts,
    availability: &'a MetadataPreludeAvailability,
    arenas: &'a BTreeMap<&'a str, &'a crate::ast::CollectionsDef>,
    constructors: &'a BTreeMap<usize, bool>,
}

impl Context<'_> {
    fn storage(&self, value: &ir::IrVar, name: &str, borrowed: bool) -> Option<Storage> {
        let mut ty = &value.ty;
        while let IrType::Ref(inner) | IrType::MutRef(inner) = ty {
            ty = inner;
        }
        if ty.vector_element().is_some() || matches!(ty, IrType::Vector(None)) {
            let width = metadata_molecule_vector_element_fixed_width(ty, self.layouts);
            return Some(if self.availability.stack_collection_vars.contains(&value.id) {
                let root = self.availability.constructed_byte_vector_roots.get(&value.id).copied().unwrap_or(value.id);
                Storage::StackVector {
                    backing_bytes: STACK_COLLECTION_BACKING_BYTES,
                    element_width_bytes: width,
                    capacity_elements: width.and_then(|width| STACK_COLLECTION_BACKING_BYTES.checked_div(width)),
                    with_capacity: self.constructors.get(&root).copied().unwrap_or(false),
                }
            } else if let Some(param) = self.params.iter().find(|param| param.binding.id == value.id) {
                Storage::VectorParameter { element_width_bytes: width, source: source(param.source).into() }
            } else if self.availability.dynamic_collection_vars.contains(&value.id) {
                Storage::EncodedVectorView { element_width_bytes: width }
            } else {
                Storage::UnsupportedVector { element_width_bytes: width }
            });
        }
        if let IrType::Named(owner) = ty
            && let Some(declaration) = self.arenas.get(owner.as_str())
        {
            let width = |ty| metadata_ir_type_fixed_width(&ir::ast_type_to_ir(ty), self.layouts);
            let members = declaration
                .members
                .iter()
                .map(|member| {
                    let (kind, element_width_bytes) = match &member.kind {
                        ValueCollectionKind::Vec(ty) => ("Vec", width(ty)),
                        ValueCollectionKind::OrderedSet(ty) => ("OrderedSet", width(ty)),
                        ValueCollectionKind::OrderedMap { key, value } => {
                            ("OrderedMap", width(key).zip(width(value)).and_then(|(a, b)| a.checked_add(b)))
                        }
                    };
                    ArenaMember { name: member.name.clone(), kind, element_width_bytes, maximum_items: member.items }
                })
                .collect();
            return Some(Storage::ArenaOwner {
                owner: owner.clone(),
                read_only: borrowed || self.params.iter().any(|param| param.binding.id == value.id && param.is_ref),
                bytes: declaration.bytes,
                allocations: declaration.allocations,
                members,
            });
        }
        let schema = match ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Context(view) => {
                view.schema().and_then(|schema| if let IrType::Named(name) = schema.as_ref() { Some(name.as_str()) } else { None })
            }
            _ => None,
        };
        if let Some(schema) = schema
            && let Some(fields) = self.layouts.get(schema)
        {
            let mut fields = fields
                .iter()
                .filter_map(|(name, field)| Some((field.offset, schema_field(self.ir, schema, name, &field.ty, self.layouts)?)))
                .collect::<Vec<_>>();
            fields.sort_by(|a, b| (a.0, &a.1.field).cmp(&(b.0, &b.1.field)));
            if !fields.is_empty() {
                return Some(Storage::EncodedRecord { fields: fields.into_iter().map(|(_, field)| field).collect() });
            }
        }
        if let IrType::Bounded(bounded) = ty {
            let operations = self
                .body
                .bounded_collection_ops
                .iter()
                .filter(|operation| operation.collection_binding == name && operation.max_elements == bounded.max_elements)
                .collect::<Vec<_>>();
            let contracts = operations
                .iter()
                .filter_map(|operation| operation.runtime_contract.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let width = metadata_ir_type_fixed_width(&bounded.element_type, self.layouts);
            let source = self
                .params
                .iter()
                .find(|param| param.binding.id == value.id)
                .map(|param| param.source)
                .or_else(|| operations.first().map(|operation| operation.source))
                .unwrap_or(ParamSource::Default);
            return Some(match bounded.kind {
                BoundedCollectionKind::CellSet => Storage::BoundedCellSet {
                    maximum_cells: bounded.max_elements,
                    element_width_bytes: width,
                    source: self::source(source).into(),
                    runtime_contracts: contracts,
                },
                BoundedCollectionKind::List => Storage::BoundedOutputPlan {
                    maximum_items: bounded.max_elements,
                    element_width_bytes: width,
                    maximum_plan_bytes: width
                        .and_then(|width| width.checked_mul(bounded.max_elements))
                        .and_then(|size| size.checked_add(12)),
                    source: self::source(source).into(),
                    runtime_contracts: contracts,
                    capacity_floors_shannons: operations
                        .iter()
                        .filter_map(|operation| operation.output_capacity_floor_shannons)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                },
            });
        }
        codec_storage(ty)
    }
}

fn encoded_field_storage(ty: &IrType, layouts: &MetadataTypeLayouts) -> Option<Storage> {
    if ty.vector_element().is_some() {
        Some(Storage::EncodedVectorView { element_width_bytes: metadata_molecule_vector_element_fixed_width(ty, layouts) })
    } else {
        codec_storage(ty)
    }
}

fn schema_field(module: &ir::IrModule, schema: &str, name: &str, ty: &IrType, layouts: &MetadataTypeLayouts) -> Option<SchemaField> {
    let origin = module.source_type_origins.get(schema).and_then(Option::as_ref);
    Some(SchemaField {
        module: module.name.clone(),
        schema: schema.into(),
        source_module: origin.map(|origin| origin.module.clone()),
        source_schema: origin.map(|origin| origin.name.clone()),
        field: name.into(),
        ty: ty.source_text(),
        element_width_bytes: metadata_molecule_vector_element_fixed_width(ty, layouts),
        storage: encoded_field_storage(ty, layouts)?,
    })
}

fn codec_storage(ty: &IrType) -> Option<Storage> {
    if let IrType::Nested(nested) = ty {
        let element = |element: &Element| match element {
            Element::Bytes => ("Vec<u8>".into(), None),
            Element::Record(record) => {
                (record.spelling().into(), Some(record.layout().fixed_fields.iter().map(|(_, field)| u32::from(field.width())).sum()))
            }
        };
        let (family, element, maximum_items, maximum_payload_bytes, maximum_encoded_bytes, fixed_identity_bytes) =
            match nested.description() {
                NestedType::Values(values) => {
                    let (name, prefix) = element(&values.element);
                    ("nested-values", name, Some(values.max_items), values.max_payload_bytes, Some(values.max_total_bytes), prefix)
                }
                NestedType::BytesView { max_payload_bytes } => ("payload-bytes", "u8".into(), None, *max_payload_bytes, None, None),
                NestedType::RecordView { element: value, max_payload_bytes } => {
                    let (name, prefix) = element(value);
                    ("record-view", name, None, *max_payload_bytes, None, prefix)
                }
            };
        return Some(Storage::ReadOnlyCodec {
            family,
            element,
            maximum_items,
            maximum_payload_bytes,
            maximum_encoded_bytes,
            fixed_identity_bytes,
            payload_field: match nested.description() {
                NestedType::RecordView { element: Element::Record(record), .. } => Some(record.layout().payload_field.clone()),
                _ => None,
            },
            element_view: nested.index_type().and_then(|ty| codec_storage(&ty)).map(Box::new),
        });
    }
    None
}

fn source(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Input => "input",
        ParamSource::Output => "output",
        ParamSource::Witness => "witness",
        ParamSource::Protected => "protected",
        ParamSource::LockArgs => "lock-args",
        ParamSource::Default => "helper-or-local",
    }
}

fn view_prefix(ty: &IrType) -> Vec<&'static str> {
    match ty {
        IrType::Ref(inner) | IrType::MutRef(inner) => view_prefix(inner),
        IrType::Context(crate::ast::ContextView::Input(_) | crate::ast::ContextView::Output(_)) => vec!["data"],
        _ => Vec::new(),
    }
}

impl Storage {
    pub(crate) fn member(&self, name: &str) -> Option<Storage> {
        match self {
            Self::ArenaOwner { owner, read_only, bytes, allocations, members } => Some(Self::ArenaOwner {
                owner: owner.clone(),
                read_only: *read_only,
                bytes: *bytes,
                allocations: *allocations,
                members: vec![members.iter().find(|member| member.name == name)?.clone()],
            }),
            Self::EncodedRecord { fields } => fields.iter().find(|field| field.field == name).map(|field| field.storage.clone()),
            Self::ReadOnlyCodec { payload_field: Some(field), maximum_payload_bytes, .. } if field == name => {
                Some(Self::ReadOnlyCodec {
                    family: "payload-bytes",
                    element: "u8".into(),
                    maximum_items: None,
                    maximum_payload_bytes: *maximum_payload_bytes,
                    maximum_encoded_bytes: None,
                    fixed_identity_bytes: None,
                    payload_field: None,
                    element_view: None,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn indexed(&self) -> Option<Storage> {
        if let Self::ReadOnlyCodec { element_view: Some(view), .. } = self {
            Some((**view).clone())
        } else {
            None
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        let width = |value: Option<usize>| value.map_or_else(|| "unavailable".into(), |value| format!("{value} bytes"));
        match self {
            Self::StackVector { backing_bytes, element_width_bytes, capacity_elements, with_capacity } => {
                let mut lines = vec![format!("Storage: invocation-local stack, fixed backing {backing_bytes} bytes."),
                    format!("Element width: {}; capacity(): {} elements; length is a runtime value.", width(*element_width_bytes), capacity_elements.map_or_else(|| "unavailable".into(), |value| value.to_string())),
                    "Budget: fixed stack storage; no arena allocation-event budget.".into()];
                if *with_capacity { lines.push("with_capacity uses this fixed backing; its argument does not change capacity().".into()); }
                lines
            }
            Self::VectorParameter { element_width_bytes, source } => vec![
                format!("Storage: vector parameter; source {source}; backing supplied by the caller."),
                format!("Element width: {}; no local allocation capacity is established for this parameter.", width(*element_width_bytes)),
            ],
            Self::EncodedVectorView { element_width_bytes } => vec![
                "Storage: encoded vector view; no owned local allocation.".into(),
                format!("Element width: {}; length comes from encoded data; local capacity() is unavailable.", width(*element_width_bytes)),
            ],
            Self::EncodedRecord {fields} => {
                let mut lines = vec!["Storage: encoded record collection fields; no owned local collection capacity.".into()];
                for field in fields {
                    lines.push(format!("Field {}:",field.field));
                    lines.extend(field.storage.lines().into_iter().map(|line|format!("  {line}")));
                }
                lines
            }
            Self::UnsupportedVector { element_width_bytes } => vec![
                "No stack or encoded-vector backing contract was established for this value.".into(),
                format!("Element width: {}; capacity unavailable; executable operations retain their admission checks.", width(*element_width_bytes)),
            ],
            Self::ArenaOwner { owner, read_only, bytes, allocations, members } => {
                let mut lines = vec![format!("Storage: invocation-local shared arena {owner}; {bytes} byte budget, {allocations} allocation events."),
                    "Member capacity is a runtime value. items is a maximum, not an initial reservation.".into()];
                lines.push(if *read_only {"Access: read-only helper/borrow view; no local write authority.".into()} else {"Access: invocation-local owner; member writes retain ownership checks.".into()});
                lines.extend(members.iter().map(|member| format!("{}: {}; element width {}; maximum {} items.", member.name, member.kind, width(member.element_width_bytes), member.maximum_items)));
                lines.push("Retired buffers and allocation events remain charged; use the declaration budget scenarios to compare growth strategies.".into());
                lines
            }
            Self::BoundedCellSet { maximum_cells, element_width_bytes, source, runtime_contracts } => vec![
                format!("Storage: bounded Cell-selection handle; source {source}; maximum {maximum_cells} Cells, actual count at runtime."),
                format!("Element data width: {}; no local allocation capacity.", width(*element_width_bytes)),
                contracts_line(runtime_contracts),
                "The scan bound does not establish a per-Cell successor, retirement policy or fixed group count.".into(),
            ],
            Self::BoundedOutputPlan { maximum_items, element_width_bytes, maximum_plan_bytes, source, runtime_contracts, capacity_floors_shannons } => vec![
                format!("Storage: bounded witness-list/plan view; source {source}; maximum {maximum_items} items, actual count at runtime."),
                format!("Plan element width: {}; maximum plan encoding: {}; no local allocation capacity.", width(*element_width_bytes), width(*maximum_plan_bytes)),
                contracts_line(runtime_contracts),
                format!("Output capacity floors: {capacity_floors_shannons:?} shannons. Output correspondence requires the selected plan contract; entry-witness ABI budgets remain separate."),
            ],
            Self::ReadOnlyCodec { family, element, maximum_items, maximum_payload_bytes, maximum_encoded_bytes, fixed_identity_bytes, .. } => {
                let mut lines = vec![format!("Storage: immutable {family} codec view; element {element}; no owned allocation capacity."),
                    if *family=="payload-bytes" {format!("Byte element width: 1 byte; payload length bound: {maximum_payload_bytes} bytes.")} else {format!("Payload bound: {maximum_payload_bytes} bytes; encoded element size varies within the codec bounds.")}];
                if let Some(items)=maximum_items { lines.push(format!("Maximum list items: {items}.")); }
                if let Some(bytes)=maximum_encoded_bytes { lines.push(format!("Maximum whole-list encoding: {bytes} bytes.")); }
                if let Some(bytes)=fixed_identity_bytes { lines.push(format!("Fixed identity fields: {bytes} bytes; payload width remains bounded and dynamic.")); }
                lines.push("Codec bounds do not grant Cell selection, witness placement or allocation authority.".into());
                lines
            }
        }
    }
}

fn contracts_line(contracts: &[String]) -> String {
    if contracts.is_empty() {
        "No executable bounded-operation contract is selected by this value's recorded operations.".into()
    } else {
        format!("Recorded bounded operation contracts: {}. Execution-path evidence remains separate.", contracts.join(", "))
    }
}

impl Value {
    pub(crate) fn hover(&self) -> String {
        self.storage.hover()
    }
}

impl Storage {
    pub(crate) fn hover(&self) -> String {
        let lines = self.lines();
        let clipped = lines.len() > 64 || lines.iter().any(|line| line.chars().count() > 512);
        let text = lines.iter().take(64).map(|line| line.chars().take(512).collect::<String>()).collect::<Vec<_>>().join("\n");
        format!(
            "\n\n**Resolved collection storage**\n\n```text\n{text}\n```{}",
            if clipped { "\n\nFull storage details are available in `cellc explain collections`." } else { "" }
        )
    }
}

impl Report {
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for value in &self.values {
            lines.push(format!("{} {} binding {}: {} (line {})", value.module, value.entry, value.binding, value.ty, value.line));
            lines.extend(value.storage.lines().into_iter().map(|line| format!("  {line}")));
        }
        for field in &self.schema_fields {
            lines.push(format!("{} schema {}.{}: {}; encoded schema field; element width {}; no local allocation capacity; runtime codec admission remains separate.",field.module,field.schema,field.field,field.ty,field.element_width_bytes.map_or_else(|| "dynamic/unavailable".into(),|n|format!("{n} bytes"))));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_report(source: &str) -> Report {
        let ast = crate::frontend::parse(source, crate::CURRENT_EDITION).unwrap();
        let ast = crate::generics::monomorphize_for_edition(&ast, crate::CURRENT_EDITION).unwrap();
        crate::types::check(&ast).unwrap();
        crate::flow::check(&ast).unwrap();
        analyze(&crate::ir::generate(&ast).unwrap()).unwrap()
    }

    fn binding<'a>(report: &'a Report, name: &str) -> &'a Value {
        report.values.iter().find(|value| value.binding == name).unwrap_or_else(|| panic!("missing {name}: {report:?}"))
    }

    #[test]
    fn stack_capacity_and_aliases_follow_actual_width_and_backing() {
        let report=source_report("module storage; struct Pair {left:u64,right:u64,} action verify()->u64 {let mut small:Vec<u16> = Vec::with_capacity(4); let copied=small; let mut wide:Vec<Hash> = Vec::new(); let mut records:Vec<Pair> = Vec::new(); return small.capacity()+wide.capacity()+records.capacity();}");
        for (name, width, capacity) in [("small", 2, 128), ("copied", 2, 128), ("wide", 32, 8), ("records", 16, 16)] {
            let Storage::StackVector { backing_bytes, element_width_bytes, capacity_elements, .. } = &binding(&report, name).storage
            else {
                panic!("{name}: {report:?}")
            };
            assert_eq!((*backing_bytes, *element_width_bytes, *capacity_elements), (256, Some(width), Some(capacity)));
        }
        assert!(binding(&report, "small").hover().contains("its argument does not change capacity()"));
    }

    #[test]
    fn a_shared_vec_type_keeps_encoded_views_parameters_and_stack_values_distinct() {
        let report=source_report("module storage; resource Bag has store {values:Vec<u64>,} fn length(values:Vec<u64>)->u64 {return values.len();} action verify(read bag:Bag)->u64 {let values=bag.values; let alias=values; let mut local:Vec<u64> = Vec::with_capacity(4); return length(alias)+local.capacity();}");
        assert!(matches!(binding(&report, "local").storage, Storage::StackVector { capacity_elements: Some(32), .. }));
        assert!(
            matches!(binding(&report, "alias").storage, Storage::EncodedVectorView { element_width_bytes: Some(8) }),
            "{report:#?}"
        );
        let parameter = report.values.iter().find(|row| row.entry == "helper:length" && row.binding == "values").unwrap();
        assert!(matches!(parameter.storage, Storage::VectorParameter { element_width_bytes: Some(8), .. }));
        let bag = binding(&report, "bag").storage.member("values").unwrap();
        assert!(matches!(bag, Storage::EncodedVectorView { element_width_bytes: Some(8) }));
        assert!(!bag.lines().join(" ").contains("32 elements"));
        assert_eq!(report.schema_fields[0].field, "values");
    }

    #[test]
    fn arena_members_retain_resolved_record_widths_and_shared_budgets() {
        let report=source_report("module storage; struct Pair<T:fixed_value> {left:T,right:T,} collections Work {limits {bytes:1024,allocations:8}, entries:Vec<Pair<u64>> {items:16}} fn count(view:&Work)->u64 {return view.entries.len();} action verify()->u64 {let work=Work::new(); return count(&work);}");
        for name in ["work", "view"] {
            let Storage::ArenaOwner { bytes, allocations, members, read_only, .. } = &binding(&report, name).storage else {
                panic!("{report:?}")
            };
            assert_eq!((*bytes, *allocations), (1024, 8));
            assert_eq!(*read_only, name == "view");
            assert_eq!((members[0].element_width_bytes, members[0].maximum_items), (Some(16), 16));
        }
        assert!(binding(&report, "work").hover().contains("capacity is a runtime value"));
    }

    #[test]
    fn bounded_cell_scans_plans_and_codec_views_retain_separate_limits() {
        let plans = source_report(include_str!("../tests/fixtures/bounded_output_plan.cell"));
        let Storage::BoundedOutputPlan {
            maximum_items,
            element_width_bytes,
            maximum_plan_bytes,
            capacity_floors_shannons,
            runtime_contracts,
            ..
        } = &binding(&plans, "plans").storage
        else {
            panic!("{plans:?}")
        };
        assert_eq!((*maximum_items, *element_width_bytes, *maximum_plan_bytes), (3, Some(40), Some(132)));
        assert_eq!(capacity_floors_shannons, &[10_000_000_000]);
        assert_eq!(runtime_contracts, &[ir::BOUNDED_OUTPUT_PLAN_V1]);
        let cells=source_report("module storage; resource Token has store,consume {amount:u64,} action verify(input cells:BoundedCellSet<Token,2>)->u64 {consume_each cell in cells {require cell.amount>0;} return 0;}");
        assert!(
            matches!(&binding(&cells,"cells").storage,Storage::BoundedCellSet {maximum_cells:2,element_width_bytes:Some(8),runtime_contracts,..} if runtime_contracts==&[ir::BOUNDED_TYPE_GROUP_INPUTS_V1])
        );
        let views = source_report(include_str!("../tests/syntax_combo/seeds/named-nested-data-views.cell"));
        assert!(matches!(
            &binding(&views, "alias").storage,
            Storage::ReadOnlyCodec { maximum_items: Some(32), maximum_payload_bytes: 256, maximum_encoded_bytes: Some(4096), .. }
        ));
        let payload = binding(&views, "before").storage.member("entries").unwrap().indexed().unwrap().member("payload").unwrap();
        assert!(matches!(payload, Storage::ReadOnlyCodec { family: "payload-bytes", maximum_payload_bytes: 256, .. }));
        assert_eq!(binding(&views, "before").access_prefix, ["data"]);
    }
}
