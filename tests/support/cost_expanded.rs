//! Frozen action/tag/record sweeps for the 0.31 cost pass.

use std::{collections::BTreeMap, fs};

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    strip_vm_abi_trailer, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ckb_script_runner::build_simple_fixture, cost_provenance::sha256, cost_stack, measured_run, options, repo_root};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    policies: Vec<PolicyFixture>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TagShape {
    Dense,
    Sparse,
    High,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecordPosition {
    First,
    Last,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFixture {
    id: String,
    actions: usize,
    width: usize,
    tags: TagShape,
    records: usize,
    selected_record: RecordPosition,
    mixed_layouts: bool,
    common_checks: bool,
}

impl PolicyFixture {
    fn tag(&self, action: usize) -> u32 {
        match self.tags {
            TagShape::Dense => action as u32,
            TagShape::Sparse => (action as u32 + 1) * 7919,
            TagShape::High => u32::MAX - (self.actions - 1 - action) as u32,
        }
    }

    fn source(&self) -> String {
        let mut source =
            format!("module expanded_cost\nresource Token has store, consume {{ amount: u64\nexpected: [u8; {}] }}\n", self.width);
        if self.common_checks {
            source.push_str("action common_check() { verification\nrequire true\n}\n");
        }
        for action in 0..self.actions {
            let mixed = self.mixed_layouts && action % 2 == 1;
            let extra = if mixed { ", witness marker: u64" } else { "" };
            source.push_str(&format!("action check_{action}(input before: Token, witness payload: [u8; {}]{extra}) {{\nverification\nrequire before.amount == {}\nrequire payload == before.expected\n", self.width, action + 1));
            if mixed {
                source.push_str("require marker == 99\n");
            }
            source.push_str("consume before\n}\n");
        }
        source
    }

    fn compile(&self, source: &str) -> CompileResult {
        compile_artifact(
            source,
            options(),
            ArtifactDeclaration {
                name: "ExpandedCostPolicy".into(),
                context: ArtifactContext::TypeGroup { resource: "Token".into() },
                dispatch: ArtifactDispatch::PolicyWitnessV1,
                actions: (0..self.actions)
                    .map(|index| ArtifactAction { tag: self.tag(index), action: format!("check_{index}") })
                    .collect(),
                common_checks: if self.common_checks { vec!["common_check".into()] } else { vec![] },
            },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_or_else(|error| panic!("{}: {error}", self.id))
    }
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Success,
    LateMismatch,
    UnknownTag,
    TruncatedArgs,
    ExtraArgs,
    TruncatedBundle,
    MalformedFinalRecord,
    WitnessAtBound,
    WitnessOverBound,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::LateMismatch => "late-mismatch",
            Self::UnknownTag => "unknown-tag",
            Self::TruncatedArgs => "truncated-args",
            Self::ExtraArgs => "extra-args",
            Self::TruncatedBundle => "truncated-bundle",
            Self::MalformedFinalRecord => "malformed-final-record",
            Self::WitnessAtBound => "witness-at-bound",
            Self::WitnessOverBound => "witness-over-bound",
        }
    }
    fn succeeds(self) -> bool {
        matches!(self, Self::Success | Self::WitnessAtBound)
    }
}

fn witness(spec: &PolicyFixture, compiled: &CompileResult, script: &packed::Script, action: usize, case: Case) -> Bytes {
    let mut payload = vec![0; spec.width];
    payload[spec.width - 1] = if matches!(case, Case::LateMismatch) { 6 } else { 7 };
    let mut args = vec![EntryWitnessArg::Bytes(payload)];
    if spec.mixed_layouts && action % 2 == 1 {
        args.push(EntryWitnessArg::U64(99));
    }
    let mut selected =
        encode_policy_action_record(&compiled.metadata, &script.calc_script_hash().unpack(), &format!("check_{action}"), &args)
            .expect("selected policy record");
    match case {
        Case::UnknownTag => {
            selected.tag =
                (0..=spec.actions as u32).find(|tag| !(0..spec.actions).any(|index| spec.tag(index) == *tag)).expect("unused tag")
        }
        Case::TruncatedArgs => {
            selected.args.pop();
        }
        Case::ExtraArgs => selected.args.push(0),
        _ => {}
    }
    let selected_hash = selected.script_hash;
    let mut records = vec![PolicyWitnessRecord {
        role: PolicyScriptRole::Type,
        script_hash: selected.script_hash,
        tag: selected.tag,
        args: selected.args,
    }];
    for index in 1..spec.records {
        let mut hash = match spec.selected_record {
            RecordPosition::First => [255; 32],
            RecordPosition::Last => [0; 32],
        };
        hash[31] = index as u8;
        assert!(match spec.selected_record {
            RecordPosition::First => hash > selected_hash,
            RecordPosition::Last => hash < selected_hash,
        });
        records.push(PolicyWitnessRecord { role: PolicyScriptRole::Type, script_hash: hash, tag: 0, args: vec![] });
    }
    let mut bundle = encode_policy_witness_bundle(&records).expect("canonical records");
    match case {
        Case::TruncatedBundle => {
            bundle.pop();
        }
        Case::MalformedFinalRecord => {
            let offset_position = 8 + 4 * spec.records;
            let last_offset = u32::from_le_bytes(bundle[offset_position..offset_position + 4].try_into().unwrap()) as usize;
            bundle[8 + last_offset + 4..8 + last_offset + 8].copy_from_slice(&21u32.to_le_bytes());
        }
        _ => {}
    }
    let mut builder = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bundle.clone())).pack());
    if matches!(case, Case::WitnessAtBound | Case::WitnessOverBound) {
        let target = if matches!(case, Case::WitnessAtBound) { 4096 } else { 4097 };
        let padding = target - 28 - bundle.len();
        builder = builder
            .lock(Some(Bytes::from(vec![0; padding / 2])).pack())
            .output_type(Some(Bytes::from(vec![0; padding - padding / 2])).pack());
    }
    builder.build().as_bytes()
}

pub fn measure(runs: &mut Vec<Value>) -> Vec<Value> {
    let manifest_bytes = include_bytes!("../fixtures/cost_corpus/expanded_fixtures.json");
    let manifest: Manifest = serde_json::from_slice(manifest_bytes).expect("frozen cost manifest");
    assert_eq!(manifest.schema, "cellscript-cost-fixtures-v2");
    assert_eq!(manifest.policies.len(), 41, "frozen policy sweep count");
    let mut rows = Vec::new();
    for spec in manifest.policies {
        let source = spec.source();
        let compiled = spec.compile(&source);
        compiled.validate().expect("expanded artifact passes independent checker");
        let lowering = compiled.verified_lowering_record.as_ref().unwrap();
        let mut code_bytes = BTreeMap::<&str, u64>::new();
        let mut code_ranges = Vec::new();
        let decoder_count = lowering.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")).count();
        let expected_decoders = if spec.actions == 1 {
            0
        } else if spec.mixed_layouts {
            2
        } else {
            1
        };
        assert_eq!(decoder_count, expected_decoders, "{}: exact repeated decoder layouts must share", spec.id);
        for entry in &lowering.entries {
            let kind = if entry.name == "_cellscript_entry" {
                "dispatch"
            } else if entry.name.starts_with(".Lpolicy_shared_decoder_") {
                "decoder"
            } else if entry.name.starts_with(".Lpolicy_action_adapter_") {
                if decoder_count == 0 {
                    "dedicated_decoder_adapter"
                } else {
                    "stub"
                }
            } else if entry.kind == cellscript_artifact_checker::EntryKind::Action {
                "action"
            } else {
                "other_runtime"
            };
            for block in lowering.blocks.iter().filter(|block| block.owner_entry == entry.id) {
                let bytes = block.range.end - block.range.start;
                *code_bytes.entry(kind).or_default() += bytes;
                code_ranges
                    .push(json!({"kind":kind,"entry_id":entry.id,"start":block.range.start,"end":block.range.end,"bytes":bytes}));
            }
        }
        let stack = cost_stack::measure(&compiled);
        let cost_stack::StaticStackBound::Bounded { static_call_chain_stack_bound_bytes } = stack else {
            panic!("{}: {stack:?}", spec.id)
        };
        let max_stack_frame_bytes =
            compiled.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
        let mut maximum_success = 0;
        let mut maximum_group_success = 0;
        let mut maximum_group_rejection = 0;
        let first_run = runs.len();
        for action in 0..spec.actions {
            let mut cases = vec![Case::Success, Case::LateMismatch];
            if action == spec.actions - 1 {
                cases.extend([
                    Case::UnknownTag,
                    Case::TruncatedArgs,
                    Case::ExtraArgs,
                    Case::TruncatedBundle,
                    Case::MalformedFinalRecord,
                    Case::WitnessAtBound,
                    Case::WitnessOverBound,
                ]);
            }
            let mut fixture = build_simple_fixture(Bytes::default(), 1, 0);
            fixture.current_type_script_input_indices = vec![0];
            let mut data = ((action + 1) as u64).to_le_bytes().to_vec();
            data.extend(vec![0; spec.width]);
            *data.last_mut().unwrap() = 7;
            fixture.inputs[0].data = Bytes::from(data);
            for case in cases {
                let name = format!("{}/action-{action}/{}", spec.id, case.name());
                let execution = measured_run(
                    &name,
                    strip_vm_abi_trailer(&compiled.artifact_bytes),
                    &fixture,
                    |tx, script| {
                        let bytes = witness(&spec, &compiled, &script, action, case);
                        tx.as_advanced_builder().set_witnesses(vec![bytes.pack()]).build()
                    },
                    runs,
                );
                assert_eq!(execution.exit_code == 0, case.succeeds(), "{name}: {:?}", execution.captured_debug);
                let group_cycles =
                    runs.last().unwrap()["group"]["observation"]["cycles"].as_u64().expect("required measured group cycles");
                if case.succeeds() {
                    maximum_success = maximum_success.max(execution.cycles);
                    maximum_group_success = maximum_group_success.max(group_cycles);
                } else {
                    maximum_group_rejection = maximum_group_rejection.max(group_cycles);
                }
            }
        }
        let row = json!({
            "name":spec.id,"source_sha256":sha256(source.as_bytes()),"elf_sha256":sha256(strip_vm_abi_trailer(&compiled.artifact_bytes)),
            "fixture_manifest_sha256":sha256(manifest_bytes),"elf_bytes":strip_vm_abi_trailer(&compiled.artifact_bytes).len(),
            "positive_cycles":maximum_success,"group_positive_cycles":maximum_group_success,"group_rejection_cycles":maximum_group_rejection,
            "max_stack_frame_bytes":max_stack_frame_bytes,"static_call_chain_stack_bound_bytes":static_call_chain_stack_bound_bytes,
            "executed_cases":runs.len()-first_run,
            "policy_code_bytes":code_bytes,"policy_code_ranges":code_ranges,
        });
        let mut summary = row.clone();
        summary.as_object_mut().unwrap().remove("policy_code_ranges");
        eprintln!("[cost-expanded] {summary}");
        rows.push(row);
    }
    let root = repo_root().join("target/cellscript-cost");
    fs::create_dir_all(&root).unwrap();
    // Keep measured baseline evidence even if a ceiling rejects the candidate.
    // This path never creates or updates checked-in budgets.
    fs::write(
        root.join("expanded-measurements.json"),
        serde_json::to_vec_pretty(&json!({"status":"measured","rows":rows,"executions":runs})).unwrap(),
    )
    .unwrap();
    let budgets: BTreeMap<String, BTreeMap<String, u64>> = serde_json::from_slice(
        &fs::read(repo_root().join("tests/fixtures/cost_corpus/expanded_budgets.json"))
            .expect("expanded budgets must be explicitly frozen from reviewed baseline evidence"),
    )
    .expect("frozen expanded budgets");
    assert_eq!(budgets.len(), rows.len());
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let budget = budgets.get(name).expect("every expanded fixture has a budget");
        for field in [
            "elf_bytes",
            "positive_cycles",
            "group_positive_cycles",
            "group_rejection_cycles",
            "max_stack_frame_bytes",
            "static_call_chain_stack_bound_bytes",
        ] {
            let value = row[field].as_u64().unwrap();
            assert!(value > 0 && value <= budget[field], "{name} {field}: {value} exceeds {}", budget[field]);
        }
    }
    rows
}
