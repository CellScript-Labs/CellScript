//! Real dependency-package editor behavior for the same complete migrations
//! whose canonical bytes and all-group execution are checked by the VM suite.
use super::migration::{package_sources, Shape};
use cellscript::lsp::{DiagnosticSeverity, LspServer, Position};

#[test]
fn editor_cross_package_budget_hover_matches_cli_and_unsaved_record_layouts() {
    let library = "module shared::rules\npublic struct Pair<T: fixed_value> { left: T, right: T }";
    let application = "module application\nuse shared::rules::Pair\ncollections Work { limits { bytes: 256, allocations: 3 }\nrows: Vec<Pair<u64>> { items: 3 }\n}\naction inspect() -> u64 { return 0 }";
    let temp = tempfile::tempdir().unwrap();
    super::support::compile_package(temp.path(), application, library, 3).unwrap();
    let main_uri = format!("file://{}", temp.path().join("application/src/main.cell").display());
    let library_uri = format!("file://{}", temp.path().join("rules/src/main.cell").display());
    let position = after(application, "collections Wor");
    let mut server = LspServer::new();
    server.open_document_with_edition(main_uri.clone(), application.into(), cellscript::NEXT_EDITION);
    let hover = server.hover(&main_uri, position).unwrap().contents;
    assert!(hover.contains("rows: 3 items x 16 bytes"), "{hover}");
    assert!(hover.contains("Reserve all members once: 80 bytes / 1 allocations — fits"), "{hover}");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(temp.path().join("application"))
        .args(["explain", "collections"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let cli = String::from_utf8(output.stdout).unwrap();
    // The editor and CLI must show the same member and aggregate calculations.
    for line in cli.lines().filter(|line| line.starts_with("  ")) {
        assert!(hover.contains(line), "CLI line absent from hover: {line}\n{hover}");
    }
    server.open_document_with_edition(
        library_uri.clone(),
        library.replace("right: T", "right: T, third: T, fourth: T"),
        cellscript::NEXT_EDITION,
    );
    let hover = server.hover(&main_uri, position).unwrap().contents;
    assert!(hover.contains("rows: 3 items x 32 bytes"), "{hover}");
    assert!(hover.contains("Grow all members incrementally: 288 bytes / 3 allocations — does not fit"), "{hover}");
    assert!(hover.contains("shortfall: 32 bytes / 0 allocation events"), "{hover}");
    server.open_document_with_edition(library_uri, library.replace("public struct", "private struct"), cellscript::NEXT_EDITION);
    let hover = server.hover(&main_uri, position).unwrap().contents;
    assert!(hover.contains("Budget estimate unavailable"), "{hover}");
    assert!(!hover.contains("Reserve all members once"), "{hover}");
}

fn after(source: &str, text: &str) -> Position {
    let offset = source.rfind(text).unwrap() + text.len();
    let prefix = &source[..offset];
    Position {
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() as u32,
        character: prefix.rsplit('\n').next().unwrap().encode_utf16().count() as u32,
    }
}

#[test]
fn editor_cross_package_migration_fields_follow_imports_and_open_buffers() {
    for (shape, source) in [
        (Shape::Bytes, include_str!("../../examples/language/batches/nested_migration.cell")),
        (Shape::Records, include_str!("../../examples/language/batches/record_migration.cell")),
    ] {
        let (mut application, library) = package_sources(source, shape);
        application = application
            .replace("use shared::rules::Registry", "use shared::rules::Registry as Status")
            .replace("registry: Registry", "registry: Status");
        if shape == Shape::Records {
            application.push_str("\nfn inspect_item(value: Messages::Item) -> u64 { return value.revision }\n");
            application.push_str("fn inspect_payload(value: Messages::Item) -> u64 { return value.payload.len() }\n");
        }
        let temp = tempfile::tempdir().unwrap();
        super::support::compile_package(temp.path(), &application, &library, 3).unwrap();
        // Neither matching spelling nor directory order makes this an import.
        std::fs::write(
            temp.path().join("application/src/decoy.cell"),
            "module decoy\npublic resource Registry has store { decoy: u64 }\npublic struct Message { decoy: u64 }\n",
        )
        .unwrap();
        let main_uri = format!("file://{}", temp.path().join("application/src/main.cell").display());
        let library_uri = format!("file://{}", temp.path().join("rules/src/main.cell").display());
        let mut server = LspServer::new();
        server.open_document_with_edition(main_uri.clone(), application.clone(), cellscript::NEXT_EDITION);
        let diagnostics = server.get_diagnostics(&main_uri);
        assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
        let position = after(&application, "registry_before.data.");
        let fields = server.completion(&main_uri, position);
        for expected in ["id", "revision"] {
            assert!(fields.iter().any(|item| item.label == expected), "{shape:?}: missing {expected}: {fields:?}");
        }
        assert!(!fields.iter().any(|item| item.label == "decoy"));
        let definition = server.goto_definition(&main_uri, after(&application, "migrate_documen")).unwrap();
        assert_eq!(definition.uri, library_uri);
        if shape == Shape::Records {
            let fields = server.completion(&main_uri, after(&application, "return value."));
            assert!(fields.iter().any(|item| item.label == "revision"), "{fields:?}");
            assert!(fields.iter().any(|item| item.label == "payload"), "{fields:?}");
            assert!(!fields.iter().any(|item| item.label == "decoy"));
            let payload = server.completion(&main_uri, after(&application, "value.payload."));
            assert!(payload.iter().any(|item| item.label == "len"), "{payload:?}");
            assert!(!payload.iter().any(|item| item.label == "push"));
        }
        // Completion uses the opened dependency's text, not its older disk copy.
        server.open_document_with_edition(
            library_uri.clone(),
            library.replace("id: u64, revision: u64 }", "id: u64, generation: u64 }"),
            cellscript::NEXT_EDITION,
        );
        let fields = server.completion(&main_uri, position);
        assert!(fields.iter().any(|item| item.label == "generation"), "{fields:?}");
        assert!(!fields.iter().any(|item| item.label == "revision"));
        // Typing a trailing dot repairs only this completion request, while
        // retaining the dependency's unsaved schema and the real diagnostics.
        let unfinished = application.replace("registry_before.data.revision", "registry_before.data.");
        assert_ne!(unfinished, application);
        server.open_document_with_edition(main_uri.clone(), unfinished.clone(), cellscript::NEXT_EDITION);
        let diagnostics = server.get_diagnostics(&main_uri);
        assert!(diagnostics.iter().any(|item| item.severity == DiagnosticSeverity::Error));
        let fields = server.completion(&main_uri, after(&unfinished, "registry_before.data."));
        assert!(fields.iter().any(|item| item.label == "generation"), "{fields:?}");
        assert!(!fields.iter().any(|item| item.label == "revision"));
        assert_eq!(
            serde_json::to_value(server.get_diagnostics(&main_uri)).unwrap(),
            serde_json::to_value(diagnostics).unwrap(),
            "completion recovery must not overwrite document diagnostics"
        );
        server.open_document_with_edition(main_uri.clone(), application.clone(), cellscript::NEXT_EDITION);
        server.open_document_with_edition(
            library_uri,
            library.replace("public resource Registry", "private resource Registry"),
            cellscript::NEXT_EDITION,
        );
        assert!(server.completion(&main_uri, position).is_empty(), "private dependency fields must not leak through completion");
        let position = after(&unfinished, "registry_before.data.");
        server.open_document_with_edition(main_uri.clone(), unfinished, cellscript::NEXT_EDITION);
        assert!(server.completion(&main_uri, position).is_empty(), "recovery must retain the dependency's private visibility");
    }
}
