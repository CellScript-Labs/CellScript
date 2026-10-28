use super::*;
use crate::CellScriptEdition;

fn action(module: &Module) -> &ActionDef {
    module.items.iter().find_map(|item| if let Item::Action(action) = item { Some(action) } else { None }).unwrap()
}

#[test]
fn unused_bindings_preserve_partial_evaluation_in_both_editions() {
    let source = r#"
module partial_evaluation
struct Config { value: u64 }
action check(witness value: u128, witness divisor: u128, witness amount: u64, witness values: [u64; 2], config: Config) {
    verification
    let unused_add = value + divisor
    let _ = value - divisor
    let _ = value * divisor
    let _ = value / divisor
    let _ = value % divisor
    let _ = value << amount
    let _ = value >> amount
    let _ = value as u8
    let _ = values[amount]
    let _ = config.value
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for level in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let mut module = crate::frontend::parse(source, edition).unwrap();
            optimize_module(&mut module, level).unwrap();
            assert_eq!(action(&module).body.len(), 10, "{edition:?} level {level} erased potentially failing evaluation");
        }
    }
}

#[test]
fn substitution_does_not_discard_or_duplicate_partial_arguments() {
    let source = r#"
module partial_arguments
struct Config { value: u64 }
fn ignore(value: u64) -> u64 { return 7 }
fn ignore_small(value: u8) -> u64 { return 7 }
fn duplicate(value: u64) -> bool { return value > 0 && value < 10 }
action check(witness value: u64, witness divisor: u64, witness values: [u64; 2], config: Config) {
    verification
    let _ = ignore(value / divisor)
    let _ = ignore_small(value as u8)
    let _ = ignore(values[divisor])
    let _ = ignore(config.value)
    let _ = duplicate(value / divisor)
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for level in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let mut module = crate::frontend::parse(source, edition).unwrap();
            optimize_module(&mut module, level).unwrap();
            let mut calls = Vec::new();
            collect_call_names_from_stmts(&action(&module).body, &mut calls);
            assert_eq!(action(&module).body.len(), 5, "{edition:?} level {level}");
            assert_eq!(calls.iter().filter(|name| *name == "ignore").count(), 3, "{edition:?} level {level}");
            assert_eq!(calls.iter().filter(|name| *name == "ignore_small").count(), 1, "{edition:?} level {level}");
            assert_eq!(calls.iter().filter(|name| *name == "duplicate").count(), 1, "{edition:?} level {level}");
        }
    }
}

#[test]
fn narrow_constant_bindings_retain_runtime_shift_width() {
    let source = r#"
module narrow_constants
action check() {
    verification
    let value: u8 = 1
    let amount: u8 = 8
    let _ = value << amount
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for level in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let mut module = crate::frontend::parse(source, edition).unwrap();
            crate::types::check(&module).unwrap();
            optimize_module(&mut module, level).unwrap();
            crate::types::check(&module).unwrap();
            assert!(
                matches!(action(&module).body.last(), Some(Stmt::Let(LetStmt {
                value: Expr::Binary(BinaryExpr { op: BinaryOp::Shl, left, right, .. }), ..
            })) if matches!(left.as_ref(), Expr::Identifier(name) if name == "value")
                && matches!(right.as_ref(), Expr::Identifier(name) if name == "amount")),
                "{edition:?} level {level}"
            );
        }
    }
}

#[test]
fn contextual_u128_overflow_cannot_be_hidden_by_host_u64_shift_wrapping() {
    let source = r#"
module contextual_shift
action check() {
    verification
    let _: u128 = (18446744073709551615 << 63) * 4
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for level in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let mut module = crate::frontend::parse(source, edition).unwrap();
            crate::types::check(&module).unwrap();
            optimize_module(&mut module, level).unwrap();
            crate::types::check(&module).unwrap();
            assert!(
                matches!(action(&module).body.first(), Some(Stmt::Let(LetStmt {
                value: Expr::Binary(BinaryExpr { op: BinaryOp::Mul, left, .. }), ..
            })) if matches!(left.as_ref(), Expr::Binary(BinaryExpr { op: BinaryOp::Shl, .. }))),
                "{edition:?} level {level}"
            );
        }
    }
}

#[test]
fn safe_constant_specialization_uses_definition_scope_and_keeps_failures() {
    let source = r#"
module safe_specialization
const LIMIT: u64 = 10
fn above(value: u64) -> bool { return value > LIMIT }
fn divide(value: u64) -> u64 { return 84 / value }
action check() -> bool {
    verification
    let _ = divide(2)
    let _ = divide(0)
    return above(11)
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        {
            let level = crate::CompilerDiagnostics::default();
            let mut module = crate::frontend::parse(source, edition).unwrap();
            crate::types::check(&module).unwrap();
            optimize_module(&mut module, level).unwrap();
            crate::types::check(&module).unwrap();
            assert_eq!(action(&module).body.len(), 2, "{edition:?} level {level}");
            assert!(matches!(action(&module).body.last(), Some(Stmt::Return(ReturnStmt { value: Some(Expr::Bool(true)), .. }))));
            let mut calls = Vec::new();
            collect_call_names_from_stmts(&action(&module).body, &mut calls);
            assert_eq!(calls, ["divide"]);
            assert!(matches!(&action(&module).body[0], Stmt::Let(LetStmt { value: Expr::Call(call), .. })
                if matches!(call.args.as_slice(), [Expr::Integer(0)])));
        }
    }
}

#[test]
fn transitive_partial_calls_retain_evaluation_and_typed_callee_context() {
    let source = r#"
module partial_closure
fn divide(value: u64) -> u64 { return 7 / value }
fn wrapper(value: u64) -> u64 { return divide(value) }
fn narrow(value: u128) -> u8 { return value as u8 }
action check(witness value: u64, witness wide: u128) {
    verification
    let _ = wrapper(value)
    let _ = wrapper(0)
    let _ = narrow(wide)
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for level in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
            let mut module = crate::frontend::parse(source, edition).unwrap();
            optimize_module(&mut module, level).unwrap();
            assert_eq!(action(&module).body.len(), 3, "{edition:?} level {level}");
            for name in ["divide", "wrapper", "narrow"] {
                assert!(
                    module.items.iter().any(|item| matches!(item, Item::Function(function) if function.name == name)),
                    "{edition:?} level {level} removed {name}"
                );
            }
        }
    }
}

#[test]
fn parameter_and_nonconstant_let_bindings_shadow_propagated_constants() {
    // Defensive AST-level coverage: normal compilation separately rejects
    // duplicate bindings, but this optimizer must not invent constants when
    // handed a module containing a shadowing binding.
    let source = r#"
module constant_shadowing
const divisor: u64 = 1
action check(witness divisor: u64, witness value: u64) {
    verification
    let _ = 7 / divisor
    let divisor = 2
    let divisor = value
    let _ = 7 / divisor
}
"#;
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        {
            let level = crate::CompilerDiagnostics::default();
            let mut module = crate::frontend::parse(source, edition).unwrap();
            optimize_module(&mut module, level).unwrap();
            let divisions = action(&module)
                .body
                .iter()
                .filter(|stmt| matches!(stmt, Stmt::Let(LetStmt { value: Expr::Binary(BinaryExpr { op: BinaryOp::Div, .. }), .. })))
                .count();
            assert_eq!(divisions, 2, "{edition:?} level {level} replaced a shadowed runtime divisor");
        }
    }
}
