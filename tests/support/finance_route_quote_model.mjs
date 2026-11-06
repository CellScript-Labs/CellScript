// Independent quote oracle. Exact-output input is found by monotone search over
// integer inputs rather than by the production quotient/ceil implementation.
import { readFileSync } from 'node:fs';

const request = JSON.parse(readFileSync(0, 'utf8'));
const MAX = 1_000_000_000_000n, F = 10_000n;
const reject = reason => { throw new Error(reason); };
function bounded(value) { if (value < 0n || value > MAX) reject('domain'); return value; }
function pool(value) {
  const state = { a: BigInt(value.a), b: BigInt(value.b), supply: BigInt(value.supply), fee_bps: BigInt(value.fee_bps) };
  for (const key of ['a', 'b', 'supply']) bounded(state[key]);
  if (!state.a || !state.b || state.supply < 1000n || state.fee_bps < 0n || state.fee_bps > 1000n) reject('pool');
  return state;
}
function price(inputReserve, outputReserve, input, fee) {
  const net = input * (F - fee), denominator = inputReserve * F + net, numerator = outputReserve * net;
  return { output: numerator / denominator, remainder: numerator % denominator, denominator, net };
}
function swap(raw, direction, exactOutput, rawAmount, rawLimit) {
  const state = pool(raw), amount = bounded(BigInt(rawAmount)), limit = bounded(BigInt(rawLimit));
  if (!amount) reject('zero');
  if (!['a_to_b', 'b_to_a'].includes(direction)) reject('direction');
  const [rin, rout] = direction === 'a_to_b' ? [state.a, state.b] : [state.b, state.a];
  let input = amount;
  if (exactOutput) {
    if (amount >= rout) reject('liquidity');
    const upper = limit < MAX - rin ? limit : MAX - rin;
    if (!upper || price(rin, rout, upper, state.fee_bps).output < amount) reject('input bound');
    let lo = 1n, hi = upper;
    while (lo < hi) {
      const mid = (lo + hi) / 2n;
      if (price(rin, rout, mid, state.fee_bps).output >= amount) hi = mid;
      else lo = mid + 1n;
    }
    input = lo;
  }
  const priced = price(rin, rout, input, state.fee_bps);
  const output = exactOutput ? amount : priced.output;
  if (!output || output >= rout || (!exactOutput && output < limit)) reject('output bound');
  bounded(rin + input);
  const next = direction === 'a_to_b' ? { ...state, a: rin + input, b: rout - output } : { ...state, a: rout - output, b: rin + input };
  const roundDen = exactOutput ? (rout - amount) * (F - state.fee_bps) : 1n;
  const roundNum = exactOutput ? input * roundDen - rin * amount * F : 0n;
  return { input, output, next, priced: priced.output, retained: priced.output - output,
    refund: exactOutput ? limit - input : 0n, feeNumerator: input * state.fee_bps, netInput: priced.net,
    priceRemainder: priced.remainder, priceDenominator: priced.denominator, inputRoundNum: roundNum, inputRoundDen: roundDen };
}
function assets(leg) { return leg.direction === 'a_to_b' ? [leg.asset_a, leg.asset_b] : [leg.asset_b, leg.asset_a]; }
function route(item) {
  const [first, second] = item.legs;
  if (first.pool_id === second.pool_id) reject('repeated pool');
  if (first.asset_a === first.asset_b || second.asset_a === second.asset_b || assets(first)[1] !== assets(second)[0]) reject('assets');
  let a, b;
  if (item.kind === 'route_out') {
    const reverse = swap(second.state, second.direction, true, item.amount, MAX);
    a = swap(first.state, first.direction, true, reverse.input, item.limit);
    b = swap(second.state, second.direction, true, item.amount, a.output);
  } else {
    a = swap(first.state, first.direction, false, item.amount, 0n);
    b = swap(second.state, second.direction, false, a.output, item.limit);
  }
  if (a.output !== b.input) reject('intermediate');
  return { input: a.input, middle: a.output, output: b.output, refund: a.refund, legs: [a, b] };
}
const results = request.cases.map(item => {
  try {
    const quote = item.kind.startsWith('route_') ? route(item) : swap(item.state, item.direction, item.kind === 'swap_out', item.amount, item.limit);
    return { id: item.id, ok: true, quote };
  } catch (error) { return { id: item.id, ok: false, error: String(error.message) }; }
});
process.stdout.write(JSON.stringify({ results }, (_, value) => typeof value === 'bigint' ? value.toString() : value));
