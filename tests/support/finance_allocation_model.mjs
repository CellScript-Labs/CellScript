// Independent shared-output economics over actual archived transaction facts.
// No production quote, compiler arithmetic, operation witness or signer enters.
import { readFileSync } from 'node:fs';
const request = JSON.parse(readFileSync(0, 'utf8'));
const MAX = 1_000_000_000_000n, ZERO = '00'.repeat(32);
const fail = reason => { throw new Error(reason); };
function data(cell, width) {
  if (!cell || typeof cell.data !== 'string' || !/^[0-9a-f]*$/.test(cell.data) || cell.data.length !== width * 2) fail('data_shape');
  return Buffer.from(cell.data, 'hex');
}
function hash(bytes, offset) { return bytes.subarray(offset, offset + 32).toString('hex'); }
function integer(bytes, offset, width) { let n = 0n; for (let i = width - 1; i >= 0; --i) n = (n << 8n) + BigInt(bytes[offset + i]); return n; }
function bounded(n) { if (n < 0n || n > MAX) fail('amount_domain'); return n; }
function entitlement(sold, numerator, denominator) { return (sold * numerator + denominator - 1n) / denominator; }
function state(cell, expected) {
  const bytes = data(cell, 320), identity = hash(bytes, 0);
  if (!cell.type || cell.type.code_hash !== expected.type_id_code || cell.type.hash_type !== 1 || cell.type.args.length !== 64 || cell.type.hash !== identity) fail('order_identity');
  if (cell.lock.code_hash !== expected.order_code || cell.lock.hash_type !== 4 || cell.lock.args !== cell.configuration_hash) fail('order_configuration');
  const numerator = bounded(integer(bytes,224,16)), denominator = bounded(integer(bytes,240,16));
  const total = bounded(integer(bytes,256,16)), filled = bounded(integer(bytes,280,16)), owed = bounded(integer(bytes,296,16));
  if (!numerator || !denominator || !total || filled >= total || entitlement(filled,numerator,denominator) !== owed || entitlement(total,numerator,denominator) > MAX) fail('order_entitlement');
  return { bytes, identity, requested:hash(bytes,64), proceeds:hash(bytes,128), numerator, denominator, total, filled, owed, revision:integer(bytes,312,8) };
}
function inspect(item) {
  const { inputs, outputs, expected } = item;
  if (!inputs.length || inputs.length > 32 || outputs.length > 32) fail('cell_domain');
  const descriptor = inputs[0], table = data(descriptor,416);
  if (descriptor.type || descriptor.lock.code_hash !== expected.allocation_code || descriptor.lock.hash_type !== 4 || descriptor.lock.args !== descriptor.data_hash) fail('table_commitment');
  if (hash(table,32) !== expected.order_code) fail('allowed_order_code');
  const count=integer(table,112,8), credits=integer(table,120,8);
  if (count < 1n || count > 2n) fail('order_count');
  if (credits < 1n || credits > 4n) fail('credit_count');
  const listed=[hash(table,128),hash(table,160)].slice(0,Number(count));
  if (new Set(listed).size !== listed.length) fail('duplicate_order');
  if (count === 1n && hash(table,160) !== ZERO) fail('inactive_order_padding');
  const participants=inputs.slice(1).filter(cell=>cell.lock.code_hash===expected.order_code && cell.lock.hash_type===4);
  if (participants.length !== listed.length) fail('participant_count');
  const orders=participants.map(cell=>({ cell, state:state(cell,expected) }));
  if (orders.some(({state},i)=>state.identity!==listed[i])) fail('participant_identity_order');
  const obligations=orders.map(({cell,state:before})=>{
    const successors=outputs.filter(output=>output.type?.hash===before.identity);
    if (successors.length!==1) fail('successor_count');
    const afterCell=successors[0], after=state(afterCell,expected);
    if (afterCell.lock.hash!==cell.lock.hash || !before.bytes.subarray(0,280).equals(after.bytes.subarray(0,280)) || after.filled<=before.filled || after.revision!==before.revision+1n) fail('successor_state');
    const due=entitlement(after.filled,before.numerator,before.denominator)-entitlement(before.filled,before.numerator,before.denominator);
    if (due<=0n) fail('zero_increment');
    return { order:before.identity, requested:before.requested, proceeds:before.proceeds,
      beforeFilled:before.filled,afterFilled:after.filled,beforeEntitlement:before.owed,afterEntitlement:after.owed,due,credited:0n };
  });
  const rows=[], seen=new Set(), budgets=new Map(); let previous=null;
  for (let i=0;i<4;++i) {
    const offset=192+i*56, order=hash(table,offset), output=integer(table,offset+32,8), amount=integer(table,offset+40,16);
    if (BigInt(i)>=credits) {if (order!==ZERO || output!==0n || amount!==0n) fail('inactive_credit_padding');continue;}
    bounded(amount); if (!amount || output>=BigInt(outputs.length)) fail('credit_domain');
    const ordinal=listed.indexOf(order);if (ordinal<0) fail('unknown_order_credit');
    const key=`${order}:${output}`;if (seen.has(key)) fail('duplicate_credit');seen.add(key);
    if (previous && (ordinal<previous.ordinal || (ordinal===previous.ordinal && output<=previous.output))) fail('canonical_credit_order');
    previous={ordinal,output};
    const obligation=obligations[ordinal], payment=outputs[Number(output)];
    if (payment.data.length!==32) fail('payment_data_shape');
    const paymentBytes=data(payment,16);
    const actualAmount=bounded(integer(paymentBytes,0,16));
    if (!payment.type || payment.type.hash!==obligation.requested || payment.type.code_hash!==expected.asset_code || payment.type.hash_type!==4 || payment.type.args.length!==64) fail('requested_asset');
    if (payment.lock.hash!==obligation.proceeds) fail('proceeds_recipient');
    obligation.credited+=amount;bounded(obligation.credited);
    const budget=budgets.get(output)??{output,amount:actualAmount,allocated:0n};budget.allocated+=amount;budgets.set(output,budget);
    rows.push({order,output,amount});
  }
  for (const budget of budgets.values()) if (budget.allocated>budget.amount) fail('shared_output_underfunded');
  for (const obligation of obligations) if (obligation.credited<obligation.due) fail('order_underpaid');
  return { orders:obligations,rows,payments:[...budgets.values()].map(budget=>({...budget,unallocated:budget.amount-budget.allocated})) };
}
const results=request.cases.map(item=>{
  try { return {id:item.id,economic_valid:true,evidence:inspect(item)}; }
  catch (error) { return {id:item.id,economic_valid:false,reason:String(error.message)}; }
});
process.stdout.write(JSON.stringify({schema:'cellscript-finance-allocation-economic-model-v1',results},(_,value)=>typeof value==='bigint'?value.toString():value));
