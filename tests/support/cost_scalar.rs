//! Scalar lifetime, addressing, specialization and outgoing-argument fixtures.

use super::{
    ckb_script_runner::build_simple_fixture, compile_cellscript, cost_provenance::sha256, cost_stack, measured_run, repo_root,
    witness_for,
};
use cellscript::{strip_vm_abi_trailer, EntryWitnessArg};
use ckb_testtool::ckb_types::bytes::Bytes;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};

struct Fixture {
    name: String,
    source: String,
    valid: Vec<EntryWitnessArg>,
    invalid: Vec<EntryWitnessArg>,
}

fn scalar(count: usize, overlapping: bool) -> Fixture {
    let name = format!("scalar-{}-{count}", if overlapping { "overlap" } else { "chain" });
    let mut source = "module scalar_cost\naction verify(witness seed: u64, witness expected: u64) { verification\n".to_string();
    for index in 0..count {
        let previous = if overlapping || index == 0 { "seed".into() } else { format!("value_{}", index - 1) };
        let increment = if overlapping { index as u64 } else { 1 };
        source.push_str(&format!("let value_{index} = {previous} + {increment}\n"));
    }
    let expected = if overlapping {
        // Consume all initially live values through a balanced reduction. A
        // serial 260-term sum tests the provenance depth limit instead of the
        // intended scalar-addressing and interference boundary.
        let mut level = (0..count).map(|index| format!("value_{index}")).collect::<Vec<_>>();
        let mut depth = 0;
        while level.len() > 1 {
            level = level
                .chunks(2)
                .enumerate()
                .map(|(index, pair)| {
                    if pair.len() == 1 {
                        return pair[0].clone();
                    }
                    let name = format!("sum_{depth}_{index}");
                    source.push_str(&format!("let {name} = {} + {}\n", pair[0], pair[1]));
                    name
                })
                .collect();
            depth += 1;
        }
        source.push_str(&format!("require {} == expected\n", level[0]));
        count as u64 * (count as u64 + 1) / 2
    } else {
        source.push_str(&format!("require value_{} == expected\n", count - 1));
        count as u64 + 1
    };
    source.push_str("}\n");
    Fixture {
        name,
        source,
        valid: vec![EntryWitnessArg::U64(1), EntryWitnessArg::U64(expected)],
        invalid: vec![EntryWitnessArg::U64(1), EntryWitnessArg::U64(expected + 1)],
    }
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = vec![scalar(8, false), scalar(64, false), scalar(260, false), scalar(64, true), scalar(260, true)];
    fixtures.push(Fixture {
        name:"nested-nine-arguments".into(),
        source:"module outgoing_cost\nfn leaf(a: u64,b: u64,c: u64,d: u64,e: u64,f: u64,g: u64,h: u64,i: u64) -> u64 { return a+i }\nfn middle(a: u64,b: u64,c: u64,d: u64,e: u64,f: u64,g: u64,h: u64,i: u64) -> u64 { return leaf(a,b,c,d,e,f,g,h,i) }\naction verify(witness a: u64,witness b: u64,witness c: u64,witness d: u64,witness e: u64,witness f: u64,witness g: u64,witness h: u64,witness i: u64) { verification\nrequire middle(a,b,c,d,e,f,g,h,i) == 10\n}".into(),
        valid:(1..=9).map(EntryWitnessArg::U64).collect(),
        invalid:(1..=9).map(|value|EntryWitnessArg::U64(if value==9 {10} else {value})).collect(),
    });
    fixtures.push(Fixture {
        name:"scalar-loop-join".into(),
        source:"module loop_cost\naction verify(witness seed: u64,witness expected: u64) { verification\nlet mut total: u64 = seed\nfor index in 0..8 { if index < 4 { total += index } else { total += 1 } }\nrequire total == expected\n}".into(),
        valid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U64(11)],
        invalid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U64(12)],
    });
    fixtures.push(Fixture {
        name:"generic-repeated".into(),
        source:"module generic_repeated_cost\nfn same<T: fixed_value>(value: T) -> T { return value }\naction verify(witness seed: u64,witness expected: u64) { verification\nlet first = same<u64>(seed)\nlet second = same<u64>(first+1)\nlet third = same<u64>(second+1)\nrequire third == expected\n}".into(),
        valid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U64(3)],
        invalid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U64(4)],
    });
    fixtures.push(Fixture {
        name:"generic-distinct".into(),
        source:"module generic_distinct_cost\nfn same<T: fixed_value>(value: T) -> T { return value }\naction verify(witness a: u64,witness b: u32,witness c: u16,witness d: u8,witness flag: bool) { verification\nrequire same(a) == 1\nrequire same(b) == 2\nrequire same(c) == 3\nrequire same(d) == 4\nrequire same(flag)\n}".into(),
        valid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U32(2),EntryWitnessArg::U16(3),EntryWitnessArg::U8(4),EntryWitnessArg::Bool(true)],
        invalid:vec![EntryWitnessArg::U64(1),EntryWitnessArg::U32(2),EntryWitnessArg::U16(3),EntryWitnessArg::U8(4),EntryWitnessArg::Bool(false)],
    });
    fixtures
}

pub fn measure(runs: &mut Vec<Value>) -> Vec<Value> {
    let mut rows = Vec::new();
    for spec in fixtures() {
        let compiled = compile_cellscript(&spec.source);
        compiled.validate().expect("scalar cost artifact validates");
        let bound = cost_stack::measure(&compiled);
        let cost_stack::StaticStackBound::Bounded { static_call_chain_stack_bound_bytes } = bound else {
            panic!("{}: {bound:?}", spec.name)
        };
        let max_stack_frame_bytes =
            compiled.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
        let mut positive_cycles = 0;
        let mut group_positive_cycles = 0;
        let mut group_rejection_cycles = 0;
        let mut witness_bytes = 0;
        for (valid, args) in [(true, &spec.valid), (false, &spec.invalid)] {
            let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
            fixture.witnesses = vec![witness_for(&compiled, args)];
            let execution = measured_run(
                &format!("{}/valid-{valid}", spec.name),
                strip_vm_abi_trailer(&compiled.artifact_bytes),
                &fixture,
                |tx, _| tx,
                runs,
            );
            assert_eq!(execution.exit_code == 0, valid, "{}: {:?}", spec.name, execution.captured_debug);
            let cycles = runs.last().unwrap()["group"]["observation"]["cycles"].as_u64().unwrap();
            if valid {
                positive_cycles = execution.cycles;
                group_positive_cycles = cycles;
                witness_bytes = execution.witness_bytes;
            } else {
                group_rejection_cycles = cycles;
            }
        }
        let row = json!({"name":spec.name,"source_sha256":sha256(spec.source.as_bytes()),"elf_sha256":sha256(strip_vm_abi_trailer(&compiled.artifact_bytes)),"elf_bytes":strip_vm_abi_trailer(&compiled.artifact_bytes).len(),"positive_cycles":positive_cycles,"group_positive_cycles":group_positive_cycles,"group_rejection_cycles":group_rejection_cycles,"witness_bytes":witness_bytes,"max_stack_frame_bytes":max_stack_frame_bytes,"static_call_chain_stack_bound_bytes":static_call_chain_stack_bound_bytes});
        eprintln!("[cost-scalar] {row}");
        rows.push(row);
    }
    let output = repo_root().join("target/cellscript-cost/scalar-measurements.json");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(output, serde_json::to_vec_pretty(&json!({"status":"measured","rows":rows})).unwrap()).unwrap();
    let budgets: BTreeMap<String, BTreeMap<String, u64>> = serde_json::from_slice(
        &fs::read(repo_root().join("tests/fixtures/cost_corpus/scalar_budgets.json"))
            .expect("scalar budgets must be frozen from measured baseline"),
    )
    .unwrap();
    assert_eq!(budgets.len(), rows.len());
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let budget = &budgets[name];
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
        assert_eq!(row["witness_bytes"].as_u64().unwrap(), budget["witness_bytes"], "{name}: witness ABI footprint");
    }
    rows
}
