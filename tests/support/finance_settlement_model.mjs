// Independent BigInt economics from strict archived C1 Cell bytes. This does
// not import the production quote, consume operation witnesses, or verify keys.
import { readFileSync } from 'node:fs';
const request = JSON.parse(readFileSync(0, 'utf8'));
const MAX = 1_000_000_000_000n;
const fail = reason => { throw new Error(reason); };
const require = (condition, reason) => { if (!condition) fail(reason); };
function bytes(cell, width) {
  require(cell && typeof cell.data === 'string' && /^[0-9a-f]*$/.test(cell.data) && cell.data.length === width * 2, 'data_shape');
  return Buffer.from(cell.data, 'hex');
}
const hash = (data, offset) => data.subarray(offset, offset + 32).toString('hex');
function integer(data, offset, width) { let n=0n; for (let i=width-1;i>=0;--i) n=(n<<8n)+BigInt(data[offset+i]); return n; }
function amount(data, offset=0) { const n=integer(data,offset,16);require(n<=MAX,'amount_domain');return n; }
const ceil = (n,d) => (n+d-1n)/d;
const capacity = cell => BigInt(cell.capacity);
function identity(cell, id, code) {
  require(cell.type?.hash===id && cell.type.code_hash===code && cell.type.hash_type===1 && cell.type.args.length===64, 'state_identity');
}
function singleton(cells, id, reason) {const found=cells.filter(cell=>cell.type?.hash===id);require(found.length===1,reason);return found[0];}
function total(cells, owner, asset) {
  return cells.filter(cell=>cell.lock.hash===owner && cell.type?.hash===asset).reduce((n,cell)=>n+amount(bytes(cell,16)),0n);
}
function cap(cells, owner) {return cells.filter(cell=>cell.lock.hash===owner).reduce((n,cell)=>n+capacity(cell),0n);}
function custody(cell, quantity, id, controller, asset, code, reason) {
  const data=bytes(cell,112);
  require(amount(data)===quantity && hash(data,16)===id && hash(data,48)===controller && hash(data,80)===asset &&
    cell.type?.hash===asset && cell.lock.code_hash===code && cell.lock.hash_type===4 && cell.lock.args===cell.tail_hash,reason);
}
function successor(before, after, configurationWidth, code) {
  const b=bytes(before,configurationWidth===208?264:320),a=bytes(after,b.length);
  require(before.lock.code_hash===code && before.lock.hash_type===4 && before.lock.args===before.configuration_hash &&
    after.lock.hash===before.lock.hash && b.subarray(0,configurationWidth).equals(a.subarray(0,configurationWidth)), 'successor_configuration');
  require(capacity(before)===capacity(after),'state_capacity');return [b,a];
}
function sibling(state, reserve, delta) {
  require(state.out_point && reserve.out_point && state.out_point.tx_hash===reserve.out_point.tx_hash &&
    BigInt(state.out_point.index)+delta===BigInt(reserve.out_point.index),'custody_predecessor');
}
function inspect(item) {
  const {inputs,outputs,expected}=item;
  require(inputs.length<=32 && outputs.length<=32 && inputs.length>=9 && outputs.length>=10,'cell_domain');
  const intents=inputs.filter(cell=>!cell.type && cell.lock.code_hash===expected.settlement_code && cell.data.length===560);
  require(intents.length===1,'intent_count');const intent=intents[0], descriptor=bytes(intent,280),controller=intent.lock.hash;
  require(intent.lock.hash_type===4 && intent.lock.args===intent.data_hash,'intent_commitment');
  const pool=hash(descriptor,0),poolController=hash(descriptor,32),order=hash(descriptor,64),orderController=hash(descriptor,96);
  const assetA=hash(descriptor,128),assetB=hash(descriptor,160),authorizer=hash(descriptor,192);
  const maximumInput=amount(descriptor,224),minimumProfit=amount(descriptor,240);
  require(pool!==order && assetA!==assetB && maximumInput>0n && authorizer!==controller,'intent_domain');
  const beforePool=singleton(inputs,pool,'pool_participant'),afterPool=singleton(outputs,pool,'pool_participant');
  const beforeOrder=singleton(inputs,order,'order_participant'),afterOrder=singleton(outputs,order,'order_participant');
  require(beforePool.lock.hash===poolController && afterPool.lock.hash===poolController &&
    beforeOrder.lock.hash===orderController && afterOrder.lock.hash===orderController,'participant_controller');
  require(inputs[0]===beforePool && outputs[0]===afterPool && inputs[3]===beforeOrder && outputs[3]===afterOrder &&
    inputs[5].lock.hash===controller && !inputs[5].type && inputs[5].data==='' && inputs[6].lock.hash===controller &&
    inputs[6].type?.hash===assetA && inputs[7]===intent,'prefix_layout');
  identity(beforePool,pool,expected.type_id_code);identity(afterPool,pool,expected.type_id_code);
  identity(beforeOrder,order,expected.type_id_code);identity(afterOrder,order,expected.type_id_code);
  const [p,q]=successor(beforePool,afterPool,208,expected.pool_code),[o,r]=successor(beforeOrder,afterOrder,280,expected.order_code);
  require(hash(p,32)===assetA && hash(p,64)===assetB && hash(o,32)===assetA && hash(o,64)===assetB && hash(p,160)===hash(o,192),'asset_pair');
  const x=amount(p,208),y=amount(p,224),supply=amount(p,240),nextX=amount(q,208),nextY=amount(q,224),fee=integer(p,192,8);
  require(x>0n && y>0n && supply>=1000n && fee<=1000n && amount(q,240)===supply && integer(q,256,8)===integer(p,256,8)+1n,'pool_state');
  custody(inputs[1],x,pool,poolController,assetA,expected.reserve_code,'reserve_state');
  custody(inputs[2],y,pool,poolController,assetB,expected.reserve_code,'reserve_state');
  custody(outputs[1],nextX,pool,poolController,assetA,expected.reserve_code,'reserve_state');
  custody(outputs[2],nextY,pool,poolController,assetB,expected.reserve_code,'reserve_state');
  sibling(beforePool,inputs[1],1n);sibling(beforePool,inputs[2],2n);
  require(capacity(inputs[1])===capacity(outputs[1]) && capacity(inputs[2])===capacity(outputs[2]),'reserve_capacity');
  const spent=nextX-x;require(spent>0n && spent<=maximumInput,'gross_input_limit');
  const scaled=spent*(10000n-fee), received=y*scaled/(x*10000n+scaled);
  require(received>0n && received<y && nextY===y-received,'pool_rounding');
  const numerator=amount(o,224),denominator=amount(o,240),totalSold=amount(o,256),filled=amount(o,280),entitlement=amount(o,296);
  require(numerator>0n && denominator>0n && totalSold>0n && filled<totalSold &&
    ceil(totalSold*numerator,denominator)<=MAX && ceil(filled*numerator,denominator)===entitlement,'order_entitlement');
  const nextFilled=amount(r,280),nextEntitlement=amount(r,296),sold=nextFilled-filled;
  require(sold>0n && nextFilled<totalSold && nextEntitlement===ceil(nextFilled*numerator,denominator) &&
    integer(r,312,8)===integer(o,312,8)+1n,'order_transition');
  const due=nextEntitlement-entitlement;require(due>0n,'zero_entitlement_increment');
  custody(inputs[4],totalSold-filled,order,orderController,assetA,expected.escrow_code,'escrow_remainder');
  custody(outputs[4],totalSold-nextFilled,order,orderController,assetA,expected.escrow_code,'escrow_remainder');
  sibling(beforeOrder,inputs[4],1n);require(capacity(inputs[4])===capacity(outputs[4]),'escrow_capacity');
  const payment=outputs[5];require(payment.type?.hash===assetB && payment.type.code_hash===expected.asset_code && payment.type.hash_type===4 && payment.type.args.length===64,'payment_asset');
  require(payment.lock.hash===hash(o,128),'maker_recipient');const paid=amount(bytes(payment,16));
  require(paid>=due,'order_underpaid');require(paid===received,'pool_payment_link');
  const beforeA=total(inputs,controller,assetA),afterA=total(outputs,controller,assetA);
  const beforeB=total(inputs,controller,assetB),afterB=total(outputs,controller,assetB);
  require(afterA>=beforeA+minimumProfit && afterB===beforeB,'net_profit_limit');
  const retained=outputs[8];require(!retained.type && retained.lock.hash===controller && retained.data===intent.data && capacity(retained)===capacity(intent),'retained_intent');
  const authority=inputs[8];require(authority.lock.hash===authorizer && !authority.type && authority.data==='','authorizer_cell');
  for(const cell of outputs.filter(cell=>cell.lock.hash===authorizer)) {
    require(cell.type ? [assetA,assetB].includes(cell.type.hash)&&cell.data.length===32 : cell.data==='','authorizer_refund_shape');
  }
  require(outputs[9].lock.hash===authorizer && !outputs[9].type && outputs[9].data==='' && cap(outputs,authorizer)>=cap(inputs,authorizer),'authorizer_capacity');
  const feePaid=inputs.reduce((n,cell)=>n+capacity(cell),0n)-outputs.reduce((n,cell)=>n+capacity(cell),0n);
  require(feePaid>=0n && feePaid<=integer(descriptor,256,8),'fee_limit');
  const capacityDebit=cap(inputs,controller)-cap(outputs,controller);require(capacityDebit<=integer(descriptor,264,8),'capacity_debit_limit');
  return {pool,order,assetA,assetB,reserveBefore:[x,y],reserveAfter:[nextX,nextY],feeBasisPoints:fee,grossInput:spent,
    poolPayment:received,orderSold:sold,orderDue:due,makerPaid:paid,makerDonation:paid-due,beforeA,afterA,profitA:afterA-beforeA,
    beforeB,afterB,maximumInput,minimumProfit,feePaid,capacityDebit,inputCount:inputs.length,outputCount:outputs.length};
}
const results=request.cases.map(item=>{try{return {id:item.id,economic_valid:true,evidence:inspect(item)};}
  catch(error){return {id:item.id,economic_valid:false,reason:String(error.message)};}});
process.stdout.write(JSON.stringify({schema:'cellscript-finance-settlement-economic-model-v1',results},(_,value)=>typeof value==='bigint'?value.toString():value));
