// Independent Molecule reference, using the existing pinned CCC installation.
// No CellScript compiler/checker imports. All identities are synthetic.
// argv[2]: package.json; argv[3]: output JSON or --check.
const fs = require('node:fs');
const { createRequire } = require('node:module');
const path = require('node:path');
const scopedRequire = createRequire(path.resolve(process.argv[2]));
const ccc = scopedRequire('@ckb-ccc/core');
const manifest = JSON.parse(fs.readFileSync(path.join(path.dirname(scopedRequire.resolve('@ckb-ccc/core')), '..', 'package.json')));
if (manifest.version !== '1.23.0') throw new Error('wire vectors require @ckb-ccc/core 1.23.0');
const mol = ccc.mol;
const zeros = n => Array(n).fill(0);
const hex = bytes => ccc.hexFrom(bytes);
const h = n => hex(Buffer.alloc(32, n));
const magic = s => hex(Buffer.from(s));
const digest = (domain, ...parts) => ccc.hashCkb(Buffer.from(`${domain}\0`), ...parts);
const Header = mol.struct({ magic: mol.Byte8, class: mol.Uint8, role: mol.Uint8, mode: mol.Uint8, count: mol.Uint8,
  sequence: mol.Uint64, minimum: mol.Uint64, required: mol.Byte32, exact: mol.Byte32, network: mol.Byte32, target: mol.Byte32, abi: mol.Byte32 });
const Member = mol.struct({ magic: mol.Byte8, status: mol.Uint8, hashType: mol.Uint8, reserved: mol.array(mol.Uint8, 6),
  admission: mol.Uint64, deployment: mol.Uint64, receipt: mol.Byte32, interface: mol.Byte32, artifact: mol.Byte32,
  script: mol.Byte32, code: mol.Byte32, tx: mol.Byte32, line: mol.Byte32, history: mol.Byte32, index: mol.Uint32 });
const OldSelection = mol.struct({ magic: mol.Byte8, header: Header, member: Member, index: mol.Uint8,
  reserved: mol.array(mol.Uint8, 7), siblings: mol.array(mol.Byte32, 5) });
const Rule = mol.struct({ tag: mol.Uint8, reserved: mol.Uint8, length: mol.Uint16, offset: mol.Uint16,
  configOffset: mol.Uint16, padding: mol.array(mol.Uint8, 8), value: mol.Byte32 });
const SchemaField = mol.struct({ offset: mol.Uint16, length: mol.Uint16, kind: mol.Uint8, padding: mol.array(mol.Uint8, 3) });
const Schema = mol.struct({ argsLength: mol.Uint16, count: mol.Uint8, padding: mol.array(mol.Uint8, 5), fields: mol.array(SchemaField, 16) });
const Policy = mol.struct({ magic: mol.Byte8, mode: mol.Uint8, count: mol.Uint8, argsLength: mol.Uint16,
  configLength: mol.Uint16, padding: mol.Uint16, implementation: mol.Byte32, schema: mol.Byte32, rules: mol.array(Rule, 16) });
const Consumer = mol.struct({ magic: mol.Byte8, policy: mol.Byte32, subject: mol.Byte32, length: mol.Uint16,
  padding: mol.array(mol.Uint8, 6), config: mol.array(mol.Uint8, 128) });
const Selection = mol.struct({ magic: mol.Byte8, policy: Policy, implementation: OldSelection, args: mol.array(mol.Uint8, 256) });
if (Policy.byteLength !== 848 || Consumer.byteLength !== 208 || Selection.byteLength !== 1768) throw new Error('wire sizes');
const script = args => ccc.Script.from({ codeHash: h(65), hashType: 'data2', args: hex(args) });
const blankRule = () => ({ tag: 0, reserved: 0, length: 0, offset: 0, configOffset: 0, padding: zeros(8), value: h(0) });
const blankField = () => ({ offset: 0, length: 0, kind: 0, padding: zeros(3) });
const rule = (tag, offset, length, value = Buffer.alloc(32), configOffset = 0) => ({ ...blankRule(), tag, offset, length, value: hex(value), configOffset });
const le = n => { const value = Buffer.alloc(8); value.writeBigUInt64LE(n); return value; };
const exact = args => Array.from({ length: Math.ceil(args.length / 32) }, (_, i) => {
  const part = args.subarray(i * 32, i * 32 + 32); const value = Buffer.alloc(32); part.copy(value);
  return rule(0, i * 32, part.length, value);
});
function implementation(mode, template) {
  const header = { magic: magic('CSOHPv1\0'), class: 0, role: 1, mode, count: 1, sequence: 10n,
    minimum: 3n, required: h(40), exact: mode === 0 ? h(1) : h(0), network: h(41), target: h(42), abi: h(43) };
  const member = { magic: magic('CSOHMv1\0'), status: 0, hashType: 4, reserved: zeros(6), admission: 5n,
    deployment: 0n, receipt: h(1), interface: h(50), artifact: h(65), script: script(template).hash(),
    code: h(65), tx: h(129), line: h(0), history: h(0), index: 7 };
  let level = Array.from({ length: 32 }, (_, i) => i === 0
    ? digest('cellscript-open-handle-member-v1', Uint8Array.of(i), Member.encode(member))
    : digest('cellscript-open-handle-empty-v1', Uint8Array.of(i)));
  const siblings = [];
  while (level.length > 1) {
    siblings.push(level[1]);
    level = Array.from({ length: level.length / 2 }, (_, i) => digest('cellscript-open-handle-node-v1', level[2 * i], level[2 * i + 1]));
  }
  return { root: digest('cellscript-open-handle-policy-v1', Header.encode(header), level[0]),
    selection: { magic: magic('CSOHWv1\0'), header, member, index: 0, reserved: zeros(7), siblings } };
}
function vector(name, implementationMode, instanceMode, args, config, rules) {
  const old = implementation(implementationMode, instanceMode === 0 ? args : Buffer.from([0xaa]));
  const fields = rules.map(r => ({ offset: r.offset, length: r.length, kind: r.tag === 1 ? 1 : 0, padding: zeros(3) }));
  const schema = digest('cellscript-instance-args-schema-v1', Schema.encode({ argsLength: args.length, count: rules.length,
    padding: zeros(5), fields: [...fields, ...Array.from({ length: 16 - fields.length }, blankField)] }));
  const policy = { magic: magic('CSINPv1\0'), mode: instanceMode, count: rules.length, argsLength: args.length,
    configLength: config.length, padding: 0, implementation: old.root, schema,
    rules: [...rules, ...Array.from({ length: 16 - rules.length }, blankRule)] };
  const policyBytes = Policy.encode(policy);
  const root = digest('cellscript-instance-policy-v1', policyBytes);
  const subject = script(args).hash();
  const consumer = Consumer.encode({ magic: magic('CSINCv1\0'), policy: root, subject, length: config.length,
    padding: zeros(6), config: [...config, ...zeros(128 - config.length)] });
  const selection = Selection.encode({ magic: magic('CSINWv1\0'), policy, implementation: old.selection,
    args: [...args, ...zeros(256 - args.length)] });
  return { name, implementation_mode: implementationMode, instance_mode: instanceMode, args: hex(args), config: hex(config),
    rules, schema, policy: hex(policyBytes), root, subject, script: hex(script(args).toBytes()),
    consumer: hex(consumer), implementation_root: old.root, implementation_selection: hex(OldSelection.encode(old.selection)), selection: hex(selection) };
}
const config = Buffer.from('1020304050607080', 'hex');
const scalar = 0x0102030405060708n;
const args = Buffer.concat([Buffer.from([10, 11, 12]), le(scalar), config.subarray(2, 7), Buffer.from([200, 201, 202, 203])]);
const literal = Buffer.alloc(32); args.copy(literal, 0, 0, 3);
const range = Buffer.concat([le(scalar - 1n), le(scalar + 1n), Buffer.alloc(16)]);
const mixed = [rule(0, 0, 3, literal), rule(1, 3, 8, range), rule(2, 11, 5, Buffer.alloc(32), 2), rule(3, 16, 4)];
const cases = [];
for (const impl of [0, 1]) for (const mode of [0, 1]) {
  cases.push(vector(`mode-${impl}-${mode}`, impl, mode, args, mode === 0 ? Buffer.alloc(0) : config, mode === 0 ? exact(args) : mixed));
  cases.push(vector(`empty-${impl}-${mode}`, impl, mode, Buffer.alloc(0), Buffer.alloc(0), []));
}
const maximum = Buffer.from(Array.from({ length: 256 }, (_, i) => i));
const maximumRules = Array.from({ length: 16 }, (_, i) => {
  const value = Buffer.alloc(32); maximum.copy(value, 0, i * 16, i * 16 + 16);
  return i < 8 ? rule(2, i * 16, 16, Buffer.alloc(32), i * 16) : rule(0, i * 16, 16, value);
});
cases.push(vector('maximum-constrained', 1, 1, maximum, maximum.subarray(0, 128), maximumRules));
cases.push(vector('maximum-exact', 0, 0, maximum, Buffer.alloc(0), exact(maximum)));
for (const width of [1, 2, 4, 8]) {
  const maximum = (1n << BigInt(width * 8)) - 1n;
  cases.push(vector(`unsigned-${width}`, 1, 1, le(maximum).subarray(0, width), Buffer.alloc(0),
    [rule(1, 0, width, Buffer.concat([le(0n), le(maximum), Buffer.alloc(16)]))]));
}
const output = JSON.stringify({ schema: 'cellscript-instance-policy-wire-vectors-v1',
  molecule_schema_hash: ccc.hashCkb(fs.readFileSync(path.resolve(__dirname, '../../src/instance_policy.mol'))),
  generator: '@ckb-ccc/core 1.23.0 independent Molecule struct/array/Script and hashCkb; synthetic non-authorizing identities', cases }, null, 2) + '\n';
if (process.argv[3] === '--check') {
  if (fs.readFileSync(path.join(__dirname, 'instance_policy_vectors.json'), 'utf8') !== output) throw new Error('instance vectors differ from independent CCC reconstruction');
} else fs.writeFileSync(process.argv[3], output);
console.log(`${process.argv[3] === '--check' ? 'Verified' : 'Generated'} ${cases.length} instance policy vectors.`);
