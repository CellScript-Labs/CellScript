// Independent mathematical integers for continuous pool/order sequences.
// No compiler, generated builder or production arithmetic helper is imported.
import { readFileSync } from 'node:fs';

const request = JSON.parse(readFileSync(0, 'utf8'));
const floor = (a, b, d) => a * b / d;
const ceil = (a, b, d) => (a * b + d - 1n) / d;
function sqrt(n) {
  let lo = 0n, hi = n + 1n;
  while (hi - lo > 1n) {
    const mid = (lo + hi) / 2n;
    if (mid * mid <= n) lo = mid;
    else hi = mid;
  }
  return lo;
}

let x = BigInt(request.a), y = BigInt(request.b);
let supply = sqrt(x * y);
const fee = BigInt(request.fee);
const trace = [{ operation: 'initialise', a: x, b: y, supply, minted: supply - 1000n }];
for (const operation of request.operations) {
  const before = { a: x, b: y, supply };
  if (operation.kind === 'deposit') {
    const a = BigInt(operation.a), b = BigInt(operation.b);
    const fromA = floor(a, supply, x), fromB = floor(b, supply, y);
    const minted = fromA < fromB ? fromA : fromB;
    const usedA = ceil(minted, x, supply), usedB = ceil(minted, y, supply);
    x += usedA; y += usedB; supply += minted;
    trace.push({ operation: 'deposit', before, a: x, b: y, supply, minted, usedA, usedB, refundA: a - usedA, refundB: b - usedB });
  } else if (operation.kind === 'swap_a' || operation.kind === 'swap_b') {
    const input = BigInt(operation.amount);
    const net = input * (10000n - fee);
    const received = operation.kind === 'swap_a' ? y * net / (x * 10000n + net) : x * net / (y * 10000n + net);
    if (operation.kind === 'swap_a') { x += input; y -= received; }
    else { y += input; x -= received; }
    trace.push({ operation: operation.kind, before, a: x, b: y, supply, input, received });
  } else if (operation.kind === 'withdraw') {
    const burned = operation.shares === 'circulating' ? supply - 1000n : BigInt(operation.shares);
    const paidA = floor(burned, x, supply), paidB = floor(burned, y, supply);
    x -= paidA; y -= paidB; supply -= burned;
    trace.push({ operation: 'withdraw', before, a: x, b: y, supply, burned, paidA, paidB });
  } else {
    throw new Error(`unknown operation ${operation.kind}`);
  }
  if (x <= 0n || y <= 0n || supply < 1000n) throw new Error('invalid model state');
}
process.stdout.write(JSON.stringify(trace, (_, value) => typeof value === 'bigint' ? value.toString() : value));
