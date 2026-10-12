// Independent TypeScript consumer of the native lifecycle planning contract.
// Inputs are exact externally pinned bytes. These decisions do not authorize a
// spend, admit a setup, or establish live deployment observations.
import { createHash } from 'node:crypto';
import { ccc } from '@ckb-ccc/shell';

type ObjectValue = Record<string, unknown>;
export function lifecycleSha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`ZK lifecycle ${message}`);
}
function object(value: unknown, fields?: string[]): ObjectValue {
  requireValue(value !== null && typeof value === 'object' && !Array.isArray(value), 'expected object');
  const result = value as ObjectValue;
  if (fields) requireValue(Object.keys(result).sort().join(',') === [...fields].sort().join(','), 'unknown or missing fields');
  return result;
}
function text(value: unknown, limit: number): string {
  requireValue(typeof value === 'string' && value.length > 0 && Buffer.byteLength(value) <= limit, 'text bounds');
  for (const char of value) requireValue(char.length !== 1 || char.charCodeAt(0) < 0xd800 || char.charCodeAt(0) > 0xdfff, 'invalid Unicode scalar');
  return value;
}
function digest(value: unknown): string {
  requireValue(typeof value === 'string' && /^[0-9a-f]{64}$/.test(value), 'noncanonical digest');
  return value;
}
function canonical(value: unknown): string {
  if (typeof value === 'number') requireValue(Number.isSafeInteger(value), 'unsafe numeric identity');
  if (typeof value === 'string' && value.length) text(value, Number.MAX_SAFE_INTEGER);
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    const obj = object(value);
    return `{${Object.keys(obj).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))).map((key) => `${canonical(key)}:${canonical(obj[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}
const equal = (a: unknown, b: unknown) => canonical(a) === canonical(b);

function parseExactJson(bytes: Uint8Array): unknown {
  const input = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  const result: unknown = JSON.parse(input);
  // JSON.parse otherwise silently keeps the last duplicate field. Match the
  // native manifest reader's rejection, including escaped equivalent keys.
  const tokens = input.match(/"(?:\\.|[^"\\])*"|[{}\[\]:,]|[^\s{}\[\]:,]+/g) ?? [];
  const stack: (Set<string> | null)[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (/^-?\d/.test(t)) requireValue(/^(0|[1-9]\d*)$/.test(t), 'noncanonical integer');
    if (t === '{') stack.push(new Set());
    else if (t === '[') stack.push(null);
    else if (t === '}' || t === ']') stack.pop();
    else if (t.startsWith('"') && tokens[i + 1] === ':') {
      const keys = stack.at(-1), key = JSON.parse(t) as string;
      requireValue(keys && !keys.has(key), 'duplicate JSON field'); keys.add(key);
    }
  }
  return result;
}

export function readLifecyclePackagePlan(bytes: Uint8Array): unknown {
  requireValue(bytes.length <= 8 * 1024 * 1024, 'package plan byte budget');
  return parseExactJson(bytes);
}

export interface LifecycleManifest {
  schema: string; application: string; version: number; status: 'active' | 'yanked';
  circuit: { source_sha256: string; relation_sha256: string; statement_sha256: string; codec_sha256: string; profile: string };
  proving: { key_sha256: string; executable_sha256: string; build_sha256: string; toolchain: string };
  verification_key_sha256: string;
  setup: { manifest_sha256: string; transcript_sha256: string | null; trust: 'public-test-only' | 'local-single-party' | 'unreviewed' };
  verifier_sha256: string;
  deployment: { chain_id: string; genesis_hash: string; verifier_data_hash: string; parent_data_hash: string; code_out_point: string; application_script: string };
  package: { member: string; lock_sha256: string; deployed_manifest_sha256: string };
}

export function readLifecycleManifest(bytes: Uint8Array, trustedSha256: string): LifecycleManifest {
  digest(trustedSha256);
  requireValue(bytes.length <= 32768, 'manifest byte budget');
  requireValue(lifecycleSha256(bytes) === trustedSha256, 'manifest differs from trusted digest');
  const m = object(parseExactJson(bytes), [
    'schema', 'application', 'version', 'status', 'circuit', 'proving', 'verification_key_sha256',
    'setup', 'verifier_sha256', 'deployment', 'package',
  ]);
  requireValue(m.schema === 'cellscript-zk-lifecycle-v1', 'unsupported schema');
  text(m.application, 128);
  requireValue(Number.isSafeInteger(m.version) && Number(m.version) >= 0, 'version bounds');
  requireValue(m.status === 'active' || m.status === 'yanked', 'unknown discovery status');
  const circuit = object(m.circuit, ['source_sha256', 'relation_sha256', 'statement_sha256', 'codec_sha256', 'profile']);
  for (const field of ['source_sha256', 'relation_sha256', 'statement_sha256', 'codec_sha256']) digest(circuit[field]);
  text(circuit.profile, 128);
  const proving = object(m.proving, ['key_sha256', 'executable_sha256', 'build_sha256', 'toolchain']);
  for (const field of ['key_sha256', 'executable_sha256', 'build_sha256']) digest(proving[field]);
  text(proving.toolchain, 128);
  digest(m.verification_key_sha256); digest(m.verifier_sha256);
  const setup = object(m.setup, ['manifest_sha256', 'transcript_sha256', 'trust']);
  digest(setup.manifest_sha256);
  if (setup.transcript_sha256 !== null) digest(setup.transcript_sha256);
  requireValue(typeof setup.trust === 'string' && ['public-test-only', 'local-single-party', 'unreviewed'].includes(setup.trust), 'unknown setup trust');
  const deployment = object(m.deployment, ['chain_id', 'genesis_hash', 'verifier_data_hash', 'parent_data_hash', 'code_out_point', 'application_script']);
  text(deployment.chain_id, 128);
  for (const field of ['genesis_hash', 'verifier_data_hash', 'parent_data_hash']) digest(deployment[field]);
  for (const [field, limit] of [['code_out_point', 36], ['application_script', 4096]] as const) {
    const encoded = text(deployment[field], limit * 2);
    requireValue(/^(?:[0-9a-f]{2})+$/.test(encoded), 'noncanonical Molecule bytes');
    const raw = Buffer.from(encoded, 'hex');
    if (field === 'code_out_point') requireValue(raw.length === 36, 'OutPoint width');
    else ccc.Script.fromBytes(raw);
  }
  const pkg = object(m.package, ['member', 'lock_sha256', 'deployed_manifest_sha256']);
  text(pkg.member, 256); digest(pkg.lock_sha256); digest(pkg.deployed_manifest_sha256);
  return m as unknown as LifecycleManifest;
}

export function compareLifecycle(
  oldBytes: Uint8Array, oldHash: string, newBytes: Uint8Array, newHash: string, packagePlan?: unknown,
) {
  const a = readLifecycleManifest(oldBytes, oldHash), b = readLifecycleManifest(newBytes, newHash);
  requireValue(a.application === b.application, 'application mismatch');
  requireValue(a.package.member === b.package.member, 'package member mismatch');
  requireValue(a.deployment.genesis_hash === b.deployment.genesis_hash && a.deployment.chain_id === b.deployment.chain_id, 'network mismatch');
  requireValue(b.version >= a.version, 'downgrade rejected');
  requireValue(b.status === 'active', 'candidate is yanked');
  requireValue(equal(a, b) || b.version > a.version, 'changed manifest must advance version');
  let packageHash: string | null = null;
  if (packagePlan !== undefined) {
    const p = object(packagePlan, ['schema', 'schema_version', 'compiler_version', 'plan_hash', 'workspace_root',
      'resolve_graph_schema', 'build_plan_schema', 'selection', 'old_graph_hash', 'new_graph_hash', 'locks',
      'reverse_dependents', 'policy', 'migration_artifacts_required', 'required_gates', 'diagnostics',
      'required_acknowledgements', 'acknowledgements', 'apply_status', 'mutates_deployed_manifest', 'performs_deployment']);
    object(p.selection, ['package', 'precise', 'scope', 'features', 'all_features', 'default_features', 'environment', 'offline']);
    requireValue(p.schema === 'cellscript-upgrade-plan-v1' && p.schema_version === 1 && p.compiler_version === '0.31.0', 'package plan schema/compiler');
    requireValue(p.resolve_graph_schema === 'cellscript-resolve-graph-v1' && p.build_plan_schema === 'cellscript-build-plan-v1', 'package graph schema');
    requireValue(p.mutates_deployed_manifest === false && p.performs_deployment === false, 'package plan exceeds lock-only authority');
    const hash = `sha256:${lifecycleSha256(Buffer.from(canonical({ ...p, plan_hash: '' })))}`;
    requireValue(p.plan_hash === hash, 'package plan digest');
    requireValue(Array.isArray(p.locks), 'package locks missing');
    const locks = p.locks.map((v) => object(v, ['member', 'lock_path', 'old_lock_sha256', 'new_lock_sha256',
      'old_lock_content', 'new_lock_content', 'old_lock', 'new_lock', 'node_changes', 'edge_changes'])).filter((l) => l.member === a.package.member);
    requireValue(locks.length === 1, 'package plan requires exactly one matching member');
    const l = locks[0];
    requireValue(typeof l.old_lock_content === 'string' && typeof l.new_lock_content === 'string', 'package lock content');
    requireValue(l.old_lock_sha256 === `sha256:${a.package.lock_sha256}` && l.new_lock_sha256 === `sha256:${b.package.lock_sha256}`
      && lifecycleSha256(Buffer.from(l.old_lock_content)) === a.package.lock_sha256
      && lifecycleSha256(Buffer.from(l.new_lock_content)) === b.package.lock_sha256, 'package lock identity mismatch');
    packageHash = hash;
  } else requireValue(a.package.lock_sha256 === b.package.lock_sha256, 'changed package lock requires upgrade plan');
  const changes: string[] = [];
  const dimensions: [unknown, unknown, string][] = [
    [a.circuit.source_sha256, b.circuit.source_sha256, 'source'], [a.proving, b.proving, 'proving'],
    [a.circuit.statement_sha256, b.circuit.statement_sha256, 'statement'], [a.circuit.codec_sha256, b.circuit.codec_sha256, 'codec'],
    [a.verification_key_sha256, b.verification_key_sha256, 'verification-key'],
    [a.circuit.relation_sha256, b.circuit.relation_sha256, 'circuit-relation'], [a.circuit.profile, b.circuit.profile, 'profile'],
    [a.setup, b.setup, 'setup'], [a.verifier_sha256, b.verifier_sha256, 'verifier'], [a.deployment, b.deployment, 'deployment'],
    [[a.status, a.version], [b.status, b.version], 'discovery'], [a.package, b.package, 'package'],
  ];
  for (const [old, next, dimension] of dimensions) if (!equal(old, next)) changes.push(dimension);
  const runtimeChanged = changes.some((c) => ['statement', 'codec', 'verification-key', 'circuit-relation', 'profile', 'verifier', 'deployment'].includes(c));
  return {
    schema: 'cellscript-zk-lifecycle-plan-v1', old_manifest_sha256: oldHash, new_manifest_sha256: newHash,
    package_plan_hash: packageHash, changes, old_proof_reusable_for_identical_statement: !runtimeChanged,
    candidate_requires_reproving: runtimeChanged, existing_cells_retain_old_rules: true,
    migration: runtimeChanged ? 'existing-authorization-required' : 'no-runtime-change',
    discovery_authorizes_spend: false, setup_trust_admitted: false,
  };
}

export function validateLifecycleCurrent(
  plan: unknown, oldBytes: Uint8Array, oldHash: string, newBytes: Uint8Array, newHash: string, packagePlan?: unknown,
): void {
  requireValue(equal(plan, compareLifecycle(oldBytes, oldHash, newBytes, newHash, packagePlan)), 'stale or substituted change plan');
}

/** Bind a canonical JSON projection of Deployed.toml, not its TOML formatting. */
export function checkLifecycleDeployment(
  manifestBytes: Uint8Array, trustedHash: string, record: unknown, observedGenesis: string, usage: 'existing-spend' | 'candidate',
): void {
  requireValue(usage === 'existing-spend' || usage === 'candidate', 'unknown deployment usage');
  const m = readLifecycleManifest(manifestBytes, trustedHash), r = object(record);
  requireValue(r.version === 2 && r.schema === 'cellscript-deployed-v0.23-edition-2026', 'deployment schema');
  requireValue(Array.isArray(r.deployments) && r.deployments.length <= 64, 'deployment record cardinality');
  const encoded = Buffer.from(canonical(r));
  requireValue(encoded.length <= 262144, 'deployment record byte budget');
  requireValue(lifecycleSha256(encoded) === m.package.deployed_manifest_sha256, 'deployed record differs from pinned digest');
  requireValue(observedGenesis === m.deployment.genesis_hash, 'observed genesis mismatch');
  const pkg = object(r.package), build = r.build === undefined || r.build === null ? undefined : object(r.build);
  requireValue(pkg.edition === '2026' || pkg.edition === '2027', 'unknown deployment edition');
  const hash = (v: unknown) => {
    requireValue(typeof v === 'string' && /^(?:0x)?[0-9a-fA-F]{64}$/.test(v), 'deployment hash width');
    return v.replace(/^0x/, '').toLowerCase();
  };
  if (build) requireValue(build.edition === pkg.edition && text(build.compatibility_profile_hash, 128), 'deployment build edition/profile');
  const rows = r.deployments.map((v) => object(v));
  for (const row of rows) {
    requireValue(row.edition === pkg.edition && text(row.compatibility_profile_hash, 128), 'deployment edition/profile');
    if (build) requireValue(row.compatibility_profile_hash === build.compatibility_profile_hash, 'deployment profile mismatch');
    requireValue(row.status === undefined || row.status === null || typeof row.status === 'string' && ['active', 'candidate', 'deprecated', 'revoked'].includes(row.status), 'unknown deployment status');
  }
  const point = Buffer.from(m.deployment.code_out_point, 'hex'), tx = point.subarray(0, 32).toString('hex'), index = point.readUInt32LE(32);
  const selected = rows.filter((row) => row.chain_id === m.deployment.chain_id && hash(row.tx_hash) === tx && row.output_index === index);
  requireValue(selected.length === 1, 'missing or ambiguous deployed parent record');
  const row = selected[0];
  requireValue(hash(row.data_hash) === m.deployment.parent_data_hash && hash(row.code_hash) === m.deployment.parent_data_hash
    && row.dep_type === 'code' && typeof row.hash_type === 'string' && ['data', 'data1', 'data2'].includes(row.hash_type), 'deployed parent/code identity mismatch');
  requireValue(row.out_point === `0x${tx}:${index}`, 'redundant deployment OutPoint mismatch');
  if (row.artifact_hash !== undefined && row.artifact_hash !== null) requireValue(hash(row.artifact_hash) === m.deployment.parent_data_hash, 'deployed artifact mismatch');
  if (build?.artifact_hash !== undefined && build.artifact_hash !== null) requireValue(hash(build.artifact_hash) === m.deployment.parent_data_hash, 'deployed build mismatch');
  if (usage === 'candidate') requireValue(row.status === 'active', 'candidate deployment is not active');
}
