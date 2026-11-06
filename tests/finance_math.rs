//! Ordinary package arithmetic checked against an independent BigInt model and
//! encoded CKB-VM execution. This is not financial custody or node acceptance.
#![cfg(not(feature = "wasm"))]

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use cellscript::{CompileOptions, CompileResult, CompilerDiagnostics, EntryWitnessArg};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde_json::{json, Value};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

const APPLICATION: &str = r#"module finance_math_checks
use finance_math::mul_div_floor
use finance_math::mul_div_ceil
use finance_math::isqrt
use finance_math::verify_mul_div_floor
use finance_math::verify_mul_div_ceil
use finance_math::verify_isqrt

action direct_floor(witness a: u128, witness b: u128, witness d: u128, witness expected: u128) -> u64 {
    require mul_div_floor(a, b, d) == expected
    return 0
}
action direct_ceil(witness a: u128, witness b: u128, witness d: u128, witness expected: u128) -> u64 {
    require mul_div_ceil(a, b, d) == expected
    return 0
}
action witnessed_floor(witness a: u128, witness b: u128, witness d: u128, witness q: u128, witness r: u128, witness expected: u128) -> u64 {
    require verify_mul_div_floor(a, b, d, q, r)
    require q == expected
    return 0
}
action witnessed_ceil(witness a: u128, witness b: u128, witness d: u128, witness q: u128, witness r: u128, witness expected: u128) -> u64 {
    require verify_mul_div_ceil(a, b, d, q, r)
    require q == expected
    return 0
}
action direct_root(witness n: u128, witness expected: u128) -> u64 {
    require isqrt(n) == expected
    return 0
}
action witnessed_root(witness n: u128, witness root: u128, witness expected: u128) -> u64 {
    require verify_isqrt(n, root)
    require root == expected
    return 0
}
"#;

fn cases() -> Vec<Value> {
    let maximum = u128::MAX;
    let mut quotients = vec![
        (0, 0, 1),
        (0, maximum, 1),
        (1, 1, 1),
        (1, 1, 2),
        (2, 3, 4),
        (maximum, 1, 1),
        (maximum, 1, 2),
        (1 << 64, 1 << 64, maximum),
        (1 << 127, 2, 2),
        (maximum, maximum, maximum),
        (maximum, maximum, maximum - 1),
        // Floor fits exactly at MAX, but ceil does not fit u128.
        (maximum - 2, (maximum - 1) / 2, (maximum - 3) / 2),
        (1_000_000_000_000, 1_000_000_000_000, 10_000),
        (1_000_000_000_000, 1_000, 10_000),
        (1, 1, 0),
        (maximum, 2, 0),
    ];
    let mut state = 0x5358_6d75_a6e7_c4a4_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..16 {
        let a = u128::from(next() % 1_000_000_000_001);
        let b = u128::from(next() % 1_000_000_000_001);
        quotients.push((a, b, u128::from(next() % 1_000_000_000_000) + 1));
        let a = (u128::from(next()) << 64) | u128::from(next());
        let b = (u128::from(next()) << 64) | u128::from(next());
        quotients.push((a, b, a.max(b).max(1)));
    }
    let mut cases = quotients
        .into_iter()
        .enumerate()
        .map(|(index, (a, b, d))| {
            json!({"id": format!("quotient-{index}"), "kind":"quotient", "a":a.to_string(),
                "b":b.to_string(), "d":d.to_string()})
        })
        .collect::<Vec<_>>();
    let mut roots = vec![0, 1, 2, 3, 4, 8, 9, 15, 16, 17, u64::MAX as u128, 1 << 64, maximum];
    for root in [1u128 << 32, (1 << 63) + 1, u64::MAX as u128] {
        let square = root * root;
        roots.extend([square - 1, square, square + 1]);
    }
    for _ in 0..12 {
        roots.push((u128::from(next()) << 64) | u128::from(next()));
    }
    cases
        .extend(roots.into_iter().enumerate().map(|(index, n)| json!({"id":format!("root-{index}"),"kind":"root","n":n.to_string()})));
    cases
}

fn reference_model(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_math_model.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node 22 is required for the independent financial BigInt model");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({
                "schema":"cellscript-finance-math-model-v1", "cases":cases,
            }))
            .unwrap(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "BigInt model: {}", String::from_utf8_lossy(&output.stderr));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema"], "cellscript-finance-math-model-v1");
    let rows = response["results"].as_array().unwrap().clone();
    assert_eq!(rows.len(), cases.len());
    rows
}

fn application(root: &Path) -> std::path::PathBuf {
    let application = root.join("application");
    std::fs::create_dir_all(application.join("src")).unwrap();
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/finance_math");
    std::fs::write(application.join("Cell.toml"), format!(
        "[package]\ncellscript_version = \">=0.33.0-alpha.2\"\nedition = \"2027\"\nname = \"finance_math_checks\"\nversion = \"0.1.0\"\n[dependencies]\nfinance_math = {{ path = \"{}\" }}\n",
        library.display()
    )).unwrap();
    std::fs::write(application.join("src/main.cell"), APPLICATION).unwrap();
    let locked = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&application).arg("lock").output().unwrap();
    assert!(locked.status.success(), "lock: {} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    application
}

fn compile(application: &Path, entry: &str, diagnostics: CompilerDiagnostics) -> CompileResult {
    let before = std::fs::read(application.join("Cell.lock")).unwrap();
    let compiled = cellscript::compile_path_with_entry_action(
        camino::Utf8Path::from_path(application).unwrap(),
        CompileOptions {
            diagnostics,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb-type-hash".into()),
            ..Default::default()
        },
        entry,
    )
    .unwrap_or_else(|error| panic!("{entry}/{diagnostics}: {error:?}"));
    assert_eq!(std::fs::read(application.join("Cell.lock")).unwrap(), before, "locked compilation must not repin");
    assert!(compiled.metadata.source_units.iter().any(|unit| unit.role == "dependency" && unit.path.ends_with("src/lib.cell")));
    compiled.validate().unwrap();
    cellscript_artifact_checker::check_bundle_values(
        &compiled.artifact_bytes,
        &serde_json::to_value(&compiled.metadata).unwrap(),
        compiled.verified_lowering_record.as_ref().unwrap(),
        compiled.source_artifact_map.as_ref().unwrap(),
        &Default::default(),
    )
    .unwrap_or_else(|error| panic!("independent {entry}/{diagnostics} bundle check: {error}"));
    compiled
}

fn execute(compiled: &CompileResult, entry: &str, values: &[u128], expected: i64, label: &str) -> Value {
    let action = compiled.metadata.actions.iter().find(|action| action.name == entry).unwrap();
    let args = values.iter().copied().map(EntryWitnessArg::U128).collect::<Vec<_>>();
    let payload = action.entry_witness_args(&args).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    let execution = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, expected, "{entry}/{label} {values:?}: {execution:?}");
    json!({"case":label,"entry":entry,"exit":execution.exit_code,
        "transaction_cycles":if expected == 0 {Some(execution.cycles)} else {None},
        "witness_bytes":execution.witness_bytes,"transaction_bytes":execution.transaction_bytes,
        "raw_transaction_hash":execution.raw_transaction_hash,"serialized_transaction_hash":execution.serialized_transaction_hash})
}

fn number(row: &Value, name: &str) -> u128 {
    row[name].as_str().unwrap().parse().unwrap()
}

#[test]
fn ordinary_math_package_matches_bigint_model_in_encoded_ckb_vm() {
    let cases = cases();
    let model = reference_model(&cases);
    let temp = tempfile::tempdir().unwrap();
    let application = application(temp.path());
    let mut measurements = Vec::new();
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for entry in ["direct_floor", "direct_ceil", "witnessed_floor", "witnessed_ceil", "direct_root", "witnessed_root"] {
            let compiled = compile(&application, entry, diagnostics);
            let mut executions = Vec::new();
            for row in &model {
                let label = row["id"].as_str().unwrap();
                if entry.ends_with("root") {
                    if row["kind"] != "root" {
                        continue;
                    }
                    let n = number(row, "n");
                    let root = number(row, "root");
                    let values = if entry == "direct_root" { vec![n, root] } else { vec![n, root, root] };
                    executions.push(execute(&compiled, entry, &values, 0, label));
                    if entry == "witnessed_root" {
                        let wrong = root + 1;
                        let expected = if wrong > u64::MAX as u128 { 5 } else { 49 };
                        executions.push(execute(&compiled, entry, &[n, wrong, wrong], expected, &format!("{label}-wrong-root")));
                    }
                    continue;
                }
                if row["kind"] != "quotient" {
                    continue;
                }
                let a = number(row, "a");
                let b = number(row, "b");
                let d = number(row, "d");
                let witnessed = entry.starts_with("witnessed");
                let ceil = entry.ends_with("ceil");
                let field = if ceil { "ceil" } else { "floor" };
                let fits = row[format!("{field}_fits_u128")].as_bool().unwrap_or(false);
                let q = if fits { number(row, field) } else { 0 };
                let r = if d == 0 { 0 } else { number(row, if ceil { "ceil_remainder" } else { "remainder" }) };
                let expected = if !witnessed && row["product_fits_u128"] != true {
                    49
                } else if d == 0 {
                    if witnessed {
                        5
                    } else {
                        20
                    }
                } else if !fits {
                    49
                } else {
                    0
                };
                let values = if witnessed { vec![a, b, d, q, r, q] } else { vec![a, b, d, q] };
                executions.push(execute(&compiled, entry, &values, expected, label));
                if witnessed && expected == 0 {
                    let wrong_q = q.checked_add(1).unwrap_or_else(|| q - 1);
                    executions.push(execute(&compiled, entry, &[a, b, d, wrong_q, r, wrong_q], 49, &format!("{label}-wrong-q")));
                    executions.push(execute(&compiled, entry, &[a, b, d, q, d, q], 5, &format!("{label}-remainder-bound")));
                    if d > 1 {
                        let wrong_r = if r == 0 { 1 } else { r - 1 };
                        executions.push(execute(&compiled, entry, &[a, b, d, q, wrong_r, q], 49, &format!("{label}-wrong-r")));
                    }
                }
            }
            let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
            measurements.push(json!({"diagnostics":diagnostics.to_string(),"entry":entry,
                "deployed_elf_bytes":elf.len(),"artifact_hash":hex::encode(compiled.artifact_hash),
                "verified_bundle_id":compiled.metadata.verified_artifact.verified_bundle_id,
                "executions":executions}));
        }
    }
    let report = json!({"schema":"cellscript-finance-math-evidence-v1",
        "scope":"isolated ordinary library arithmetic; not financial custody, node admission, or whole-operation acceptance",
        "compiler":cellscript::VERSION,"seed":"0x53586d75a6e7c4a4",
        "library_hash":hex::encode(cellscript::ckb_blake2b256(include_bytes!("../examples/finance_math/src/lib.cell"))),
        "model_hash":hex::encode(cellscript::ckb_blake2b256(include_bytes!("support/finance_math_model.mjs"))),
        "application_hash":hex::encode(cellscript::ckb_blake2b256(APPLICATION.as_bytes())),
        "model":model,"measurements":measurements});
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-math-evidence.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
