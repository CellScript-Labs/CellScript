use crate::evidence::{chain_identity_matches, node_commit_matches_revision};
use crate::{FiberUdtArgInfo, RegistrationReportV1};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfoSnapshot {
    pub version: String,
    pub commit_hash: String,
    pub pubkey: String,
    pub node_name: Option<String>,
    pub chain_hash: String,
    pub udt_cfg_infos: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNodeRegistrationEvidence {
    pub node: NodeInfoSnapshot,
    pub exact_local_config_observed: bool,
    pub exact_signed_announcement_observed: bool,
}

#[derive(Debug, Clone)]
pub struct FiberRpcClient {
    url: String,
    client: reqwest::blocking::Client,
}

impl FiberRpcClient {
    pub fn trusted_local(url: impl Into<String>) -> anyhow::Result<Self> {
        let url = url.into();
        let parsed = reqwest::Url::parse(&url)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            anyhow::bail!("Fiber RPC URL must use http or https");
        }
        let host = parsed.host_str().ok_or_else(|| anyhow::anyhow!("Fiber RPC URL has no host"))?;
        let trusted = host == "localhost" || host == "::1" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !trusted {
            anyhow::bail!("refusing unauthenticated non-loopback Fiber RPC endpoint '{url}'");
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self { url, client })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn node_info(&self) -> anyhow::Result<NodeInfoSnapshot> {
        let result = self.rpc("node_info", json!([]))?;
        Ok(NodeInfoSnapshot {
            version: string_field(&result, "version")?,
            commit_hash: string_field(&result, "commit_hash")?,
            pubkey: string_field(&result, "pubkey")?,
            node_name: result.get("node_name").and_then(Value::as_str).map(str::to_string),
            chain_hash: string_field(&result, "chain_hash")?,
            udt_cfg_infos: udt_array(&result)?.clone(),
        })
    }

    pub fn graph_nodes(&self) -> anyhow::Result<Vec<Value>> {
        let result = self.rpc("graph_nodes", json!([{"limit": "0x64", "after": null}]))?;
        Ok(result
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Fiber graph_nodes result omitted nodes"))?
            .clone())
    }

    fn rpc(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({"id": 1, "jsonrpc": "2.0", "method": method, "params": params}))
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        if let Some(error) = response.get("error") {
            anyhow::bail!("Fiber JSON-RPC method {method} failed: {error}");
        }
        response.get("result").cloned().ok_or_else(|| anyhow::anyhow!("Fiber JSON-RPC method {method} returned no result"))
    }
}

pub fn verify_local_registration(
    client: &FiberRpcClient,
    expected: &FiberUdtArgInfo,
    binding_fingerprint: &str,
    configuration_hash: &str,
    expected_fiber_revision: &str,
    expected_ckb_identity: &str,
) -> anyhow::Result<(LocalNodeRegistrationEvidence, RegistrationReportV1)> {
    expected.validate()?;
    if expected_ckb_identity.len() != 66 || !expected_ckb_identity.starts_with("0x") {
        anyhow::bail!(
            "live Fiber registration requires the exact 0x-prefixed CKB genesis hash; a CKB Git revision cannot be verified through Fiber node_info"
        );
    }
    let node = client.node_info()?;
    if !node_commit_matches_revision(&node.commit_hash, expected_fiber_revision) {
        anyhow::bail!("Fiber node commit '{}' does not match pinned revision {}", node.commit_hash, expected_fiber_revision);
    }
    if !chain_identity_matches(&node.chain_hash, expected_ckb_identity) {
        anyhow::bail!(
            "Fiber node chain hash {} does not match pinned CKB genesis identity {}",
            node.chain_hash,
            expected_ckb_identity
        );
    }
    let exact_local_config_observed = node.udt_cfg_infos.iter().any(|observed| udt_semantically_equal(observed, expected));
    if !exact_local_config_observed {
        anyhow::bail!("restarted Fiber node_info does not report the exact generated UDT configuration");
    }
    let graph_nodes = client.graph_nodes()?;
    let exact_signed_announcement_observed = graph_nodes.iter().any(|graph_node| {
        graph_node.get("pubkey").and_then(Value::as_str) == Some(node.pubkey.as_str())
            && udt_array(graph_node).is_ok_and(|configs| configs.iter().any(|observed| udt_semantically_equal(observed, expected)))
    });
    if !exact_signed_announcement_observed {
        anyhow::bail!("local node's signed graph announcement has not converged with the exact generated UDT configuration");
    }
    let evidence =
        LocalNodeRegistrationEvidence { node: node.clone(), exact_local_config_observed, exact_signed_announcement_observed };
    let report = RegistrationReportV1 {
        schema: RegistrationReportV1::schema(),
        status: crate::OperationalState::LocalNodeAdvertised,
        binding_fingerprint: binding_fingerprint.to_string(),
        fiber_rpc_url: client.url().to_string(),
        trusted_local_rpc: true,
        node_version: node.version,
        node_commit_hash: node.commit_hash,
        node_pubkey: node.pubkey,
        chain_hash: node.chain_hash,
        exact_udt_observed: true,
        signed_announcement_observed: true,
        configuration_hash: configuration_hash.to_string(),
    };
    Ok((evidence, report))
}

fn udt_semantically_equal(observed: &Value, expected: &FiberUdtArgInfo) -> bool {
    if observed.get("name").and_then(Value::as_str) != Some(expected.name.as_str()) {
        return false;
    }
    let Some(script) = observed.get("script") else {
        return false;
    };
    if normalize_hex_value(script.get("code_hash")) != Some(expected.script.code_hash.to_ascii_lowercase())
        || script.get("hash_type").and_then(Value::as_str).map(str::to_ascii_lowercase)
            != Some(expected.script.hash_type.to_ascii_lowercase())
        || script.get("args").and_then(Value::as_str) != Some(expected.script.args.as_str())
        || parse_u128(observed.get("auto_accept_amount")) != expected.auto_accept_amount
    {
        return false;
    }
    let Some(dependencies) = observed.get("cell_deps").and_then(Value::as_array) else {
        return false;
    };
    dependencies_semantically_equal(dependencies, &expected.cell_deps)
}

fn dependencies_semantically_equal(observed: &[Value], expected: &[crate::FiberUdtDep]) -> bool {
    if observed.len() != expected.len() {
        return false;
    }
    let mut matched = vec![false; observed.len()];
    expected.iter().all(|expected_dep| {
        let Some(index) = observed
            .iter()
            .enumerate()
            .position(|(index, observed_dep)| !matched[index] && dependency_equal(observed_dep, expected_dep))
        else {
            return false;
        };
        matched[index] = true;
        true
    })
}

fn dependency_equal(observed: &Value, expected: &crate::FiberUdtDep) -> bool {
    match (&expected.cell_dep, &expected.type_id) {
        (Some(expected), None) => {
            if observed.get("type_id").is_some_and(|value| !value.is_null()) {
                return false;
            }
            let Some(cell_dep) = observed.get("cell_dep").filter(|value| !value.is_null()) else {
                return false;
            };
            let Some(out_point) = cell_dep.get("out_point") else {
                return false;
            };
            normalize_hex_value(out_point.get("tx_hash")) == Some(expected.out_point.tx_hash.to_ascii_lowercase())
                && normalize_hex_number(out_point.get("index"))
                    == normalize_hex_number(Some(&Value::String(expected.out_point.index.clone())))
                && cell_dep.get("dep_type").and_then(Value::as_str).map(str::to_ascii_lowercase)
                    == Some(expected.dep_type.to_ascii_lowercase())
        }
        (None, Some(expected)) => {
            if observed.get("cell_dep").is_some_and(|value| !value.is_null()) {
                return false;
            }
            let Some(type_id) = observed.get("type_id").filter(|value| !value.is_null()) else {
                return false;
            };
            normalize_hex_value(type_id.get("code_hash")) == Some(expected.code_hash.to_ascii_lowercase())
                && type_id.get("hash_type").and_then(Value::as_str).map(str::to_ascii_lowercase)
                    == Some(expected.hash_type.to_ascii_lowercase())
                && normalize_hex_value(type_id.get("args")) == Some(expected.args.to_ascii_lowercase())
        }
        _ => false,
    }
}

fn udt_array(value: &Value) -> anyhow::Result<&Vec<Value>> {
    let value = value.get("udt_cfg_infos").ok_or_else(|| anyhow::anyhow!("Fiber result omitted udt_cfg_infos"))?;
    if let Some(array) = value.as_array() {
        return Ok(array);
    }
    value.get("0").and_then(Value::as_array).ok_or_else(|| anyhow::anyhow!("Fiber udt_cfg_infos is not an array"))
}

fn string_field(value: &Value, field: &str) -> anyhow::Result<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string).ok_or_else(|| anyhow::anyhow!("Fiber result omitted {field}"))
}

fn normalize_hex_value(value: Option<&Value>) -> Option<String> {
    value?.as_str().map(str::to_ascii_lowercase)
}

fn normalize_hex_number(value: Option<&Value>) -> Option<u128> {
    parse_u128(value)
}

fn parse_u128(value: Option<&Value>) -> Option<u128> {
    match value? {
        Value::Null => None,
        Value::String(value) => value.strip_prefix("0x").and_then(|value| u128::from_str_radix(value, 16).ok()),
        Value::Number(value) => value.as_u64().map(u128::from),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn semantic_config_comparison_accepts_fiber_hex_quantities() {
        let expected = FiberUdtArgInfo {
            name: "asset".to_string(),
            script: crate::FiberScriptConfig {
                code_hash: format!("0x{}", "11".repeat(32)),
                hash_type: "data2".to_string(),
                args: "^0x22$".to_string(),
            },
            auto_accept_amount: Some(42),
            cell_deps: vec![crate::FiberUdtDep {
                cell_dep: Some(crate::FiberCellDep {
                    out_point: crate::fiber_config::FiberOutPoint {
                        tx_hash: format!("0x{}", "33".repeat(32)),
                        index: "0x0".to_string(),
                    },
                    dep_type: "code".to_string(),
                }),
                type_id: None,
            }],
        };
        let mut observed = json!({
            "name": "asset",
            "script": {"code_hash": expected.script.code_hash, "hash_type": "Data2", "args": "^0x22$"},
            "auto_accept_amount": "0x2a",
            "cell_deps": [{
                "cell_dep": {"out_point": {"tx_hash": format!("0x{}", "33".repeat(32)), "index": "0x0"}, "dep_type": "code"},
                "type_id": null
            }]
        });
        assert!(udt_semantically_equal(&observed, &expected));

        observed["cell_deps"][0]["type_id"] = json!({
            "code_hash": format!("0x{}", "44".repeat(32)),
            "hash_type": "type",
            "args": format!("0x{}", "55".repeat(32)),
        });
        assert!(!udt_semantically_equal(&observed, &expected));
    }

    #[test]
    fn remote_unauthenticated_rpc_is_rejected() {
        assert!(FiberRpcClient::trusted_local("http://example.com:8227").is_err());
        assert!(FiberRpcClient::trusted_local("ftp://127.0.0.1:8227").is_err());
    }

    #[test]
    fn trusted_local_rpc_does_not_follow_redirects() {
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let target_url = format!("http://{}", target.local_addr().unwrap());
        let redirect = TcpListener::bind("127.0.0.1:0").unwrap();
        let redirect_url = format!("http://{}", redirect.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = redirect.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });

        let client = FiberRpcClient::trusted_local(redirect_url).unwrap();
        assert!(client.rpc("node_info", json!([])).is_err());
        server.join().unwrap();
        assert!(matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    }
}
