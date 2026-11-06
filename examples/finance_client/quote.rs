//! Application-owned bounded quotations. These values are proposals: genuine
//! executing Scripts still enforce identities, custody, authorization and flow.

use std::{error::Error, fmt};

pub const MAX_AMOUNT: u128 = 1_000_000_000_000;
pub const FEE_DENOMINATOR: u128 = 10_000;
pub const LOCKED_MINIMUM: u128 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuoteError {
    OutOfDomain(&'static str),
    ZeroAmount(&'static str),
    InvalidFee,
    InvalidPool,
    InvalidOrder,
    ArithmeticOverflow(&'static str),
    ZeroDenominator,
    ZeroOutput,
    ZeroShares,
    ExcessiveBurn,
    ExcessiveFill,
    ZeroEntitlementIncrement,
    Underpayment,
    InsufficientLiquidity,
    RepeatedPool,
    InvalidRouteAssets,
    MaximumExceeded { name: &'static str, maximum: u128, actual: u128 },
    LimitNotMet { name: &'static str, minimum: u128, actual: u128 },
}

impl fmt::Display for QuoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfDomain(name) => write!(formatter, "{name} exceeds the admitted 0..={MAX_AMOUNT} domain"),
            Self::ZeroAmount(name) => write!(formatter, "{name} must be positive"),
            Self::InvalidFee => write!(formatter, "fee numerator must be in 0..=1000 with denominator {FEE_DENOMINATOR}"),
            Self::InvalidPool => {
                formatter.write_str("pool requires positive bounded reserves and total LP at least the locked minimum")
            }
            Self::InvalidOrder => formatter.write_str("order state does not match its immutable terms and cumulative entitlement"),
            Self::ArithmeticOverflow(name) => write!(formatter, "checked arithmetic overflow in {name}"),
            Self::ZeroDenominator => formatter.write_str("division denominator must be positive"),
            Self::ZeroOutput => formatter.write_str("the admitted operation requires positive output amounts"),
            Self::ZeroShares => formatter.write_str("deposit must mint positive circulating LP shares"),
            Self::ExcessiveBurn => formatter.write_str("withdrawal exceeds circulating LP shares"),
            Self::ExcessiveFill => formatter.write_str("fill exceeds the actual remaining order amount"),
            Self::ZeroEntitlementIncrement => {
                formatter.write_str("fill has zero cumulative entitlement increment, including final dust")
            }
            Self::Underpayment => formatter.write_str("payment does not cover the cumulative entitlement increment"),
            Self::InsufficientLiquidity => formatter.write_str("desired output must leave a positive reserve"),
            Self::RepeatedPool => formatter.write_str("a two-pool route must use each distinct pool once"),
            Self::InvalidRouteAssets => {
                formatter.write_str("route legs require distinct paired assets and a matching intermediate asset")
            }
            Self::MaximumExceeded { name, maximum, actual } => write!(formatter, "{name} permits at most {maximum}, quoted {actual}"),
            Self::LimitNotMet { name, minimum, actual } => {
                write!(formatter, "{name} limit requires at least {minimum}, quoted {actual}")
            }
        }
    }
}

impl Error for QuoteError {}

type Result<T> = std::result::Result<T, QuoteError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolState {
    pub reserve_a: u128,
    pub reserve_b: u128,
    pub total_lp: u128,
    pub fee_bps: u64,
}

impl PoolState {
    pub fn validate(self) -> Result<Self> {
        bounded(self.reserve_a, "reserve A")?;
        bounded(self.reserve_b, "reserve B")?;
        bounded(self.total_lp, "total LP")?;
        fee(self.fee_bps)?;
        if self.reserve_a == 0 || self.reserve_b == 0 || self.total_lp < LOCKED_MINIMUM {
            return Err(QuoteError::InvalidPool);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitQuote {
    pub next: PoolState,
    pub used_a: u128,
    pub used_b: u128,
    pub minted: u128,
    pub locked: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepositQuote {
    pub next: PoolState,
    pub minted: u128,
    pub used_a: u128,
    pub used_b: u128,
    /// Unused amounts within the two offered maxima, before unrelated wallet change.
    pub refund_a: u128,
    pub refund_b: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    AForB,
    BForA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapQuote {
    pub next: PoolState,
    pub direction: Direction,
    pub amount_in: u128,
    pub amount_out: u128,
    /// Exact retained fee as a rational number; it is not rounded before pricing.
    pub fee_numerator: u128,
    pub fee_denominator: u128,
    pub effective_input_numerator: u128,
    pub output_remainder: u128,
    pub output_denominator: u128,
    /// Floor price for the actual input. Exact-output swaps may pay less.
    pub priced_output: u128,
    /// Whole output units retained in reserves beyond the fractional remainder.
    pub retained_output: u128,
    pub input_refund: u128,
    /// Excess input caused by ceil, as this rational number. Exact-in uses 0/1.
    pub input_rounding_numerator: u128,
    pub input_rounding_denominator: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WithdrawQuote {
    pub next: PoolState,
    pub burned: u128,
    pub paid_a: u128,
    pub paid_b: u128,
}

fn bounded(value: u128, name: &'static str) -> Result<u128> {
    if value > MAX_AMOUNT {
        return Err(QuoteError::OutOfDomain(name));
    }
    Ok(value)
}

fn positive(value: u128, name: &'static str) -> Result<u128> {
    bounded(value, name)?;
    if value == 0 {
        return Err(QuoteError::ZeroAmount(name));
    }
    Ok(value)
}

fn fee(value: u64) -> Result<()> {
    if value > 1000 {
        return Err(QuoteError::InvalidFee);
    }
    Ok(())
}

fn add(left: u128, right: u128, name: &'static str) -> Result<u128> {
    left.checked_add(right).ok_or(QuoteError::ArithmeticOverflow(name))
}

fn multiply(left: u128, right: u128, name: &'static str) -> Result<u128> {
    left.checked_mul(right).ok_or(QuoteError::ArithmeticOverflow(name))
}

fn floor(left: u128, right: u128, denominator: u128) -> Result<u128> {
    if denominator == 0 {
        return Err(QuoteError::ZeroDenominator);
    }
    Ok(multiply(left, right, "mul-div product")? / denominator)
}

fn ceil(left: u128, right: u128, denominator: u128) -> Result<u128> {
    if denominator == 0 {
        return Err(QuoteError::ZeroDenominator);
    }
    let product = multiply(left, right, "mul-div product")?;
    add(product / denominator, u128::from(product % denominator != 0), "ceil quotient")
}

fn minimum(actual: u128, limit: u128, name: &'static str) -> Result<()> {
    bounded(limit, name)?;
    if actual < limit {
        return Err(QuoteError::LimitNotMet { name, minimum: limit, actual });
    }
    Ok(())
}

pub fn initialise(a: u128, b: u128, fee_bps: u64, min_shares: u128) -> Result<InitQuote> {
    positive(a, "initial reserve A")?;
    positive(b, "initial reserve B")?;
    fee(fee_bps)?;
    bounded(min_shares, "minimum LP")?;
    let supply = multiply(a, b, "initial reserve product")?.isqrt();
    if supply <= LOCKED_MINIMUM {
        return Err(QuoteError::ZeroShares);
    }
    let minted = supply - LOCKED_MINIMUM;
    minimum(minted, min_shares, "minimum LP")?;
    let next = PoolState { reserve_a: a, reserve_b: b, total_lp: supply, fee_bps }.validate()?;
    Ok(InitQuote { next, used_a: a, used_b: b, minted, locked: LOCKED_MINIMUM })
}

pub fn deposit(state: PoolState, max_a: u128, max_b: u128, min_shares: u128) -> Result<DepositQuote> {
    state.validate()?;
    bounded(max_a, "offered A")?;
    bounded(max_b, "offered B")?;
    bounded(min_shares, "minimum LP")?;
    let minted = floor(max_a, state.total_lp, state.reserve_a)?.min(floor(max_b, state.total_lp, state.reserve_b)?);
    if minted == 0 {
        return Err(QuoteError::ZeroShares);
    }
    minimum(minted, min_shares, "minimum LP")?;
    let next_supply = bounded(add(state.total_lp, minted, "LP issuance")?, "total LP")?;
    let used_a = ceil(minted, state.reserve_a, state.total_lp)?;
    let used_b = ceil(minted, state.reserve_b, state.total_lp)?;
    let next = PoolState {
        reserve_a: add(state.reserve_a, used_a, "reserve A deposit")?,
        reserve_b: add(state.reserve_b, used_b, "reserve B deposit")?,
        total_lp: next_supply,
        ..state
    }
    .validate()?;
    let refund_a = max_a.checked_sub(used_a).ok_or(QuoteError::InvalidPool)?;
    let refund_b = max_b.checked_sub(used_b).ok_or(QuoteError::InvalidPool)?;
    Ok(DepositQuote { next, minted, used_a, used_b, refund_a, refund_b })
}

pub fn swap_exact_in(state: PoolState, direction: Direction, amount_in: u128, min_out: u128) -> Result<SwapQuote> {
    state.validate()?;
    positive(amount_in, "swap input")?;
    bounded(min_out, "minimum output")?;
    let (reserve_in, reserve_out) = match direction {
        Direction::AForB => (state.reserve_a, state.reserve_b),
        Direction::BForA => (state.reserve_b, state.reserve_a),
    };
    let fee_numerator = multiply(amount_in, u128::from(state.fee_bps), "fee numerator")?;
    let effective_input_numerator = multiply(amount_in, FEE_DENOMINATOR - u128::from(state.fee_bps), "effective input")?;
    let denominator =
        add(multiply(reserve_in, FEE_DENOMINATOR, "scaled input reserve")?, effective_input_numerator, "price denominator")?;
    let numerator = multiply(reserve_out, effective_input_numerator, "price numerator")?;
    let amount_out = numerator / denominator;
    if amount_out == 0 || amount_out >= reserve_out {
        return Err(QuoteError::ZeroOutput);
    }
    minimum(amount_out, min_out, "minimum output")?;
    let increased = add(reserve_in, amount_in, "swap reserve")?;
    let decreased = reserve_out.checked_sub(amount_out).ok_or(QuoteError::InvalidPool)?;
    let next = match direction {
        Direction::AForB => PoolState { reserve_a: increased, reserve_b: decreased, ..state },
        Direction::BForA => PoolState { reserve_a: decreased, reserve_b: increased, ..state },
    }
    .validate()?;
    Ok(SwapQuote {
        next,
        direction,
        amount_in,
        amount_out,
        fee_numerator,
        fee_denominator: FEE_DENOMINATOR,
        effective_input_numerator,
        output_remainder: numerator % denominator,
        output_denominator: denominator,
        priced_output: amount_out,
        retained_output: 0,
        input_refund: 0,
        input_rounding_numerator: 0,
        input_rounding_denominator: 1,
    })
}

pub fn swap_exact_out(state: PoolState, direction: Direction, desired: u128, max_input: u128) -> Result<SwapQuote> {
    state.validate()?;
    positive(desired, "desired output")?;
    bounded(max_input, "maximum input")?;
    let (reserve_in, reserve_out) = match direction {
        Direction::AForB => (state.reserve_a, state.reserve_b),
        Direction::BForA => (state.reserve_b, state.reserve_a),
    };
    if desired >= reserve_out {
        return Err(QuoteError::InsufficientLiquidity);
    }
    let numerator =
        multiply(multiply(reserve_in, desired, "exact-output reserve product")?, FEE_DENOMINATOR, "scaled exact-output product")?;
    let denominator = multiply(reserve_out - desired, FEE_DENOMINATOR - u128::from(state.fee_bps), "exact-output denominator")?;
    let required = add(numerator / denominator, u128::from(numerator % denominator != 0), "exact-output ceil")?;
    if required > max_input {
        return Err(QuoteError::MaximumExceeded { name: "maximum input", maximum: max_input, actual: required });
    }
    let mut quote = swap_exact_in(state, direction, required, desired)?;
    let retained = quote.amount_out.checked_sub(desired).ok_or(QuoteError::InvalidPool)?;
    quote.next = match direction {
        Direction::AForB => PoolState { reserve_b: add(quote.next.reserve_b, retained, "retained output rounding")?, ..quote.next },
        Direction::BForA => PoolState { reserve_a: add(quote.next.reserve_a, retained, "retained output rounding")?, ..quote.next },
    }
    .validate()?;
    quote.amount_out = desired;
    quote.retained_output = retained;
    quote.input_refund = max_input - required;
    quote.input_rounding_numerator =
        multiply(required, denominator, "rounded exact-output input")?.checked_sub(numerator).ok_or(QuoteError::InvalidPool)?;
    quote.input_rounding_denominator = denominator;
    Ok(quote)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteLeg {
    pub pool_id: [u8; 32],
    pub asset_a: [u8; 32],
    pub asset_b: [u8; 32],
    pub state: PoolState,
    pub direction: Direction,
}

impl RouteLeg {
    fn assets(self) -> ([u8; 32], [u8; 32]) {
        match self.direction {
            Direction::AForB => (self.asset_a, self.asset_b),
            Direction::BForA => (self.asset_b, self.asset_a),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteQuote {
    pub pool_ids: [[u8; 32]; 2],
    pub asset_in: [u8; 32],
    pub intermediate_asset: [u8; 32],
    pub asset_out: [u8; 32],
    pub amount_in: u128,
    pub intermediate_amount: u128,
    pub amount_out: u128,
    pub input_refund: u128,
    pub legs: [SwapQuote; 2],
}

fn route(legs: [RouteLeg; 2], quotes: [SwapQuote; 2]) -> Result<RouteQuote> {
    if quotes[0].amount_out != quotes[1].amount_in {
        return Err(QuoteError::InvalidRouteAssets);
    }
    Ok(RouteQuote {
        pool_ids: [legs[0].pool_id, legs[1].pool_id],
        asset_in: legs[0].assets().0,
        intermediate_asset: legs[0].assets().1,
        asset_out: legs[1].assets().1,
        amount_in: quotes[0].amount_in,
        intermediate_amount: quotes[0].amount_out,
        amount_out: quotes[1].amount_out,
        input_refund: quotes[0].input_refund,
        legs: quotes,
    })
}

fn validate_route(legs: &[RouteLeg; 2]) -> Result<()> {
    if legs[0].pool_id == legs[1].pool_id {
        return Err(QuoteError::RepeatedPool);
    }
    if legs.iter().any(|leg| leg.asset_a == leg.asset_b) || legs[0].assets().1 != legs[1].assets().0 {
        return Err(QuoteError::InvalidRouteAssets);
    }
    for leg in legs {
        leg.state.validate()?;
    }
    Ok(())
}

pub fn route_exact_in(legs: [RouteLeg; 2], amount_in: u128, min_out: u128) -> Result<RouteQuote> {
    validate_route(&legs)?;
    let first = swap_exact_in(legs[0].state, legs[0].direction, amount_in, 0)?;
    let second = swap_exact_in(legs[1].state, legs[1].direction, first.amount_out, min_out)?;
    route(legs, [first, second])
}

pub fn route_exact_out(legs: [RouteLeg; 2], desired: u128, max_input: u128) -> Result<RouteQuote> {
    validate_route(&legs)?;
    let required_second = swap_exact_out(legs[1].state, legs[1].direction, desired, MAX_AMOUNT)?.amount_in;
    let first = swap_exact_out(legs[0].state, legs[0].direction, required_second, max_input)?;
    let second = swap_exact_out(legs[1].state, legs[1].direction, desired, first.amount_out)?;
    route(legs, [first, second])
}

pub fn withdraw(state: PoolState, burned: u128, min_a: u128, min_b: u128) -> Result<WithdrawQuote> {
    state.validate()?;
    positive(burned, "LP burn")?;
    bounded(min_a, "minimum A")?;
    bounded(min_b, "minimum B")?;
    if burned > state.total_lp - LOCKED_MINIMUM {
        return Err(QuoteError::ExcessiveBurn);
    }
    let paid_a = floor(burned, state.reserve_a, state.total_lp)?;
    let paid_b = floor(burned, state.reserve_b, state.total_lp)?;
    if paid_a == 0 || paid_b == 0 {
        return Err(QuoteError::ZeroOutput);
    }
    minimum(paid_a, min_a, "minimum A")?;
    minimum(paid_b, min_b, "minimum B")?;
    let next = PoolState {
        reserve_a: state.reserve_a - paid_a,
        reserve_b: state.reserve_b - paid_b,
        total_lp: state.total_lp - burned,
        ..state
    }
    .validate()?;
    Ok(WithdrawQuote { next, burned, paid_a, paid_b })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderState {
    pub total: u128,
    pub numerator: u128,
    pub denominator: u128,
    pub filled: u128,
    pub entitlement: u128,
}

impl OrderState {
    pub fn validate(self) -> Result<Self> {
        positive(self.total, "order total")?;
        positive(self.numerator, "price numerator")?;
        positive(self.denominator, "price denominator")?;
        bounded(self.filled, "cumulative sold")?;
        bounded(self.entitlement, "cumulative entitlement")?;
        order_entitlement(self.total, self.numerator, self.denominator)?;
        if self.filled > self.total || self.entitlement != order_entitlement(self.filled, self.numerator, self.denominator)? {
            return Err(QuoteError::InvalidOrder);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillQuote {
    pub next: OrderState,
    pub sold: u128,
    pub payment_due: u128,
    pub paid: u128,
    pub donation: u128,
    pub remaining: u128,
    pub complete: bool,
}

pub fn order_entitlement(sold: u128, numerator: u128, denominator: u128) -> Result<u128> {
    bounded(sold, "cumulative sold")?;
    positive(numerator, "price numerator")?;
    positive(denominator, "price denominator")?;
    bounded(ceil(sold, numerator, denominator)?, "maker entitlement")
}

pub fn fill_order(state: OrderState, sold: u128, paid: u128) -> Result<FillQuote> {
    state.validate()?;
    positive(sold, "fill sold")?;
    bounded(paid, "fill payment")?;
    if sold > state.total - state.filled {
        return Err(QuoteError::ExcessiveFill);
    }
    let filled = add(state.filled, sold, "cumulative sold")?;
    let entitlement = order_entitlement(filled, state.numerator, state.denominator)?;
    let payment_due = entitlement.checked_sub(state.entitlement).ok_or(QuoteError::InvalidOrder)?;
    if payment_due == 0 {
        return Err(QuoteError::ZeroEntitlementIncrement);
    }
    if paid < payment_due {
        return Err(QuoteError::Underpayment);
    }
    let next = OrderState { filled, entitlement, ..state }.validate()?;
    let remaining = state.total - filled;
    Ok(FillQuote { next, sold, payment_due, paid, donation: paid - payment_due, remaining, complete: remaining == 0 })
}
