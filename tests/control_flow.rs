use cellscript::{compile, CompileOptions};

#[test]
fn loop_control_requires_an_enclosing_loop() {
    for keyword in ["break", "continue"] {
        let source = format!(
            r#"
module invalid_loop_control;

fn invalid() {{
    {keyword};
}}
"#
        );
        let error = compile(&source, CompileOptions::default()).expect_err("loop control outside a loop must fail");
        assert!(error.message.contains("only valid inside a loop"), "{}", error.message);
    }
}

#[test]
fn labeled_loop_control_requires_a_visible_label() {
    let source = r#"
module invalid_loop_label;

fn invalid() -> u64 {
    while true {
        break missing;
    }
    return 0;
}
"#;
    let error = compile(source, CompileOptions::default()).expect_err("unknown loop label must fail");
    assert!(error.message.contains("unknown loop label 'missing'"), "{}", error.message);
}

#[test]
fn labeled_loop_control_compiles_to_riscv_elf() {
    let source = r#"
module valid_loop_control;

action verify() -> u64 {

        let mut total: u64 = 0;
        label outer: for i in 0..4 {
            for j in 0..4 {
                if j == 0 {
                    continue;
                }
                if i == 2 {
                    break outer;
                }
                total += 1;
            }
        }
        require total == 6;
        return 0;
}
"#;
    compile(
        source,
        CompileOptions { target: Some("riscv64-elf".to_string()), target_profile: Some("ckb".to_string()), ..Default::default() },
    )
    .expect("labeled loop control should compile to ELF");
}
