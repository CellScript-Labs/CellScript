//! Real compiler/checker boundary mutations. Rebinding sidecar and semantic
//! identities must not turn a contradictory builder/policy projection valid.
//! The machine cases cover the exact bounded policy wrapper and adapters; they
//! do not establish arbitrary program equivalence or action predicate meaning.

use cellscript::artifact::{ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
use cellscript::{
    compile_path_with_executable_surface_policy, CellScriptEdition, CompileEntryScope, CompileOptions, ExecutableSurfacePolicy,
};
use cellscript_artifact_checker::{
    canonical_hash, check_bundle_values, parse_elf, CheckerBudgets, CheckerError, CheckerRejectionCode, EntryDispatchContract,
    PolicyWitnessContract, SourceArtifactMap, ValueProvenance, VerifiedLoweringRecord, LOWERING_RECORD_SCHEMA, SOURCE_MAP_SCHEMA,
    TYPED_SEMANTICS_SCHEMA,
};
use serde_json::Value;

const SOURCE: &str = r#"
module policy_artifact_checker;
resource Token has store, consume { amount: u64, }
action check_z() {  require true; }
action check_a() {  require true; }
action mint(witness amount: u64, witness recipient: Address) {

    require amount > 0;
    create Token { amount: amount, } with_lock(recipient)
}
action burn(input token: Token) {  consume token; }
"#;

const STACK_ARGS_SOURCE: &str = r#"
module policy_stack_args;
resource Token has store, consume { amount: u64, }
action mint(
    witness amount: u64, witness p1: u64, witness p2: u64,
    witness p3: u64, witness p4: u64, witness p5: u64,
    witness p6: u64, witness p7: u64, witness p8: u64,
    witness recipient: Address
) {

    require amount > 0;
    create Token { amount: amount, } with_lock(recipient)
}
"#;

fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        name: "token-policy".to_string(),
        context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 40, action: "burn".to_string() }, ArtifactAction { tag: 10, action: "mint".to_string() }],
        common_checks: vec!["check_z".to_string(), "check_a".to_string()],
    }
}

#[derive(Clone)]
struct Fixture {
    artifact: Vec<u8>,
    metadata: Value,
    record: VerifiedLoweringRecord,
    source_map: SourceArtifactMap,
}

fn fixed_external_codec(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::external_codec::CheckedFixedExternalCodec, CheckerError> {
    cellscript_artifact_checker::external_codec::check_fixed_external_codec(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        &CheckerBudgets::default(),
    )
}
fn external_fixture(source: &str, opt: cellscript::CompilerDiagnostics) -> Fixture {
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    Fixture::new_source_with(source, CellScriptEdition::Edition2027, opt, selected)
}
const EXTERNAL_SOURCE: &str = r#"
module external_codec;
resource Token has store, consume { amount: u64, }
action burn(input token: Token, witness value: u64) {
     require token.amount > 0; require value > 0; consume token;
}
"#;

include!("policy_artifact_checker/witness_decoding.rs");
include!("policy_artifact_checker/code_cell_origin.rs");
include!("policy_artifact_checker/fixed_cell_fields.rs");
include!("policy_artifact_checker/fixture.rs");
include!("policy_artifact_checker/nominal_operands.rs");
include!("policy_artifact_checker/generic_catalogs.rs");
include!("policy_artifact_checker/policy_dispatch.rs");
include!("policy_artifact_checker/policy_abi.rs");
include!("policy_artifact_checker/dependency_substitution.rs");
include!("policy_artifact_checker/type_group.rs");
