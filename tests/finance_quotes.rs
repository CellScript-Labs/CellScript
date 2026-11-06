//! Public Rust client quotations versus independently implemented BigInt math.
//! Script execution and transaction custody evidence belong to finance_protocol.

#[path = "../examples/finance_client/quote.rs"]
mod quote;

#[path = "finance_quotes/routes.rs"]
mod routes;

use quote::{Direction, OrderState, PoolState, QuoteError, FEE_DENOMINATOR, LOCKED_MINIMUM, MAX_AMOUNT};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn bigint(script: &str, request: &Value) -> Value {
    let mut process = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support").join(script))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process.stdin.take().unwrap().write_all(&serde_json::to_vec(request).unwrap()).unwrap();
    let response = process.wait_with_output().unwrap();
    assert!(response.status.success(), "independent model: {}", String::from_utf8_lossy(&response.stderr));
    serde_json::from_slice(&response.stdout).unwrap()
}

fn number(value: &Value, name: &str) -> u128 {
    value[name].as_str().unwrap().parse().unwrap()
}

fn state(state: PoolState) -> Value {
    json!({"a":state.reserve_a.to_string(),"b":state.reserve_b.to_string(),"supply":state.total_lp.to_string(),"fee_bps":state.fee_bps})
}

fn same_state(actual: PoolState, expected: &Value) {
    assert_eq!(actual.reserve_a, number(expected, "a"));
    assert_eq!(actual.reserve_b, number(expected, "b"));
    assert_eq!(actual.total_lp, number(expected, "supply"));
}

#[test]
fn public_pool_quotes_match_independent_continuous_and_boundary_traces() {
    let scenarios = [
        (
            100_000,
            500_000,
            30,
            vec![
                json!({"kind":"deposit","a":"10000","b":"70000"}),
                json!({"kind":"swap_a","amount":"1234"}),
                json!({"kind":"swap_b","amount":"4321"}),
                json!({"kind":"withdraw","shares":"10000"}),
                json!({"kind":"withdraw","shares":"circulating"}),
            ],
        ),
        (
            10_001,
            99_999,
            0,
            vec![
                json!({"kind":"deposit","a":"700","b":"500"}),
                json!({"kind":"swap_a","amount":"13"}),
                json!({"kind":"swap_b","amount":"127"}),
                json!({"kind":"withdraw","shares":"circulating"}),
            ],
        ),
        (
            1_000,
            100_000,
            1000,
            vec![
                json!({"kind":"swap_a","amount":"1"}),
                json!({"kind":"swap_b","amount":"1000"}),
                json!({"kind":"withdraw","shares":"circulating"}),
            ],
        ),
        (
            MAX_AMOUNT,
            MAX_AMOUNT,
            1000,
            vec![
                json!({"kind":"withdraw","shares":"1"}),
                json!({"kind":"deposit","a":"1","b":"1"}),
                json!({"kind":"withdraw","shares":"circulating"}),
            ],
        ),
    ];
    let mut evidence = Vec::new();
    for (a, b, fee, operations) in scenarios {
        let request = json!({"a":a.to_string(),"b":b.to_string(),"fee":fee.to_string(),"operations":operations});
        let model = bigint("finance_model.mjs", &request);
        let initial = quote::initialise(a, b, fee, 0).unwrap();
        same_state(initial.next, &model[0]);
        assert_eq!(initial.minted, number(&model[0], "minted"));
        assert_eq!((initial.used_a, initial.used_b, initial.locked), (a, b, LOCKED_MINIMUM));
        let mut current = initial.next;
        let mut observed = vec![json!({"operation":"initialise","quote":state(current),"minted":initial.minted.to_string()})];
        for (operation, expected) in operations.iter().zip(model.as_array().unwrap().iter().skip(1)) {
            let name = operation["kind"].as_str().unwrap();
            let result = match name {
                "deposit" => {
                    let result = quote::deposit(current, number(operation, "a"), number(operation, "b"), 0).unwrap();
                    assert_eq!(result.minted, number(expected, "minted"));
                    assert_eq!(result.used_a, number(expected, "usedA"));
                    assert_eq!(result.used_b, number(expected, "usedB"));
                    assert_eq!(result.refund_a, number(expected, "refundA"));
                    assert_eq!(result.refund_b, number(expected, "refundB"));
                    current = result.next;
                    json!({"minted":result.minted.to_string(),"usedA":result.used_a.to_string(),"usedB":result.used_b.to_string(),
                        "refundA":result.refund_a.to_string(),"refundB":result.refund_b.to_string()})
                }
                "swap_a" | "swap_b" => {
                    let direction = if name == "swap_a" { Direction::AForB } else { Direction::BForA };
                    let input = number(operation, "amount");
                    let result = quote::swap_exact_in(current, direction, input, 0).unwrap();
                    assert_eq!(result.amount_out, number(expected, "received"));
                    assert_eq!(result.amount_in, input);
                    assert_eq!(result.direction, direction);
                    assert_eq!(result.fee_numerator, input * u128::from(fee));
                    assert_eq!(result.fee_denominator, FEE_DENOMINATOR);
                    let reserve_out = if direction == Direction::AForB { current.reserve_b } else { current.reserve_a };
                    assert_eq!(
                        result.amount_out * result.output_denominator + result.output_remainder,
                        reserve_out * result.effective_input_numerator
                    );
                    current = result.next;
                    json!({"received":result.amount_out.to_string(),"fee_numerator":result.fee_numerator.to_string(),
                        "fee_denominator":result.fee_denominator.to_string(),"rounding_remainder":result.output_remainder.to_string()})
                }
                "withdraw" => {
                    let burned = if operation["shares"] == "circulating" {
                        current.total_lp - LOCKED_MINIMUM
                    } else {
                        number(operation, "shares")
                    };
                    let result = quote::withdraw(current, burned, 0, 0).unwrap();
                    assert_eq!(result.burned, number(expected, "burned"));
                    assert_eq!(result.paid_a, number(expected, "paidA"));
                    assert_eq!(result.paid_b, number(expected, "paidB"));
                    current = result.next;
                    json!({"burned":result.burned.to_string(),"paidA":result.paid_a.to_string(),"paidB":result.paid_b.to_string()})
                }
                _ => panic!("unsupported trace operation"),
            };
            same_state(current, expected);
            assert_eq!(current.fee_bps, fee);
            observed.push(json!({"operation":name,"quote":state(current),"amounts":result}));
        }
        evidence.push(json!({"request":request,"independent_model":model,"public_quotes":observed}));
    }
    let report = json!({"schema":"cellscript-finance-public-quote-parity-v1",
        "scope":"public client versus independent model; not Script execution or node acceptance",
        "quote_source_hash":hex::encode(cellscript::ckb_blake2b256(include_bytes!("../examples/finance_client/quote.rs"))),
        "model_source_hash":hex::encode(cellscript::ckb_blake2b256(include_bytes!("support/finance_model.mjs"))),"scenarios":evidence});
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-quote-evidence.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn bounded_quotes_reject_domain_dust_slippage_and_locked_minimum_violations() {
    assert!(matches!(quote::initialise(MAX_AMOUNT + 1, 1, 0, 0), Err(QuoteError::OutOfDomain(_))));
    assert!(matches!(quote::initialise(0, 10_000, 0, 0), Err(QuoteError::ZeroAmount(_))));
    assert_eq!(quote::initialise(1_000, 1_000, 0, 0), Err(QuoteError::ZeroShares));
    assert_eq!(quote::initialise(10_000, 10_000, 1001, 0), Err(QuoteError::InvalidFee));
    assert!(matches!(quote::initialise(10_000, 10_000, 0, 9_001), Err(QuoteError::LimitNotMet { .. })));
    let initial = quote::initialise(100_000, 500_000, 30, 0).unwrap().next;
    assert_eq!(PoolState { reserve_a: 0, ..initial }.validate(), Err(QuoteError::InvalidPool));
    assert_eq!(PoolState { total_lp: LOCKED_MINIMUM - 1, ..initial }.validate(), Err(QuoteError::InvalidPool));
    assert_eq!(quote::deposit(initial, 0, 1, 0), Err(QuoteError::ZeroShares));
    let deposit = quote::deposit(initial, 10_000, 70_000, 0).unwrap();
    assert!(matches!(quote::deposit(initial, 10_000, 70_000, deposit.minted + 1), Err(QuoteError::LimitNotMet { .. })));
    let full = quote::initialise(MAX_AMOUNT, MAX_AMOUNT, 0, 0).unwrap().next;
    assert!(matches!(quote::deposit(full, 1, 1, 0), Err(QuoteError::OutOfDomain(_))));
    assert!(matches!(quote::swap_exact_in(initial, Direction::AForB, 0, 0), Err(QuoteError::ZeroAmount(_))));
    assert!(matches!(quote::swap_exact_in(initial, Direction::AForB, MAX_AMOUNT, 0), Err(QuoteError::OutOfDomain(_))));
    let swap = quote::swap_exact_in(initial, Direction::AForB, 1234, 0).unwrap();
    assert!(matches!(quote::swap_exact_in(initial, Direction::AForB, 1234, swap.amount_out + 1), Err(QuoteError::LimitNotMet { .. })));
    let extreme = PoolState { reserve_a: MAX_AMOUNT, reserve_b: 1, total_lp: LOCKED_MINIMUM, fee_bps: 0 };
    assert_eq!(quote::swap_exact_in(extreme, Direction::AForB, 1, 0), Err(QuoteError::ZeroOutput));
    let fractional = quote::initialise(1_000, 100_000, 1000, 0).unwrap().next;
    let fee_quote = quote::swap_exact_in(fractional, Direction::AForB, 1, 0).unwrap();
    assert_eq!((fee_quote.amount_out, fee_quote.fee_numerator, fee_quote.fee_denominator), (89, 1000, 10_000));
    assert!(matches!(quote::withdraw(initial, 0, 0, 0), Err(QuoteError::ZeroAmount(_))));
    assert_eq!(quote::withdraw(initial, initial.total_lp - LOCKED_MINIMUM + 1, 0, 0), Err(QuoteError::ExcessiveBurn));
    let withdrawn = quote::withdraw(initial, 10_000, 0, 0).unwrap();
    assert!(matches!(quote::withdraw(initial, 10_000, withdrawn.paid_a + 1, 0), Err(QuoteError::LimitNotMet { .. })));
    let dust = PoolState { reserve_a: 1, reserve_b: 1, total_lp: LOCKED_MINIMUM + 1, fee_bps: 0 };
    assert_eq!(quote::withdraw(dust, 1, 0, 0), Err(QuoteError::ZeroOutput));
}

#[test]
fn cumulative_order_prices_match_bigint_and_donations_never_buy_future_credit() {
    let mut cases = Vec::new();
    let mut triples =
        vec![(0, 2, 3), (1, 2, 3), (2, 2, 3), (3, 2, 3), (MAX_AMOUNT, MAX_AMOUNT, MAX_AMOUNT), (MAX_AMOUNT, MAX_AMOUNT, 1)];
    let mut seed = 0x5356_0000_0000_0001_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        u128::from(seed) % MAX_AMOUNT + 1
    };
    for _ in 0..32 {
        triples.push((next(), next(), next()));
    }
    for (index, (sold, numerator, denominator)) in triples.iter().copied().enumerate() {
        cases.push(json!({"id":format!("entitlement-{index}"),"kind":"quotient","a":sold.to_string(),"b":numerator.to_string(),"d":denominator.to_string()}));
    }
    let response = bigint("finance_math_model.mjs", &json!({"schema":"cellscript-finance-math-model-v1","cases":cases}));
    for ((sold, numerator, denominator), row) in triples.into_iter().zip(response["results"].as_array().unwrap()) {
        let expected = number(row, "ceil");
        let quote = quote::order_entitlement(sold, numerator, denominator);
        if expected > MAX_AMOUNT {
            assert!(matches!(quote, Err(QuoteError::OutOfDomain(_))));
        } else {
            assert_eq!(quote.unwrap(), expected);
        }
    }
    let order = OrderState { total: 3, numerator: 2, denominator: 3, filled: 0, entitlement: 0 };
    let first = quote::fill_order(order, 1, 2).unwrap();
    assert_eq!((first.sold, first.payment_due, first.paid, first.donation, first.remaining, first.complete), (1, 1, 2, 1, 2, false));
    assert_eq!((first.next.filled, first.next.entitlement), (1, 1));
    assert_eq!(quote::fill_order(first.next, 1, 0), Err(QuoteError::Underpayment));
    let second = quote::fill_order(first.next, 1, 1).unwrap();
    assert_eq!((second.next.filled, second.next.entitlement), (2, 2));
    assert_eq!(quote::fill_order(second.next, 1, 1), Err(QuoteError::ZeroEntitlementIncrement));
    let completion = quote::fill_order(first.next, 2, 1).unwrap();
    assert_eq!((completion.payment_due, completion.remaining, completion.complete), (1, 0, true));
    assert_eq!(quote::fill_order(completion.next, 1, 1), Err(QuoteError::ExcessiveFill));
    assert_eq!(quote::fill_order(order, 4, 3), Err(QuoteError::ExcessiveFill));
    assert!(matches!(quote::fill_order(order, 0, 1), Err(QuoteError::ZeroAmount(_))));
    assert_eq!(quote::fill_order(OrderState { entitlement: 1, ..order }, 1, 1), Err(QuoteError::InvalidOrder));
    assert!(matches!(quote::fill_order(OrderState { denominator: 0, ..order }, 1, 1), Err(QuoteError::ZeroAmount(_))));
    assert!(matches!(quote::fill_order(order, 1, MAX_AMOUNT + 1), Err(QuoteError::OutOfDomain(_))));
}
