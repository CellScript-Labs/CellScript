use super::*;
use quote::{RouteLeg, RouteQuote, SwapQuote};

fn direction(value: Direction) -> &'static str {
    if value == Direction::AForB {
        "a_to_b"
    } else {
        "b_to_a"
    }
}

fn swap_case(id: usize, pool: PoolState, way: Direction, output: bool, amount: u128, limit: u128) -> Value {
    json!({"id":format!("swap-{id}"),"kind":if output {"swap_out"} else {"swap_in"},"state":state(pool),"direction":direction(way),"amount":amount.to_string(),"limit":limit.to_string()})
}

fn leg(value: RouteLeg) -> Value {
    json!({"pool_id":hex::encode(value.pool_id),"asset_a":hex::encode(value.asset_a),"asset_b":hex::encode(value.asset_b),"state":state(value.state),"direction":direction(value.direction)})
}

fn same_swap(actual: SwapQuote, expected: &Value) {
    same_state(actual.next, &expected["next"]);
    assert_eq!(u128::from(actual.next.fee_bps), number(&expected["next"], "fee_bps"));
    for (value, name) in [
        (actual.amount_in, "input"),
        (actual.amount_out, "output"),
        (actual.priced_output, "priced"),
        (actual.retained_output, "retained"),
        (actual.input_refund, "refund"),
        (actual.fee_numerator, "feeNumerator"),
        (actual.effective_input_numerator, "netInput"),
        (actual.output_remainder, "priceRemainder"),
        (actual.output_denominator, "priceDenominator"),
        (actual.input_rounding_numerator, "inputRoundNum"),
        (actual.input_rounding_denominator, "inputRoundDen"),
    ] {
        assert_eq!(value, number(expected, name), "{name}");
    }
    assert_eq!(actual.fee_denominator, FEE_DENOMINATOR);
    assert_eq!(actual.amount_out + actual.retained_output, actual.priced_output);
    assert!(actual.output_remainder < actual.output_denominator);
    assert!(actual.input_rounding_numerator < actual.input_rounding_denominator);
}

fn route_legs(first: Direction, second: Direction) -> [RouteLeg; 2] {
    let one = PoolState { reserve_a: 50_000, reserve_b: 120_000, total_lp: 77_459, fee_bps: 30 };
    let two = PoolState { reserve_a: 100_000, reserve_b: 10_000, total_lp: 31_622, fee_bps: 1000 };
    let leg = |id, a, b, pool: PoolState, way| {
        let (a, b, pool) = if way == Direction::AForB {
            (a, b, pool)
        } else {
            (b, a, PoolState { reserve_a: pool.reserve_b, reserve_b: pool.reserve_a, ..pool })
        };
        RouteLeg { pool_id: [id; 32], asset_a: [a; 32], asset_b: [b; 32], state: pool, direction: way }
    };
    [leg(1, 11, 12, one, first), leg(2, 12, 13, two, second)]
}

#[test]
fn exact_output_quotes_match_independent_integer_search_and_expose_retained_rounding() {
    let mut cases = Vec::new();
    for fee in [0, 1, 30, 1000] {
        for way in [Direction::AForB, Direction::BForA] {
            let pool = PoolState { reserve_a: 100_000, reserve_b: 500_000, total_lp: 223_606, fee_bps: fee };
            for amount in [1, 2, 7, 1_000, 10_000] {
                cases.push((pool, way, true, amount, MAX_AMOUNT));
            }
        }
    }
    for pool in [
        PoolState { reserve_a: 1, reserve_b: MAX_AMOUNT, total_lp: 1_000_000, fee_bps: 30 },
        PoolState { reserve_a: MAX_AMOUNT - 10, reserve_b: MAX_AMOUNT, total_lp: MAX_AMOUNT, fee_bps: 0 },
        PoolState { reserve_a: 1, reserve_b: 2, total_lp: 1000, fee_bps: 0 },
    ] {
        for amount in [0, 1, 2, 9, 10, pool.reserve_b, pool.reserve_b + 1] {
            cases.push((pool, Direction::AForB, true, amount, MAX_AMOUNT));
        }
    }
    let ordinary = PoolState { reserve_a: 100_000, reserve_b: 500_000, total_lp: 223_606, fee_bps: 30 };
    let needed = quote::swap_exact_out(ordinary, Direction::AForB, 100, MAX_AMOUNT).unwrap().amount_in;
    for limit in [0, needed - 1, needed, MAX_AMOUNT + 1] {
        cases.push((ordinary, Direction::AForB, true, 100, limit));
    }
    for (pool, way, output, amount, limit) in [
        (PoolState { fee_bps: 1001, ..ordinary }, Direction::AForB, true, 1, 100),
        (PoolState { reserve_a: 0, ..ordinary }, Direction::AForB, true, 1, 100),
        (ordinary, Direction::AForB, true, MAX_AMOUNT + 1, MAX_AMOUNT),
        (ordinary, Direction::BForA, false, 13, 0),
    ] {
        cases.push((pool, way, output, amount, limit));
    }
    let mut seed = 13u64;
    for _ in 0..64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let pool = PoolState {
            reserve_a: u128::from(seed % 900_000 + 1_000),
            reserve_b: u128::from(seed.rotate_left(19) % 900_000 + 1_000),
            total_lp: 10_000,
            fee_bps: seed % 1001,
        };
        cases.push((pool, Direction::AForB, true, u128::from(seed.rotate_left(37) % 1000 + 1), MAX_AMOUNT));
    }
    let request = json!({"cases":cases.iter().enumerate().map(|(i,&(pool,way,out,amount,limit))|swap_case(i,pool,way,out,amount,limit)).collect::<Vec<_>>()});
    let model = bigint("finance_route_quote_model.mjs", &request);
    assert_eq!(model["results"].as_array().unwrap().len(), cases.len());
    for (&(pool, way, out, amount, limit), expected) in cases.iter().zip(model["results"].as_array().unwrap()) {
        let actual =
            if out { quote::swap_exact_out(pool, way, amount, limit) } else { quote::swap_exact_in(pool, way, amount, limit) };
        match actual {
            Ok(quote) => {
                assert_eq!(expected["ok"], true, "{expected}");
                same_swap(quote, &expected["quote"]);
                if out && quote.amount_in > 1 {
                    let prior = quote::swap_exact_in(pool, way, quote.amount_in - 1, 0);
                    assert!(prior.map_or(true, |prior| prior.amount_out < amount));
                }
            }
            Err(error) => assert_eq!(expected["ok"], false, "{error}: {expected}"),
        }
    }
    let skew = PoolState { reserve_a: 1, reserve_b: MAX_AMOUNT, total_lp: 1_000_000, fee_bps: 0 };
    let quote = quote::swap_exact_out(skew, Direction::AForB, 1, 10).unwrap();
    assert_eq!(
        (quote.amount_in, quote.amount_out, quote.retained_output, quote.next.reserve_b),
        (1, 1, MAX_AMOUNT / 2 - 1, MAX_AMOUNT - 1)
    );
    std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-exact-output-quote-evidence.json"),serde_json::to_vec_pretty(&json!({"scope":"public quote versus independent BigInt search; transaction acceptance is separate","request":request,"model":model})).unwrap()).unwrap();
}

fn same_route(actual: RouteQuote, expected: &Value) {
    assert_eq!(actual.amount_in, number(expected, "input"));
    assert_eq!(actual.intermediate_amount, number(expected, "middle"));
    assert_eq!(actual.amount_out, number(expected, "output"));
    assert_eq!(actual.input_refund, number(expected, "refund"));
    assert_eq!(actual.legs[0].amount_out, actual.legs[1].amount_in);
    assert_eq!(actual.legs[1].input_refund, 0);
    for (leg, expected) in actual.legs.into_iter().zip(expected["legs"].as_array().unwrap()) {
        same_swap(leg, expected);
    }
}

#[test]
fn two_pool_routes_match_independent_search_and_consume_the_exact_intermediate_amount() {
    let mut cases = Vec::new();
    for first in [Direction::AForB, Direction::BForA] {
        for second in [Direction::AForB, Direction::BForA] {
            let legs = route_legs(first, second);
            for amount in [1, 100, 1000] {
                cases.push((legs, false, amount, 0));
                cases.push((legs, true, amount, MAX_AMOUNT));
            }
        }
    }
    let legs = route_legs(Direction::AForB, Direction::AForB);
    let required = quote::route_exact_out(legs, 100, MAX_AMOUNT).unwrap().amount_in;
    for limit in [required - 1, required, MAX_AMOUNT + 1] {
        cases.push((legs, true, 100, limit));
    }
    let mut repeated = legs;
    repeated[1].pool_id = repeated[0].pool_id;
    cases.push((repeated, true, 100, MAX_AMOUNT));
    let mut broken = legs;
    broken[1].asset_a = [19; 32];
    cases.push((broken, false, 1000, 0));
    let mut same_asset = legs;
    same_asset[0].asset_b = same_asset[0].asset_a;
    cases.push((same_asset, true, 1, MAX_AMOUNT));
    cases.push((legs, true, 0, MAX_AMOUNT));
    cases.push((legs, true, 10_000, MAX_AMOUNT));
    cases.push((legs, false, 100, MAX_AMOUNT));
    let request = json!({"cases":cases.iter().enumerate().map(|(i,(legs,out,amount,limit))|json!({"id":format!("route-{i}"),"kind":if *out{"route_out"}else{"route_in"},"legs":legs.map(leg),"amount":amount.to_string(),"limit":limit.to_string()})).collect::<Vec<_>>()});
    let model = bigint("finance_route_quote_model.mjs", &request);
    for (&(legs, out, amount, limit), expected) in cases.iter().zip(model["results"].as_array().unwrap()) {
        let actual = if out { quote::route_exact_out(legs, amount, limit) } else { quote::route_exact_in(legs, amount, limit) };
        match actual {
            Ok(quote) => {
                assert_eq!(expected["ok"], true, "{expected}");
                same_route(quote, &expected["quote"]);
                assert_eq!(quote.pool_ids, [legs[0].pool_id, legs[1].pool_id]);
            }
            Err(error) => assert_eq!(expected["ok"], false, "{error}: {expected}"),
        }
    }
    assert_eq!(quote::route_exact_out(repeated, 100, MAX_AMOUNT), Err(QuoteError::RepeatedPool));
    assert_eq!(quote::route_exact_in(broken, 1000, 0), Err(QuoteError::InvalidRouteAssets));
    std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-route-quote-evidence.json"),serde_json::to_vec_pretty(&json!({"scope":"public quote versus independent BigInt search; transaction acceptance is separate","request":request,"model":model})).unwrap()).unwrap();
}
