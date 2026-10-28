//! Cross-check the user-facing budget report against a separate logical model
//! and actual source execution at the byte/event admission boundaries.
use cellscript::{CompileOptions, CompileResult, NEXT_EDITION};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde_json::Value;
use std::process::Command;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/value_arena_reference.rs"]
#[allow(dead_code)]
mod reference;

fn source(width: usize, items: usize, bytes: usize, events: usize, reserve: bool) -> String {
    let reservations = if reserve {
        format!("work.rows.reserve({items})\nwork.amounts.reserve({items})\nwork.seen.reserve({items})")
    } else {
        String::new()
    };
    let literal = "x".repeat(width);
    format!(
        r#"module budget_execution
collections Work {{ limits {{ bytes: {bytes}, allocations: {events} }}
    rows: Vec<[u8; {width}]> {{ items: {items} }}
    amounts: OrderedMap<u64, u8> {{ items: {items} }}
    seen: OrderedSet<u64> {{ items: {items} }}
}}
action inspect() -> u64 {{
    let mut work = Work::new()
    {reservations}
    let value: u8 = 7
    for i in 0..{items} {{
        work.rows.push(b"{literal}")
        work.amounts.insert(i, value)
        work.seen.insert(i)
    }}
    require work.rows.len() == {items}
    require work.amounts.len() == {items}
    require work.seen.len() == {items}
    require work.rows.get({last}) == b"{literal}"
    require work.amounts.get({last}) == value
    require work.seen.contains({last})
    work.drop()
    return 0
}}"#,
        last = items - 1
    )
}

fn execute(compiled: &CompileResult) -> i64 {
    let payload = compiled.metadata.actions[0].entry_witness_args(&[]).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    // Boundary validation needs to finish the 1024-item profile. This explicit
    // measurement limit is not a business acceptance or deployment ceiling.
    let result = ckb_script_runner::execute_cellscript_script_with_cycle_limit(
        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
        &fixture,
        100_000_000,
    );
    assert!(result.exit_code == 0 || result.exit_code == 21, "unexpected VM failure: {result:?}");
    result.exit_code
}

#[test]
fn collection_budget_reports_match_reference_and_vm_byte_event_thresholds() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"budget_execution\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let mut executions = 0;
    for (width, items) in [(1, 1), (1, 3), (17, 8), (17, 32), (64, 128), (17, 1024)] {
        std::fs::write(root.join("src/main.cell"), source(width, items, 65536, 256, false)).unwrap();
        let output =
            Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["explain", "collections", "--json"]).output().unwrap();
        assert!(output.status.success(), "{} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        let owner = &report["owners"][0];
        for reserve in [true, false] {
            // The reference executes interleaved operations rather than summing
            // the report's geometric capacities. Its records retain old storage.
            let mut arena = reference::Arena::new(4 * 1024 * 1024, 256);
            let mut vectors =
                [reference::Vector::new(width, items), reference::Vector::new(9, items), reference::Vector::new(8, items)];
            if reserve {
                for vector in &mut vectors {
                    vector.reserve(&mut arena, items as u64).unwrap();
                }
            }
            for _ in 0..items {
                for vector in &mut vectors {
                    vector.push(&mut arena, 7).unwrap();
                }
            }
            let fit = &owner[if reserve { "reserve_all_once" } else { "grow_all_incrementally" }];
            let required_bytes = fit["cost"]["bytes"].as_u64().unwrap() as usize;
            let required_events = fit["cost"]["allocations"].as_u64().unwrap() as usize;
            assert_eq!((required_bytes, required_events), (arena.used, arena.records.len()));
            let retired = arena.records.iter().filter(|record| !record.live).map(|record| record.data.len() + 32).sum::<usize>();
            let reported_retired = if reserve {
                0
            } else {
                owner["members"].as_array().unwrap().iter().map(|member| member["retired_bytes"].as_u64().unwrap() as usize).sum()
            };
            assert_eq!(reported_retired, retired);
            let cases = if required_bytes <= 65536 {
                assert!(fit["fits"].as_bool().unwrap());
                vec![
                    (required_bytes, required_events, 0),
                    (required_bytes - 16, required_events, 21),
                    (required_bytes, required_events - 1, 21),
                ]
            } else {
                assert!(!fit["fits"].as_bool().unwrap());
                assert_eq!(fit["bytes_shortfall"], required_bytes - 65536);
                vec![(65536, required_events, 21)]
            };
            for (bytes, events, expected) in cases {
                for diagnostics in
                    [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()]
                {
                    eprintln!(
                        "budget-boundary width={width} items={items} reserve={reserve} bytes={bytes} events={events} {diagnostics}"
                    );
                    let program = source(width, items, bytes, events, reserve);
                    let compiled = cellscript::compile(
                        &program,
                        CompileOptions {
                            edition: NEXT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        execute(&compiled),
                        expected,
                        "width={width}, items={items}, reserve={reserve}, bytes={bytes}, events={events}, {diagnostics}"
                    );
                    executions += 1;
                }
            }
        }
    }
    assert_eq!(executions, 68);
}
