//! Edition 2027 source frontend over the shared declaration, value and statement
//! kernel. All consumers use one parser and the same path-completeness checks
//! before the shared typed semantic foundation.

use crate::ast;
use crate::edition::CellScriptEdition;
use crate::error::{CompileError, Result};
use crate::lexer;

mod authoring;
mod migrate;

pub use migrate::{legacy_temporal_migration_diagnostics, migrate_legacy_temporal_source, TemporalMigrationCandidate};

pub fn parse(source: &str, edition: CellScriptEdition) -> Result<ast::Module> {
    let tokens = lexer::lex(source)?;
    let module = match edition {
        CellScriptEdition::Edition2027 => crate::parser::parse(&tokens)?,
    };
    validate_authoring(&module)?;
    Ok(module)
}

/// A source-rewrite entry point: one lexing pass, the same complete parser,
/// and the exact source slices needed to retain comments and token grouping.
pub fn parse_document(source: &str, edition: CellScriptEdition) -> Result<ParsedDocument<'_>> {
    let mut syntax = lexer::lex_document(source)?;
    let (ast, nodes) = match edition {
        CellScriptEdition::Edition2027 => crate::parser::parse_with_syntax(&syntax.tokens)?,
    };
    validate_authoring(&ast)?;
    syntax.nodes = nodes;
    Ok(ParsedDocument { syntax, ast })
}

pub struct ParsedDocument<'a> {
    pub syntax: crate::syntax::SyntaxDocument<'a>,
    pub ast: ast::Module,
}

pub fn parse_diagnostics(source: &str, edition: CellScriptEdition) -> std::result::Result<ast::Module, Vec<CompileError>> {
    let tokens = lexer::lex(source).map_err(|error| vec![error])?;
    match edition {
        CellScriptEdition::Edition2027 => authoring::parse_diagnostics(&tokens),
    }
}

fn validate_authoring(module: &ast::Module) -> Result<()> {
    match authoring::successor_completeness_diagnostics(module).into_iter().next() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Item;
    use crate::error::Span;

    #[test]
    fn current_edition_accepts_explicit_and_markerless_authoring() {
        let implicit = "module demo;\naction main(witness value: u64) -> u64 {  return value; }";
        assert!(parse(implicit, CellScriptEdition::Edition2027).is_ok());

        let explicit = "module demo;\naction main(witness value: u64) -> u64 {  return value; }";
        assert!(parse(explicit, CellScriptEdition::Edition2027).is_ok());

        let concise = "module demo;\naction main(witness value: u64) -> u64 { return value; }";
        assert!(parse(concise, CellScriptEdition::Edition2027).is_ok());
    }

    #[test]
    fn authoring_frontend_keeps_legacy_lifecycle_and_source_entry_organization() {
        let consume = r#"
module demo;
resource Token has consume { amount: u64, }
action main(input token: Token) {  consume token; }
"#;
        let module = parse(consume, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        assert!(matches!(action.body.as_slice(), [ast::Stmt::Discard(ast::Expr::Consume(_))]));
        assert!(
            action.next_surface.as_ref().unwrap().dispositions.is_empty(),
            "a consume must not acquire a business disposition policy"
        );

        let multiple = r#"
module demo;
action first() {  return; }
action second() {  return; }
"#;
        assert_eq!(parse(multiple, CellScriptEdition::Edition2027).unwrap().items.len(), 2);

        let capability_only = r#"
module demo;
resource Token has consume { amount: u64, }
action main(witness value: u64) -> u64 {  return value; }
"#;
        assert!(parse(capability_only, CellScriptEdition::Edition2027).is_ok());
    }

    #[test]
    fn diagnostic_frontend_keeps_source_spans() {
        let source = "module demo;\naction main(value: ) { return }";
        let diagnostics = parse_diagnostics(source, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_ne!(diagnostics[0].span, Span::default());
    }

    #[test]
    fn native_container_words_remain_ordinary_authoring_names() {
        let source = r#"
module type_script;
const lock_script: u64 = 3;
fn type_script(value: u64) -> u64 { value + lock_script }
action main(witness value: u64) -> u64 { return type_script(value); }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);
    }

    #[test]
    fn removed_lock_declaration_has_one_diagnostic_across_consumers() {
        let source = "module removed;\nlock_script unlock() -> bool { true }";
        let expected = parse(source, crate::CURRENT_EDITION).unwrap_err();
        assert_eq!(expected.code.unwrap().as_str(), "E1032");
        assert_eq!(&source[expected.span.start..expected.span.end], "lock_script");
        assert!(expected.message.contains("use 'lock'"));
        assert!(expected.message.contains("Lock-group artifact"));
        let formatted = crate::fmt::format_source(source, crate::CURRENT_EDITION, Default::default()).unwrap_err();
        let metadata = crate::compile_metadata(source, crate::CURRENT_EDITION, None).unwrap_err();
        let diagnostics = parse_diagnostics(source, crate::CURRENT_EDITION).unwrap_err();
        for error in [&formatted, &metadata, &diagnostics[0]] {
            assert_eq!(error.code, expected.code);
            assert_eq!(error.message, expected.message);
            assert_eq!(error.span, expected.span);
        }
    }

    #[test]
    fn authoring_lock_constraint_blocks_keep_the_legacy_verification_model() {
        let source = "module demo;\nlock owner(witness value: u64) { require value > 0;\n return true; }";
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Lock(lock) = module.items.last().unwrap() else { panic!("expected lock") };
        assert!(matches!(lock.body[0], ast::Stmt::Discard(ast::Expr::Require(_))));
        assert!(lock.next_surface.is_some());
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(!formatted.contains("verification"));
    }

    #[test]
    fn next_frontend_parses_native_type_script_surface() {
        let source = r#"
module demo;

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

action transfer(
        input token: Token,
        witness recipient: Address,
        output next: Token,
    ) {
        require token.amount > 0;

        replace token -> next {
            data {
                owner = same,
                amount = same,
            };

            type_script = same;
            lock = exact(recipient);
            capacity = same;

        }
    }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native Edition 2027 entry to lower to an action");
        };
        let surface = action.next_surface.as_ref().expect("native surface marker");
        assert_eq!(action.name, "transfer");
        assert!(matches!(action.body[0], ast::Stmt::Discard(ast::Expr::Require(_))));
        assert_eq!(surface.dispositions.len(), 1);
        assert_eq!(action.params.len(), 2);
        assert_eq!(action.outputs.len(), 1);

        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("action transfer("));
        assert!(formatted.contains("replace token -> next"));
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);

        let missing_field = source.replace("                amount = same,\n", "");
        assert_ne!(missing_field, source, "the missing-field mutation must execute");
        let missing_field = parse(&missing_field, CellScriptEdition::Edition2027).unwrap();
        assert!(crate::types::check(&missing_field).unwrap_err().message.contains("cover every field"));
        let explicit_source = source.replace("output next: Token", "output next: Token from group_output[0]");
        assert!(parse(&explicit_source, CellScriptEdition::Edition2027).is_err());
        let mismatched_output = source
            .replace(
                "resource Token has",
                "resource Other has store, replace, relock { owner: Address, amount: u64 }\nresource Token has",
            )
            .replace("output next: Token", "output next: Other");
        let mismatched_output = parse(&mismatched_output, CellScriptEdition::Edition2027).unwrap();
        assert!(crate::types::check(&mismatched_output).unwrap_err().message.contains("replace relation type mismatch"));
        let scalar_input = source.replace("input token: Token", "input token: u64");
        let scalar_input = parse(&scalar_input, CellScriptEdition::Edition2027).unwrap();
        let error = crate::types::check(&scalar_input).unwrap_err();
        assert!(error.message.contains("must name a Cell-backed"), "{error}");
        let witness_only = r#"
module demo;
action transfer(witness amount: u64) {
        require amount > 0;
    }
"#;
        assert!(parse(witness_only, CellScriptEdition::Edition2027).is_ok());
        let retired_entry = source.replace("action transfer(", "type_script transfer(");
        assert!(parse(&retired_entry, CellScriptEdition::Edition2027).is_err());
        let unhashed_lock = source.replace("lock = exact(recipient)", "lock = recipient");
        assert!(parse(&unhashed_lock, CellScriptEdition::Edition2027).unwrap_err().message.contains("exact"));
    }

    #[test]
    fn next_frontend_parses_fresh_retire_and_audit_surfaces_fail_closed() {
        let fresh = r#"
module demo;
#[type_id("demo::Token:v1")]
resource Token has store, create, burn identity(ckb_type_id) { amount: u64, }
action mint(
        witness amount: u64,
        witness recipient: Address,
        output next: Token,
    ) {
        require amount > 0;
        audit issuance_policy {
            expected_evidence = external_policy(recipient);
        }
        fresh next {
            data { amount = amount, };
            identity = ckb_type_id;
            lock = exact(recipient);
        }
    }
"#;
        let module = parse(fresh, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native fresh entry to lower to an action");
        };
        let surface = action.next_surface.as_ref().expect("native surface marker");
        assert_eq!(surface.audits.len(), 1);
        assert!(matches!(surface.dispositions[0].resolve(&action.body), Some(crate::ast::ResolvedDisposition::Fresh(_))));
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("audit issuance_policy"));
        assert!(formatted.contains("fresh next"));
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);

        let duplicate_audit = fresh.replace(
            "        fresh next {",
            "        audit issuance_policy { expected_evidence = external_policy(amount); }\n        fresh next {",
        );
        assert!(parse(&duplicate_audit, CellScriptEdition::Edition2027).unwrap_err().message.contains("duplicate audit declaration"));
        let missing_field = fresh.replace("            data { amount = amount, };", "            data { };");
        let missing_field = parse(&missing_field, CellScriptEdition::Edition2027).unwrap();
        assert!(crate::types::check(&missing_field).unwrap_err().message.contains("field"));
        let explicit_count = fresh.replace("identity = ckb_type_id;", "identity = ckb_type_id;\ncardinality = one;");
        assert!(parse(&explicit_count, CellScriptEdition::Edition2027)
            .unwrap_err()
            .message
            .contains("unknown fresh clause 'cardinality'"));

        let retire = r#"
module demo;
resource Note has store, consume, burn identity(field(note_id)) { note_id: u64, amount: u64, }
action retire_note(input note: Note) {
        require note.amount == 0;
        retire note {
            absence = field(note_id);
        }
    }
"#;
        let module = parse(retire, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native retirement entry to lower to an action");
        };
        assert!(matches!(
            action.next_surface.as_ref().unwrap().dispositions[0].resolve(&action.body),
            Some(crate::ast::ResolvedDisposition::Retire(_))
        ));
        let missing_absence = retire.replace("            absence = field(note_id);\n", "");
        assert!(parse(&missing_absence, CellScriptEdition::Edition2027).is_err());

        let reordered_outputs = r#"
module demo;
resource Token has store, create { amount: u64, }
action mint(
        witness amount: u64,
        witness recipient: Address,
        output first: Token,
        output second: Token,
    ) {
        fresh second {
            data { amount = amount, };
            identity = none;
            lock = exact(recipient);
        }
        fresh first {
            data { amount = amount, };
            identity = none;
            lock = exact(recipient);
        }
    }
"#;
        assert!(
            parse(reordered_outputs, CellScriptEdition::Edition2027).is_ok(),
            "declaration ordinals, not source checking order, bind physical outputs"
        );
    }

    #[test]
    fn next_frontend_parses_checked_pool_surface_fail_closed() {
        let source = r#"
module demo;
resource Token has store, create, consume { owner: Address, amount: u64, }
action merge(
        input left: Token,
        input right: Token,
        witness recipient: Address,
        output merged: Token,
    ) {
        require left.amount > 0;
        pool value_flow {
            inputs { left, right };
            outputs { merged };
            data {
                owner { merged = recipient, },
                amount = conserve,
            };
            lock { merged = exact(recipient), };
        }
    }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native pool entry to lower to an action");
        };
        assert!(matches!(
            action.next_surface.as_ref().unwrap().dispositions[0].resolve(&action.body),
            Some(crate::ast::ResolvedDisposition::Pool(_))
        ));
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("pool value_flow"));
        assert!(formatted.contains("amount = conserve"));
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);

        let no_conservation =
            source.replace("                amount = conserve,", "                amount { merged = merged.amount, },");
        assert!(parse(&no_conservation, CellScriptEdition::Edition2027).unwrap_err().message.contains("at least one numeric field"));
        let missing_lock = source.replace("            lock { merged = exact(recipient), };", "            lock { };");
        assert!(parse(&missing_lock, CellScriptEdition::Edition2027).unwrap_err().message.contains("assign every pool output"));
        let reused_input = source.replace("inputs { left, right }", "inputs { left, left }");
        assert!(parse(&reused_input, CellScriptEdition::Edition2027).unwrap_err().message.contains("each be distinct"));
    }

    #[test]
    fn shared_frontend_parses_lock_roles() {
        let source = r#"
module demo;

resource Vault has store {
    owner: Address,
}

lock unlock(
        protected vault: Vault,
        lock_args owner: Address,
        witness claimed_owner: Address,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
    }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Lock(lock) = module.items.last().unwrap() else {
            panic!("expected native Edition 2027 entry to lower to a lock");
        };
        assert!(lock.next_surface.is_some());
        assert_eq!(lock.name, "unlock");
        assert_eq!(lock.body.len(), 2);
        assert_eq!(lock.params.len(), 3);

        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("lock unlock("));
        assert!(formatted.contains("protected vault: Vault"));
        assert!(formatted.contains("lock_args owner: Address"));
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);

        let explicit_source = source.replace("protected vault: Vault", "protected vault: Vault from group_input[0]");
        assert!(parse(&explicit_source, CellScriptEdition::Edition2027).is_err());
        let no_protected = source.replace("        protected vault: Vault,\n", "");
        let no_protected = parse(&no_protected, CellScriptEdition::Edition2027).unwrap();
        assert!(crate::types::check(&no_protected).unwrap_err().message.contains("vault"));
        let explicit_group = source.replace("lock unlock(", "lock unlock on lock_group(");
        assert!(parse(&explicit_group, CellScriptEdition::Edition2027).unwrap_err().message.contains("on"));
    }

    const BRANCH_REPLACEMENT: &str = r#"
module branch_authoring;
resource Token has store, replace, relock { id: u64, amount: u64, }
action update(
        input token: Token,
        witness increment: bool,
        output next: Token,
    ) {
        let minimum = 1;
        require token.amount >= minimum;
        if increment {
            replace token -> next {
                data = same except { amount = token.amount + 1, };
                lock = same;
                type_script = same;
                capacity = same;
            }
        } else {
            replace token -> next {
                data { id = same, amount = same, };
                lock = same;
                type_script = same;
                capacity = same;
            }
        }
    }
"#;

    #[test]
    fn group_bound_actions_use_shared_statements_and_keep_branch_occurrences() {
        let module = parse(BRANCH_REPLACEMENT, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        let surface = action.next_surface.as_ref().unwrap();
        assert_eq!(surface.dispositions.len(), 2);
        for (index, disposition) in surface.dispositions.iter().enumerate() {
            let ast::NextDisposition::Replace(replacement) = disposition else { panic!("expected replace") };
            assert_eq!(replacement.statement_index, 2);
            assert_eq!(replacement.relation_index, index);
            assert_eq!(replacement.relation(&action.body).unwrap().before, "token");
        }
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(!formatted.contains("entry update"));
        assert!(!formatted.contains("enforce"));
        assert!(!formatted.contains("verification"));
        assert!(formatted.find("let minimum").unwrap() < formatted.find("require token.amount").unwrap());
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);

        let ir = crate::ir::generate(&module).unwrap();
        let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected IR action") };
        assert_eq!(action.successor_relations.len(), 2);
        let first = &action.successor_relations[0];
        let second = &action.successor_relations[1];
        assert_eq!(first.input_local_id, second.input_local_id);
        assert_eq!(first.output_local_id, second.output_local_id);
        assert_ne!(first.consume_block, second.consume_block);
        assert_ne!(first.create_block, second.create_block);
        assert_ne!(first.success_block, second.success_block);
        assert!(first.data_fields.iter().any(|(_, treatment)| treatment.starts_with("set-from-expression:")));
        assert!(second.data_fields.iter().all(|(_, treatment)| treatment == "preserve"));
    }

    #[test]
    fn group_bound_actions_reject_missing_and_duplicate_path_accounting() {
        let missing = BRANCH_REPLACEMENT.replace(
            "        } else {",
            "        }
        if increment {",
        );
        assert!(parse(&missing, CellScriptEdition::Edition2027).is_err());
        let early_return =
            BRANCH_REPLACEMENT.replace("        let minimum = 1;", "        if increment { return; }\n        let minimum = 1;");
        assert!(parse(&early_return, CellScriptEdition::Edition2027).unwrap_err().message.contains("every accepting path"));
        let duplicate = BRANCH_REPLACEMENT.replace(
            "        let minimum = 1;",
            "        replace token -> next { data { id = same, amount = same, }; lock = same; type_script = same; capacity = same; }\n        let minimum = 1;",
        );
        assert!(parse(&duplicate, CellScriptEdition::Edition2027).unwrap_err().message.contains("twice on one accepting path"));
    }

    #[test]
    fn group_bound_match_arms_share_the_statement_kernel() {
        let source = BRANCH_REPLACEMENT
            .replace("resource Token has", "enum Choice { Increment, Preserve }\nresource Token has")
            .replace("witness increment: bool", "witness increment: Choice")
            .replace("        if increment {", "        match increment { Choice::Increment => {")
            .replace("        } else {", "        }, Choice::Preserve => {")
            .replace("\n    }\n", "\n        }\n    }\n");
        let module = parse(&source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        assert_eq!(action.next_surface.as_ref().unwrap().dispositions.len(), 2);
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);
    }

    #[test]
    fn group_bound_entries_reject_retired_business_spellings_and_missing_clauses() {
        for old in [
            BRANCH_REPLACEMENT.replace("input token: Token,", "entry token: Token,"),
            BRANCH_REPLACEMENT.replace("require token.amount >= minimum;", "verify { enforce token.amount >= minimum }"),
            BRANCH_REPLACEMENT.replace("type_script = same", "identity = same"),
            BRANCH_REPLACEMENT.replace("lock = same", "lock_script = same"),
        ] {
            assert!(parse(&old, CellScriptEdition::Edition2027).is_err(), "retired surface must reject: {old}");
        }
        for clause in ["lock = same;", "type_script = same;", "capacity = same;"] {
            let missing = BRANCH_REPLACEMENT.replace(clause, "");
            assert!(parse(&missing, CellScriptEdition::Edition2027).unwrap_err().message.contains("must state every treatment"));
        }
    }

    #[test]
    fn fresh_lock_targets_check_their_explicit_domains_in_the_lexical_scope() {
        let source = r#"
module lock_domains;
resource Token has store, create { amount: u64, }
action mint(
        witness recipient: ScriptHash,
        output token: Token,
    ) {
        let destination = recipient;
        fresh token {
            data { amount = 1, };
            identity = none;
            lock = exact_hash(destination);
        }
    }
"#;
        crate::types::check(&parse(source, CellScriptEdition::Edition2027).unwrap()).unwrap();
        for wrong in [
            source.replace("recipient: ScriptHash", "recipient: Address"),
            source.replace("recipient: ScriptHash", "recipient: Hash"),
            source.replace("exact_hash(destination)", "exact(destination)"),
        ] {
            let module = parse(&wrong, CellScriptEdition::Edition2027).unwrap();
            assert!(crate::types::check(&module).unwrap_err().message.contains("lock `exact"));
        }
        let address =
            source.replace("recipient: ScriptHash", "recipient: Address").replace("exact_hash(destination)", "exact(destination)");
        crate::types::check(&parse(&address, CellScriptEdition::Edition2027).unwrap()).unwrap();
    }

    #[test]
    fn group_bound_failure_exits_do_not_acquire_successor_obligations() {
        let (before_else, _) = BRANCH_REPLACEMENT.split_once("        } else {").unwrap();
        let source = format!("{before_else}        }} else {{ require false; }}\n    }}\n");
        let module = parse(&source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let ir = crate::ir::generate(&module).unwrap();
        let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected action") };
        assert_eq!(action.successor_relations.len(), 1);

        let all_fail = BRANCH_REPLACEMENT.replace("require token.amount >= minimum;", "require false;");
        let module = parse(&all_fail, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let ir = crate::ir::generate(&module).unwrap();
        let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected action") };
        assert!(action.successor_relations.is_empty(), "unreachable source relations must not become execution evidence");
    }

    #[test]
    fn optimizing_group_entries_preserves_scopes_and_rebuilds_occurrence_references() {
        let source = BRANCH_REPLACEMENT
            .replace("if increment {", "if true {")
            .replace("        let minimum = 1;", "        let unused = 7;\n        let minimum = 1;");
        let mut module = parse(&source, CellScriptEdition::Edition2027).unwrap();
        crate::optimize::optimize_module(&mut module, crate::CompilerDiagnostics::default()).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        let surface = action.next_surface.as_ref().unwrap();
        // A constant statement branch still owns its lexical scope. Both
        // source occurrences retain valid references; executable reachability
        // subsequently selects the literal branch.
        assert_eq!(surface.dispositions.len(), 2);
        for disposition in &surface.dispositions {
            let ast::NextDisposition::Replace(replacement) = disposition else { panic!("expected replacement") };
            assert_eq!(replacement.relation(&action.body).unwrap().before, "token");
        }
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(!formatted.contains("unused"));
        assert!(formatted.contains("if true"));
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&reparsed).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
    }

    #[test]
    fn optimizing_group_dispositions_keeps_source_and_executable_values_together() {
        let source = r#"
module native_optimization;
#[type_id("native_optimization::Token")]
resource Token has store, create identity(ckb_type_id) { amount: u64, }
action mint(
        witness recipient: ScriptHash,
        output token: Token,
    ) {
        let unused = 7;
        let amount = 3;
        audit issuance { expected_evidence = external_policy(recipient); }
        fresh token {
            data { amount = amount, };
            identity = ckb_type_id;
            lock = exact_hash(recipient);
        }
    }
"#;
        let mut module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::optimize::optimize_module(&mut module, crate::CompilerDiagnostics::default()).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        let surface = action.next_surface.as_ref().unwrap();
        assert!(matches!(surface.body_items.as_slice(), [ast::NextBodyItem::Audit(0), ast::NextBodyItem::Statement(0)]));
        let Some(ast::ResolvedDisposition::Fresh(fresh)) = surface.dispositions[0].resolve(&action.body) else {
            panic!("expected fresh")
        };
        assert!(matches!(fresh.data_fields[0].1, ast::Expr::Integer(3)));
        let ast::Stmt::Lifecycle(source) = &action.body[0] else { panic!("expected shared lifecycle statement") };
        let kernel = source.kernel(fresh.identity.as_ref()).unwrap();
        let ast::Stmt::Discard(ast::Expr::CreateUnique(create)) = &kernel[0] else { panic!("expected unique creation kernel") };
        assert!(matches!(create.fields[0].1, ast::Expr::Integer(3)));
        let formatted = crate::fmt::format_default(&module).unwrap();
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&reparsed).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
    }

    #[test]
    fn contextual_disposition_words_remain_ordinary_callable_names() {
        let source = r#"
module contextual_helpers;
fn pool(value: u64) -> u64 { value }
fn audit(value: u64) -> u64 { value }
fn retire(value: u64) -> u64 { value }
fn fresh(value: u64) -> u64 { value }
fn effects(value: u64) -> u64 { value }
fn entry(value: u64) -> u64 { value }
fn verify(value: u64) -> u64 { value }
fn enforce(value: u64) -> u64 { value }
resource Token has store, create { amount: u64, }
action mint(
        witness amount: u64,
        witness recipient: ScriptHash,
        output token: Token,
    ) {
        let checked = enforce(verify(entry(effects(fresh(retire(audit(pool(amount))))))));
        require checked > 0;
        fresh token {
            data { amount = checked, };
            identity = none;
            lock = exact_hash(recipient);
        }
    }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);
        let mut optimized = module;
        crate::optimize::optimize_module(&mut optimized, crate::CompilerDiagnostics::default()).unwrap();
        let formatted = crate::fmt::format_default(&optimized).unwrap();
        crate::types::check(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap();
    }

    #[test]
    fn nested_relation_values_keep_all_source_and_execution_occurrences() {
        let source = r#"
module nested_relations;
resource Token has store, replace, relock { amount: u64, enabled: bool, }
action update(
        input left: Token,
        input right: Token,
        output next_left: Token,
        output next_right: Token,
    ) {
        replace left -> next_left {
            data {
                amount = same,
                enabled = {
                    replace right -> next_right {
                        data { amount = same, enabled = same, };
                        lock = same;
                        type_script = same;
                        capacity = same;
                    }
                    true
                },
            };
            lock = same;
            type_script = same;
            capacity = same;
        }
    }
"#;
        let assignment = source
            .replace("enabled = {", "enabled = {\n                    let mut flag = false;\n                    flag = {")
            .replace(
                "                    true\n                }",
                "                    true\n                    };\n                    flag\n                }",
            );
        for nested in [source, assignment.as_str()] {
            let module = parse(nested, CellScriptEdition::Edition2027).unwrap();
            crate::types::check(&module).unwrap();
            let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
            assert_eq!(ast::statement_replace_relations(&action.body[0]).len(), 2);
            let ir = crate::ir::generate(&module).unwrap();
            let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected IR action") };
            assert_eq!(action.successor_relations.len(), 2);
        }
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        let source_relations = ast::statement_replace_relations(&action.body[0]);
        assert_eq!(source_relations.iter().map(|relation| relation.before.as_str()).collect::<Vec<_>>(), vec!["left", "right"]);
        assert_eq!(action.next_surface.as_ref().unwrap().dispositions.len(), 2);
        let ir = crate::ir::generate(&module).unwrap();
        let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected IR action") };
        assert_eq!(
            action.successor_relations.iter().map(|relation| relation.input_binding.as_str()).collect::<Vec<_>>(),
            vec!["right", "left"]
        );
        assert_eq!(action.successor_relations[0].occurrence, 0);
        assert_eq!(action.successor_relations[1].occurrence, 1);
        let formatted = crate::fmt::format_default(&module).unwrap();
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&reparsed).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
    }

    #[test]
    fn branch_field_obligations_keep_the_output_role_identity() {
        let source = r#"
module field_roles;
resource Token has store { amount: u64, }
action inspect(read before: Token, witness choose: bool,  output left: Token, output right: Token) {
    if choose { preserve left from before { amount, } } else { preserve right from before { amount, } }
}
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let error = crate::types::check(&module).unwrap_err();
        assert!(error.message.contains("incomplete branch constraints"), "{error}");
        assert!(error.message.contains("output 'left'") || error.message.contains("output 'right'"), "{error}");
    }

    #[test]
    fn branch_field_obligations_agree_for_single_and_block_requirements() {
        let source = r#"
module requirement_forms;
resource Token has store { amount: u64, }
action inspect(witness choose: bool,  output next: Token) {
    if choose { require next.amount > 0; } else { require { next.amount > 0, }; }
}
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        let incomplete = source.replace("require { next.amount > 0, };", "require { true, };");
        let error = crate::types::check(&parse(&incomplete, CellScriptEdition::Edition2027).unwrap()).unwrap_err();
        assert!(error.message.contains("incomplete branch constraints"), "{error}");
    }

    #[test]
    fn conditional_relation_values_materialize_each_sibling_field_read() {
        let source = r#"
module conditional_values;
resource Token has store, replace, relock { amount: u64, }
action update(
        input token: Token,
        witness increment: bool,
        output next: Token,
    ) {
        replace token -> next {
            data { amount = if increment { token.amount + 1 } else { token.amount }, };
            lock = same;
            type_script = same;
            capacity = same;
        }
    }
"#;
        let lifted = source
            .replace(
                "        replace token -> next",
                "        let updated = if increment { token.amount + 1 } else { token.amount };\n        replace token -> next",
            )
            .replace("data { amount = if increment { token.amount + 1 } else { token.amount }, };", "data { amount = updated, };");
        for source in [source, lifted.as_str()] {
            let module = parse(source, CellScriptEdition::Edition2027).unwrap();
            crate::types::check(&module).unwrap();
            let ir = crate::ir::generate(&module).unwrap();
            let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected action") };
            let input = action.params.iter().find(|param| param.name == "token").unwrap().binding.id;
            let read_blocks = action.body.blocks.iter().filter(|block| block.instructions.iter().any(|instruction|
                matches!(instruction, crate::ir::IrInstruction::FieldAccess { obj: crate::ir::IrOperand::Var(var), field, .. } if var.id == input && field == "amount")
            )).map(|block| block.id).collect::<Vec<_>>();
            assert_eq!(read_blocks.len(), 2, "both IfExpr lowerings must retain independent sibling reads");
            assert_ne!(read_blocks[0], read_blocks[1]);
        }
    }

    #[test]
    fn loop_conditions_cannot_dispose_a_group_role_repeatedly() {
        let source = BRANCH_REPLACEMENT.replace(
            "        let minimum = 1;",
            "        while { replace token -> next { data { id = same, amount = same, }; lock = same; type_script = same; capacity = same; } false } {}\n        let minimum = 1;",
        );
        let error = parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert!(error.message.contains("inside a loop"), "{error}");
    }

    #[test]
    fn fresh_values_register_nested_successors_without_duplicate_source_items() {
        let source = r#"
module fresh_nested_relations;
resource Token has store, create, replace, relock { amount: u64, enabled: bool, }
action update(
        input token: Token,
        witness recipient: ScriptHash,
        output next: Token,
        output outer: Token,
    ) {
        fresh outer {
            data {
                amount = 0,
                enabled = {
                    replace token -> next {
                        data { amount = same, enabled = same, };
                        lock = same;
                        type_script = same;
                        capacity = same;
                    }
                    true
                },
            };
            identity = none;
            lock = exact_hash(recipient);
        }
    }
"#;
        let mut module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&module).unwrap();
        for optimized in [false, true] {
            if optimized {
                crate::optimize::optimize_module(&mut module, crate::CompilerDiagnostics::default()).unwrap();
            }
            let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
            let surface = action.next_surface.as_ref().unwrap();
            assert_eq!(surface.dispositions.len(), 2);
            assert_eq!(surface.body_items.len(), 1, "the fresh constructor owns the complete source range");
            let formatted = crate::fmt::format_default(&module).unwrap();
            assert_eq!(formatted.matches("replace token -> next").count(), 1);
            let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
            crate::types::check(&reparsed).unwrap();
            assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);
            let ir = crate::ir::generate(&module).unwrap();
            let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected IR action") };
            assert_eq!(action.successor_relations.len(), 1);
        }
    }

    #[test]
    fn branch_local_lifecycle_policies_keep_their_source_and_cfg_occurrences() {
        let cases = [
            include_str!("../tests/syntax_combo/authoring/branch-fresh.cell"),
            include_str!("../tests/syntax_combo/authoring/branch-retire.cell"),
            include_str!("../tests/syntax_combo/authoring/branch-pool.cell"),
            include_str!("../tests/syntax_combo/authoring/branch-pool-many.cell"),
            include_str!("../tests/syntax_combo/authoring/branch-continue-retire.cell"),
        ];
        for source in cases {
            let mut module = parse(source, CellScriptEdition::Edition2027).unwrap();
            crate::types::check(&module).unwrap();
            let formatted = crate::fmt::format_default(&module).unwrap();
            let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
            assert!(crate::ast::semantic_shape::SemanticEq::same_semantics(&module, &reparsed));
            for optimized in [false, true] {
                if optimized {
                    crate::optimize::optimize_module(&mut module, crate::CompilerDiagnostics::default()).unwrap();
                }
                let ir = crate::ir::generate(&module).unwrap();
                let crate::ir::IrItem::Action(action) = ir.items.last().unwrap() else { panic!("expected action") };
                let mut sites = std::collections::BTreeMap::new();
                for source in &action.source_dispositions {
                    let execution = source.execution.as_ref().expect("actual lifecycle event");
                    sites
                        .entry(source.input_binding.as_ref().or(source.output_binding.as_ref()).unwrap())
                        .or_insert_with(Vec::new)
                        .push(execution.operation_block);
                }
                for relation in &action.successor_relations {
                    sites.entry(&relation.input_binding).or_insert_with(Vec::new).push(relation.consume_block);
                    sites.entry(&relation.output_binding).or_insert_with(Vec::new).push(relation.create_block);
                }
                for alternatives in sites.values() {
                    assert_eq!(alternatives.len(), 2);
                    assert_ne!(alternatives[0], alternatives[1], "alternative role events must retain distinct CFG sites");
                }
            }
        }
    }

    #[test]
    fn branch_local_lifecycle_rejects_missing_duplicate_and_loop_dispositions() {
        let prefix = "module branches; resource Note has store, consume, burn identity(field(id)) { id: u64, } action close(input note: Note, witness choose: bool)";
        for body in [
            "{ if choose { retire note { absence = field(id); } } }",
            "{ retire note { absence = field(id); } retire note { absence = field(id); } }",
            "{ while choose { retire note { absence = field(id); } } }",
        ] {
            let error = parse(&format!("{prefix} {body}"), CellScriptEdition::Edition2027).unwrap_err();
            assert!(error.message.contains("path") || error.message.contains("twice") || error.message.contains("loop"), "{error}");
        }
    }
}
