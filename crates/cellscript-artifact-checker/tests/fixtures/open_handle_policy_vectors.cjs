// Independent fixed Molecule/CKB-hash reference. No compiler/checker imports.
// argv[2]: package.json whose installation pins @ckb-ccc/core 1.23.0.
// argv[3]: output JSON, or --check to verify the checked-in vectors.
// All identities below are synthetic and non-authorizing.
const fs = require('node:fs');
const { createRequire } = require('node:module');
const path = require('node:path');
const scopedRequire = createRequire(path.resolve(process.argv[2]));
const ccc = scopedRequire('@ckb-ccc/core');
const manifest = JSON.parse(fs.readFileSync(path.join(path.dirname(scopedRequire.resolve('@ckb-ccc/core')), '..', 'package.json')));
if (manifest.version !== '1.23.0') throw new Error('wire vectors require @ckb-ccc/core 1.23.0');
const mol = ccc.mol;
const h = value => `0x${Buffer.alloc(32, value).toString('hex')}`;
const magic = value => `0x${Buffer.from(value).toString('hex')}`;
const Header = mol.struct({
  magic: mol.Byte8, class: mol.Uint8, role: mol.Uint8, mode: mol.Uint8, count: mol.Uint8,
  sequence: mol.Uint64, minimum: mol.Uint64, required: mol.Byte32, exact: mol.Byte32,
  network: mol.Byte32, target: mol.Byte32, abi: mol.Byte32,
});
const Member = mol.struct({
  magic: mol.Byte8, status: mol.Uint8, hashType: mol.Uint8, reserved: mol.array(mol.Uint8, 6),
  admission: mol.Uint64, deployment: mol.Uint64, receipt: mol.Byte32, interface: mol.Byte32,
  artifact: mol.Byte32, script: mol.Byte32, code: mol.Byte32, tx: mol.Byte32,
  line: mol.Byte32, history: mol.Byte32, index: mol.Uint32,
});
const Selection = mol.struct({
  magic: mol.Byte8, header: Header, member: Member, index: mol.Uint8,
  reserved: mol.array(mol.Uint8, 7), siblings: mol.array(mol.Byte32, 5),
});
if (Header.byteLength !== 188 || Member.byteLength !== 292 || Selection.byteLength !== 656) throw new Error('fixed schema sizes');
const digest = (kind, ...parts) => ccc.hashCkb(Buffer.from(`cellscript-open-handle-${kind}-v1\0`), ...parts);
const member = id => ({
  magic: magic('CSOHMv1\0'), status: 0, hashType: 4, reserved: Array(6).fill(0),
  admission: 5n, deployment: 0n, receipt: h(id), interface: h(50), artifact: h(id + 64),
  script: h(id + 96), code: h(id + 64), tx: h(id + 128), line: h(0), history: h(0), index: id,
});
const cases = [];
for (const count of [1, 3, 32]) {
  const header = {
    magic: magic('CSOHPv1\0'), class: 0, role: 1, mode: 1, count, sequence: 10n, minimum: 3n,
    required: h(40), exact: h(0), network: h(41), target: h(42), abi: h(43),
  };
  const members = Array.from({ length: count }, (_, index) => member(index + 1));
  const leaves = Array.from({ length: 32 }, (_, index) => index < count
    ? digest('member', Uint8Array.of(index), Member.encode(members[index]))
    : digest('empty', Uint8Array.of(index)));
  const levels = [leaves];
  while (levels.at(-1).length > 1) {
    const prior = levels.at(-1);
    levels.push(Array.from({ length: prior.length / 2 }, (_, index) => digest('node', prior[2 * index], prior[2 * index + 1])));
  }
  const root = digest('policy', Header.encode(header), levels.at(-1)[0]);
  const selections = members.map((value, index) => {
    let position = index;
    const siblings = levels.slice(0, -1).map(level => {
      const sibling = level[position ^ 1];
      position = Math.floor(position / 2);
      return sibling;
    });
    const bytes = Selection.encode({ magic: magic('CSOHWv1\0'), header, member: value, index, reserved: Array(7).fill(0), siblings });
    const decoded = Selection.decode(bytes);
    if (ccc.hexFrom(Selection.encode(decoded)) !== ccc.hexFrom(bytes)) throw new Error('Molecule roundtrip');
    return ccc.hexFrom(bytes);
  });
  cases.push({ count, header: ccc.hexFrom(Header.encode(header)), members: members.map(value => ccc.hexFrom(Member.encode(value))), root, selections });
}
const output = JSON.stringify({
  schema: 'cellscript-open-handle-policy-wire-vectors-v1',
  molecule_schema_hash: ccc.hashCkb(fs.readFileSync(path.resolve(__dirname, '../../src/open_handle_policy.mol'))),
  generator: '@ckb-ccc/core 1.23.0 Molecule struct/array and hashCkb; synthetic identities only', cases,
}, null, 2) + '\n';
if (process.argv[3] === '--check') {
  if (fs.readFileSync(path.join(__dirname, 'open_handle_policy_vectors.json'), 'utf8') !== output) {
    throw new Error('open-handle wire vectors differ from independent CCC reconstruction');
  }
} else {
  fs.writeFileSync(process.argv[3], output);
}
console.log(`${process.argv[3] === '--check' ? 'Verified' : 'Generated'} ${cases.length} policies and ${cases.reduce((sum, item) => sum + item.count, 0)} membership witnesses.`);
