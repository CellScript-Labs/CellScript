// CLI mcp behavior; included in the original cli integration target.

#[test]
fn cellscript_mcp_lists_read_only_tools() {
    let responses = run_mcp_messages(vec![
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "clientInfo": { "name": "cellscript-test", "version": "0" },
                "capabilities": {}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "cellscript_command_tree",
                "arguments": {}
            }
        }),
    ]);

    assert_eq!(responses.len(), 3, "unexpected MCP responses: {responses:?}");
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "cellscript-mcp");
    let tools = responses[1]["result"]["tools"].as_array().expect("tools array");
    let names = tools.iter().filter_map(|tool| tool["name"].as_str()).collect::<Vec<_>>();
    assert!(names.contains(&"cellscript_check"), "missing check tool: {names:?}");
    assert!(names.contains(&"cellscript_protocol_graph"), "missing graph tool: {names:?}");
    assert!(names.contains(&"cellscript_evidence_levels"), "missing evidence tool: {names:?}");
    assert!(!names.iter().any(|name| name.contains("sign") || name.contains("submit") || name.contains("publish")), "{names:?}");

    let command_tree = &responses[2]["result"]["structuredContent"];
    assert_eq!(command_tree["source"], "cellscript::cli::commands::CliParser::command");
    let commands = command_tree["commands"].as_array().expect("commands array");
    assert!(commands.iter().any(|command| command["name"] == "explain"), "missing explain group: {command_tree}");
    assert!(commands.iter().any(|command| command["name"] == "tx"), "missing tx group: {command_tree}");
    assert!(commands.iter().any(|command| command["name"] == "deploy"), "missing deploy group: {command_tree}");
    assert!(command_tree["legacy_aliases"]
        .as_array()
        .is_some_and(|aliases| aliases.iter().any(|alias| alias["legacy"] == "validate-tx" && alias["canonical"] == "tx validate")));
}

#[test]
fn cellscript_mcp_reads_the_diagnostics_topic() {
    let responses = run_mcp_messages(vec![serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "cellscript_docs_topic",
            "arguments": { "topic": "diagnostics" }
        }
    })]);

    let result = &responses[0]["result"];
    assert_eq!(result["isError"], false, "unexpected MCP result: {result}");
    let documents = result["structuredContent"]["documents"].as_array().expect("diagnostics documents");
    assert!(documents.iter().any(|document| {
        document["path"].as_str().is_some_and(|path| path.ends_with("Tutorial-13-Agentic-Loops-and-cellscript-mcp.md"))
    }));
}

#[test]
fn cellscript_mcp_reads_the_022_language_and_fiber_topics() {
    for (topic, expected_path) in [
        ("language-0.22", "docs/releases/CELLSCRIPT_0_22_RELEASE_NOTES.md"),
        ("fiber-interop", "examples/fiber/README.md"),
        ("release-0.22", "docs/releases/CELLSCRIPT_0_22_RELEASE_NOTES.md"),
    ] {
        let responses = run_mcp_messages(vec![serde_json::json!({
            "jsonrpc": "2.0",
            "id": topic,
            "method": "tools/call",
            "params": {
                "name": "cellscript_docs_topic",
                "arguments": { "topic": topic }
            }
        })]);
        let result = &responses[0]["result"];
        let documents = result["structuredContent"]["documents"].as_array().expect("0.22 topic documents");
        assert!(
            documents.iter().any(|document| document["path"] == expected_path),
            "missing {expected_path} from {topic}: {documents:?}"
        );
    }
}

#[test]
fn cellscript_mcp_check_tool_preserves_structured_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "mcp-demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module demo::main

resource Token has store {
    amount: u64,
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#,
    )
    .unwrap();

    let responses = run_mcp_messages(vec![
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "cellscript_check",
                "arguments": {
                    "cwd": root.display().to_string(),
                    "target_profile": "ckb"
                }
            }
        }),
    ]);

    let result = &responses[1]["result"];
    assert_eq!(result["isError"], false, "unexpected MCP result: {result}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["status"], "ok", "unexpected tool status: {structured}");
    assert_eq!(structured["evidence_level"], "compile-only");
    assert_eq!(structured["writes"], false);
    assert!(structured["stderr"].as_str().is_some(), "stderr boundary missing: {structured}");
    let stdout = structured["stdout"].as_str().expect("stdout");
    let check_json: serde_json::Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(check_json["status"], "ok", "unexpected check JSON: {check_json}");
}
