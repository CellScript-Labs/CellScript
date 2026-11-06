import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import * as sdk from '../dist/index.js';
const actual=JSON.parse(readFileSync(new URL('../actual.json',import.meta.url),'utf8'));
test('ordinary generated Lock codec reproduces an actual archived native transaction',()=>{
  assert.equal(Buffer.from(sdk.encodeLockEntryWitness('verify',actual.args)).toString('hex'),actual.payload);
  const fields=Object.fromEntries(Object.entries(actual.fields).map(([name,hex])=>[name,Buffer.from(hex,'hex')]));
  assert.equal(Buffer.from(sdk.encodeFixedCellData('OrderState',fields)).toString('hex'),actual.data);
});
