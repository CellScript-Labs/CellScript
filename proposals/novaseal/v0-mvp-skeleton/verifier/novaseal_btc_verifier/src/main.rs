use std::{env, fs, process};

use novaseal_btc_verifier::{decode_fixed_hex, decode_hex, verify_bip340_message32, verify_ipc_blob};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct VectorReport {
    positive: Vec<VectorCase>,
    negative: Vec<VectorCase>,
}

#[derive(Debug, Deserialize)]
struct VectorCase {
    id: String,
    message32: String,
    xonly_pubkey: String,
    signature64: String,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct IpcVectorReport {
    vectors: Vec<IpcVectorCase>,
    malformed: Vec<IpcVectorCase>,
}

#[derive(Debug, Deserialize)]
struct IpcVectorCase {
    id: String,
    ipc_blob: String,
    expected: String,
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let result = match args.get(1).map(String::as_str) {
        Some("verify") => verify_command(&args[2..]),
        Some("verify-vectors") => verify_vectors_command(&args[2..]),
        Some("verify-ipc") => verify_ipc_command(&args[2..]),
        Some("verify-ipc-vectors") => verify_ipc_vectors_command(&args[2..]),
        _ => Err(usage()),
    };

    match result {
        Ok(()) => process::exit(0),
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    }
}

fn usage() -> String {
    "usage:\n  novaseal_btc_verifier verify --message32 <hex> --xonly-pubkey <hex> --signature64 <hex>\n  novaseal_btc_verifier verify-vectors --vectors <path>\n  novaseal_btc_verifier verify-ipc --blob <hex>\n  novaseal_btc_verifier verify-ipc-vectors --vectors <path>"
        .to_string()
}

fn flag_value(args: &[String], flag: &str) -> Result<String, String> {
    args.windows(2).find_map(|window| (window[0] == flag).then(|| window[1].clone())).ok_or_else(|| format!("missing {flag}"))
}

fn verify_command(args: &[String]) -> Result<(), String> {
    let message = decode_fixed_hex(&flag_value(args, "--message32")?, 32)?;
    let pubkey = decode_fixed_hex(&flag_value(args, "--xonly-pubkey")?, 32)?;
    let signature = decode_fixed_hex(&flag_value(args, "--signature64")?, 64)?;

    match verify_bip340_message32(&message, &pubkey, &signature) {
        Ok(()) => {
            println!("{}", json!({"status": "accept"}));
            Ok(())
        }
        Err(err) => {
            println!("{}", json!({"status": "reject", "reason": err.to_string()}));
            Err("verification rejected".to_string())
        }
    }
}

fn verify_ipc_command(args: &[String]) -> Result<(), String> {
    let blob = decode_hex(&flag_value(args, "--blob")?)?;

    match verify_ipc_blob(&blob) {
        Ok(()) => {
            println!("{}", json!({"status": "accept"}));
            Ok(())
        }
        Err(err) => {
            println!("{}", json!({"status": "reject", "reason": err.to_string()}));
            Err("verification rejected".to_string())
        }
    }
}

fn verify_vectors_command(args: &[String]) -> Result<(), String> {
    let path = flag_value(args, "--vectors")?;
    let contents = fs::read_to_string(&path).map_err(|err| format!("failed to read {path}: {err}"))?;
    let report = serde_json::from_str::<VectorReport>(&contents).map_err(|err| format!("invalid vector report: {err}"))?;

    let mut checked = 0usize;
    let mut matched = 0usize;
    let mut mismatches = Vec::new();

    for case in report.positive.iter().chain(report.negative.iter()) {
        checked += 1;
        let actual_accept = verify_case(case).unwrap_or(false);
        let expected_accept = case.expected == "accept";
        if actual_accept == expected_accept {
            matched += 1;
        } else {
            mismatches.push(json!({
                "id": case.id,
                "expected": case.expected,
                "actual": if actual_accept { "accept" } else { "reject" },
            }));
        }
    }

    println!(
        "{}",
        json!({
            "status": if mismatches.is_empty() { "ok" } else { "mismatch" },
            "checked": checked,
            "matched": matched,
            "mismatches": mismatches,
        })
    );

    if mismatches.is_empty() { Ok(()) } else { Err("vector verification mismatches".to_string()) }
}

fn verify_ipc_vectors_command(args: &[String]) -> Result<(), String> {
    let path = flag_value(args, "--vectors")?;
    let contents = fs::read_to_string(&path).map_err(|err| format!("failed to read {path}: {err}"))?;
    let report = serde_json::from_str::<IpcVectorReport>(&contents).map_err(|err| format!("invalid IPC vector report: {err}"))?;

    let mut checked = 0usize;
    let mut matched = 0usize;
    let mut mismatches = Vec::new();

    for case in report.vectors.iter().chain(report.malformed.iter()) {
        checked += 1;
        let actual_accept = verify_ipc_case(case).unwrap_or(false);
        let expected_accept = case.expected == "accept";
        if actual_accept == expected_accept {
            matched += 1;
        } else {
            mismatches.push(json!({
                "id": case.id,
                "expected": case.expected,
                "actual": if actual_accept { "accept" } else { "reject" },
            }));
        }
    }

    println!(
        "{}",
        json!({
            "status": if mismatches.is_empty() { "ok" } else { "mismatch" },
            "checked": checked,
            "matched": matched,
            "mismatches": mismatches,
        })
    );

    if mismatches.is_empty() { Ok(()) } else { Err("IPC vector verification mismatches".to_string()) }
}

fn verify_case(case: &VectorCase) -> Result<bool, String> {
    let message = decode_fixed_hex(&case.message32, 32)?;
    let pubkey = decode_fixed_hex(&case.xonly_pubkey, 32)?;
    let signature = decode_fixed_hex(&case.signature64, 64)?;
    Ok(verify_bip340_message32(&message, &pubkey, &signature).is_ok())
}

fn verify_ipc_case(case: &IpcVectorCase) -> Result<bool, String> {
    let blob = decode_hex(&case.ipc_blob)?;
    Ok(verify_ipc_blob(&blob).is_ok())
}
