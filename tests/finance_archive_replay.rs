//! Independently restore archived transactions without compiling application
//! sources. Run after the owning finance evidence producers have written v2.
#![cfg(all(feature = "cli", not(feature = "wasm")))]

#[path = "support/finance_archive.rs"]
pub mod finance_archive;
#[path = "support/finance_lock_sdk_parity.rs"]
mod lock_sdk_parity;

use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use finance_archive::{format, load_archive, ArchiveError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn archives() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("CELLSCRIPT_FINANCE_ARCHIVE") {
        return vec![path.into()];
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence");
    let mut directories = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("produce finance_protocol archives first: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("report.json").is_file())
        .collect::<Vec<_>>();
    directories.sort();
    assert!(!directories.is_empty(), "no finance VM archives found");
    directories
}

#[test]
#[ignore = "requires fresh v2 finance archives; separately replays every case in CKB-VM"]
fn archived_financial_transactions_replay_from_bound_bytes_in_fresh_contexts() {
    // Validate every immutable byte snapshot before spending VM work on any
    // case, including when an evidence producer has not finished its archive.
    let archives = archives()
        .into_iter()
        .map(|directory| {
            let archive = load_archive(&directory).unwrap_or_else(|error| panic!("{}: {error}", directory.display()));
            (directory, archive)
        })
        .collect::<Vec<_>>();
    for (directory, archive) in archives {
        let report = archive.replay_all(100_000_000).unwrap_or_else(|error| panic!("{}: {error}", directory.display()));
        assert_eq!(report["cases"].as_array().unwrap().len(), archive.cases().len());
        std::fs::write(directory.join("fresh-vm-replay.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        eprintln!("{}: {} cases replayed from {}", directory.display(), archive.cases().len(), archive.identity());
    }
}

fn snapshot(directory: &Path) -> (tempfile::TempDir, Value) {
    let report: Value = serde_json::from_slice(&std::fs::read(directory.join("report.json")).unwrap()).unwrap();
    let destination = tempfile::tempdir().unwrap();
    for program in report["programs"].as_array().expect("v2 archive programs") {
        for file in program["files"].as_object().unwrap().values() {
            let name = file["file"].as_str().unwrap();
            std::fs::copy(directory.join(name), destination.path().join(name)).unwrap();
        }
    }
    (destination, report)
}

fn write_report(directory: &Path, mut report: Value) {
    for case in report["cases"].as_array_mut().unwrap() {
        case["case_id"] = json!(format::case_identity(case).unwrap());
    }
    report["archive_id"] = json!(format::archive_identity(&report).unwrap());
    std::fs::write(directory.join("report.json"), serde_json::to_vec(&report).unwrap()).unwrap();
}

fn rejected(directory: &Path, stage: &str) {
    let error = match load_archive(directory) {
        Ok(_) => panic!("tampered archive passed"),
        Err(error) => error,
    };
    assert_eq!(error.stage, stage, "{error}");
}

#[test]
#[ignore = "requires a v2 archived real transaction; tests hash, resolution and actual VM rejection"]
fn archived_transaction_input_dependency_and_source_tampering_fail_closed() {
    let source = archives()
        .into_iter()
        .find(|directory| {
            std::fs::read(directory.join("report.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|report| report["schema"] == format::SCHEMA)
        })
        .expect("produce a v2 archive first");
    let original = load_archive(&source).unwrap();
    assert!(!original.programs().is_empty());
    let (dir, mut report) = snapshot(&source);
    report["cases"][0]["transaction"] = json!("gg");
    write_report(dir.path(), report);
    rejected(dir.path(), "parse");
    let (dir, mut report) = snapshot(&source);
    report["cases"][0]["transaction_hash"] = json!("00".repeat(32));
    write_report(dir.path(), report);
    rejected(dir.path(), "integrity");
    let (dir, mut report) = snapshot(&source);
    report["cases"][0]["inputs"][0]["data"] = json!("00");
    write_report(dir.path(), report);
    rejected(dir.path(), "integrity");
    let (dir, mut report) = snapshot(&source);
    report["dependencies"][0]["data"] = json!("00");
    write_report(dir.path(), report);
    rejected(dir.path(), "integrity");
    let (dir, mut report) = snapshot(&source);
    report["cases"][0]["inputs"].as_array_mut().unwrap().clear();
    write_report(dir.path(), report);
    rejected(dir.path(), "resolution");
    let (dir, mut report) = snapshot(&source);
    report["dependencies"].as_array_mut().unwrap().pop();
    write_report(dir.path(), report);
    rejected(dir.path(), "resolution");
    let (dir, mut report) = snapshot(&source);
    let file = report["programs"][0]["files"]["sources.json"]["file"].as_str().unwrap().to_string();
    let mut sources: Value = serde_json::from_slice(&std::fs::read(dir.path().join(&file)).unwrap()).unwrap();
    sources["units"][0]["bytes"] = json!("00");
    let bytes = serde_json::to_vec(&sources).unwrap();
    std::fs::write(dir.path().join(&file), &bytes).unwrap();
    report["programs"][0]["files"]["sources.json"]["hash"] = json!(format::hash(&bytes));
    report["programs"][0]["files"]["sources.json"]["size_bytes"] = json!(bytes.len());
    write_report(dir.path(), report);
    rejected(dir.path(), "integrity");
    let (dir, report) = snapshot(&source);
    let file = report["programs"][0]["files"]["elf"]["file"].as_str().unwrap();
    std::fs::write(dir.path().join(file), [0u8]).unwrap();
    write_report(dir.path(), report);
    rejected(dir.path(), "integrity");
    let (dir, mut report) = snapshot(&source);
    let case = &mut report["cases"][0];
    assert_eq!(case["outcome"], "accepted");
    let raw = hex::decode(case["transaction"].as_str().unwrap()).unwrap();
    let tx = packed::Transaction::from_slice(&raw).unwrap().into_view();
    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    let witness = packed::WitnessArgs::from_slice(&witnesses[0].raw_data())
        .unwrap()
        .as_builder()
        .lock(Some(Bytes::from(vec![0; 65])).pack())
        .build();
    witnesses[0] = witness.as_bytes().pack();
    let tx = tx.as_advanced_builder().set_witnesses(witnesses).build();
    case["transaction"] = json!(hex::encode(tx.data().as_slice()));
    case["transaction_bytes"] = json!(tx.data().as_slice().len());
    case["transaction_hash"] = json!(hex::encode(tx.hash().as_slice()));
    case["serialized_transaction_hash"] = json!(format::hash(tx.data().as_slice()));
    case["witness_bytes"] = json!(tx.witnesses().into_iter().map(|w| w.raw_data().len()).sum::<usize>());
    write_report(dir.path(), report);
    let archive = load_archive(dir.path()).expect("all checksum identities were rebound");
    let error = archive.replay_case(0, 100_000_000).expect_err("rehashed signature substitution must fail in the actual VM");
    assert!(matches!(error, ArchiveError { stage: "vm", .. }), "{error}");
    let (dir, mut report) = snapshot(&source);
    let index = report["cases"].as_array().unwrap().iter().position(|case| case["outcome"] == "rejected").unwrap();
    report["cases"][index]["expected_groups"] = json!(["Inputs[999].Lock"]);
    write_report(dir.path(), report);
    let archive = load_archive(dir.path()).expect("the forged expectation has consistent checksum identities");
    let error = archive.replay_case(index, 100_000_000).expect_err("a real policy rejection must match its expected group");
    assert!(matches!(error, ArchiveError { stage: "expectation", .. }), "{error}");
}
