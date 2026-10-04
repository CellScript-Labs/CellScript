import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { encodeZkStatement, encodeZkPublicInputs, encodeZkRequest, bindZkProverOutput } from './zk_builder.ts';
const vectors = JSON.parse(readFileSync(new URL('../../tests/fixtures/zk-wire-vectors.json', import.meta.url))).vectors;
const bytes = hex => Uint8Array.from(Buffer.from(hex, 'hex'));
test('TypeScript and Rust share exact statement, limb and Molecule vectors', () => {
  for (const vector of vectors) {
    const raw = bytes(vector.statement);
    const value = { domain:raw.slice(0,32), action:raw.slice(32,64), scriptHash:raw.slice(64,96), oldDataHash:raw.slice(96,128), newDataHash:raw.slice(128,160), inputTransactionHash:raw.slice(160,192), inputOutputIndex:new DataView(raw.buffer).getUint32(192,true), transactionHash:raw.slice(196) };
    const key = bytes(vector.key), proof = bytes(vector.proof), pi = bytes(vector.public_inputs);
    assert.deepEqual(encodeZkStatement(value), raw);
    assert.deepEqual(encodeZkPublicInputs(value), pi);
    assert.deepEqual(encodeZkRequest(value,key,proof), bytes(vector.request));
    assert.deepEqual(bindZkProverOutput(value,key,proof,pi), bytes(vector.request));
    for (const index of [-1, 0x100000000, 1.5, NaN]) assert.throws(() => encodeZkStatement({...value,inputOutputIndex:index}));
    assert.throws(() => encodeZkStatement({...value,transactionHash:new Uint8Array(31)}));
    assert.throws(() => encodeZkRequest(value,key,proof.slice(1)));
    assert.throws(() => encodeZkRequest(value,key.slice(1),proof));
    for (let i=0;i<pi.length;i++) { const bad=pi.slice(); bad[i]^=1; assert.throws(() => bindZkProverOutput(value,key,proof,bad)); }
  }
});
