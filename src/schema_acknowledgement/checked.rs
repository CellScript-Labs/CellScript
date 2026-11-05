//! Compiler-owned schema identity, retained before IR types become wire text.

#[cfg(not(feature = "wasm"))]
use crate::ast;
use crate::ast::TypeArgument;
use crate::error::{CompileError, Result};
use crate::ir::{IrField, IrType, IrTypeKind};
#[cfg(not(feature = "wasm"))]
use crate::ir::{IrItem, IrModule};
#[cfg(not(feature = "wasm"))]
use crate::resolve::ModuleResolver;
use crate::resolve::ResolvedTypeIdentity;
use cellscript_artifact_checker::canonical_hash;
use serde_json::{json, Value};
#[cfg(not(feature = "wasm"))]
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(crate) struct SchemaReviewCompilation {
    pub(crate) roles: BTreeMap<(String, String), String>,
    types: BTreeMap<String, CheckedType>,
    enums: BTreeMap<String, crate::ir::IrEnumLayout>,
    identities: BTreeMap<String, Option<ResolvedTypeIdentity>>,
    pub(crate) source_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckedSchemaField {
    pub(crate) source_type: String,
    pub(crate) root_field: String,
    pub(crate) identity: String,
}

#[derive(Debug, Clone)]
struct CheckedType {
    kind: IrTypeKind,
    fields: Vec<IrField>,
}

impl SchemaReviewCompilation {
    #[cfg(not(feature = "wasm"))]
    pub(crate) fn build(ir: &IrModule, ast: &ast::Module, resolver: Option<&ModuleResolver>) -> Result<Self> {
        let roles = ir
            .items
            .iter()
            .filter_map(|item| match item {
                IrItem::Action(action) => Some(action),
                _ => None,
            })
            .flat_map(|action| {
                action
                    .body
                    .cell_bindings
                    .iter()
                    .map(move |binding| ((action.name.clone(), binding.binding.clone()), binding.ty.clone()))
            })
            .collect();
        let types = ir
            .external_type_defs
            .iter()
            .chain(ir.items.iter().filter_map(|item| match item {
                IrItem::TypeDef(ty) => Some(ty),
                _ => None,
            }))
            .map(|ty| (ty.name.clone(), CheckedType { kind: ty.kind, fields: ty.fields.clone() }))
            .collect();
        let mut sources = BTreeMap::new();
        let mut pending = VecDeque::from([ast.name.clone()]);
        while let Some(name) = pending.pop_front() {
            if sources.contains_key(&name) {
                continue;
            }
            let module = if name == ast.name {
                ast
            } else {
                resolver
                    .and_then(|resolver| resolver.module(&name))
                    .ok_or_else(|| CompileError::without_span("schema source closure contains an unresolved module"))?
            };
            sources.insert(name.clone(), crate::fmt::format_default(module)?);
            if let Some(resolver) = resolver {
                pending.extend(resolver.imports_for_module(&name).iter().map(|import| import.module_path.join("::")));
            }
        }
        let source_identity = canonical_hash("cellscript-schema-reviewed-source-closure-v2", &sources)
            .map_err(|error| CompileError::without_span(format!("failed to bind schema source closure: {error}")))?;
        Ok(Self {
            roles,
            types,
            enums: ir.enum_layouts.iter().map(|(name, layout)| (name.clone(), layout.clone())).collect(),
            identities: ir.nominal_identities.clone(),
            source_identity,
        })
    }

    pub(crate) fn has_cell_schema(&self, name: &str) -> bool {
        self.types.get(name).is_some_and(|ty| matches!(ty.kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt))
    }

    pub(crate) fn schema_identity(&self, root: &str) -> Result<String> {
        if !self.has_cell_schema(root) {
            return Err(CompileError::without_span(format!("'{root}' has no checked Cell schema")));
        }
        self.nominal_graph_identity(root)
    }

    pub(crate) fn equivalent_schema(&self, other: &Self, name: &str) -> Result<Option<String>> {
        let identity = other
            .identities
            .get(name)
            .and_then(Option::as_ref)
            .ok_or_else(|| CompileError::without_span("relation schema has no resolved nominal identity"))?;
        for (candidate, ty) in &self.types {
            if !matches!(ty.kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt) {
                continue;
            }
            let known = self
                .identities
                .get(candidate)
                .and_then(Option::as_ref)
                .ok_or_else(|| CompileError::without_span("old/new Cell schema catalog contains an ambiguous nominal identity"))?;
            if known == identity {
                return Ok(Some(candidate.clone()));
            }
        }
        Ok(None)
    }

    fn nominal_graph_identity(&self, root: &str) -> Result<String> {
        let mut pending = BTreeSet::from([root.to_string()]);
        let mut definitions = BTreeMap::new();
        while let Some(name) = pending.pop_first() {
            if definitions.contains_key(&name) {
                continue;
            }
            if definitions.len() >= 4096 {
                return Err(CompileError::without_span("schema identity closure exceeds 4096 definitions"));
            }
            let identity = self.identity(&name)?;
            if let Some(ty) = self.types.get(&name) {
                let fields = ty
                    .fields
                    .iter()
                    .map(|field| {
                        Ok(json!({
                            "name": field.name, "type": self.type_identity(&field.ty, &mut pending)?,
                            "offset": field.offset, "width_bytes": field.fixed_size,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?;
                definitions.insert(name, json!({"identity": identity, "kind": match ty.kind {
                    IrTypeKind::Resource => "resource", IrTypeKind::Shared => "shared", IrTypeKind::Receipt => "receipt", IrTypeKind::Struct => "struct",
                }, "fields": fields}));
            } else if let Some(layout) = self.enums.get(&name) {
                let variants = layout.variants.iter().map(|variant| {
                    let fields = variant.fields.iter().map(|field| Ok(json!({
                        "index": field.index, "type": self.type_identity(&field.ty, &mut pending)?, "offset": field.offset, "width": field.width,
                    }))).collect::<Result<Vec<_>>>()?;
                    Ok(json!({"name": variant.name, "tag": variant.tag, "width": variant.payload_width, "fields": fields}))
                }).collect::<Result<Vec<_>>>()?;
                definitions.insert(name, json!({"identity": identity, "kind": "enum", "tag_width": layout.tag_width, "encoded_size": layout.encoded_size, "variants": variants}));
            } else {
                return Err(CompileError::without_span("schema identity references an absent checked definition"));
            }
        }
        // Definition labels are traversal keys only. Hash semantic identities
        // and layouts in a canonical order, independent of import aliases.
        let mut definitions = definitions.into_values().collect::<Vec<_>>();
        definitions.sort_by_key(|definition| serde_json::to_string(&definition["identity"]).unwrap_or_default());
        definitions.dedup();
        canonical_hash("cellscript-checked-cell-schema-v2", &json!({"root": self.identity(root)?, "definitions": definitions}))
            .map_err(|error| CompileError::without_span(format!("failed to hash checked Cell schema: {error}")))
    }

    pub(crate) fn field_types(&self, root: &str) -> Result<BTreeMap<String, CheckedSchemaField>> {
        let mut fields = BTreeMap::new();
        self.collect_fields(root, "", None, 0, &mut BTreeSet::new(), &mut fields)?;
        Ok(fields)
    }

    fn collect_fields(
        &self,
        name: &str,
        prefix: &str,
        root_field: Option<&str>,
        depth: usize,
        ancestors: &mut BTreeSet<String>,
        fields: &mut BTreeMap<String, CheckedSchemaField>,
    ) -> Result<()> {
        if depth > 32 || fields.len() > 4096 {
            return Err(CompileError::without_span("schema field expansion exceeds its bounded review depth/count"));
        }
        let Some(ty) = self.types.get(name) else { return Ok(()) };
        if !ancestors.insert(name.to_string()) {
            return Ok(());
        }
        for field in &ty.fields {
            let path = if prefix.is_empty() { field.name.clone() } else { format!("{prefix}.{}", field.name) };
            let root_field = root_field.unwrap_or(&field.name);
            let mut references = BTreeSet::new();
            let identity = self.type_identity(&field.ty, &mut references)?;
            let layouts = references
                .iter()
                .map(|name| Ok(json!({"identity": self.identity(name)?, "schema": self.nominal_graph_identity(name)?})))
                .collect::<Result<Vec<_>>>()?;
            let identity = canonical_hash("cellscript-checked-schema-field-v2", &json!({"type": identity, "layouts": layouts}))
                .map_err(|error| CompileError::without_span(format!("failed to hash checked schema field: {error}")))?;
            let contract = CheckedSchemaField {
                source_type: crate::typed_semantics::render_type(&field.ty),
                root_field: root_field.to_string(),
                identity,
            };
            if fields.insert(path.clone(), contract.clone()).is_some_and(|old| old != contract) {
                return Err(CompileError::without_span("schema field expansion has ambiguous structured paths"));
            }
            let mut targets = Vec::new();
            self.field_targets(&field.ty, "", &mut targets);
            for (suffix, reference) in targets {
                self.collect_fields(&reference, &format!("{path}{suffix}"), Some(root_field), depth + 1, ancestors, fields)?;
            }
        }
        ancestors.remove(name);
        Ok(())
    }

    fn field_targets(&self, ty: &IrType, suffix: &str, targets: &mut Vec<(String, String)>) {
        match ty {
            IrType::Named(name) if self.types.contains_key(name) => targets.push((suffix.to_string(), name.clone())),
            IrType::Array(inner, _) | IrType::Vector(Some(inner)) => self.field_targets(inner, &format!("{suffix}[]"), targets),
            IrType::Tuple(items) => {
                for (index, inner) in items.iter().enumerate() {
                    self.field_targets(inner, &format!("{suffix}.{index}"), targets);
                }
            }
            IrType::Applied(application) => {
                for (index, argument) in application.arguments.iter().enumerate() {
                    if let Some(inner) = argument.as_type() {
                        self.field_targets(inner, &format!("{suffix}.<argument:{index}>"), targets);
                    }
                }
            }
            IrType::Bounded(bound) => self.field_targets(&bound.element_type, &format!("{suffix}[]"), targets),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.field_targets(inner, suffix, targets),
            // The stored commitment is one digest, not an inline copy of T.
            // Its type identity still binds T's entire checked layout graph.
            IrType::Commitment(_) | IrType::Opening(_) => {}
            IrType::Context(view) => {
                if let Some(inner) = view.schema() {
                    self.field_targets(inner, &format!("{suffix}.<view>"), targets);
                }
            }
            IrType::Selection(selection) => {
                for (index, schema) in selection.schemas.iter().enumerate() {
                    targets.push((format!("{suffix}.<role:{index}>"), schema.clone()));
                }
            }
            IrType::Nested(view) => {
                use crate::nested_values::{Element, NestedType};
                match view.description() {
                    NestedType::Values(crate::nested_values::ValuesType { element: Element::Record(record), .. }) => {
                        targets.push((format!("{suffix}[]"), record.spelling().to_string()))
                    }
                    NestedType::RecordView { element: Element::Record(record), .. } => {
                        targets.push((suffix.to_string(), record.spelling().to_string()))
                    }
                    NestedType::Values(_) | NestedType::RecordView { .. } | NestedType::BytesView { .. } => {}
                }
            }
            IrType::U8
            | IrType::U16
            | IrType::U32
            | IrType::I32
            | IrType::U64
            | IrType::U128
            | IrType::Bool
            | IrType::Unit
            | IrType::Address
            | IrType::Hash
            | IrType::Named(_)
            | IrType::Vector(None)
            | IrType::OpenHandle(_)
            | IrType::WitnessBytes(_) => {}
        }
    }

    fn identity(&self, name: &str) -> Result<Value> {
        self.identities
            .get(name)
            .and_then(Option::as_ref)
            .map(resolved_identity)
            .ok_or_else(|| CompileError::without_span(format!("checked schema '{name}' has no unambiguous resolved nominal identity")))
    }

    fn type_identity(&self, ty: &IrType, references: &mut BTreeSet<String>) -> Result<Value> {
        let recurse = |ty: &IrType, references: &mut BTreeSet<String>| self.type_identity(ty, references);
        Ok(match ty {
            IrType::Named(name) if self.types.contains_key(name) || self.enums.contains_key(name) => {
                references.insert(name.clone());
                self.identity(name)?
            }
            IrType::Named(name) => json!({"kind": "compiler-domain", "name": name}),
            IrType::Array(inner, count) => json!({"kind": "array", "count": count, "inner": recurse(inner, references)?}),
            IrType::Tuple(items) => {
                json!({"kind": "tuple", "items": items.iter().map(|ty| recurse(ty, references)).collect::<Result<Vec<_>>>()?})
            }
            IrType::Applied(application) => {
                json!({"kind": "application", "constructor": application.constructor, "arguments": application.arguments.iter().map(|argument| match argument {
                TypeArgument::Type(ty) => recurse(ty, references), TypeArgument::UnsignedConstant(value) => Ok(json!({"unsigned": value.to_string()})),
            }).collect::<Result<Vec<_>>>()?})
            }
            IrType::Vector(inner) => {
                json!({"kind": "vector", "inner": inner.as_deref().map(|ty| recurse(ty, references)).transpose()?})
            }
            IrType::Bounded(bound) => {
                json!({"kind": "bounded", "family": bound.kind.constructor(), "maximum": bound.max_elements, "element": recurse(&bound.element_type, references)?})
            }
            IrType::Commitment(inner) => json!({"kind": "commitment", "inner": recurse(inner, references)?}),
            IrType::Opening(inner) => json!({"kind": "opening", "inner": recurse(inner, references)?}),
            IrType::Ref(inner) => json!({"kind": "reference", "inner": recurse(inner, references)?}),
            IrType::MutRef(inner) => json!({"kind": "mutable-reference", "inner": recurse(inner, references)?}),
            IrType::Context(view) => {
                json!({"kind": "context", "constructor": view.constructor(), "schema": view.schema().map(|ty| recurse(ty, references)).transpose()?})
            }
            IrType::Selection(selection) => {
                references.extend(selection.schemas.iter().cloned());
                json!({"kind": "selection", "catalog": selection.catalog, "names": selection.names,
                    "schemas": selection.schemas.iter().map(|schema| self.identity(schema)).collect::<Result<Vec<_>>>()?, "selected": selection.selected})
            }
            IrType::Nested(view) => {
                use crate::nested_values::{Element, NestedType};
                let element = |element: &Element, references: &mut BTreeSet<String>| -> Result<Value> {
                    match element {
                        Element::Bytes => Ok(json!({"kind": "bytes"})),
                        Element::Record(record) => {
                            references.insert(record.spelling().to_string());
                            self.identity(record.spelling())
                        }
                    }
                };
                match view.description() {
                    NestedType::Values(values) => json!({"kind": "nested-values", "element": element(&values.element, references)?,
                        "maximum_items": values.max_items, "maximum_payload_bytes": values.max_payload_bytes, "maximum_total_bytes": values.max_total_bytes}),
                    NestedType::BytesView { max_payload_bytes } => {
                        json!({"kind": "nested-bytes-view", "maximum_payload_bytes": max_payload_bytes})
                    }
                    NestedType::RecordView { element: value, max_payload_bytes } => {
                        json!({"kind": "nested-record-view", "element": element(value, references)?, "maximum_payload_bytes": max_payload_bytes})
                    }
                }
            }
            IrType::OpenHandle(handle) => {
                json!({"kind": "open-handle", "class": handle.class.constructor(), "interface": handle.interface})
            }
            IrType::WitnessBytes(view) => json!({"kind": "witness-bytes", "owner": view.owner.as_str(), "maximum": view.maximum}),
            IrType::U8
            | IrType::U16
            | IrType::U32
            | IrType::I32
            | IrType::U64
            | IrType::U128
            | IrType::Bool
            | IrType::Unit
            | IrType::Address
            | IrType::Hash => json!({"kind": "primitive", "type": crate::typed_semantics::render_type(ty)}),
        })
    }
}

fn resolved_identity(identity: &ResolvedTypeIdentity) -> Value {
    match identity {
        ResolvedTypeIdentity::Primitive(ty) => json!({"kind": "primitive", "type": crate::generics::render_type(ty)}),
        ResolvedTypeIdentity::PreludeOption(inner) => json!({"kind": "prelude-option", "argument": resolved_identity(inner)}),
        ResolvedTypeIdentity::OpenHandle(handle) => {
            json!({"kind": "open-handle", "class": handle.class.constructor(), "interface": handle.interface})
        }
        ResolvedTypeIdentity::Nominal { declaration, arguments } => {
            json!({"kind": "nominal", "module": declaration.module, "name": declaration.name, "arguments": arguments.iter().map(resolved_identity).collect::<Vec<_>>()})
        }
        ResolvedTypeIdentity::Application { constructor, arguments, packed_domain } => {
            json!({"kind": "application", "constructor": constructor, "packed_domain": packed_domain, "arguments": arguments.iter().map(|argument| match argument {
            TypeArgument::Type(ty) => resolved_identity(ty), TypeArgument::UnsignedConstant(value) => json!({"unsigned": value.to_string()}),
        }).collect::<Vec<_>>()})
        }
        ResolvedTypeIdentity::Array(inner, count) => json!({"kind": "array", "count": count, "inner": resolved_identity(inner)}),
        ResolvedTypeIdentity::Tuple(items) => {
            json!({"kind": "tuple", "items": items.iter().map(resolved_identity).collect::<Vec<_>>()})
        }
        ResolvedTypeIdentity::Reference { mutable, inner } => {
            json!({"kind": "reference", "mutable": mutable, "inner": resolved_identity(inner)})
        }
    }
}
