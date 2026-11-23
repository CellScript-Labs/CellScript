use super::deployment_status_violation;
use crate::error::Result;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub(super) fn verify_live_deployments(
    deployed: &crate::package::DeployedManifest,
    rpc_url: &str,
    network_filter: Option<&str>,
    violations: &mut Vec<String>,
) -> Result<serde_json::Value> {
    let chain_info = ckb_rpc_call(rpc_url, "get_blockchain_info", serde_json::json!([]))?;
    let chain = chain_info.get("chain").or_else(|| chain_info.get("chain_id")).and_then(|value| value.as_str()).map(str::to_string);
    let mut evidence = Vec::new();
    let mut checked = 0usize;

    for deployment in &deployed.deployments {
        if network_filter.is_some_and(|network| network != deployment.network) {
            continue;
        }
        checked += 1;
        let mut deployment_violations = Vec::new();
        if let Some(violation) = deployment_status_violation(deployment) {
            deployment_violations.push(violation);
        }

        match chain.as_deref() {
            Some(chain) if chain_id_matches(chain, &deployment.chain_id) => {}
            Some(chain) => deployment_violations.push(format!(
                "chain_id mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                deployment.network, chain, deployment.chain_id
            )),
            None => deployment_violations.push("RPC get_blockchain_info did not return chain".to_string()),
        }

        let out_point = serde_json::json!({
            "tx_hash": deployment.tx_hash,
            "index": format!("0x{:x}", deployment.output_index),
        });
        let live = ckb_rpc_call(rpc_url, "get_live_cell", serde_json::json!([out_point, true]))?;
        let rpc_status = live.get("status").and_then(|value| value.as_str()).unwrap_or("unknown").to_string();
        if rpc_status != "live" {
            deployment_violations.push(format!(
                "deployment for network '{}' is not live at {}: RPC status '{}'",
                deployment.network, deployment.out_point, rpc_status
            ));
        }

        let rpc_data_hash = live_cell_data_hash(&live);
        match rpc_data_hash.as_deref() {
            Some(hash) if hex_eq(hash, &deployment.data_hash) => {}
            Some(hash) => deployment_violations.push(format!(
                "live data_hash mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                deployment.network, hash, deployment.data_hash
            )),
            None => deployment_violations
                .push(format!("RPC get_live_cell for network '{}' did not return cell.data.hash", deployment.network)),
        }

        let rpc_code_hash =
            live_cell_code_hash_for_deployment(&live, deployment, rpc_data_hash.as_deref(), &mut deployment_violations);
        if let Some(hash) = rpc_code_hash.as_deref()
            && !hex_eq(hash, &deployment.code_hash)
        {
            deployment_violations.push(format!(
                "live code_hash mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                deployment.network, hash, deployment.code_hash
            ));
        }

        if let Some(type_id) = &deployment.type_id {
            let rpc_type_args = live_cell_type_script(&live).and_then(|script| script.get("args")).and_then(|value| value.as_str());
            match rpc_type_args {
                Some(args) if hex_eq(args, type_id) => {}
                Some(args) => deployment_violations.push(format!(
                    "type_id mismatch for network '{}': RPC type args '{}', Deployed.toml has '{}'",
                    deployment.network, args, type_id
                )),
                None => deployment_violations.push(format!(
                    "deployment for network '{}' declares type_id but live cell has no type script args",
                    deployment.network
                )),
            }
        }

        for violation in &deployment_violations {
            if !violations.contains(violation) {
                violations.push(violation.clone());
            }
        }
        evidence.push(serde_json::json!({
            "network": deployment.network,
            "chain_id": deployment.chain_id,
            "deployment_status": deployment.status.as_ref(),
            "out_point": deployment.out_point,
            "rpc_status": rpc_status,
            "status": if deployment_violations.is_empty() { "live-verified" } else { "failed" },
            "expected_data_hash": deployment.data_hash,
            "rpc_data_hash": rpc_data_hash,
            "expected_code_hash": deployment.code_hash,
            "rpc_code_hash": rpc_code_hash,
            "hash_type": deployment.hash_type,
            "violations": deployment_violations,
        }));
    }

    if checked == 0 {
        violations.push(match network_filter {
            Some(network) => format!("no deployment record found for requested live network '{}'", network),
            None => "no deployment records found for live verification".to_string(),
        });
    }

    Ok(serde_json::json!({
        "enabled": true,
        "rpc_url": rpc_url,
        "network": network_filter,
        "chain": chain,
        "checked": checked,
        "evidence": evidence,
    }))
}

fn live_cell_data_hash(live: &serde_json::Value) -> Option<String> {
    live.pointer("/cell/data/hash").or_else(|| live.pointer("/cell/data_hash")).and_then(|value| value.as_str()).map(str::to_string)
}

fn live_cell_type_script(live: &serde_json::Value) -> Option<&serde_json::Value> {
    let script = live.pointer("/cell/output/type")?;
    (!script.is_null()).then_some(script)
}

fn live_cell_code_hash_for_deployment(
    live: &serde_json::Value,
    deployment: &crate::package::DeploymentRecord,
    rpc_data_hash: Option<&str>,
    violations: &mut Vec<String>,
) -> Option<String> {
    match normalize_hash_type(&deployment.hash_type).as_deref() {
        Some("data" | "data1" | "data2") => rpc_data_hash.map(str::to_string),
        Some("type") => {
            let Some(script) = live_cell_type_script(live) else {
                violations.push(format!(
                    "deployment for network '{}' uses hash_type 'type' but live cell has no type script",
                    deployment.network
                ));
                return None;
            };
            match ckb_script_hash_from_json(script) {
                Ok(hash) => Some(hash),
                Err(error) => {
                    violations
                        .push(format!("failed to compute live type script hash for network '{}': {}", deployment.network, error));
                    None
                }
            }
        }
        Some(other) => {
            violations.push(format!("unsupported deployment hash_type '{}' for live verification", other));
            None
        }
        None => {
            violations.push("deployment hash_type is empty".to_string());
            None
        }
    }
}

pub(in crate::cli) fn ckb_script_hash_from_json(script: &serde_json::Value) -> Result<String> {
    let code_hash = script
        .get("code_hash")
        .and_then(|value| value.as_str())
        .ok_or_else(|| crate::error::CompileError::without_span("script has no code_hash"))?;
    let hash_type = script
        .get("hash_type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| crate::error::CompileError::without_span("script has no hash_type"))?;
    let args = script.get("args").and_then(|value| value.as_str()).unwrap_or("0x");

    let code_hash_bytes = hex::decode(code_hash.trim_start_matches("0x"))
        .map_err(|error| crate::error::CompileError::without_span(format!("invalid script code_hash: {}", error)))?;
    if code_hash_bytes.len() != 32 {
        return Err(crate::error::CompileError::without_span(format!(
            "script code_hash must be 32 bytes, got {}",
            code_hash_bytes.len()
        )));
    }
    let hash_type_byte = ckb_hash_type_byte(hash_type)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("unsupported script hash_type '{}'", hash_type)))?;
    let args_bytes = hex::decode(args.trim_start_matches("0x"))
        .map_err(|error| crate::error::CompileError::without_span(format!("invalid script args: {}", error)))?;

    let mut args_molecule = Vec::with_capacity(4 + args_bytes.len());
    args_molecule.extend_from_slice(&(args_bytes.len() as u32).to_le_bytes());
    args_molecule.extend_from_slice(&args_bytes);

    let header_size = 4 + 4 * 3;
    let field_sizes = [32usize, 1usize, args_molecule.len()];
    let mut cursor = header_size;
    let mut offsets = Vec::with_capacity(3);
    for size in field_sizes {
        offsets.push(cursor);
        cursor += size;
    }

    let mut serialized = Vec::with_capacity(cursor);
    serialized.extend_from_slice(&(cursor as u32).to_le_bytes());
    for offset in offsets {
        serialized.extend_from_slice(&(offset as u32).to_le_bytes());
    }
    serialized.extend_from_slice(&code_hash_bytes);
    serialized.push(hash_type_byte);
    serialized.extend_from_slice(&args_molecule);

    Ok(format!("0x{}", crate::hex_encode(&crate::ckb_blake2b256(&serialized))))
}

fn ckb_rpc_call(rpc_url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let endpoint = parse_http_rpc_url(rpc_url)?;
    let body = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    }))
    .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON-RPC request: {}", error)))?;

    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to connect to CKB RPC '{}': {}", rpc_url, error)))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint.path,
        endpoint.host_header,
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to write CKB RPC request '{}': {}", method, error))
    })?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read CKB RPC response '{}': {}", method, error))
    })?;
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return Err(crate::error::CompileError::without_span("CKB RPC returned malformed HTTP response"));
    };
    let status_line = headers.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        return Err(crate::error::CompileError::without_span(format!("CKB RPC '{}' returned HTTP status '{}'", method, status_line)));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse CKB RPC response '{}': {}", method, error))
    })?;
    if let Some(error) = value.get("error") {
        return Err(crate::error::CompileError::without_span(format!("CKB RPC '{}' failed: {}", method, error)));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| crate::error::CompileError::without_span(format!("CKB RPC '{}' returned no result", method)))
}

struct HttpRpcEndpoint {
    host: String,
    host_header: String,
    port: u16,
    path: String,
}

fn parse_http_rpc_url(url: &str) -> Result<HttpRpcEndpoint> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| crate::error::CompileError::without_span("only http:// CKB RPC URLs are supported"))?;
    let (host_port, path) = rest.split_once('/').map_or((rest, "/"), |(host_port, path)| (host_port, path));
    let path = if path == "/" { "/".to_string() } else { format!("/{path}") };
    let (host, port) = if let Some((host, port)) = host_port.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|error| crate::error::CompileError::without_span(format!("invalid CKB RPC port '{}': {}", port, error)))?;
        (host.to_string(), port)
    } else {
        (host_port.to_string(), 80)
    };
    if host.is_empty() {
        return Err(crate::error::CompileError::without_span("CKB RPC host is empty"));
    }
    Ok(HttpRpcEndpoint { host, host_header: host_port.to_string(), port, path })
}

fn chain_id_matches(rpc_chain: &str, expected: &str) -> bool {
    let rpc = normalize_chain_id(rpc_chain);
    let expected = normalize_chain_id(expected);
    rpc == expected || (rpc == "ckb" && expected == "ckb-mainnet") || (rpc == "ckb-mainnet" && expected == "ckb")
}

fn normalize_chain_id(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn hex_eq(left: &str, right: &str) -> bool {
    left.trim_start_matches("0x").eq_ignore_ascii_case(right.trim_start_matches("0x"))
}

fn normalize_hash_type(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn ckb_hash_type_byte(value: &str) -> Option<u8> {
    match normalize_hash_type(value)?.as_str() {
        "data" => Some(0),
        "type" => Some(1),
        "data1" => Some(2),
        "data2" => Some(4),
        _ => None,
    }
}
