// Adversarial security audit test cases for CellScript compiler.
// These tests probe for crashes, hangs, memory corruption, and DoS vectors.

#[cfg(test)]
mod adversarial {
    use cellscript::compile;
    use cellscript::CompileOptions;

    /// Helper: attempt full compilation, return error message or ok
    fn try_compile(source: impl AsRef<str>) -> Result<String, String> {
        compile(source.as_ref(), CompileOptions::default()).map(|_| "OK".to_string()).map_err(|e| e.message)
    }

    // ---- DEEPLY NESTED EXPRESSIONS (stack overflow) ----

    #[test]
    fn deeply_nested_parentheses_in_parser() {
        // 256 levels of nesting should hit the parser guard, not the process stack.
        let depth = 256;
        let input =
            format!("module test\naction test() -> u64\nwhere\n    return ({})\n", "(".repeat(depth) + "1" + &")".repeat(depth));
        let result = try_compile(input);
        // Should be an error (recursion limit exceeded), not a panic/crash
        assert!(result.is_err(), "Expected error for deeply nested parens, got: {:?}", result);
    }

    #[test]
    fn deeply_nested_binary_expressions() {
        // Create a chain of binary additions: ((((1 + 1) + 1) + 1) + ...)
        let depth = 300;
        let expr: String = (0..depth).map(|_| "1u64 + ".to_string()).collect::<Vec<_>>().join("");
        let input = format!(
            "module test\naction test() -> u64\nwhere\n    return {}\n",
            &expr[..expr.len() - 4] // strip trailing " + "
        );
        let result = try_compile(input);
        // Should either succeed or error gracefully, not panic
        let _ = result;
    }

    #[test]
    fn deeply_nested_if_expressions() {
        let depth = 200;
        let mut input = "module test\naction test() -> u64\nwhere\n    return ".to_string();
        for _ in 0..depth {
            input.push_str("if true then ");
        }
        input.push_str("1u64");
        for _ in 0..depth {
            input.push_str(" else 0u64");
        }
        input.push('\n');
        let result = try_compile(input);
        let _ = result; // should not panic
    }

    #[test]
    fn deeply_nested_struct_literal() {
        // Nest struct init expressions deeply
        let depth = 150;
        let mut input = "module test\nstruct S { x: u64, y: u64 }\naction test() -> S\nwhere\n    return ".to_string();
        for _ in 0..depth {
            input.push_str("S {{ x: 1u64, y: ");
        }
        input.push_str("1u64");
        for _ in 0..depth {
            input.push_str(" }}");
        }
        input.push('\n');
        let result = try_compile(input);
        let _ = result; // should not panic
    }

    // ---- VERY LONG IDENTIFIERS ----

    #[test]
    fn very_long_identifier() {
        let name = "a".repeat(100_000);
        let input = format!("module test\naction {}() -> u64\nwhere\n    return 1u64\n", name);
        let result = try_compile(&input);
        // Should handle gracefully (lexer has 64KB identifier limit)
        assert!(result.is_err(), "Expected error for very long identifier");
        let err = result.unwrap_err();
        assert!(err.contains("byte limit"), "Expected byte limit error, got: {}", err);
    }

    // ---- VERY LONG STRINGS ----

    #[test]
    fn very_long_string_literal() {
        let string_content = "\"".to_string() + &"A".repeat(2_000_000) + "\"";
        let input = format!("module test\naction test() -> u64\nwhere\n    let s = {}\n    return 1u64\n", string_content);
        let result = try_compile(&input);
        assert!(result.is_err(), "Expected error for very long string");
    }

    // ---- EMPTY INPUTS ----

    #[test]
    fn empty_input() {
        let result = try_compile("");
        assert!(result.is_err(), "Empty input should be an error");
    }

    #[test]
    fn whitespace_only_input() {
        let result = try_compile("   \n  \t  \n");
        assert!(result.is_err(), "Whitespace-only input should be an error");
    }

    #[test]
    fn null_bytes_in_input() {
        // Null bytes should either be rejected or handled; not cause UB
        let input = "module test\naction test() -> u64\nwhere\n    // \x00 null\n    return 1u64\n";
        let result = try_compile(input);
        // Should not panic; either error or ok is acceptable
        let _ = result;
    }

    // ---- NUMERIC EDGE CASES ----

    #[test]
    fn numeric_zero() {
        let input = "module test\naction test() -> u64\nwhere\n    return 0\n";
        let result = try_compile(input);
        assert!(result.is_ok(), "Zero should compile: {:?}", result);
    }

    #[test]
    fn numeric_u64_max() {
        let input = format!("module test\naction test() -> u64\nwhere\n    return {}\n", u64::MAX);
        let result = try_compile(&input);
        assert!(result.is_ok(), "u64::MAX should compile: {:?}", result);
    }

    #[test]
    fn numeric_overflow_single_literal() {
        // 18446744073709551616 is u64::MAX + 1, should be rejected
        let input = "module test\naction test() -> u64\nwhere\n    return 18446744073709551616\n";
        let result = try_compile(input);
        assert!(result.is_err(), "u64::MAX + 1 should be rejected");
    }

    #[test]
    fn numeric_very_long() {
        // 1000 digit number
        let digits = "9".repeat(1000);
        let input = format!("module test\naction test() -> u64\nwhere\n    return {}\n", digits);
        let result = try_compile(&input);
        assert!(result.is_err(), "Very long number should be rejected or truncated");
    }

    #[test]
    fn hex_literal_max() {
        let input = "module test\naction test() -> u64\nwhere\n    return 0xFFFFFFFFFFFFFFFF\n";
        let result = try_compile(input);
        assert!(result.is_ok(), "Max hex literal should compile: {:?}", result);
    }

    #[test]
    fn hex_literal_overflow() {
        let input = "module test\naction test() -> u64\nwhere\n    return 0x10000000000000000\n";
        let result = try_compile(input);
        assert!(result.is_err(), "Overflow hex should be rejected");
    }

    // ---- DIVISION BY ZERO ----

    #[test]
    fn compile_time_division_by_zero_literal() {
        // CellScript doesn't have Rust-style literal suffixes (5u64).
        // Use let-annotation syntax to get typed division.
        let input = "module test\naction test() -> u64\nwhere\n    let x: u64 = 5 / 0\n    return x\n";
        let result = try_compile(input);
        assert!(result.is_err(), "Division by zero should be caught at compile time");
        let err = result.unwrap_err();
        assert!(err.contains("division") || err.contains("zero"), "Error should mention division or zero: {}", err);
    }

    #[test]
    fn compile_time_modulo_by_zero_literal() {
        let input = "module test\naction test() -> u64\nwhere\n    let x: u64 = 5 % 0\n    return x\n";
        let result = try_compile(input);
        assert!(result.is_err(), "Modulo by zero should be caught at compile time");
        let err = result.unwrap_err();
        assert!(
            err.contains("modulo") || err.contains("remainder") || err.contains("zero"),
            "Error should mention modulo/remainder/zero: {}",
            err
        );
    }

    // ---- MALFORMED SYNTAX ----

    #[test]
    fn incomplete_module_declaration() {
        let result = try_compile("module");
        assert!(result.is_err());
    }

    #[test]
    fn action_with_mismatched_braces() {
        let result = try_compile("module test\naction test() -> u64\nwhere\n    return 1u64\n{");
        assert!(result.is_err());
    }

    #[test]
    fn random_bytes() {
        // Random binary data as input
        let bytes: Vec<u8> = (0..=255).collect();
        let input = String::from_utf8_lossy(&bytes);
        let result = try_compile(&input);
        // Should error gracefully, not crash
        assert!(result.is_err());
    }

    #[test]
    fn many_consecutive_newlines() {
        let input = "module test\n".to_string() + &"\n".repeat(100_000) + "action test() -> u64\nwhere\n    return 1u64\n";
        let result = try_compile(&input);
        let _ = result; // should not hang or crash
    }

    // ---- LEXER EDGE CASES ----

    #[test]
    fn unterminated_string() {
        let input = r#"module test
action test() -> u64
where
    let s = "unterminated
    return 1u64
"#;
        let result = try_compile(input);
        assert!(result.is_err(), "Unterminated string should error");
    }

    #[test]
    fn unterminated_block_comment() {
        let input = "module test\n/* unterminated\naction test() -> u64\nwhere\n    return 1u64\n";
        let result = try_compile(input);
        assert!(result.is_err(), "Unterminated block comment should error");
    }

    #[test]
    fn nested_block_comments() {
        let input = r#"module test
action test() -> u64
where
    /* outer /* inner */ still outer */
    return 1u64
"#;
        let result = try_compile(input);
        let _ = result; // behavior undefined but should not crash
    }

    #[test]
    fn very_long_line_comment() {
        // 2MB line comment (lexer limit is 1MB for comments)
        let comment = "// ".to_string() + &"x".repeat(2_500_000);
        let input = format!("module test\naction test() -> u64\nwhere\n    {}\n    return 1u64\n", comment);
        let result = try_compile(&input);
        assert!(result.is_err(), "Overly long comment should be rejected");
    }

    // ---- UNICODE EDGE CASES ----

    #[test]
    fn emoji_in_identifier() {
        let input = "module test\naction test_😀() -> u64\nwhere\n    return 1u64\n";
        let result = try_compile(input);
        let _ = result; // should handle gracefully
    }

    #[test]
    fn emoji_in_string() {
        let input = "module test\naction test() -> u64\nwhere\n    let s = \"😀\"\n    return 1u64\n";
        let result = try_compile(input);
        let _ = result;
    }

    // ---- VERY LARGE PROGRAMS (resource consumption) ----

    #[test]
    fn many_top_level_items() {
        let mut input = "module test\n".to_string();
        for i in 0..1000 {
            input.push_str(&format!("const C{}: u64 = {}\n", i, i));
        }
        input.push_str("action test() -> u64\nwhere\n    return C0\n");
        let result = try_compile(&input);
        let _ = result; // should not OOM
    }

    #[test]
    fn many_params_in_action() {
        let mut params = Vec::new();
        for i in 0..500 {
            params.push(format!("p{}: u64", i));
        }
        let body = "    return p0\n".to_string();
        let input = format!("module test\naction test({}) -> u64\nwhere\n{}\n", params.join(", "), body);
        let result = try_compile(&input);
        let _ = result; // should not crash
    }

    // ---- TYPE CHECKER RECURSION ----

    #[test]
    fn deeply_nested_type_expression() {
        // Deeply nested generic types like Vec<Vec<Vec<...<u64>>>>
        let depth = 200;
        let mut ty = "u64".to_string();
        for _ in 0..depth {
            ty = format!("Vec<{}>", ty);
        }
        let input = format!("module test\naction test() -> {}\nwhere\n    return vec_new()\n", ty);
        let result = try_compile(&input);
        // Parser has recursion guard on types, should fail gracefully
        assert!(result.is_err(), "Expected error for deeply nested types");
    }

    // ---- IMPORT/RESOLUTION ----

    #[test]
    fn self_referencing_module() {
        let input = "module test\nuse test::foo\naction test() -> u64\nwhere\n    return 1u64\n";
        let result = try_compile(input);
        // Should be rejected as circular or unresolved
        assert!(result.is_err(), "Self-referencing import should error");
    }

    // ---- COMPILER PIPELINE CONSISTENCY ----

    #[test]
    fn compile_returns_valid_utf8_artifact_for_asm_target() {
        let input = "module test\naction test() -> u64\nwhere\n    return 42u64\n";
        let result = compile(input, CompileOptions::default());
        if let Ok(r) = result {
            // Default target is riscv64-asm, artifact should be valid UTF-8
            let _ = String::from_utf8(r.artifact_bytes).expect("ASM artifact should be valid UTF-8");
        }
        // It's ok if it errors, we just check the artifact format when it succeeds
    }

    #[test]
    fn compile_metadata_returns_consistent_types() {
        let input = "module test\naction test() -> u64\nwhere\n    return 42u64\n";
        let result = cellscript::compile_metadata(input, None);
        if let Ok(meta) = result {
            assert!(!meta.actions.is_empty(), "Should have at least one action");
            assert_eq!(meta.actions[0].name, "test");
        }
    }

    // ---- HEAP ALLOCATION PROBES ----

    #[test]
    fn very_long_byte_string() {
        // byte string literal close to the 1MB limit
        let byte_content = "\"b\"".to_string() + &"\\x41".repeat(1_500_000);
        let input = format!("module test\naction test() -> u64\nwhere\n    let b = {}\n    return 1u64\n", byte_content);
        let result = try_compile(&input);
        assert!(result.is_err(), "Overly long byte string should be rejected");
    }

    // ---- LEFT-ASSOCIATIVE BINARY CHAIN (bypasses parser recursion guard) ----

    #[test]
    fn deep_left_associative_additions_parser_ok() {
        // Parser uses iterative precedence climbing for binary ops,
        // so additions parse with recursion depth 0.
        // But the AST is deeply left-nested: (((1+1)+1)+1)+...
        // This tests whether the type checker / IR lowering can handle it
        // without stack overflow.
        let depth = 500;
        let expr: String = (0..depth).map(|_| "1u64 + ".to_string()).collect::<Vec<_>>().join("");
        let input = format!("module test\naction test() -> u64\nwhere\n    return {}\n", &expr[..expr.len() - 4]);
        let result = try_compile(&input);
        // Should not panic or stack overflow — should either succeed or error gracefully
        let _ = result;
    }

    #[test]
    fn deep_left_associative_comparisons() {
        // Same pattern but with comparison operators to exercise the type checker
        let depth = 200;
        let expr: String = (0..depth).map(|_| "1u64 == ".to_string()).collect::<Vec<_>>().join("");
        let input = format!("module test\naction test() -> bool\nwhere\n    return {}\n", &expr[..expr.len() - 5]);
        let result = try_compile(&input);
        // Must not panic
        let _ = result;
    }

    #[test]
    fn deep_left_associative_mixed_ops() {
        // Interleave +, -, *, / to exercise different binary paths
        let depth = 150;
        let ops = ["+", "-", "*", "+"];
        let expr: String = (0..depth).map(|i| format!("1u64 {} ", ops[i % ops.len()])).collect::<Vec<_>>().join("");
        let input = format!("module test\naction test() -> u64\nwhere\n    return {}\n", &expr[..expr.len() - 4]);
        let result = try_compile(&input);
        // Must not panic
        let _ = result;
    }
}
