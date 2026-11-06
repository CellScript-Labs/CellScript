import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import * as sdk from '../dist/index.js';
const oracle = JSON.parse(readFileSync(new URL('../native.json', import.meta.url), 'utf8'));
const args = [true,'255','65535','4294967295','18446744073709551615','-2147483648',
  '340282366920938463463374607431768211455','0x'+'a5'.repeat(32),'0xa5a5a5',null];
test('native Lock payload and fixed Document bytes agree', () => {
  assert.equal(Buffer.from(sdk.encodeLockEntryWitness('authorize',args)).toString('hex'),oracle.payload);
  assert.equal(Buffer.from(sdk.encodeLockEntryWitness('authorize',args.slice(0,-1))).toString('hex'),oracle.payload);
  const revision = new Uint8Array(8);new DataView(revision.buffer).setBigUint64(0,7n,true);
  assert.equal(Buffer.from(sdk.encodeFixedCellData('Document',{revision,digest:new Uint8Array(32).fill(0xa5)})).toString('hex'),oracle.document);
});
test('entry kind, count, scalar precision, ranges and fixed widths fail closed', () => {
  const bad = (index,value) => {const input=[...args];input[index]=value;assert.throws(()=>sdk.encodeLockEntryWitness('authorize',input));};
  for(const [index,value] of [[0,1],[1,'256'],[1,'-1'],[2,'65536'],[3,'4294967296'],[4,'18446744073709551616'],
    [4,Number.MAX_SAFE_INTEGER+1],[5,'-2147483649'],[5,'2147483648'],[6,'340282366920938463463374607431768211456'],
    [6,new Uint8Array(15)],[7,'0x'+'a5'.repeat(31)],[8,'0xa5a5']]) bad(index,value);
  assert.throws(()=>sdk.encodeLockEntryWitness('authorize',args.slice(0,-2)),/missing/);
  assert.throws(()=>sdk.encodeLockEntryWitness('authorize',[...args,true]),/extra/);
  assert.throws(()=>sdk.encodeEntryWitness('action','authorize',args),/no declared entry/);
});
