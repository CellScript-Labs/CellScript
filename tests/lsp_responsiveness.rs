#![cfg(feature = "lsp")]

use cellscript::lsp::{LspServer, Position};
use std::time::Instant;

#[test]
fn bare_spawn_reports_a_source_error_and_user_spawn_clears_it() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("spawn.cell");
    let source = "module spawn_boundary\naction verify() -> u64 { verification return spawn(\"child\") }";
    std::fs::write(&path, source).unwrap();
    let uri = tower_lsp::lsp_types::Url::from_file_path(&path).unwrap().to_string();
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.into());
    let diagnostics = server.get_diagnostics(&uri);
    let error = diagnostics.iter().find(|diagnostic| diagnostic.code.as_deref() == Some("E1021")).expect("bare spawn diagnostic");
    assert_eq!(error.severity, cellscript::lsp::DiagnosticSeverity::Error);
    assert!(error.message.contains("manifest-bound CellDep spawn"));
    assert_eq!(error.range.start.line, 1);
    server.open_document(
        uri.clone(),
        "module spawn_boundary\nfn spawn(value: u64) -> u64 { return value }\naction verify() -> u64 { verification return spawn(7) }"
            .into(),
    );
    assert!(server.get_diagnostics(&uri).iter().all(|diagnostic| diagnostic.severity != cellscript::lsp::DiagnosticSeverity::Error));
}

#[test]
fn diagnostic_snapshots_use_unsaved_dependency_sources_before_resolution() {
    use cellscript::{compile_path_metadata_with_diagnostics_for_sources, CompileOptions};
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        root.path().join("Cell.toml"),
        "[package]\nname = \"overlay\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let entry = src.join("main.cell");
    let dependency = src.join("dependency.cell");
    std::fs::write(&entry, "module overlay\nuse dependency::value\naction check() -> u64 { return value() }").unwrap();
    let original = "module dependency\npublic fn value() -> u64 { return 7 }";
    std::fs::write(&dependency, original).unwrap();
    let entry = camino::Utf8PathBuf::from_path_buf(entry.canonicalize().unwrap()).unwrap();
    let dependency = camino::Utf8PathBuf::from_path_buf(dependency.canonicalize().unwrap()).unwrap();
    let mut sources = std::collections::BTreeMap::new();
    assert!(!compile_path_metadata_with_diagnostics_for_sources(&entry, &sources, CompileOptions::default()).has_errors());
    sources.insert(dependency.clone(), "module dependency\npublic fn value() -> bool { return true }".into());
    let changed = compile_path_metadata_with_diagnostics_for_sources(&entry, &sources, CompileOptions::default());
    assert!(changed.has_errors());
    assert!(changed.metadata.is_none());
    assert!(changed.diagnostics.iter().any(|diagnostic| diagnostic.file.as_ref() == Some(&entry)));
    sources.insert(dependency.clone(), "module dependency\npublic fn value(".into());
    let invalid = compile_path_metadata_with_diagnostics_for_sources(&entry, &sources, CompileOptions::default());
    assert!(invalid.has_errors());
    assert!(invalid.metadata.is_none());
    assert!(invalid.diagnostics.iter().any(|diagnostic| diagnostic.file.as_ref() == Some(&dependency)));
    assert_eq!(std::fs::read_to_string(&dependency).unwrap(), original);
}

#[test]
fn rapid_incremental_edits_publish_the_latest_version_and_ignore_old_updates() {
    use serde_json::{json, Value};
    use std::io::{BufRead, Read, Write};
    use std::process::{Command, Stdio};

    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_cellc"))
            .arg("--lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut input = child.0.stdin.take().unwrap();
    let output = child.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut output = std::io::BufReader::new(output);
        loop {
            let mut length = None;
            loop {
                let mut line = String::new();
                if output.read_line(&mut line).unwrap() == 0 {
                    return;
                }
                if line.trim().is_empty() {
                    break;
                }
                if let Some(value) = line.strip_prefix("Content-Length:") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let mut bytes = vec![0; length.unwrap()];
            if output.read_exact(&mut bytes).is_err() {
                return;
            }
            if send.send(serde_json::from_slice::<Value>(&bytes).unwrap()).is_err() {
                return;
            }
        }
    });
    let mut write = |message: Value| {
        let bytes = serde_json::to_vec(&message).unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
        input.write_all(&bytes).unwrap();
        input.flush().unwrap();
    };
    let next = || receive.recv_timeout(std::time::Duration::from_secs(30)).expect("LSP response deadline");
    write(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}));
    assert!(next()["result"]["capabilities"]["referencesProvider"].as_bool().unwrap());
    write(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    let uri = "untitled:incremental-revisions";
    write(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"cellscript","version":1,
        "text":"module live\naction value() -> u64 { verification return 0 }\n"}}}));
    for version in 2..=65 {
        write(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":version},"contentChanges":[{
            "range":{"start":{"line":version,"character":0},"end":{"line":version,"character":0}},
            "text":format!("fn value{version}() -> u64 {{ return {version} }}\n")
        }]}}));
    }
    loop {
        let message = next();
        if message["method"] == "textDocument/publishDiagnostics" && message["params"]["version"] == 65 {
            assert_eq!(message["params"]["diagnostics"], json!([]));
            break;
        }
    }
    write(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":64},"contentChanges":[{"text":"malformed stale input"}]}}));
    write(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":uri}}}));
    loop {
        let message = next();
        if message["id"] == 2 {
            let symbols = message["result"].as_array().expect("current document symbols");
            for version in 2..=65 {
                assert!(symbols.iter().any(|symbol| symbol["name"] == format!("value{version}")), "missing value{version}: {message}");
            }
            break;
        }
    }
    drop(child);
    reader.join().unwrap();
}

/// Reproduce the audited source size, reference count and workspace size under
/// the actual Cargo release profile. Timing is reported, never a host deadline.
#[test]
#[ignore = "release-profile measurement; run explicitly with --release --ignored --nocapture"]
fn imported_role_references_release_profile() {
    measure(16, 128, true);
}

#[test]
#[ignore = "release-profile scaling measurement; run explicitly with --release --ignored --nocapture"]
fn role_references_scaling_release_profile() {
    for modules in [1, 16, 64] {
        for uses in [32, 64, 128] {
            for imported in [false, true] {
                measure(modules, uses, imported);
            }
        }
    }
}

fn measure(modules: usize, uses: usize, imported: bool) {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        root.path().join("Cell.toml"),
        "[package]\nname = \"role_latency\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
    )
    .unwrap();
    std::fs::write(src.join("schema.cell"), "module schema\npublic struct Entry { revision: u64, payload: Vec<u8> }\n").unwrap();
    for i in 1..modules {
        std::fs::write(src.join(format!("extra{i}.cell")), format!("module extra{i}\npublic struct Extra{i} {{ value: u64 }}\n"))
            .unwrap();
    }
    let fixture = include_str!("lsp_responsiveness/fixtures/role_references.cell");
    assert_eq!(fixture.len(), 12_404);
    let source = fixture
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            ["let revision", "let count"].iter().all(|prefix| {
                line.strip_prefix(prefix).and_then(|tail| tail.get(..3)).and_then(|n| n.parse::<usize>().ok()).is_none_or(|n| n < uses)
            })
        })
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let source =
        if imported { source } else { source.replace("use schema::Entry", "struct Entry { revision: u64, payload: Vec<u8> }") };
    assert_eq!(source.matches("selected.orders").count(), uses);
    let path = src.join("main.cell");
    std::fs::write(&path, &source).unwrap();
    let uri = tower_lsp::lsp_types::Url::from_file_path(path).unwrap().to_string();
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.clone());
    assert!(server.get_diagnostics(&uri).iter().all(|d| d.severity != cellscript::lsp::DiagnosticSeverity::Error));
    let offset = source.find("orders: Order").unwrap();
    let prefix = &source[..offset];
    let position = Position {
        line: prefix.lines().count() as u32 - 1,
        character: prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
    };
    let start = Instant::now();
    let references = server.find_references(&uri, position);
    let elapsed = start.elapsed();
    assert_eq!(references.len(), uses + 1);
    assert!(references.iter().all(|reference| reference.uri == uri));
    eprintln!(
        "role-reference-benchmark bytes={} uses={uses} modules={modules} imported={imported} references={} cold_ms={:.3}",
        source.len(),
        references.len(),
        elapsed.as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    let repeated = server.find_references(&uri, position);
    let elapsed = start.elapsed();
    assert_eq!(serde_json::to_value(&repeated).unwrap(), serde_json::to_value(&references).unwrap());
    eprintln!("role-reference-repeat uses={uses} modules={modules} imported={imported} warm_ms={:.3}", elapsed.as_secs_f64() * 1000.0);
}

#[test]
fn byte_string_eof_diagnostics_use_utf16_ranges_and_clear_after_repair() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("byte_boundary.cell");
    let prefix = "// π🙂\nmodule byte_boundary\naction inspect() -> u64 { verification /* π🙂 */ let payload = ";
    let valid = format!("{prefix}b\"\\x2A\" require payload[0] == 42 return 0 }}");
    std::fs::write(&path, &valid).unwrap();
    let uri = tower_lsp::lsp_types::Url::from_file_path(&path).unwrap().to_string();
    for edition in [cellscript::CellScriptEdition::Edition2026, cellscript::CellScriptEdition::Edition2027] {
        let mut server = LspServer::new();
        for tail in ["b\"unfinished", "b\"unfinished\\"] {
            let source = format!("{prefix}{tail}");
            let expected = cellscript::lexer::lex(&source).unwrap_err();
            server.open_document_with_edition(uri.clone(), source.clone(), edition);
            let diagnostics = server.get_diagnostics(&uri);
            let error = diagnostics.iter().find(|diagnostic| diagnostic.code == expected.code).expect("byte string EOF diagnostic");
            assert_eq!(error.severity, cellscript::lsp::DiagnosticSeverity::Error);
            assert_eq!(error.range.start.line, expected.span.line as u32 - 1);
            let before = &source[..expected.span.start];
            let expected_character = before.rsplit('\n').next().unwrap().encode_utf16().count() as u32;
            assert_eq!(error.range.start.character, expected_character);
            assert_eq!(error.range.end.line, error.range.start.line);
            assert_eq!(error.range.end.character, expected_character + (expected.span.end - expected.span.start) as u32);
            server.open_document_with_edition(uri.clone(), valid.clone(), edition);
            assert!(server
                .get_diagnostics(&uri)
                .iter()
                .all(|diagnostic| diagnostic.severity != cellscript::lsp::DiagnosticSeverity::Error));
        }
    }
}
