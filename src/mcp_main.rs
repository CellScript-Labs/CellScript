use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_DOC_BYTES_PER_FILE: usize = 12_000;

fn main() {
    if let Err(error) = run_stdio_server() {
        eprintln!("cellscript-mcp error: {error}");
        std::process::exit(1);
    }
}

fn run_stdio_server() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_json_rpc(request),
            Err(error) => Some(json_rpc_error(Value::Null, -32700, format!("parse error: {error}"))),
        };
        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_json_rpc(request: Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();

    let result = match method {
        "initialize" => Ok(initialize_result()),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_specs() })),
        "tools/call" => handle_tools_call(request.get("params").cloned().unwrap_or(Value::Null)),
        _ => Err((-32601, format!("unknown method '{method}'"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json_rpc_error(id, code, message),
    })
}

fn json_rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "serverInfo": {
            "name": "cellscript-mcp",
            "version": cellscript::VERSION
        },
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        }
    })
}

fn handle_tools_call(params: Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| (-32602, "tools/call requires params.name".to_string()))?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let result = match name {
        "cellscript_command_tree" => command_tree_tool(arguments),
        "cellscript_check" => check_tool(arguments),
        "cellscript_constraints" => constraints_tool(arguments),
        "cellscript_metadata" => metadata_tool(arguments),
        "cellscript_template_layouts" => template_layouts_tool(arguments),
        "cellscript_protocol_graph" => protocol_graph_tool(arguments),
        "cellscript_explain" => explain_tool(arguments),
        "cellscript_gate_policy" => gate_policy_tool(arguments),
        "cellscript_docs_topic" => docs_topic_tool(arguments),
        "cellscript_evidence_levels" => Ok(evidence_levels_tool()),
        other => Err(anyhow::anyhow!("unknown CellScript MCP tool '{other}'")),
    };
    Ok(match result {
        Ok(value) => tool_result(value, false),
        Err(error) => tool_result(
            json!({
                "status": "failed",
                "error": error.to_string(),
                "evidence_level": "mcp-boundary",
                "writes": false
            }),
            true,
        ),
    })
}

fn tool_result(structured: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| structured.to_string());
    json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "structuredContent": structured,
        "isError": is_error
    })
}

fn command_tree_tool(_arguments: Value) -> anyhow::Result<Value> {
    let command = cellscript::cli::commands::CliParser::command();
    Ok(json!({
        "status": "ok",
        "evidence_level": "compile-only",
        "writes": false,
        "source": "cellscript::cli::commands::CliParser::command",
        "commands": command
            .get_subcommands()
            .filter(|command| !command.is_hide_set())
            .map(command_node)
            .collect::<Vec<_>>(),
        "legacy_aliases": legacy_aliases()
    }))
}

fn check_tool(arguments: Value) -> anyhow::Result<Value> {
    let mut args = vec!["check".to_string(), "--json".to_string(), "--message-format=json".to_string()];
    if optional_bool(&arguments, "all_targets") {
        args.push("--all-targets".to_string());
    }
    if optional_bool(&arguments, "production") {
        args.push("--production".to_string());
    }
    push_optional_flag(&mut args, &arguments, "target_profile", "--target-profile");
    run_cellc_vec(args, cwd_from_arguments(&arguments)?, "compile-only")
}

fn constraints_tool(arguments: Value) -> anyhow::Result<Value> {
    let mut args = vec!["constraints".to_string(), input_arg(&arguments), "--json".to_string()];
    push_optional_flag(&mut args, &arguments, "target", "--target");
    push_optional_flag(&mut args, &arguments, "target_profile", "--target-profile");
    run_cellc_vec(args, cwd_from_arguments(&arguments)?, "compile-only")
}

fn metadata_tool(arguments: Value) -> anyhow::Result<Value> {
    let mut args = vec!["metadata".to_string(), input_arg(&arguments)];
    push_optional_flag(&mut args, &arguments, "target", "--target");
    push_optional_flag(&mut args, &arguments, "target_profile", "--target-profile");
    run_cellc_vec(args, cwd_from_arguments(&arguments)?, "compile-only")
}

fn template_layouts_tool(arguments: Value) -> anyhow::Result<Value> {
    let report = metadata_tool(arguments)?;
    let metadata = serde_json::from_str::<Value>(report["stdout"].as_str().unwrap_or_default())?;
    Ok(json!({
        "status": report["status"],
        "evidence_level": "compile-only",
        "writes": false,
        "template_layouts": metadata.get("template_layouts").cloned().unwrap_or_else(|| json!([])),
        "metadata_schema_version": metadata.get("metadata_schema_version").cloned().unwrap_or(Value::Null),
        "stdout": report["stdout"],
        "stderr": report["stderr"],
        "exit_code": report["exit_code"]
    }))
}

fn protocol_graph_tool(arguments: Value) -> anyhow::Result<Value> {
    let mut args = vec!["explain".to_string(), "graph".to_string(), input_arg(&arguments), "--json".to_string()];
    push_optional_flag(&mut args, &arguments, "target", "--target");
    push_optional_flag(&mut args, &arguments, "target_profile", "--target-profile");
    run_cellc_vec(args, cwd_from_arguments(&arguments)?, "compile-only")
}

fn explain_tool(arguments: Value) -> anyhow::Result<Value> {
    let code = required_string(&arguments, "code")?;
    run_cellc_vec(vec!["explain".to_string(), code, "--json".to_string()], cwd_from_arguments(&arguments)?, "compile-only")
}

fn gate_policy_tool(_arguments: Value) -> anyhow::Result<Value> {
    let path = PathBuf::from("docs/CELLSCRIPT_GATE_POLICY.md");
    Ok(json!({
        "status": "ok",
        "evidence_level": "project-doc",
        "writes": false,
        "path": path,
        "recommended_commands": [
            "./scripts/cellscript_gate.sh dev",
            "./scripts/cellscript_gate.sh ci",
            "./scripts/cellscript_gate.sh backend",
            "./scripts/cellscript_gate.sh release-quick",
            "./scripts/cellscript_gate.sh release"
        ],
        "content": read_bounded_file(&path)?
    }))
}

fn docs_topic_tool(arguments: Value) -> anyhow::Result<Value> {
    let topic = required_string(&arguments, "topic")?;
    let paths = docs_for_topic(&topic)?;
    let docs = paths
        .iter()
        .map(|path| {
            Ok(json!({
                "path": path,
                "content": read_bounded_file(path)?
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(json!({
        "status": "ok",
        "topic": topic,
        "evidence_level": "project-doc",
        "writes": false,
        "documents": docs
    }))
}

fn evidence_levels_tool() -> Value {
    json!({
        "status": "ok",
        "writes": false,
        "levels": [
            {
                "name": "compile-only",
                "meaning": "Compiler or metadata evidence; no CKB node acceptance is claimed."
            },
            {
                "name": "builder-backed",
                "meaning": "A builder supplied concrete transaction material or assumptions."
            },
            {
                "name": "node-dry-run",
                "meaning": "A CKB node estimated cycles or ran equivalent preflight checks."
            },
            {
                "name": "tx-pool-accepted",
                "meaning": "A CKB node accepted the transaction into the tx-pool."
            },
            {
                "name": "submitted",
                "meaning": "The adapter submitted a transaction hash."
            },
            {
                "name": "externally-attested",
                "meaning": "An external review, audit, or registry attestation is present and must be verified separately."
            }
        ],
        "forbidden_mcp_defaults": [
            "signing",
            "deployment submission",
            "registry mutation",
            "publish",
            "shell or editor configuration changes"
        ]
    })
}

fn command_node(command: &clap::Command) -> Value {
    json!({
        "name": command.get_name(),
        "about": command.get_about().map(|about| about.to_string()).unwrap_or_default(),
        "subcommands": command
            .get_subcommands()
            .filter(|subcommand| !subcommand.is_hide_set())
            .map(command_node)
            .collect::<Vec<_>>()
    })
}

fn legacy_aliases() -> Vec<Value> {
    vec![
        json!({ "legacy": "explain-profile", "canonical": "explain profile" }),
        json!({ "legacy": "explain-proof", "canonical": "explain proof" }),
        json!({ "legacy": "explain-assumptions", "canonical": "explain assumptions" }),
        json!({ "legacy": "explain-generics", "canonical": "explain generics" }),
        json!({ "legacy": "explain-graph", "canonical": "explain graph" }),
        json!({ "legacy": "trace-tx", "canonical": "tx trace" }),
        json!({ "legacy": "validate-tx", "canonical": "tx validate" }),
        json!({ "legacy": "solve-tx", "canonical": "tx solve" }),
        json!({ "legacy": "deploy-plan", "canonical": "deploy plan" }),
        json!({ "legacy": "verify-deploy", "canonical": "deploy verify" }),
        json!({ "legacy": "diff-deploy", "canonical": "deploy diff" }),
        json!({ "legacy": "lock-deps", "canonical": "deploy lock-deps" }),
        json!({ "legacy": "registry-verify", "canonical": "registry verify" }),
        json!({ "legacy": "package-verify", "canonical": "package verify" }),
        json!({ "legacy": "registry-add", "canonical": "registry add" }),
        json!({ "legacy": "login", "canonical": "auth capability create" }),
    ]
}

fn tool_specs() -> Vec<Value> {
    vec![
        tool_spec(
            "cellscript_command_tree",
            "Discover the canonical CellScript command tree and migration-safe command surface.",
            json!({ "type": "object", "additionalProperties": false }),
        ),
        tool_spec(
            "cellscript_check",
            "Run read-only package checking through `cellc check --json --message-format=json`.",
            schema_with_properties(json!({
                "cwd": string_schema("Package directory to check."),
                "target_profile": string_schema("Target profile, for example ckb."),
                "all_targets": { "type": "boolean" },
                "production": { "type": "boolean" }
            })),
        ),
        tool_spec("cellscript_constraints", "Emit compiler constraints JSON for an input without writing files.", input_schema()),
        tool_spec("cellscript_metadata", "Emit CompileMetadata JSON for an input without writing files.", input_schema()),
        tool_spec("cellscript_template_layouts", "Extract TemplateLayout metadata from CompileMetadata.", input_schema()),
        tool_spec("cellscript_protocol_graph", "Derive the cyclic ProtocolGraph audit view as JSON.", input_schema()),
        tool_spec(
            "cellscript_explain",
            "Explain a CellScript diagnostic/runtime code through the existing compiler command.",
            schema_with_properties(json!({
                "cwd": string_schema("Working directory for command execution."),
                "code": string_schema("Diagnostic code, runtime error code, or error name.")
            })),
        ),
        tool_spec(
            "cellscript_gate_policy",
            "Read the project gate policy and recommended validation commands.",
            json!({ "type": "object", "additionalProperties": false }),
        ),
        tool_spec(
            "cellscript_docs_topic",
            "Read bounded repository docs for a CellScript topic.",
            schema_with_properties(json!({
                "topic": {
                    "type": "string",
                    "enum": [
                        "language-basics",
                        "ckb-model",
                        "package-cli",
                        "metadata-audit",
                        "builder-deployment",
                        "diagnostics",
                        "roadmap-0.21"
                    ]
                }
            })),
        ),
        tool_spec(
            "cellscript_evidence_levels",
            "Explain CellScript evidence levels and forbidden MCP defaults.",
            json!({ "type": "object", "additionalProperties": false }),
        ),
    ]
}

fn tool_spec(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn input_schema() -> Value {
    schema_with_properties(json!({
        "cwd": string_schema("Working directory for command execution."),
        "input": string_schema("Input .cell file, package directory, or Cell.toml. Defaults to '.'."),
        "target": string_schema("Target architecture."),
        "target_profile": string_schema("Target profile, for example ckb.")
    }))
}

fn schema_with_properties(properties: Value) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    })
}

fn string_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "description": description
    })
}

fn run_cellc_vec(args: Vec<String>, cwd: Option<PathBuf>, evidence_level: &str) -> anyhow::Result<Value> {
    let mut command = Command::new(cellc_path());
    command.args(&args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let command_line = ["cellc"].into_iter().chain(args.iter().map(String::as_str)).collect::<Vec<_>>();
    Ok(json!({
        "status": if output.status.success() { "ok" } else { "failed" },
        "evidence_level": evidence_level,
        "writes": false,
        "command": command_line,
        "exit_code": output.status.code(),
        "stdout": stdout,
        "stderr": stderr
    }))
}

fn cellc_path() -> PathBuf {
    if let Ok(path) = std::env::var("CELLSCRIPT_CELLC") {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe() {
        let sibling = current.with_file_name(if cfg!(windows) { "cellc.exe" } else { "cellc" });
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("cellc")
}

fn cwd_from_arguments(arguments: &Value) -> anyhow::Result<Option<PathBuf>> {
    Ok(optional_string(arguments, "cwd")?.map(PathBuf::from))
}

fn input_arg(arguments: &Value) -> String {
    optional_string(arguments, "input").unwrap_or(None).unwrap_or_else(|| ".".to_string())
}

fn push_optional_flag(args: &mut Vec<String>, arguments: &Value, key: &str, flag: &str) {
    if let Some(value) = optional_string(arguments, key).unwrap_or(None) {
        args.push(flag.to_string());
        args.push(value);
    }
}

fn optional_bool(arguments: &Value, key: &str) -> bool {
    arguments.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn required_string(arguments: &Value, key: &str) -> anyhow::Result<String> {
    optional_string(arguments, key)?.ok_or_else(|| anyhow::anyhow!("missing required argument '{key}'"))
}

fn optional_string(arguments: &Value, key: &str) -> anyhow::Result<Option<String>> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        _ => Err(anyhow::anyhow!("argument '{key}' must be a string")),
    }
}

fn docs_for_topic(topic: &str) -> anyhow::Result<Vec<PathBuf>> {
    let paths = match topic {
        "language-basics" => vec![
            "docs/wiki/Tutorial-02-Language-Basics.md",
            "docs/wiki/Tutorial-03-Resources-and-Cell-Effects.md",
            "examples/token.cell",
        ],
        "ckb-model" => {
            vec!["docs/wiki/CKB-Glossary.md", "docs/wiki/Tutorial-05-CKB-Target-Profiles.md", "docs/CELLSCRIPT_CKB_STD_COMPAT.md"]
        }
        "package-cli" => vec!["docs/wiki/Tutorial-04-Packages-and-CLI-Workflow.md", "roadmap/CELLSCRIPT_0_21_CLI_UX_PLAN.md"],
        "metadata-audit" => vec![
            "docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md",
            "docs/wiki/Tutorial-11-Scoped-Invariants-and-ProofPlan.md",
        ],
        "builder-deployment" => vec![
            "docs/CELLSCRIPT_CKB_ADAPTER.md",
            "docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md",
            "docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md",
        ],
        "diagnostics" => vec!["docs/wiki/Tutorial-13-Agentic-Loops-and-cellc-mcp.md", "docs/wiki/Tutorial-07-LSP-and-Tooling.md"],
        "roadmap-0.21" => vec!["docs/CELLSCRIPT_0_21_ROADMAP.md", "roadmap/CELLSCRIPT_0_21_CLI_UX_PLAN.md"],
        _ => return Err(anyhow::anyhow!("unknown docs topic '{topic}'")),
    };
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

fn read_bounded_file(path: &Path) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(path)?;
    if content.len() <= MAX_DOC_BYTES_PER_FILE {
        Ok(content)
    } else {
        Ok(format!("{}\n\n[truncated by cellscript-mcp after {} bytes]", &content[..MAX_DOC_BYTES_PER_FILE], MAX_DOC_BYTES_PER_FILE))
    }
}
