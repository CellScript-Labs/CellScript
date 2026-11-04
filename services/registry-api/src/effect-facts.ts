/** Current inferred facts. This validates claims; it does not verify execution. */
const sourceKinds = ["cell-dep", "current-script", "group-input", "group-output", "header-dep", "header-evidence", "input", "output", "parameter", "transaction", "witness"] as const;
const accessKinds = ["capacity", "cardinality", "cell-data", "environment-evidence", "header", "index", "lock-script", "occupied-capacity", "out-point", "role-selection", "script", "since", "transaction", "type-script", "witness"] as const;
const lifecycleKinds = ["claim", "consume", "create", "destroy", "replace", "settle", "transfer"] as const;
const processKinds = ["close", "exec", "inherited-descriptors", "pipe-create", "pipe-read", "pipe-write", "process-id", "spawn", "wait"] as const;
type SourceKind = typeof sourceKinds[number];
type Source = { kind: Exclude<SourceKind, "parameter"> } | { kind: "parameter"; index: number };
type Read = { source: Source; access: typeof accessKinds[number] };
export type EffectFacts = {
  context_reads: Read[];
  lifecycle_ops: (typeof lifecycleKinds[number])[];
  process_ops: (typeof processKinds[number])[];
};
const limit = 4096;
const compare = (left: string | number, right: string | number): number => left < right ? -1 : left > right ? 1 : 0;

function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("effect facts require an object");
  return value as Record<string, unknown>;
}
function exactKeys(value: unknown, keys: readonly string[]): Record<string, unknown> {
  const record = object(value);
  if (Object.keys(record).length !== keys.length || keys.some((key) => !Object.hasOwn(record, key))) {
    throw new Error("effect facts require exactly the current fields");
  }
  return record;
}
function member<T extends string>(value: unknown, choices: readonly T[]): T {
  if (typeof value !== "string" || !choices.includes(value as T)) throw new Error("unknown effect fact identity");
  return value as T;
}
function read(value: unknown): Read {
  const record = exactKeys(value, ["source", "access"]);
  const rawSource = object(record["source"]);
  const kind = member(rawSource["kind"], sourceKinds);
  exactKeys(rawSource, kind === "parameter" ? ["kind", "index"] : ["kind"]);
  let source: Source;
  if (kind === "parameter") {
    const index = rawSource["index"];
    if (typeof index !== "number" || !Number.isInteger(index) || index < 0 || index > 65535) {
      throw new Error("effect parameter position must fit u16");
    }
    source = { kind, index };
  } else {
    source = { kind };
  }
  return { source, access: member(record["access"], accessKinds) };
}
function compareReads(left: Read, right: Read): number {
  return compare(left.source.kind, right.source.kind)
    || compare(left.source.kind === "parameter" ? left.source.index : 0, right.source.kind === "parameter" ? right.source.index : 0)
    || compare(left.access, right.access);
}
function set<T>(value: unknown, parse: (item: unknown) => T, order: (left: T, right: T) => number): T[] {
  if (!Array.isArray(value) || value.length > limit) throw new Error("effect facts require a bounded array");
  const items = value.map(parse);
  for (let index = 1; index < items.length; index += 1) {
    if (order(items[index - 1]!, items[index]!) >= 0) throw new Error("effect facts must be unique and canonically ordered");
  }
  return items;
}

export function readEffectFacts(value: unknown): EffectFacts {
  const record = exactKeys(value, ["context_reads", "lifecycle_ops", "process_ops"]);
  const facts: EffectFacts = {
    context_reads: set(record["context_reads"], read, compareReads),
    lifecycle_ops: set(record["lifecycle_ops"], (item) => member(item, lifecycleKinds), compare),
    process_ops: set(record["process_ops"], (item) => member(item, processKinds), compare),
  };
  for (const operation of ["claim", "destroy", "replace", "settle", "transfer"] as const) {
    if (facts.lifecycle_ops.includes(operation)
      && (!facts.lifecycle_ops.includes("consume") || (operation !== "destroy" && !facts.lifecycle_ops.includes("create")))) {
      throw new Error("compound lifecycle effect lacks its constituents");
    }
  }
  return facts;
}

/** An unavailable generic-template contract is explicit null, never missing. */
export function callableEffectFacts(callable: Record<string, unknown>): EffectFacts | null {
  const fields = ["identity", "name", "kind", "visibility", "type_parameters", "params", "return_type", "outputs", "effects", "entry_witness_abi", "builder_contract_hash"];
  exactKeys(callable, fields);
  if (!Array.isArray(callable["type_parameters"])) throw new Error("effect contract requires type parameters");
  if (callable["type_parameters"].length > 0) {
    if (callable["effects"] !== null) throw new Error("generic source effect evidence must be explicit null");
    return null;
  }
  return readEffectFacts(callable["effects"]);
}

/** Candidate facts must be a subset in every independent dimension. */
export function effectFactsAccept(required: EffectFacts, candidate: EffectFacts): boolean {
  const readKey = ({ source, access }: Read): string => `${source.kind}/${source.kind === "parameter" ? source.index : 0}/${access}`;
  const reads = new Set(required.context_reads.map(readKey));
  const cells = new Set(required.lifecycle_ops);
  const processes = new Set(required.process_ops);
  return candidate.context_reads.every((item) => reads.has(readKey(item)))
    && candidate.lifecycle_ops.every((item) => cells.has(item))
    && candidate.process_ops.every((item) => processes.has(item));
}
