//! Edition 2027 source frontend over the shared declaration, value and statement
//! kernel. The bounded native preview retains its own syntax checks. All routes
//! produce structured AST before the shared typed semantic foundation.

use crate::ast;
use crate::edition::CellScriptEdition;
use crate::error::{CompileError, Result};
use crate::lexer;

mod authoring;
mod migrate;
mod next;

pub use migrate::{legacy_temporal_migration_diagnostics, migrate_legacy_temporal_source, TemporalMigrationCandidate};

/// Tooling uses the same contextual surface selection as the parser. This
/// remains usable while a native container body is incomplete during editing.
pub(crate) fn uses_native_preview(source: &str) -> bool {
    lexer::lex(source).is_ok_and(|tokens| next::has_native_surface(&tokens))
}

pub fn parse(source: &str, edition: CellScriptEdition) -> Result<ast::Module> {
    match edition {
        CellScriptEdition::Edition2027 => next::parse(source),
    }
}

/// A source-rewrite entry point: one lexing pass, the same complete parser,
/// and the exact source slices needed to retain comments and token grouping.
pub fn parse_document(source: &str, edition: CellScriptEdition) -> Result<ParsedDocument<'_>> {
    let mut syntax = lexer::lex_document(source)?;
    let (ast, nodes) = match edition {
        CellScriptEdition::Edition2027 => next::parse_tokens_with_syntax(&syntax.tokens)?,
    };
    syntax.nodes = nodes;
    Ok(ParsedDocument { syntax, ast })
}

pub struct ParsedDocument<'a> {
    pub syntax: crate::syntax::SyntaxDocument<'a>,
    pub ast: ast::Module,
}

pub fn parse_diagnostics(source: &str, edition: CellScriptEdition) -> std::result::Result<ast::Module, Vec<CompileError>> {
    match edition {
        CellScriptEdition::Edition2027 => next::parse_diagnostics(source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Item;
    use crate::error::Span;

    #[test]
    fn current_edition_accepts_explicit_and_markerless_authoring() {
        let implicit = "module demo;\naction main(value: u64) -> u64 {  return value; }";
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
        assert!(action.next_surface.is_none(), "a legacy consume must not acquire a native disposition policy");

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
action main(value: u64) -> u64 { return type_script(value); }
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);
    }

    #[test]
    fn authoring_lock_constraint_blocks_keep_the_legacy_verification_model() {
        let source = "module demo;\nlock owner(witness value: u64) { require value > 0;\n return true; }";
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Lock(lock) = module.items.last().unwrap() else { panic!("expected lock") };
        assert!(matches!(lock.body[0], ast::Stmt::Discard(ast::Expr::Require(_))));
        assert!(lock.next_surface.is_none());
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

type_script TokenTransfer on type_group<Token> {
    action transfer(
        input token: Token from group_input[0],
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
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
}
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native Edition 2027 entry to lower to an action");
        };
        let surface = action.next_surface.as_ref().expect("native surface marker");
        assert_eq!(surface.container_name, "TokenTransfer");
        assert_eq!(surface.trigger_type, "Token");
        assert!(matches!(action.body[0], ast::Stmt::Discard(ast::Expr::Require(_))));
        assert_eq!(surface.dispositions.len(), 1);
        assert_eq!(action.params.len(), 2);
        assert_eq!(action.outputs.len(), 1);

        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("type_script TokenTransfer on type_group<Token>"));
        assert!(formatted.contains("replace token -> next"));
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);

        let missing_field = source.replace("                amount = same,\n", "");
        assert!(parse(&missing_field, CellScriptEdition::Edition2027).unwrap_err().message.contains("exhaustively list fields"));
        let wrong_ordinal = source.replace("group_output[0]", "group_output[1]");
        assert!(parse(&wrong_ordinal, CellScriptEdition::Edition2027).unwrap_err().message.contains("non-canonical"));
        let mismatched_output = source
            .replace(
                "resource Token has",
                "resource Other has store, replace, relock { owner: Address, amount: u64 }\nresource Token has",
            )
            .replace("output next: Token", "output next: Other");
        assert!(parse(&mismatched_output, CellScriptEdition::Edition2027)
            .unwrap_err()
            .message
            .contains("input/output ports must all use"));
        let unhashed_lock = source.replace("lock = exact(recipient)", "lock = recipient");
        assert!(parse(&unhashed_lock, CellScriptEdition::Edition2027).unwrap_err().message.contains("exact"));
    }

    #[test]
    fn next_frontend_parses_fresh_retire_and_audit_surfaces_fail_closed() {
        let fresh = r#"
module demo;
#[type_id("demo::Token:v1")]
resource Token has store, create, burn identity(ckb_type_id) { amount: u64, }
type_script TokenMint on type_group<Token> {
    action mint(
        witness amount: u64 from group_witness.input_type,
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        require amount > 0;
        audit issuance_policy {
            expected_evidence = external_policy(recipient);
        }
        fresh next {
            data { amount = amount; }
            identity = ckb_type_id;
            type_script = declared;
            lock = exact(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
    }
}
"#;
        let module = parse(fresh, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native fresh entry to lower to an action");
        };
        let surface = action.next_surface.as_ref().expect("native surface marker");
        assert_eq!(surface.audits.len(), 1);
        assert!(matches!(surface.dispositions.as_slice(), [crate::ast::NextDisposition::Fresh(_)]));
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("audit issuance_policy"));
        assert!(formatted.contains("fresh next"));
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);

        let duplicate_audit = fresh.replace(
            "        fresh next {",
            "        audit issuance_policy { expected_evidence = external_policy(amount); }\n        fresh next {",
        );
        assert!(parse(&duplicate_audit, CellScriptEdition::Edition2027).unwrap_err().message.contains("duplicate audit declaration"));
        let missing_field = fresh.replace("            data { amount = amount; }", "            data { }");
        assert!(parse(&missing_field, CellScriptEdition::Edition2027).unwrap_err().message.contains("exhaustively list fields"));

        let retire = r#"
module demo;
resource Note has store, consume, burn identity(field(note_id)) { note_id: u64, amount: u64, }
type_script NoteRetirement on type_group<Note> {
    action retire_note(input note: Note from group_input[0]) {
        require note.amount == 0;
        retire note {
            absence = field(note_id);
            data = discarded;
            lock = none;
            type_script = absent;
            capacity = released;
            cardinality = one;
        }
    }
}
"#;
        let module = parse(retire, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native retirement entry to lower to an action");
        };
        assert!(matches!(action.next_surface.as_ref().unwrap().dispositions.as_slice(), [crate::ast::NextDisposition::Retire(_)]));
        let missing_capacity = retire.replace("            capacity = released;\n", "");
        assert!(parse(&missing_capacity, CellScriptEdition::Edition2027).is_err());

        let reordered_outputs = r#"
module demo;
resource Token has store, create { amount: u64, }
type_script TokenMint on type_group<Token> {
    action mint(
        witness amount: u64 from group_witness.input_type,
        witness recipient: Address from group_witness.input_type,
        output first: Token from group_output[0],
        output second: Token from group_output[1],
    ) {
        fresh second {
            data { amount = amount; }
            identity = none;
            type_script = declared;
            lock = exact(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
        fresh first {
            data { amount = amount; }
            identity = none;
            type_script = declared;
            lock = exact(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
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
type_script TokenPool on type_group<Token> {
    action merge(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        witness recipient: Address from group_witness.input_type,
        output merged: Token from group_output[0],
    ) {
        require left.amount > 0;
        pool value_flow {
            inputs { left, right }
            outputs { merged }
            data {
                owner { merged = recipient; }
                amount = conserve;
            }
            identity = pooled;
            type_script = same;
            lock { merged = exact(recipient); }
            capacity = builder_computed;
            cardinality = declared;
        }
    }
}
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else {
            panic!("expected native pool entry to lower to an action");
        };
        assert!(matches!(action.next_surface.as_ref().unwrap().dispositions.as_slice(), [crate::ast::NextDisposition::Pool(_)]));
        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("pool value_flow"));
        assert!(formatted.contains("amount = conserve"));
        assert_eq!(crate::fmt::format_default(&parse(&formatted, CellScriptEdition::Edition2027).unwrap()).unwrap(), formatted);

        let no_conservation =
            source.replace("                amount = conserve;", "                amount { merged = merged.amount; }");
        assert!(parse(&no_conservation, CellScriptEdition::Edition2027).unwrap_err().message.contains("at least one numeric field"));
        let missing_lock = source.replace("            lock { merged = exact(recipient); }", "            lock { }");
        assert!(parse(&missing_lock, CellScriptEdition::Edition2027).unwrap_err().message.contains("assign every pool output"));
        let reused_input = source.replace("inputs { left, right }", "inputs { left, left }");
        assert!(parse(&reused_input, CellScriptEdition::Edition2027).unwrap_err().message.contains("each be distinct"));
    }

    #[test]
    fn next_frontend_parses_native_lock_script_surface() {
        let source = r#"
module demo;

resource Vault has store {
    owner: Address,
}

lock_script VaultOwner on lock_group {
    lock unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
    }
}
"#;
        let module = parse(source, CellScriptEdition::Edition2027).unwrap();
        let Item::Lock(lock) = module.items.last().unwrap() else {
            panic!("expected native Edition 2027 entry to lower to a lock");
        };
        let surface = lock.next_surface.as_ref().expect("native lock surface marker");
        assert_eq!(surface.container_name, "VaultOwner");
        assert_eq!(lock.body.len(), 2);
        assert_eq!(lock.params.len(), 3);

        let formatted = crate::fmt::format_default(&module).unwrap();
        assert!(formatted.contains("lock_script VaultOwner on lock_group"));
        assert!(formatted.contains("protected vault: Vault from group_input[0]"));
        assert!(formatted.contains("lock_args owner: Address from current_script.args"));
        let reparsed = parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(crate::fmt::format_default(&reparsed).unwrap(), formatted);

        let wrong_ordinal = source.replace("group_input[0]", "group_input[1]");
        assert!(parse(&wrong_ordinal, CellScriptEdition::Edition2027).unwrap_err().message.contains("non-canonical"));
        let no_protected = source.replace("        protected vault: Vault from group_input[0],\n", "");
        assert!(parse(&no_protected, CellScriptEdition::Edition2027).unwrap_err().message.contains("exactly one protected"));
    }

    const BRANCH_REPLACEMENT: &str = r#"
module branch_authoring;
resource Token has store, replace, relock { id: u64, amount: u64, }
type_script TokenUpdate on type_group<Token> {
    action update(
        input token: Token from group_input[0],
        witness increment: bool from group_witness.input_type,
        output next: Token from group_output[0],
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
                data { same { id, amount, }, };
                lock = same;
                type_script = same;
                capacity = same;
            }
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
            "        replace token -> next { data { same { id, amount, } }; lock = same; type_script = same; capacity = same; }\n        let minimum = 1;",
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
            .replace("\n    }\n}\n", "\n        }\n    }\n}\n");
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
            BRANCH_REPLACEMENT.replace("action update", "entry update"),
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
type_script TokenMint on type_group<Token> {
    action mint(
        witness recipient: ScriptHash from group_witness.input_type,
        output token: Token from group_output[0],
    ) {
        let destination = recipient;
        fresh token {
            data { amount = 1; }
            identity = none;
            type_script = declared;
            lock = exact_hash(destination);
            capacity = builder_computed;
            cardinality = one;
        }
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
        let source = format!("{before_else}        }} else {{ require false; }}\n    }}\n}}\n");
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
type_script TokenMint on type_group<Token> {
    action mint(
        witness recipient: ScriptHash from group_witness.input_type,
        output token: Token from group_output[0],
    ) {
        let unused = 7;
        let amount = 3;
        audit issuance { expected_evidence = external_policy(recipient); }
        fresh token {
            data { amount = amount; }
            identity = ckb_type_id;
            type_script = declared;
            lock = exact_hash(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
    }
}
"#;
        let mut module = parse(source, CellScriptEdition::Edition2027).unwrap();
        crate::optimize::optimize_module(&mut module, crate::CompilerDiagnostics::default()).unwrap();
        let Item::Action(action) = module.items.last().unwrap() else { panic!("expected action") };
        let surface = action.next_surface.as_ref().unwrap();
        assert!(matches!(
            surface.body_items.as_slice(),
            [ast::NextBodyItem::Audit(0), ast::NextBodyItem::Disposition { index: 0, start_statement: 0, statement_count: 1 }]
        ));
        let ast::NextDisposition::Fresh(fresh) = &surface.dispositions[0] else { panic!("expected fresh") };
        assert!(matches!(fresh.data_fields[0].1, ast::Expr::Integer(3)));
        let ast::Stmt::Discard(ast::Expr::CreateUnique(create)) = &action.body[0] else { panic!("expected unique creation") };
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
type_script TokenMint on type_group<Token> {
    action mint(
        witness amount: u64 from group_witness.input_type,
        witness recipient: ScriptHash from group_witness.input_type,
        output token: Token from group_output[0],
    ) {
        let checked = enforce(verify(entry(effects(fresh(retire(audit(pool(amount))))))));
        require checked > 0;
        fresh token {
            data { amount = checked; }
            identity = none;
            type_script = declared;
            lock = exact_hash(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
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
type_script TokenUpdate on type_group<Token> {
    action update(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        output next_left: Token from group_output[0],
        output next_right: Token from group_output[1],
    ) {
        replace left -> next_left {
            data {
                amount = same,
                enabled = {
                    replace right -> next_right {
                        data { same { amount, enabled, }, };
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
action inspect(read before: Token, witness choose: bool) -> (left: Token, right: Token) {
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
action inspect(witness choose: bool) -> next: Token {
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
type_script TokenUpdate on type_group<Token> {
    action update(
        input token: Token from group_input[0],
        witness increment: bool from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        replace token -> next {
            data { amount = if increment { token.amount + 1 } else { token.amount }, };
            lock = same;
            type_script = same;
            capacity = same;
        }
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
            "        while { replace token -> next { data { same { id, amount, } }; lock = same; type_script = same; capacity = same; } false } {}\n        let minimum = 1;",
        );
        let error = parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert!(error.message.contains("inside a loop"), "{error}");
    }

    #[test]
    fn fresh_values_register_nested_successors_without_duplicate_source_items() {
        let source = r#"
module fresh_nested_relations;
resource Token has store, create, replace, relock { amount: u64, enabled: bool, }
type_script TokenUpdate on type_group<Token> {
    action update(
        input token: Token from group_input[0],
        witness recipient: ScriptHash from group_witness.input_type,
        output next: Token from group_output[0],
        output outer: Token from group_output[1],
    ) {
        fresh outer {
            data {
                amount = 0;
                enabled = {
                    replace token -> next {
                        data { same { amount, enabled, }, };
                        lock = same;
                        type_script = same;
                        capacity = same;
                    }
                    true
                };
            }
            identity = none;
            type_script = declared;
            lock = exact_hash(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
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
}
