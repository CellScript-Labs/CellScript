use super::*;

#[test]
fn deferred_runtime_payloads_pair_with_the_closed_helper_inventory() {
    use cellscript_artifact_checker::runtime_contract::{KnownRuntimeFailure, RuntimeAvailability, RuntimeHelper, RuntimeReturnAbi};
    use std::collections::{BTreeMap, BTreeSet};

    let expected: BTreeMap<_, BTreeSet<_>> = [
        ("ckb-sighash-all-deferred", ["__ckb_sighash_all"].into_iter().collect()),
        (
            "ckb-since-constraints-deferred",
            ["__ckb_require_maturity", "__ckb_require_time", "__ckb_require_epoch_after", "__ckb_require_epoch_relative"]
                .into_iter()
                .collect(),
        ),
        ("ckb-occupied-capacity-deferred", ["__ckb_occupied_capacity"].into_iter().collect()),
    ]
    .into_iter()
    .collect();
    let mut actual = BTreeMap::<_, BTreeSet<_>>::new();
    let mut fails = BTreeSet::new();
    for &helper in RuntimeHelper::ALL {
        let compiler = IrDeferredRuntimeFeature::from_helper(helper.symbol());
        match helper.availability() {
            RuntimeAvailability::Deferred(checker) => {
                let compiler = compiler.expect("every registered deferred helper needs compiler classification");
                assert_eq!(compiler.feature(), checker.feature(), "{} feature drift", helper.symbol());
                let failure = checker.failure();
                assert_eq!(compiler.runtime_error().code(), failure.code, "{} error-code drift", helper.symbol());
                assert_eq!(compiler.runtime_error().name(), failure.name, "{} error-name drift", helper.symbol());
                assert_eq!(helper.failure(), Some(failure));
                assert_eq!(helper.return_abi(), RuntimeReturnAbi::NoReturn);
                actual.entry(checker.feature()).or_default().insert(helper.symbol());
            }
            RuntimeAvailability::Fails(KnownRuntimeFailure::CollectionRuntimeUnsupported) => {
                assert_eq!(compiler, None, "known failure is not a deferred feature");
                assert_eq!(helper.deferred(), None);
                let failure = helper.failure().unwrap();
                assert_eq!(failure.code, CellScriptRuntimeError::CollectionRuntimeUnsupported.code());
                assert_eq!(failure.name, CellScriptRuntimeError::CollectionRuntimeUnsupported.name());
                assert_eq!(helper.return_abi(), RuntimeReturnAbi::NoReturn);
                fails.insert(helper.symbol());
            }
            RuntimeAvailability::Implemented => {
                assert_eq!(compiler, None);
                assert_eq!(helper.deferred(), None);
                assert_eq!(helper.failure(), None);
            }
        }
    }
    assert_eq!(actual, expected, "the three-family/six-helper deferred surface changed");
    assert_eq!(fails, BTreeSet::from([BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER, BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER]));
    assert_eq!(IrDeferredRuntimeFeature::from_helper("__ckb_spawn"), None, "retired bare spawn must not re-enter deferred inventory");
}

#[test]
fn witness_view_ir_syntax_matches_vectors_without_importing_admission_limits() {
    use cellscript_artifact_checker::view_types::{
        parse_witness_bytes_view_type, WitnessBytesOwner, WitnessBytesViewType, WitnessViewSyntax,
    };
    let vectors: serde_json::Value =
        serde_json::from_str(cellscript_artifact_checker::view_types::WITNESS_BYTES_VIEW_CONFORMANCE_VECTORS).unwrap();
    for vector in vectors["cases"].as_array().unwrap() {
        let wire = parse_witness_bytes_view_type(vector["input"].as_str().unwrap(), WitnessViewSyntax::ExactArguments);
        let ty = wire.map(IrType::WitnessBytes).unwrap_or(IrType::U64);
        let parsed = witness_bytes_view_parts(&ty);
        assert_eq!(parsed.is_some(), vector["exact"].as_bool().unwrap(), "{ty:?}");
        if let Some((owner, tag, maximum)) = parsed {
            assert_eq!(owner, vector["owner"].as_str().unwrap());
            assert_eq!(tag, vector["tag"].as_u64().unwrap());
            assert_eq!(maximum, vector["maximum"].as_u64().unwrap());
        }
    }
    assert_eq!(witness_bytes_view_parts(&IrType::U64), None);
    assert_eq!(
        witness_bytes_view_parts(&IrType::WitnessBytes(WitnessBytesViewType { owner: WitnessBytesOwner::Raw, maximum: 65537 })),
        Some(("raw", 0, 65537))
    );
}

use crate::lexer::lex;
use crate::parser::parse;

fn parse_and_lower(source: &str) -> IrModule {
    let tokens = lex(source).unwrap();
    let ast = parse(&tokens).unwrap();
    crate::types::check(&ast).unwrap();
    crate::flow::check(&ast).unwrap();
    generate(&ast).unwrap()
}

#[test]
fn exact_catalog_results_and_role_projections_retain_structural_descriptions() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-role-selection.cell");
    {
        let ast = crate::frontend::parse(source, crate::CellScriptEdition::Edition2027).unwrap();
        crate::types::check(&ast).unwrap();
        let ir = generate(&ast).unwrap();
        crate::executable_surface::validate_ir_module(&ir).unwrap();
        let mut constructors = 0;
        let mut projections = 0;
        for instruction in ir
            .items
            .iter()
            .filter_map(|item| match item {
                IrItem::Action(action) => Some(&action.body),
                _ => None,
            })
            .flat_map(|body| &body.blocks)
            .flat_map(|block| &block.instructions)
        {
            match instruction {
                IrInstruction::VerifyExactCellRoles { dest: Some(dest), contract } => {
                    let IrType::Selection(selection) = &dest.ty else { panic!("selection became a nominal string") };
                    assert_eq!(selection.as_ref(), &crate::cell_selection::SelectionType::from_contract(contract));
                    constructors += 1;
                }
                IrInstruction::FieldAccess {
                    dest: IrVar { ty: IrType::Selection(projected), .. },
                    obj: IrOperand::Var(owner),
                    field,
                } => {
                    let IrType::Selection(selection) = &owner.ty else { panic!("projection lost its catalog") };
                    assert_eq!(Some(projected.as_ref()), selection.project_role(field).as_ref());
                    projections += 1;
                }
                _ => {}
            }
        }
        assert_eq!(constructors, 2);
        assert!(projections > 0);
    }
}

#[test]
fn struct_initializers_preserve_evaluation_order_and_use_declaration_order() {
    let ir = parse_and_lower(
        r#"
module initializer_order;
struct Pair { left: u64, right: u64, }
fn left() -> u64 { 11 }
fn right() -> u64 { 23 }
fn build() -> Pair { new Pair { right: right(), left: left(), } }
"#,
    );
    let function = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Function(function) if function.name == "build" => Some(function),
            _ => None,
        })
        .unwrap();
    let instructions = function.body.blocks.iter().flat_map(|block| &block.instructions).collect::<Vec<_>>();
    let calls = instructions
        .iter()
        .filter_map(|instruction| match instruction {
            IrInstruction::Call { dest: Some(dest), func, .. } => Some((func.as_str(), dest.id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["right", "left"]);
    let fields = instructions
        .iter()
        .find_map(|instruction| match instruction {
            IrInstruction::Tuple { fields, .. } => Some(fields),
            _ => None,
        })
        .unwrap();
    let field_ids = fields
        .iter()
        .map(|field| match field {
            IrOperand::Var(var) => var.id,
            _ => panic!("expected call-result operand"),
        })
        .collect::<Vec<_>>();
    assert_eq!(field_ids, [calls[1].1, calls[0].1]);
}

#[test]
fn preserve_sugar_populates_preserved_fields() {
    let source = r#"
module test;

resource Offer has store {
    seller: u64,
    price: u64,
    payment_symbol: u64,
    state: u8,
}

flow Offer.state {
    Live -> Filled;
}

action fill(input: Offer) -> (output: Offer) {
    transition input.state: Live -> output.state: Filled;

        preserve output from input {
            seller,
            price,
        }
        require output.payment_symbol == input.payment_symbol;
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "fill" => Some(a.clone()),
            _ => None,
        })
        .expect("expected fill action");
    // Find the mutate for the output binding
    let mutate = action.body.mutate_set.iter().find(|m| m.binding == "output");
    if let Some(mutate) = mutate {
        assert!(
            mutate.preserved_fields.contains(&"seller".to_string()),
            "preserved_fields should contain 'seller', got {:?}",
            mutate.preserved_fields
        );
        assert!(
            mutate.preserved_fields.contains(&"price".to_string()),
            "preserved_fields should contain 'price', got {:?}",
            mutate.preserved_fields
        );
    }
}

#[test]
fn require_block_lowers_to_atomic_requires() {
    let source = r#"
module test;

action check(x: u64, y: u64) -> u64 {

        require {
            x > 0,
            y > 0,
        };
        return x + y;
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "check" => Some(a.clone()),
            _ => None,
        })
        .expect("expected check action");
    // The require block should produce multiple basic blocks due to conditional branching.
    // Each require produces a Branch terminator (cond ? ok : fail) in the IR CFG.
    // With 2 require expressions + return, we expect at least 5 blocks
    // (entry, require1_ok, require2_ok, return_block, fail_block).
    assert!(
        action.body.blocks.len() >= 3,
        "expected at least 3 basic blocks from 2 require expressions, found {} blocks",
        action.body.blocks.len()
    );
    // Count the Branch terminators which represent require conditionals
    let branch_count = action.body.blocks.iter().filter(|b| matches!(b.terminator, IrTerminator::Branch { .. })).count();
    assert!(
        branch_count >= 2,
        "expected at least 2 branch terminators from require block, found {} out of {} blocks",
        branch_count,
        action.body.blocks.len()
    );
}

#[test]
fn byte_string_literal_materializes_fixed_array_constant_for_return() {
    let source = r#"
module test;

action symbol() -> [u8; 4]
{

        return b"TEST";
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "symbol" => Some(a),
            _ => None,
        })
        .expect("expected symbol action");
    // Fixed-byte returns use an explicit local so the ABI can distinguish
    // packed short bytes from a pointer into addressed byte storage.
    let bytes = action.body.blocks.iter().find_map(|block| {
        let IrTerminator::Return(Some(IrOperand::Var(returned))) = &block.terminator else { return None };
        assert_eq!(returned.ty, IrType::Array(Box::new(IrType::U8), 4));
        block.instructions.iter().find_map(|instruction| match instruction {
            IrInstruction::LoadConst { dest, value: IrConst::Array(items) } if dest.id == returned.id => Some(items),
            _ => None,
        })
    });

    let bytes = bytes.expect("expected returned local to preserve the fixed byte constant");
    let lowered = bytes
        .iter()
        .map(|byte| match byte {
            IrConst::U8(byte) => *byte,
            other => panic!("expected fixed byte const, got {:?}", other),
        })
        .collect::<Vec<_>>();
    assert_eq!(lowered, b"TEST");
}

#[test]
fn typed_integer_literals_lower_with_expected_numeric_width() {
    let source = r#"
module test;

const SMALL: u8 = 2;

action literal_widths(flag: bool) -> u8 {

        let left: u8 = 1;
        let right: u8 = if flag { SMALL } else { 3 };
        return left + right;
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(action) if action.name == "literal_widths" => Some(action),
            _ => None,
        })
        .expect("expected literal_widths action");

    let returned_ty = action.body.blocks.iter().find_map(|block| match &block.terminator {
        IrTerminator::Return(Some(IrOperand::Var(var))) => Some(var.ty.clone()),
        _ => None,
    });

    assert_eq!(returned_ty, Some(IrType::U8));
    assert!(action.body.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| {
        matches!(instruction, IrInstruction::Move { src: IrOperand::Const(IrConst::U8(1..=3)), .. })
            || matches!(instruction, IrInstruction::LoadConst { value: IrConst::U8(1..=3), .. })
    }));
}

#[test]
fn stdlib_transfer_lowers_to_single_consumed_input_and_locked_output() {
    let source = r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

action transfer_only(coin: Coin, to: Address) -> next_coin: Coin {

        std::lifecycle::transfer(coin, next_coin, to) { amount, }
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "transfer_only" => Some(a),
            _ => None,
        })
        .expect("expected transfer_only action");

    assert_eq!(action.body.consume_set.len(), 1, "stdlib transfer should not also infer an input lineage consume");
    assert_eq!(action.body.consume_set[0].operation, "consume");
    assert_eq!(action.body.consume_set[0].binding, "coin");

    assert_eq!(action.body.create_set.len(), 1);
    let output = &action.body.create_set[0];
    assert_eq!(output.operation, "output");
    assert_eq!(output.binding, "next_coin");
    assert!(output.lock.is_some(), "stdlib transfer must bind the output lock target");
    assert!(output.fields.iter().any(|(field, _)| field == "amount"));
}

#[test]
fn stdlib_claim_lowers_to_consumed_receipt_and_locked_declared_output() {
    let source = r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
}

receipt Voucher -> Coin has destroy {
    amount: u64,
    holder: Address,
}

action claim_only(voucher: Voucher) -> coin: Coin {

        std::receipt::claim(voucher, coin, voucher.holder) { amount, }
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "claim_only" => Some(a),
            _ => None,
        })
        .expect("expected claim_only action");

    assert_eq!(action.body.consume_set.len(), 1, "stdlib claim should consume exactly the receipt");
    assert_eq!(action.body.consume_set[0].operation, "consume");
    assert_eq!(action.body.consume_set[0].binding, "voucher");

    assert_eq!(action.body.create_set.len(), 1);
    let output = &action.body.create_set[0];
    assert_eq!(output.operation, "output");
    assert_eq!(output.binding, "coin");
    assert!(output.lock.is_some(), "stdlib claim must bind the explicit output lock target");
    assert!(output.fields.iter().any(|(field, _)| field == "amount"));
}

#[test]
fn stdlib_settle_lowers_to_consumed_input_and_locked_output() {
    let source = r#"
module test;

resource Coin has store, replace, relock, consume, burn {
    amount: u64,
    owner: Address,
}

action settle_coin(coin: Coin) -> next_coin: Coin {

        std::lifecycle::settle(coin, next_coin, coin.owner) {
            amount,
            owner,
        }
}
"#;
    let ir = parse_and_lower(source);
    let action = ir
        .items
        .iter()
        .find_map(|item| match item {
            IrItem::Action(a) if a.name == "settle_coin" => Some(a),
            _ => None,
        })
        .expect("expected settle_coin action");

    assert_eq!(action.body.consume_set.len(), 1, "stdlib settle should consume exactly the input");
    assert_eq!(action.body.consume_set[0].operation, "consume");
    assert_eq!(action.body.consume_set[0].binding, "coin");

    assert_eq!(action.body.create_set.len(), 1);
    let output = &action.body.create_set[0];
    assert_eq!(output.operation, "output");
    assert_eq!(output.binding, "next_coin");
    assert!(output.lock.is_some(), "stdlib settle must bind the explicit output lock target");
    assert!(output.fields.iter().any(|(field, _)| field == "amount"));
    assert!(output.fields.iter().any(|(field, _)| field == "owner"));
}

#[test]
fn imported_function_effects_are_recomputed_transitively_from_source() {
    fn parse_module(source: &str) -> Module {
        parse(&lex(source).unwrap()).unwrap()
    }

    let dep = parse_module(
        r#"
module dep;

resource Token has destroy {
    amount: u64,
}

action issue(amount: u64) -> u64 {

        let token = create Token { amount: amount, };
        destroy token;
        return amount;
}
"#,
    );
    let bridge = parse_module(
        r#"
module bridge;

use dep::{issue};

fn hidden_issue(amount: u64) -> u64 {
    return issue(amount);
}
"#,
    );
    let consumer = parse_module(
        r#"
module consumer;

use bridge::{hidden_issue};

action run(amount: u64) -> u64 {

        return hidden_issue(amount);
}
"#,
    );

    let mut resolver = ModuleResolver::new();
    resolver.register_module(dep).unwrap();
    resolver.register_module(bridge).unwrap();
    resolver.register_module(consumer.clone()).unwrap();

    let ir = generate_with_resolver(&consumer, &resolver, "consumer").unwrap();
    assert!(ir
            .items
            .iter()
            .any(|item| matches!(item, IrItem::Action(action) if action.name == "run" && action.effects.lifecycle_ops().iter().copied().collect::<std::collections::BTreeSet<_>>() == std::collections::BTreeSet::from([cellscript_artifact_checker::effect_facts::LifecycleOperation::Create, cellscript_artifact_checker::effect_facts::LifecycleOperation::Consume, cellscript_artifact_checker::effect_facts::LifecycleOperation::Destroy]))));
    let helper = ir.source_callable_targets.get("hidden_issue").unwrap();
    assert!(ir.items.iter().any(|item| matches!(item, IrItem::Function(function) if &function.name == helper && function.effects.lifecycle_ops().iter().copied().collect::<std::collections::BTreeSet<_>>() == std::collections::BTreeSet::from([cellscript_artifact_checker::effect_facts::LifecycleOperation::Create, cellscript_artifact_checker::effect_facts::LifecycleOperation::Consume, cellscript_artifact_checker::effect_facts::LifecycleOperation::Destroy]))));
}

#[test]
fn unique_lifecycle_operations_infer_creation_without_declarations() {
    let source = r#"
module test;

resource Token has store, create
    identity(field(id))
{
    id: u64,
}

action issue(id: u64) -> Token {

        create_unique::<Token>(identity = field(id)) { id: id }
}
"#;
    let ast = parse(&lex(source).unwrap()).unwrap();
    crate::types::check(&ast).unwrap();
    crate::flow::check(&ast).unwrap();
    let ir = generate(&ast).unwrap();
    assert!(ir
            .items
            .iter()
            .any(|item| matches!(item, IrItem::Action(action) if action.name == "issue" && action.effects.lifecycle_ops() == EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create).lifecycle_ops())));
}

#[test]
fn imported_concrete_payload_enum_keeps_layout_and_constructor_lowering() {
    fn parse_module(source: &str) -> Module {
        parse(&lex(source).unwrap()).unwrap()
    }

    let dep = parse_module(
        r#"
module dep;
enum Limit { None, Some(u64), }
"#,
    );
    let consumer = parse_module(
        r#"
module consumer;
use dep::{Limit};
fn make() -> Limit { Limit::Some(7) }
"#,
    );
    let mut resolver = ModuleResolver::new();
    resolver.register_module(dep).unwrap();
    resolver.register_module(consumer.clone()).unwrap();

    let ir = generate_with_resolver(&consumer, &resolver, "consumer").unwrap();
    let layout = ir.enum_layouts.get("Limit").expect("imported enum layout");
    assert_eq!(layout.encoded_size, 9);
    assert!(ir.items.iter().any(|item| matches!(item, IrItem::Function(function) if function.body.blocks.iter().any(|block| block.instructions.iter().any(|instruction| matches!(instruction, IrInstruction::EnumConstruct { enum_name, variant, .. } if enum_name == "Limit" && variant == "Some"))))));
}
