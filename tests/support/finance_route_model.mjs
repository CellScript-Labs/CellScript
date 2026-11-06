// Independent integer reference: no production quotation or compiler imports.
import { readFileSync } from 'node:fs';
const r = JSON.parse(readFileSync(0, 'utf8'));
const pools = r.pools.map(p => Object.fromEntries(Object.entries(p).map(([k,v]) => [k,BigInt(v)])));
const ceil = (n,d) => n/d + (n%d === 0n ? 0n : 1n);
function forward(p, input) {
  const net = input*(10000n-p.fee);
  return p.b*net/(p.a*10000n+net);
}
function backward(p, output) {
  if (output <= 0n || output >= p.b) throw Error('invalid desired output');
  return ceil(p.a*output*10000n,(p.b-output)*(10000n-p.fee));
}
let spent, middle, received;
if (r.mode === 'exact-input') {
  spent=BigInt(r.amount); middle=forward(pools[0],spent); received=forward(pools[1],middle);
} else if (r.mode === 'exact-output') {
  received=BigInt(r.amount); middle=backward(pools[1],received); spent=backward(pools[0],middle);
} else throw Error('unknown mode');
const next=[{a:pools[0].a+spent,b:pools[0].b-middle},{a:pools[1].a+middle,b:pools[1].b-received}];
process.stdout.write(JSON.stringify({spent,middle,received,next},(_,v)=>typeof v==='bigint'?v.toString():v));
