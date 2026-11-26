#[path = "support/fixed_action.rs"]
mod fixed_action;

use cellscript::{CompileOptions, CompileResult, CURRENT_EDITION};

// Preserve the original diagnostic while making generated fixture failures reproducible.
fn compile(source: &str, options: CompileOptions) -> cellscript::error::Result<CompileResult> {
    let result = fixed_action::compile_fixture(source, options);
    if let Err(error) = &result {
        eprintln!("source rejected: {error}\n{source}");
    }
    result
}
use cellscript_artifact_checker::{
    canonical_bytes, canonical_hash, check_bundle, check_bundle_values, parse_elf, CheckerBudgets, CheckerRejectionCode, EdgeKind,
    SourceArtifactMap, TypedSemanticConstant, TypedSemanticOperation, TypedSemanticOperationDetail, VerifiedLoweringRecord,
    LOWERING_RECORD_SCHEMA, SOURCE_MAP_SCHEMA,
};
use serde_json::Value;

#[cfg(feature = "cli")]
mod common;

#[path = "support/collection_coordinate_helpers.rs"]
mod collection_coordinate_helpers;

#[path = "support/borrowed_collection_reads.rs"]
mod borrowed_collection_reads;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[path = "artifact_checker/allocated_arguments.rs"]
mod allocated_arguments;
#[path = "artifact_checker/allocated_calls.rs"]
mod allocated_calls;
#[path = "artifact_checker/allocated_copy_lifetime.rs"]
mod allocated_copy_lifetime;
#[path = "artifact_checker/allocated_isolation.rs"]
mod allocated_isolation;
#[path = "artifact_checker/allocated_parameters.rs"]
mod allocated_parameters;
#[path = "artifact_checker/allocated_paths.rs"]
mod allocated_paths;
#[path = "artifact_checker/allocated_queries.rs"]
mod allocated_queries;
#[path = "support/allocated_scalar_definitions.rs"]
mod allocated_scalar_definitions;
#[path = "artifact_checker/allocated_scalars.rs"]
mod allocated_scalars;
#[path = "artifact_checker/bounded_calls.rs"]
mod bounded_calls;
#[path = "artifact_checker/bounded_hash_properties.rs"]
mod bounded_hash_properties;
#[path = "artifact_checker/bounded_parameters.rs"]
mod bounded_parameters;
#[path = "artifact_checker/bounded_properties.rs"]
mod bounded_properties;
#[path = "artifact_checker/bounded_results.rs"]
mod bounded_results;
#[path = "artifact_checker/bounded_scalars.rs"]
mod bounded_scalars;
#[path = "artifact_checker/committed_aggregates.rs"]
mod committed_aggregates;
#[path = "artifact_checker/instruction_effects.rs"]
mod instruction_effects;

#[path = "artifact_checker/nominal_calls.rs"]
mod nominal_calls;

#[path = "artifact_checker/allocated_sorted_inputs.rs"]
mod allocated_sorted_inputs;

#[path = "artifact_checker/allocated_wide_transport.rs"]
mod allocated_wide_transport;

#[path = "artifact_checker/cell_data_results.rs"]
mod cell_data_results;

#[path = "artifact_checker/bounded_admission.rs"]
mod bounded_admission;

#[path = "artifact_checker/memory_hashes.rs"]
mod memory_hashes;

#[path = "artifact_checker/cache_forwarders.rs"]
mod cache_forwarders;

#[path = "artifact_checker/product_buffers.rs"]
mod product_buffers;

#[path = "artifact_checker/loaded_image.rs"]
mod loaded_image;

#[path = "artifact_checker/admission_hardening.rs"]
mod admission_hardening;

#[path = "artifact_checker/effects.rs"]
mod effects;

#[path = "artifact_checker/authoring_successors.rs"]
mod authoring_successors;
#[path = "artifact_checker/lock_artifacts.rs"]
mod lock_artifacts;

#[path = "artifact_checker/pipe.rs"]
mod pipe;

#[path = "artifact_checker/spawn.rs"]
mod spawn;

fn assert_bounded_admitted(fixture: &Fixture) {
    let features = fixture.metadata["runtime"]["fail_closed_runtime_features"].as_array().unwrap();
    for feature in [
        "allocated-values-artifact-proof",
        "nested-values-artifact-proof",
        "exact-role-selection-artifact-proof",
        "cell-data-view-artifact-proof",
    ] {
        assert!(!features.iter().any(|value| value == feature), "retired blanket blocker: {feature}");
    }
    fixture.check().unwrap();
}

const FIXTURE_SOURCE: &str = r#"
module artifact_checker_fixture;

fn increment(value: u64) -> u64 {
    return value + 1;
}

action main(witness value: u64) -> u64 {

        return increment(value);
}
"#;

fn cached_nested_source(hot: bool) -> String {
    let reads = (0..if hot { 48 } else { 2 })
        .map(|index| {
            let method = ["cell_data_u8", "cell_data_u32_le", "cell_data_u64_le"][index % 3];
            format!("require ckb::{method}(view, {index}) == 0;\n")
        })
        .collect::<String>();
    format!(
        r#"
module nested_cache::proof;
resource Token has store {{ id: u64, }}
action verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {{

    let first = items[0];
    let view = ckb::input::<Token>(0);
    {reads}
    require view.type_hash == view.lock_hash;
    require first[0] == 170;
    return 0;
}}
"#
    )
}

include!("artifact_checker/cached_reads_and_views.rs");
include!("artifact_checker/typed_cell_data_fields.rs");
include!("artifact_checker/nested_role_views.rs");
include!("artifact_checker/machine_entry_contracts.rs");
include!("artifact_checker/fixture.rs");
include!("artifact_checker/runtime_machine_checks.rs");
include!("artifact_checker/runtime_provenance.rs");
include!("artifact_checker/terminal_obligations.rs");
include!("artifact_checker/allocated_value_growth.rs");
include!("artifact_checker/allocated_value_search.rs");
include!("artifact_checker/allocated_value_transport.rs");
include!("artifact_checker/allocated_value_storage.rs");
include!("artifact_checker/committed_state.rs");
include!("artifact_checker/artifact_admission.rs");
include!("artifact_checker/nested_value_transport.rs");
include!("artifact_checker/typed_collection_semantics.rs");
include!("artifact_checker/nested_layouts.rs");
include!("artifact_checker/nested_lifetime.rs");
include!("artifact_checker/nested_cell_transport.rs");
include!("artifact_checker/nested_cell_admission.rs");
include!("artifact_checker/allocated_arena_bounds.rs");
