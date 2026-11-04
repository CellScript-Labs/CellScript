//! Compiler-owned inventory of the typed IR surface and its executable status.
//!
//! The generated Markdown and JSON matrices are checked by `cellscript-tools`.
//! Keep entries conservative: `complete` means the operation has no
//! compiler-recognized fail-closed shape; `shape-gated` means production
//! compilation accepts only shapes for which the metadata classifier reports
//! no fail-closed feature.

use crate::error::{CompileError, Result};
use crate::ir::{IrBody, IrInstruction, IrItem, IrModule, IrTerminator, IrType};
use serde::Serialize;

pub const EXECUTABLE_SURFACE_SCHEMA: &str = "cellscript-executable-surface-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExecutableSurfaceEntry {
    pub id: &'static str,
    pub layer: &'static str,
    pub status: &'static str,
    pub production_policy: &'static str,
    pub conditions: &'static str,
    pub fail_closed_features: &'static [&'static str],
}

const ACCEPT: &str = "accepted";
const ACCEPT_WHEN_CLOSED: &str = "accepted only when the shape classifier reports no fail-closed feature";
const FRONTEND_ONLY: &str = "not materialized as a runtime value";

/// Older bundles retain these experimental admission blockers. Rebuilding with
/// the current checked contracts is required; a flag is not a machine proof.
pub(crate) fn experimental_artifact_admission_detail(feature: &str) -> Option<&'static str> {
    matches!(
        feature,
        "cell-data-view-artifact-proof"
            | "exact-role-selection-artifact-proof"
            | "nested-values-artifact-proof"
            | "allocated-values-artifact-proof"
    )
    .then_some("Legacy experimental artifact admission blocker; rebuild and verify with the current bounded runtime contracts")
}

macro_rules! entry {
    ($id:literal, $layer:literal, $status:literal, $policy:expr, $conditions:literal) => {
        ExecutableSurfaceEntry {
            id: $id,
            layer: $layer,
            status: $status,
            production_policy: $policy,
            conditions: $conditions,
            fail_closed_features: &[],
        }
    };
    ($id:literal, $layer:literal, $status:literal, $policy:expr, $conditions:literal, [$($feature:literal),+ $(,)?]) => {
        ExecutableSurfaceEntry {
            id: $id,
            layer: $layer,
            status: $status,
            production_policy: $policy,
            conditions: $conditions,
            fail_closed_features: &[$($feature),+],
        }
    };
}

pub static EXECUTABLE_SURFACE: &[ExecutableSurfaceEntry] = &[
    entry!("ir:allocated-value", "instruction", "bounded", ACCEPT,
        "Invocation-local fixed-value Vec and ordered Map/Set within declared byte, allocation-event and item limits. Independent constructor, allocation, bounds, lifecycle, readonly-helper and copied-result contracts are mandatory; owned dynamic values and Cell resources remain unsupported."),
    entry!("artifact:cell-data-view-proof", "artifact-policy", "bounded", ACCEPT,
        "Immutable fixed and bounded nested Cell data views require checked source coordinates, field/codec instructions, readonly helper transport and retained-buffer lifetime. Nominal schemas and lifecycle authority remain distinct; metadata alone is not machine or transaction evidence."),
    entry!("ir:verify-exact-cell-roles", "instruction", "bounded", ACCEPT,
        "Closed fixed/bounded-table role catalogs require complete bounded scans, explicit identity/authority/correspondence, checked typed projections and retained-table lifetime. Builders bind mandatory sites to the selected entry and reject unbound conditional paths; peer authorization remains with each Script."),
    entry!(
        "artifact:nested-values-proof", "artifact-policy", "bounded", ACCEPT,
        "Bounded byte-list and fixed-identity/byte-payload record-list Molecule views require canonical witness/Cell decoding, exact named limits, checked readonly whole/derived helper spans and actual successor comparisons. Recursive schemas, escaping views and owned dynamic mutation remain unsupported."
    ),
    entry!(
        "runtime:gather-hash-arguments", "runtime", "shape-gated", ACCEPT_WHEN_CLOSED,
        "Experimental gathered hashes require proven local byte/offset vectors and checked transaction span bounds.",
        ["gather-hash-materialization"]
    ),
    entry!(
        "runtime:spawn-hex4-arguments", "runtime", "shape-gated", ACCEPT_WHEN_CLOSED,
        "Experimental returning four-argument hex SPAWN/WAIT requires a proven local Vec<u8>; the external child verifier remains separately unresolved.",
        ["spawn-argv-materialization"]
    ),
    entry!(
        "runtime:exec-hex4-arguments", "runtime", "shape-gated", ACCEPT_WHEN_CLOSED,
        "Experimental four-argument hex EXEC requires a proven local Vec<u8>; external-verifier delegation remains separately unresolved.",
        ["exec-argv-materialization"]
    ),
    entry!(
        "runtime:trusted-external-delegation", "runtime", "bounded", ACCEPT_WHEN_CLOSED,
        "EXEC or SPAWN/WAIT is admitted only through a trusted_* intrinsic with a compile-time 32-byte DATA_HASH, an exact versioned Cell.toml declaration, an emitted pre-delegation identity check, and a trusted-external evidence record that never claims to prove the verifier's internals."
    ),
    entry!("type:u8", "type", "complete", ACCEPT, "One-byte unsigned scalar with checked source representability."),
    entry!("type:u16", "type", "complete", ACCEPT, "Two-byte little-endian unsigned scalar."),
    entry!("type:u32", "type", "complete", ACCEPT, "Four-byte little-endian unsigned scalar."),
    entry!("type:i32", "type", "complete", ACCEPT, "Four-byte signed scalar with signed comparison, division, and remainder."),
    entry!("type:u64", "type", "complete", ACCEPT, "Eight-byte little-endian unsigned scalar."),
    entry!(
        "type:u128",
        "type",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Sixteen-byte value with full-range decimal literals plus checked add, subtract, multiply, divide, remainder, comparison, casts, calls, parameters, and returns."
    ),
    entry!("type:bool", "type", "complete", ACCEPT, "Canonical boolean scalar."),
    entry!("type:unit", "type", "compile-time-only", FRONTEND_ONLY, "Control-flow and no-value result marker."),
    entry!("type:Address", "type", "complete", ACCEPT, "Fixed 32-byte address value."),
    entry!("type:Hash", "type", "complete", ACCEPT, "Fixed 32-byte hash value."),
    entry!("type:Array", "type", "bounded", ACCEPT_WHEN_CLOSED, "Compile-time length and recursively fixed element layout."),
    entry!(
        "type:GenericValue",
        "type",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Struct, enum, and function templates monomorphize before IR under explicit value abilities, deterministic budgets, and hidden-Cell rejection."
    ),
    entry!(
        "type:Option",
        "type",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Built-in Option<T> uses the ordinary fixed-width generic enum and tagged-union lowering path."
    ),
    entry!("type:Tuple", "type", "bounded", ACCEPT_WHEN_CLOSED, "Non-recursive aggregate with deterministic field offsets."),
    entry!("type:Named", "type", "shape-gated", ACCEPT_WHEN_CLOSED, "Concrete struct, enum, or Cell schema with a deterministic metadata layout."),
    entry!(
        "type:Ref",
        "type",
        "compile-time-only",
        FRONTEND_ONLY,
        "Read-only view with field-path, canonical-root reborrow, lifecycle-crossing, and non-escape checks before lowering."
    ),
    entry!("type:MutRef", "type", "reserved", "rejected by current semantic checks", "No executable general mutable-reference ABI."),
    entry!(
        "semantic:value-pattern",
        "semantic",
        "bounded",
        ACCEPT,
        "Recursive fixed enum, tuple, and struct patterns plus binding-free or-patterns with exhaustiveness and linear wildcard checks."
    ),
    entry!(
        "semantic:borrow-region",
        "semantic",
        "compile-time-only",
        FRONTEND_ONLY,
        "Field-path and reborrow regions retain one canonical Cell root and cannot materialize, escape, or cross a lifecycle operation."
    ),
    entry!(
        "semantic:loop-control",
        "semantic",
        "complete",
        ACCEPT,
        "Nearest and labeled break/continue targets lower to explicit CFG jumps after compile-time target validation."
    ),
    entry!("ir-item:type-def", "ir-item", "bounded", ACCEPT_WHEN_CLOSED, "Concrete fixed-layout type definition."),
    entry!("ir-item:invariant", "ir-item", "compile-time-only", FRONTEND_ONLY, "Proof-planning invariant record."),
    entry!("ir-item:action", "ir-item", "bounded", ACCEPT_WHEN_CLOSED, "Executable transaction action entry."),
    entry!("type:Nested", "type", "bounded", ACCEPT_WHEN_CLOSED, "Resolved immutable nested values and derived views with admitted record layouts; decoding and lifetime evidence remain mandatory."),
    entry!("ir-item:function", "ir-item", "bounded", ACCEPT_WHEN_CLOSED, "Resolved helper callable."),
    entry!("ir-item:lock", "ir-item", "bounded", ACCEPT_WHEN_CLOSED, "Executable lock predicate entry."),
    entry!("ir-terminator:return", "terminator", "complete", ACCEPT, "Typed return with an optional value."),
    entry!("ir-terminator:jump", "terminator", "complete", ACCEPT, "Validated direct CFG edge."),
    entry!("ir-terminator:branch", "terminator", "complete", ACCEPT, "Validated boolean conditional CFG edge."),
    entry!("ir:load-const", "instruction", "complete", ACCEPT, "Materializes supported scalar and fixed-byte constants."),
    entry!("ir:load-var", "instruction", "complete", ACCEPT, "Loads a checked local binding."),
    entry!("ir:store-var", "instruction", "complete", ACCEPT, "Stores a checked local binding without changing Cell authority."),
    entry!(
        "ir:binary",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Scalar arithmetic, bitwise, shifts, and complete u128 operators execute directly; dynamic shifts have width guards and fixed-byte equality requires addressable operands.",
        ["fixed-byte-comparison"]
    ),
    entry!("ir:unary", "instruction", "bounded", ACCEPT, "Boolean not, scalar negation, and compile-time reference conversions."),
    entry!(
        "ir:field-access",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Requires a known fixed schema/aggregate/tuple layout, a typed named-role projection, or a checked bounded Cell/nested view field.",
        ["field-access"]
    ),
    entry!(
        "ir:index",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Known fixed arrays and bounded stack collections with checked scalar/element layout, or immutable bounded nested list/payload indexing.",
        ["index-access"]
    ),
    entry!(
        "ir:length",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Static lengths or validated bounded collection length words.",
        ["dynamic-length"]
    ),
    entry!(
        "ir:type-hash",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Schema parameter or verified output Type Script hash.",
        ["type-hash"]
    ),
    entry!(
        "ir:collection-new",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack buffer or verifier-covered create-output vector.",
        ["collection-new"]
    ),
    entry!(
        "ir:collection-capacity",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection; hidden Cell ownership is rejected.",
        ["collection-capacity", "cell-backed-collection-capacity"]
    ),
    entry!(
        "ir:collection-push",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Fixed-width bounded value or verified output-vector construction.",
        ["collection-push", "cell-backed-collection-push"]
    ),
    entry!(
        "ir:collection-extend",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded fixed-width stack collection or verified output vector.",
        ["collection-extend", "cell-backed-collection-extend"]
    ),
    entry!(
        "ir:collection-clear",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection only.",
        ["collection-clear", "cell-backed-collection-clear"]
    ),
    entry!(
        "ir:collection-contains",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with comparable fixed-width elements.",
        ["collection-contains", "cell-backed-collection-contains"]
    ),
    entry!(
        "ir:collection-remove",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with fixed-width elements.",
        ["collection-remove", "cell-backed-collection-remove"]
    ),
    entry!(
        "ir:collection-insert",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with checked capacity and index.",
        ["collection-insert", "cell-backed-collection-insert"]
    ),
    entry!(
        "ir:collection-set",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with checked index.",
        ["collection-set", "cell-backed-collection-set"]
    ),
    entry!(
        "ir:collection-pop",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with fixed-width result.",
        ["collection-pop", "cell-backed-collection-pop"]
    ),
    entry!(
        "ir:collection-reverse",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with fixed-width elements.",
        ["collection-reverse", "cell-backed-collection-reverse"]
    ),
    entry!(
        "ir:collection-truncate",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with checked target length.",
        ["collection-truncate", "cell-backed-collection-truncate"]
    ),
    entry!(
        "ir:collection-swap",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Bounded stack collection with checked indexes.",
        ["collection-swap", "cell-backed-collection-swap"]
    ),
    entry!(
        "ir:bounded-cell-load",
        "instruction",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Exact current Type Script group input scan with runtime cardinality, identity, role, and fixed-width decode checks."
    ),
    entry!(
        "ir:bounded-plan-load",
        "instruction",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Canonical bounded-output-plan-v1 Molecule FixVec decoding with exact length and runtime cardinality checks."
    ),
    entry!(
        "ir:bounded-output-verify",
        "instruction",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Plan-relative GroupOutput data, lock, Type Script role, and declared capacity-floor verification."
    ),
    entry!(
        "ir:bounded-output-end",
        "instruction",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Exact plan-count to current Type Script GroupOutput-count correspondence."
    ),
    entry!("ir:call", "instruction", "bounded", ACCEPT_WHEN_CLOSED, "Resolved typed callable with a closed ABI and effect summary."),
    entry!(
        "artifact:ckb-sighash-all",
        "artifact-policy",
        "reserved",
        "rejected by production policy",
        "The generic env::sighash_all contract is deferred and unconditionally aborts with runtime error 66. The separately bounded env::sighash_all_zero_lock signing-message domain remains executable.",
        ["ckb-sighash-all-deferred"]
    ),
    entry!(
        "artifact:ckb-since-constraints",
        "artifact-policy",
        "reserved",
        "rejected by production policy",
        "Legacy require_maturity, require_time, require_epoch_after and require_epoch_relative unconditionally abort with runtime error 86. Typed InputView.since reads, checked decoding and temporal comparisons remain executable.",
        ["ckb-since-constraints-deferred"]
    ),
    entry!(
        "artifact:ckb-occupied-capacity-floor",
        "artifact-policy",
        "reserved",
        "rejected by production policy",
        "The legacy compile-visible occupied_capacity floor unconditionally aborts with runtime error 87. The separate ckb::cell_occupied_capacity reads actual Cell occupied capacity; witnesses do not contribute to occupied capacity.",
        ["ckb-occupied-capacity-deferred"]
    ),
    entry!(
        "source:bare-spawn",
        "source",
        "rejected",
        "rejected by semantic checking (E1021)",
        "Bare named spawn has no child binding or argument/descriptor transport. Use a manifest-bound CellDep spawn API with checked transport. Resolved user functions named spawn remain ordinary calls."
    ),
    entry!("ir:read-ref", "instruction", "bounded", ACCEPT, "Explicit Input or CellDep read-only Cell view."),
    entry!("ir:move", "instruction", "complete", ACCEPT, "Typed local move; ownership validity is checked before lowering."),
    entry!("ir:tuple", "instruction", "bounded", ACCEPT_WHEN_CLOSED, "Deterministic fixed aggregate construction."),
    entry!(
        "ir:enum-construct",
        "instruction",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "Concrete fixed-width payload enum construction, including pre-IR generic enum monomorphizations."
    ),
    entry!("ir:enum-tag", "instruction", "bounded", ACCEPT_WHEN_CLOSED, "Validated concrete payload enum tag."),
    entry!("ir:enum-payload", "instruction", "bounded", ACCEPT_WHEN_CLOSED, "Fixed-width concrete enum payload field."),
    entry!(
        "ir:consume",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Explicit Cell-backed input consumption.",
        ["consume-expression", "non-cell-consume"]
    ),
    entry!("ir:create", "instruction", "bounded", ACCEPT_WHEN_CLOSED, "Output construction covered by create-set verification."),
    entry!(
        "ir:transfer",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Verifier-covered output construction and lock replacement.",
        ["transfer-expression"]
    ),
    entry!(
        "ir:destroy",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Explicit destructible Cell-backed operand.",
        ["destroy-expression"]
    ),
    entry!(
        "ir:claim",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Verifier-covered receipt claim output.",
        ["claim-expression"]
    ),
    entry!(
        "ir:settle",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Verifier-covered settlement output.",
        ["settle-expression"]
    ),
    entry!(
        "ir:create-unique",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Verifier-covered output plus executable identity policy.",
        ["create-unique-expression"]
    ),
    entry!(
        "ir:replace-unique",
        "instruction",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "Verifier-covered replacement plus executable identity policy.",
        ["replace-unique-expression"]
    ),
    entry!("ir:cell-metadata-equality", "instruction", "complete", ACCEPT, "Lock-hash or capacity equality over validated Cell views."),
    entry!(
        "artifact:create-output-verification",
        "artifact-policy",
        "shape-gated",
        ACCEPT_WHEN_CLOSED,
        "All constructed output fields and output lock must be materializable by the verifier.",
        ["output-verification-incomplete", "output-lock-verification-incomplete"]
    ),
    entry!(
        "artifact:cell-backed-collection-return",
        "artifact-policy",
        "reserved",
        "rejected by production policy",
        "Returning a hidden Cell-backed collection has no linear ownership ABI.",
        ["cell-backed-collection-return"]
    ),
    entry!(
        "artifact:bounded-consume-each-runtime",
        "artifact-policy",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "The bounded-type-group-inputs-v1 fixed-width shape is executable; all other BoundedCellSet sources and element shapes remain fail-closed.",
        ["bounded-consume-each-runtime"]
    ),
    entry!(
        "artifact:bounded-create-each-runtime",
        "artifact-policy",
        "bounded",
        ACCEPT_WHEN_CLOSED,
        "The bounded-output-plan-v1 fixed-width shape is executable when the output has a complete create template, explicit lock, no custom identity, and a declared capacity floor; all other shapes remain fail-closed.",
        ["bounded-create-each-runtime"]
    ),
];

pub fn validate_ir_module(module: &IrModule) -> Result<()> {
    module.validate_entry_selection()?;
    crate::zk_contract::validate(module)?;
    validate_ir_types(module)
}

/// Type/surface admission also applies to callers that supply IR directly to
/// codegen. Source-only contracts are validated by the compile pipeline.
pub(crate) fn validate_ir_types(module: &IrModule) -> Result<()> {
    for external in &module.external_type_defs {
        require_registered("ir-item:type-def")?;
        for field in &external.fields {
            validate_ir_type(&field.ty)?;
        }
        if let Some(claim_output) = &external.claim_output {
            validate_ir_type(claim_output)?;
        }
    }
    for external in &module.external_callable_abis {
        validate_params_and_return(&external.params, None)?;
    }
    for item in &module.items {
        require_registered(ir_item_surface_id(item))?;
        match item {
            IrItem::TypeDef(definition) => {
                for field in &definition.fields {
                    validate_ir_type(&field.ty)?;
                }
                if let Some(claim_output) = &definition.claim_output {
                    validate_ir_type(claim_output)?;
                }
            }
            IrItem::Action(action) => {
                validate_params_and_return(&action.params, action.return_type.as_ref())?;
                validate_body(&action.body)?;
            }
            IrItem::Function(function) => {
                validate_params_and_return(&function.params, function.return_type.as_ref())?;
                validate_body(&function.body)?;
            }
            IrItem::Lock(lock) => {
                validate_params_and_return(&lock.params, Some(&IrType::Bool))?;
                validate_body(&lock.body)?;
            }
            IrItem::Invariant(_) => {}
        }
    }
    Ok(())
}

pub fn validate_fail_closed_features<'a>(features: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let registered = EXECUTABLE_SURFACE
        .iter()
        .flat_map(|entry| entry.fail_closed_features.iter().copied())
        .collect::<std::collections::BTreeSet<_>>();
    let unknown = features.into_iter().filter(|feature| !registered.contains(feature)).collect::<Vec<_>>();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(CompileError::without_span(format!(
            "executable surface reported unregistered fail-closed feature(s): {}",
            unknown.join(", ")
        ))
        .with_code(crate::error::DiagnosticCode::E2105))
    }
}

fn validate_params_and_return(params: &[crate::ir::IrParam], return_type: Option<&IrType>) -> Result<()> {
    for param in params {
        validate_ir_type(&param.ty)?;
    }
    if let Some(return_type) = return_type {
        validate_ir_type(return_type)?;
    }
    Ok(())
}

fn validate_body(body: &IrBody) -> Result<()> {
    for block in &body.blocks {
        for instruction in &block.instructions {
            require_registered(ir_instruction_surface_id(instruction))?;
            crate::ir::visit_instruction_types(instruction, validate_ir_type)?;
        }
        require_registered(ir_terminator_surface_id(&block.terminator))?;
        if let crate::ir::IrTerminator::Return(Some(crate::ir::IrOperand::Var(value)))
        | crate::ir::IrTerminator::Branch { cond: crate::ir::IrOperand::Var(value), .. } = &block.terminator
        {
            validate_ir_type(&value.ty)?;
        }
    }
    Ok(())
}

fn validate_ir_type(ty: &IrType) -> Result<()> {
    crate::ir::validate_fixed_layout(ty)?;
    require_registered(ir_type_surface_id(ty))?;
    match ty {
        IrType::Named(name) => validate_unmigrated_named_type(name),
        IrType::Context(view) => {
            if let Some(schema) = view.schema() {
                if !matches!(schema.as_ref(), IrType::Named(_)) {
                    return Err(CompileError::without_span("transaction view requires a nominal Cell schema")
                        .with_code(crate::error::DiagnosticCode::E2105));
                }
                validate_ir_type(schema)?;
            }
            Ok(())
        }
        IrType::Bounded(collection) => {
            if !(1..=1024).contains(&collection.max_elements) {
                return Err(CompileError::without_span("bounded collection cardinality must be in 1..=1024")
                    .with_code(crate::error::DiagnosticCode::E2105));
            }
            validate_ir_type(&collection.element_type)
        }
        IrType::Commitment(value) | IrType::Opening(value) => validate_ir_type(value),
        IrType::OpenHandle(handle) => {
            if handle.has_bounded_module_path() {
                Ok(())
            } else {
                Err(CompileError::without_span("open handle requires a bounded resolved module-interface designation")
                    .with_code(crate::error::DiagnosticCode::E2105))
            }
        }
        IrType::WitnessBytes(view) => {
            if view.maximum <= crate::ast::MAX_WITNESS_BYTES_VIEW {
                Ok(())
            } else {
                Err(CompileError::without_span("bounded witness view exceeds its admitted byte extent")
                    .with_code(crate::error::DiagnosticCode::E2105))
            }
        }
        IrType::Selection(selection) => {
            if !selection.is_well_formed() {
                return Err(CompileError::without_span("exact selection requires a bounded retained catalog/role description")
                    .with_code(crate::error::DiagnosticCode::E2105));
            }
            for schema in &selection.schemas {
                validate_ir_type(&IrType::Named(schema.clone()))?;
            }
            Ok(())
        }
        IrType::Vector(element) => {
            if let Some(element) = element {
                validate_ir_type(element)?;
            }
            Ok(())
        }
        IrType::Applied(application) => {
            for argument in application.arguments.iter().filter_map(crate::ast::TypeArgument::as_type) {
                validate_ir_type(argument)?;
            }
            // Migration barrier: executable families must acquire a resolved
            // description before this boundary can admit them. In particular,
            // a dynamic family must not fall through nominal scalar lowering.
            Err(CompileError::without_span(format!(
                "type application '{}' has no admitted executable family description",
                application.constructor
            ))
            .with_code(crate::error::DiagnosticCode::E2105))
        }
        IrType::Array(inner, _) | IrType::Ref(inner) | IrType::MutRef(inner) => validate_ir_type(inner),
        IrType::Tuple(items) => {
            for item in items {
                validate_ir_type(item)?;
            }
            Ok(())
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
        | IrType::Nested(_) => Ok(()),
    }
}

/// Named IR contains declaration labels. Applications, invocation-local
/// selections, aggregates and references must use their structural variants,
/// including unknown families that never passed source checking.
fn validate_unmigrated_named_type(name: &str) -> Result<()> {
    let invalid = || {
        CompileError::without_span(
            "executable types require retained structural arguments; malformed or unresolved named type syntax is not executable",
        )
        .with_code(crate::error::DiagnosticCode::E2105)
    };
    let mut depth = 0usize;
    for character in name.chars() {
        match character {
            '<' | '[' | '(' => {
                depth += 1;
                if depth > 32 {
                    return Err(invalid());
                }
            }
            '>' | ']' | ')' => depth = depth.checked_sub(1).ok_or_else(invalid)?,
            _ => {}
        }
    }
    if depth != 0 {
        return Err(invalid());
    }
    let ty = crate::generics::parse_type_repr(name).ok_or_else(invalid)?;
    let crate::ast::Type::Named(label) = &ty else { return Err(invalid()) };
    if ty.as_context_view().is_some()
        || crate::nested_values::is_nested_name(label)
        || crate::ast::OpenHandleClass::from_constructor(label).is_some()
        || matches!(
            label.as_str(),
            "Vec"
                | "InputView"
                | "OutputView"
                | "CellDataView"
                | "WitnessBytesView"
                | "ExactCellSelection"
                | "BoundedCellSet"
                | "BoundedList"
                | "Commitment"
                | "Opening"
        )
        || !label.split("::").all(|part| {
            let mut characters = part.chars();
            characters.next().is_some_and(|first| first == '_' || first.is_alphabetic())
                && characters.all(|character| character == '_' || character.is_alphanumeric())
        })
    {
        return Err(invalid());
    }
    Ok(())
}

fn require_registered(id: &str) -> Result<()> {
    if EXECUTABLE_SURFACE.iter().any(|entry| entry.id == id) {
        Ok(())
    } else {
        Err(CompileError::without_span(format!("IR surface classifier emitted unregistered ID '{id}'"))
            .with_code(crate::error::DiagnosticCode::E2105))
    }
}

fn ir_item_surface_id(item: &IrItem) -> &'static str {
    match item {
        IrItem::TypeDef(_) => "ir-item:type-def",
        IrItem::Invariant(_) => "ir-item:invariant",
        IrItem::Action(_) => "ir-item:action",
        IrItem::Function(_) => "ir-item:function",
        IrItem::Lock(_) => "ir-item:lock",
    }
}

fn ir_type_surface_id(ty: &IrType) -> &'static str {
    match ty {
        IrType::U8 => "type:u8",
        IrType::U16 => "type:u16",
        IrType::U32 => "type:u32",
        IrType::I32 => "type:i32",
        IrType::U64 => "type:u64",
        IrType::U128 => "type:u128",
        IrType::Bool => "type:bool",
        IrType::Unit => "type:unit",
        IrType::Address => "type:Address",
        IrType::Hash => "type:Hash",
        IrType::Array(_, _) => "type:Array",
        IrType::Tuple(_) => "type:Tuple",
        IrType::Named(_)
        | IrType::Applied(_)
        | IrType::Context(_)
        | IrType::Vector(_)
        | IrType::Bounded(_)
        | IrType::Commitment(_)
        | IrType::Opening(_)
        | IrType::OpenHandle(_)
        | IrType::Selection(_)
        | IrType::WitnessBytes(_) => "type:Named",
        IrType::Nested(_) => "type:Nested",
        IrType::Ref(_) => "type:Ref",
        IrType::MutRef(_) => "type:MutRef",
    }
}

fn ir_instruction_surface_id(instruction: &IrInstruction) -> &'static str {
    match instruction {
        IrInstruction::AllocatedValue(_) => "ir:allocated-value",
        IrInstruction::LoadConst { .. } => "ir:load-const",
        IrInstruction::LoadVar { .. } => "ir:load-var",
        IrInstruction::StoreVar { .. } => "ir:store-var",
        IrInstruction::Binary { .. } => "ir:binary",
        IrInstruction::Unary { .. } => "ir:unary",
        IrInstruction::FieldAccess { .. } => "ir:field-access",
        IrInstruction::Index { .. } => "ir:index",
        IrInstruction::Length { .. } => "ir:length",
        IrInstruction::TypeHash { .. } => "ir:type-hash",
        IrInstruction::CollectionNew { .. } => "ir:collection-new",
        IrInstruction::CollectionCapacity { .. } => "ir:collection-capacity",
        IrInstruction::CollectionPush { .. } => "ir:collection-push",
        IrInstruction::CollectionExtend { .. } => "ir:collection-extend",
        IrInstruction::CollectionClear { .. } => "ir:collection-clear",
        IrInstruction::CollectionContains { .. } => "ir:collection-contains",
        IrInstruction::CollectionRemove { .. } => "ir:collection-remove",
        IrInstruction::CollectionInsert { .. } => "ir:collection-insert",
        IrInstruction::CollectionSet { .. } => "ir:collection-set",
        IrInstruction::CollectionPop { .. } => "ir:collection-pop",
        IrInstruction::CollectionReverse { .. } => "ir:collection-reverse",
        IrInstruction::CollectionTruncate { .. } => "ir:collection-truncate",
        IrInstruction::CollectionSwap { .. } => "ir:collection-swap",
        IrInstruction::BoundedCellLoad { .. } => "ir:bounded-cell-load",
        IrInstruction::VerifyExactCellRoles { .. } => "ir:verify-exact-cell-roles",
        IrInstruction::ReadExactCellRole { .. } => "ir:verify-exact-cell-roles",
        IrInstruction::BoundedPlanLoad { .. } => "ir:bounded-plan-load",
        IrInstruction::BoundedOutputVerify { .. } => "ir:bounded-output-verify",
        IrInstruction::BoundedOutputEnd { .. } => "ir:bounded-output-end",
        IrInstruction::Call { .. } => "ir:call",
        IrInstruction::ReadRef { .. } => "ir:read-ref",
        IrInstruction::Move { .. } => "ir:move",
        IrInstruction::Tuple { .. } => "ir:tuple",
        IrInstruction::EnumConstruct { .. } => "ir:enum-construct",
        IrInstruction::EnumTag { .. } => "ir:enum-tag",
        IrInstruction::EnumPayload { .. } => "ir:enum-payload",
        IrInstruction::Consume { .. } => "ir:consume",
        IrInstruction::Create { .. } => "ir:create",
        IrInstruction::Transfer { .. } => "ir:transfer",
        IrInstruction::Destroy { .. } => "ir:destroy",
        IrInstruction::Claim { .. } => "ir:claim",
        IrInstruction::Settle { .. } => "ir:settle",
        IrInstruction::CreateUnique { .. } => "ir:create-unique",
        IrInstruction::ReplaceUnique { .. } => "ir:replace-unique",
        IrInstruction::CellMetadataEquality { .. } => "ir:cell-metadata-equality",
    }
}

fn ir_terminator_surface_id(terminator: &IrTerminator) -> &'static str {
    match terminator {
        IrTerminator::Return(_) => "ir-terminator:return",
        IrTerminator::Jump(_) => "ir-terminator:jump",
        IrTerminator::Branch { .. } => "ir-terminator:branch",
    }
}

pub fn executable_surface_json() -> String {
    let value = serde_json::json!({
        "schema": EXECUTABLE_SURFACE_SCHEMA,
        "entries": EXECUTABLE_SURFACE,
    });
    let mut rendered = serde_json::to_string_pretty(&value).expect("static executable surface serializes");
    rendered.push('\n');
    rendered
}

pub fn executable_surface_markdown() -> String {
    let mut rendered = String::from(
        "# CellScript Executable Surface Matrix\n\n\
**Status**: generated from the compiler-owned 0.26 executable-surface registry\n\n\
This file is generated. Run `cellscript-tools check-executable-surface --write` after changing the registry.\n\n\
Production compilation means `--production` or `--deny-fail-closed`; both stop before codegen when a selected shape reports any listed fail-closed feature. Metadata-only compilation remains available for diagnostics and Playground inspection.\n\n\
| ID | Layer | Status | Production policy | Conditions | Fail-closed features |\n\
|---|---|---|---|---|---|\n",
    );
    for entry in EXECUTABLE_SURFACE {
        let features = if entry.fail_closed_features.is_empty() { "none".to_string() } else { entry.fail_closed_features.join(", ") };
        rendered.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` |\n",
            entry.id, entry.layer, entry.status, entry.production_policy, entry.conditions, features
        ));
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::{validate_fail_closed_features, EXECUTABLE_SURFACE};
    use std::collections::BTreeSet;

    #[test]
    fn registry_ids_and_fail_closed_features_are_stable_and_unique() {
        let mut ids = BTreeSet::new();
        let mut features = BTreeSet::new();
        for entry in EXECUTABLE_SURFACE {
            assert!(ids.insert(entry.id), "duplicate executable-surface ID: {}", entry.id);
            for feature in entry.fail_closed_features {
                assert!(features.insert(*feature), "fail-closed feature appears under multiple surface entries: {feature}");
                assert!(
                    feature.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                    "fail-closed feature must be lowercase kebab-case: {feature}"
                );
            }
        }
    }

    #[test]
    fn unregistered_fail_closed_features_are_rejected_even_before_policy_selection() {
        let error = validate_fail_closed_features(["new-unclassified-runtime-shape"]).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2105"));
        assert!(error.message.contains("unregistered"));
    }

    #[test]
    fn nested_view_admission_visits_all_generic_arguments() {
        use super::validate_ir_type;
        use crate::ir::IrType;
        for name in [
            "Vec<u64>",
            "Wrapper<Vec<[u8; 2]>>",
            "InputView<Token>",
            "OutputView<u64, 16>",
            "Wrapper<CellDataView<Token>>",
            "CellDepView",
            "Vec<NestedValues<u64, 4>>",
            "Wrapper<u64, NestedBytesView<4, 8>>",
            "Wrapper<(&NestedRecordView<Token, 4>, u64)>",
            "Outer<Inner<[NestedValues<u64, 4>; 2]>>",
            "ScriptHandle<foreign::module>",
            "VerifierHandle<foreign::module>",
            "InstanceScriptHandle<foreign::module>",
            "Unknown<u64>",
            "Outer<Inner<[u64; 2]>, (u64, &Token)>",
            "[u64; 2]",
            "&Token",
            "WitnessBytesView<entry,64>",
            "ExactCellSelection<Token>",
        ] {
            assert_eq!(validate_ir_type(&IrType::Named(name.into())).unwrap_err().code.as_deref(), Some("E2105"), "{name}");
        }
        for name in ["Token", "owner::Token"] {
            validate_ir_type(&IrType::Named(name.into())).unwrap();
        }
        validate_ir_type(&IrType::vector(IrType::U64)).unwrap();
        validate_ir_type(&IrType::context(crate::ast::ContextView::Input(IrType::Named("Token".into())))).unwrap();
        assert!(validate_ir_type(&IrType::context(crate::ast::ContextView::Input(IrType::U64))).is_err());
        assert!(validate_ir_type(&IrType::vector(IrType::Named("Wrapper<Vec<u64>>".into()))).is_err());
        for interface in ["", "owner::::module", "owner::<module>", "0owner::module", &"a".repeat(513)] {
            let handle = crate::ast::OpenHandleType { class: crate::ast::OpenHandleClass::Instance, interface: interface.into() };
            assert_eq!(validate_ir_type(&IrType::OpenHandle(Box::new(handle))).unwrap_err().code.as_deref(), Some("E2105"));
        }
        let valid = crate::cell_selection::SelectionType {
            catalog: Some("Transfers".into()),
            names: vec!["tokens".into()],
            schemas: vec!["Token".into()],
            selected: None,
        };
        validate_ir_type(&IrType::Selection(Box::new(valid.clone()))).unwrap();
        let mut invalid = valid;
        // Encoding uses zipped names/schemas. Admission must inspect retained
        // fields, never encode and parse away this extra unbound role.
        invalid.names.push("unbound".into());
        assert_eq!(validate_ir_type(&IrType::Selection(Box::new(invalid))).unwrap_err().code.as_deref(), Some("E2105"));
        let too_deep = format!("{}u64{}", "Wrapper<".repeat(33), ">".repeat(33));
        assert_eq!(validate_ir_type(&IrType::Named(too_deep)).unwrap_err().code.as_deref(), Some("E2105"));
    }

    #[test]
    fn unresolved_applications_cannot_hide_in_executable_bodies() {
        use crate::ir::{IrConst, IrInstruction, IrItem, IrOperand, IrTerminator, IrType, IrVar};
        let tokens = crate::lexer::lex("module admission fn compute() -> u64 { 7 }").unwrap();
        let ast = crate::parser::parse(&tokens).unwrap();
        let original = crate::ir::generate(&ast).unwrap();
        super::validate_ir_module(&original).unwrap();
        let forged = IrVar { id: 1, name: "forged".into(), ty: IrType::Named("Unknown<u64>".into()) };
        let scalar = IrVar { id: 2, name: "scalar".into(), ty: IrType::U64 };
        let cases = [
            (vec![IrInstruction::LoadConst { dest: forged.clone(), value: IrConst::U64(7) }], IrTerminator::Return(None)),
            (
                vec![IrInstruction::Call {
                    dest: Some(scalar.clone()),
                    func: crate::ir::IrCallTarget::Function("foreign".into()),
                    args: vec![IrOperand::Var(forged.clone())],
                }],
                IrTerminator::Return(None),
            ),
            (
                vec![IrInstruction::CollectionPush { collection: IrOperand::Var(scalar), value: IrOperand::Var(forged.clone()) }],
                IrTerminator::Return(None),
            ),
            (Vec::new(), IrTerminator::Return(Some(IrOperand::Var(forged)))),
        ];
        for (instructions, terminator) in cases {
            let mut module = original.clone();
            let IrItem::Function(function) = &mut module.items[0] else { panic!("expected function") };
            function.body.blocks[0].instructions = instructions;
            function.body.blocks[0].terminator = terminator;
            assert_eq!(super::validate_ir_module(&module).unwrap_err().code.as_deref(), Some("E2105"));
            for format in [crate::ArtifactFormat::RiscvAssembly, crate::ArtifactFormat::RiscvElf] {
                let error = crate::codegen::CodeGenerator::new(Default::default()).generate(&module, format).unwrap_err();
                assert_eq!(error.code.as_deref(), Some("E2105"));
            }
        }
    }
}
