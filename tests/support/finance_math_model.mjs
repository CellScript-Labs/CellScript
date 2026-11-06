// Independent arbitrary-precision model. This file does not load the CellScript
// library or translate its algorithms; all inputs/results use decimal strings.
import { readFileSync } from 'node:fs';

const maximum = (1n << 128n) - 1n;
const integer = (value) => {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) {
    throw new Error('model inputs must be canonical unsigned decimal strings');
  }
  const result = BigInt(value);
  if (result > maximum) throw new Error('model input exceeds u128');
  return result;
};

function squareRoot(n) {
  if (n < 2n) return n;
  let estimate = n;
  let next = (estimate + n / estimate) / 2n;
  while (next < estimate) {
    estimate = next;
    next = (estimate + n / estimate) / 2n;
  }
  if (estimate * estimate > n || (estimate + 1n) * (estimate + 1n) <= n) {
    throw new Error('independent square-root relation failed');
  }
  return estimate;
}

const request = JSON.parse(readFileSync(0, 'utf8'));
if (request.schema !== 'cellscript-finance-math-model-v1' || !Array.isArray(request.cases)) {
  throw new Error('unsupported model request');
}
const results = request.cases.map((item) => {
  if (item.kind === 'root') {
    const n = integer(item.n);
    return { ...item, root: squareRoot(n).toString() };
  }
  if (item.kind !== 'quotient') throw new Error('unsupported model operation');
  const a = integer(item.a);
  const b = integer(item.b);
  const d = integer(item.d);
  const product = a * b;
  if (d === 0n) {
    return { ...item, product: product.toString(), product_fits_u128: product <= maximum, error: 'zero-denominator' };
  }
  const floor = product / d;
  const remainder = product % d;
  const ceil = (product + d - 1n) / d;
  const ceilRemainder = ceil * d - product;
  if (floor * d + remainder !== product || remainder < 0n || remainder >= d) {
    throw new Error('independent floor relation failed');
  }
  if (ceil * d - ceilRemainder !== product || ceilRemainder < 0n || ceilRemainder >= d) {
    throw new Error('independent ceil relation failed');
  }
  return {
    ...item,
    product: product.toString(),
    product_fits_u128: product <= maximum,
    floor: floor.toString(),
    ceil: ceil.toString(),
    remainder: remainder.toString(),
    ceil_remainder: ceilRemainder.toString(),
    floor_fits_u128: floor <= maximum,
    ceil_fits_u128: ceil <= maximum,
  };
});
process.stdout.write(JSON.stringify({ schema: request.schema, results }));
