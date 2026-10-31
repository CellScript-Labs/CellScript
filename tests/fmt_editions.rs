//! Formatter verification uses the same edition router as compilation.
use cellscript::{ast::Visibility, fmt, frontend, CellScriptEdition};

#[cfg(feature = "cli")]
mod common;

const NATIVE_TYPE: &str = include_str!("syntax_combo/seeds/edition-2027-type-script.cell");
const NATIVE_LOCK: &str = include_str!("syntax_combo/seeds/edition-2027-lock-script.cell");
const LEGACY: &str = "module format_example\naction inspect(witness value: u64) -> u64 { verification return value }";

#[test]
fn explicit_edition_formats_native_type_and_lock_entries_idempotently() {
    for (source, marker) in
        [(NATIVE_TYPE, "type_script TokenTransfer on type_group<Token>"), (NATIVE_LOCK, "lock_script VaultOwner on lock_group")]
    {
        for config in
            [fmt::FormatConfig::default(), fmt::FormatConfig { indent_width: 2, trailing_newline: false, ..Default::default() }]
        {
            let formatted = fmt::format_source(source, CellScriptEdition::Edition2027, config.clone()).unwrap();
            assert!(formatted.contains(marker));
            fmt::verify_idempotent_for_edition(&formatted, CellScriptEdition::Edition2027, config.clone()).unwrap();
            assert_eq!(fmt::format_source(&formatted, CellScriptEdition::Edition2027, config).unwrap(), formatted);
            assert!(frontend::parse(&formatted, CellScriptEdition::Edition2027).is_ok());
        }
        assert!(fmt::format_source(source, CellScriptEdition::Edition2026, Default::default()).is_err());
        assert!(fmt::verify_idempotent(source, Default::default()).is_err());
    }
}

#[test]
fn default_verifier_keeps_edition_2026_instead_of_guessing_from_source() {
    fmt::verify_idempotent(LEGACY, Default::default()).unwrap();
    let concise = "module concise\naction inspect(witness value: u64) -> u64 { return value }";
    assert!(fmt::verify_idempotent(concise, Default::default()).is_err());
    fmt::verify_idempotent_for_edition(concise, CellScriptEdition::Edition2027, Default::default()).unwrap();
    let canonical = fmt::format_source(concise, CellScriptEdition::Edition2027, Default::default()).unwrap();
    assert!(canonical.contains("verification"));
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        fmt::verify_idempotent_for_edition(LEGACY, edition, Default::default()).unwrap();
    }
}

#[test]
fn formatter_preserves_implicit_and_explicit_visibility_distinctions() {
    let source = "module visibility\nfn implicit() -> u64 { return 0 }\npublic fn exported() -> u64 { return 1 }\npublic(package) fn in_package() -> u64 { return 2 }\nprivate fn hidden() -> u64 { return 3 }";
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let formatted = fmt::format_source(source, edition, Default::default()).unwrap();
        assert!(!formatted.contains("public fn implicit"));
        assert!(formatted.contains("public fn exported"));
        assert!(formatted.contains("public(package) fn in_package"));
        assert!(formatted.contains("private fn hidden"));
        let parsed = frontend::parse(&formatted, edition).unwrap();
        for (name, visibility) in [
            ("implicit", Visibility::LegacyPublic),
            ("exported", Visibility::Public),
            ("in_package", Visibility::Package),
            ("hidden", Visibility::Private),
        ] {
            assert_eq!(parsed.visibility_of(name), visibility, "{edition}: {name}");
        }
    }
    for (source, entry) in [(NATIVE_TYPE, "transfer"), (NATIVE_LOCK, "unlock")] {
        let formatted = fmt::format_source(source, CellScriptEdition::Edition2027, Default::default()).unwrap();
        assert!(!formatted.contains("public type_script"));
        assert!(!formatted.contains("public lock_script"));
        assert_eq!(
            frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap().visibility_of(entry),
            Visibility::LegacyPublic
        );
    }
}

#[test]
fn formatter_propagates_frontend_errors_before_returning_canonical_source() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for source in [
            "module invalid\nunknown declaration",
            "module invalid\nfn value() -> u64 { return 1 + }",
            "module invalid\nfn value() -> u64 { return \"unfinished",
        ] {
            let expected = frontend::parse(source, edition).unwrap_err();
            let actual = fmt::format_source(source, edition, Default::default()).unwrap_err();
            assert_eq!(actual.code, expected.code);
            assert_eq!(actual.message, expected.message);
            assert_eq!(actual.span, expected.span);
        }
    }
    let malformed_native = NATIVE_LOCK.replace("group_input[0]", "group_input[1]");
    let error = fmt::format_source(&malformed_native, CellScriptEdition::Edition2027, Default::default()).unwrap_err();
    assert!(error.message.contains("non-canonical"), "{error}");
}

#[cfg(feature = "cli")]
#[test]
fn cli_fmt_and_check_obey_the_package_edition_without_rewriting_rejected_input() {
    for (edition, source) in [("2026", LEGACY), ("2027", NATIVE_TYPE), ("2027", NATIVE_LOCK)] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"format_example\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
        let path = root.path().join("src/main.cell");
        std::fs::write(&path, source).unwrap();
        let dirty = common::cellc_command().current_dir(root.path()).args(["fmt", "--check", "--json"]).output().unwrap();
        assert!(!dirty.status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        let written = common::cellc_command().current_dir(root.path()).args(["fmt", "--json"]).output().unwrap();
        assert!(written.status.success(), "{} {}", String::from_utf8_lossy(&written.stdout), String::from_utf8_lossy(&written.stderr));
        let expected = fmt::format_source(source, edition.parse().unwrap(), Default::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        let clean = common::cellc_command().current_dir(root.path()).args(["fmt", "--check", "--json"]).output().unwrap();
        assert!(clean.status.success(), "{} {}", String::from_utf8_lossy(&clean.stdout), String::from_utf8_lossy(&clean.stderr));
    }
    for (edition, source) in [("2026", NATIVE_LOCK), ("2027", "module invalid\nfn f() {"), ("2099", LEGACY)] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"format_rejected\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
        let path = root.path().join("src/main.cell");
        std::fs::write(&path, source).unwrap();
        let rejected = common::cellc_command().current_dir(root.path()).args(["fmt", "--json"]).output().unwrap();
        assert!(!rejected.status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        if edition == "2099" {
            let diagnostic = format!("{} {}", String::from_utf8_lossy(&rejected.stdout), String::from_utf8_lossy(&rejected.stderr));
            assert!(diagnostic.contains("2099") && diagnostic.contains("edition"), "{diagnostic}");
        }
    }
}
