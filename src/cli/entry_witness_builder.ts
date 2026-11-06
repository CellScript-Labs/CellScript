/** Raw native metadata stays complete, without expanding every lowering fact
 * into an unbounded declaration type. Dedicated SDK contracts remain typed. */
export type CellScriptJson = null | boolean | number | string | readonly CellScriptJson[] | { readonly [key: string]: CellScriptJson };
export type CellScriptMetadata = Readonly<Record<string, CellScriptJson>> & {
  readonly interface_hash: string;
};

interface GeneratedActionSpec {
  readonly name: string;
  readonly runtimeAccesses: readonly unknown[];
  readonly runtimeInputRequirements: readonly unknown[];
  readonly actionScanSelectors: ActionScanSelectors;
  readonly verifierObligations: readonly unknown[];
  readonly failClosedRuntimeFeatures: readonly string[];
  readonly boundedOutputPlans: readonly unknown[];
}

export type EntryWitnessInput = bigint | number | string | boolean | Uint8Array | null;
export interface EntryWitnessParameter {
  readonly name: string;
  readonly type: string;
  readonly encoding: "runtime" | "schema" | "u128" | "bytes" | "unit" | "bool" | "unsigned" | "signed" | "unsupported";
  readonly width: number;
}
export interface EntryWitnessEntry {
  readonly kind: "action" | "lock";
  readonly name: string;
  readonly params: readonly EntryWitnessParameter[];
}
export interface EntryWitnessContracts {
  readonly schema: "cellscript-generated-entry-witness-contract-v1";
  readonly payload_abi: string;
  readonly placement_abi: string;
  readonly placement_field: string;
  readonly magic: readonly number[];
  readonly entries: readonly EntryWitnessEntry[];
}

function entryWitnessBytes(value: EntryWitnessInput, label: string): Uint8Array {
  if (value instanceof Uint8Array) return new Uint8Array(value);
  if (typeof value === "string" && /^0x(?:[0-9a-f]{2})*$/.test(value)) {
    return Uint8Array.from(value.slice(2).match(/../g) ?? [], byte => Number.parseInt(byte, 16));
  }
  throw new Error("CellScript entry witness '" + label + "' requires bytes or canonical hexadecimal");
}

function entryWitnessInteger(value: EntryWitnessInput, width: number, signed: boolean, label: string): Uint8Array {
  if (!(typeof value === "bigint" || (typeof value === "number" && Number.isSafeInteger(value))
    || (typeof value === "string" && /^-?(0|[1-9][0-9]*)$/.test(value)))) {
    throw new Error("CellScript entry witness '" + label + "' requires an exact integer");
  }
  const integer = BigInt(value as bigint | number | string);
  const bits = BigInt(width * 8);
  const lower = signed ? -(1n << (bits - 1n)) : 0n;
  const upper = signed ? (1n << (bits - 1n)) - 1n : (1n << bits) - 1n;
  if (integer < lower || integer > upper) throw new Error("CellScript entry witness '" + label + "' exceeds its integer range");
  let remaining = integer < 0n ? (1n << bits) + integer : integer;
  return Uint8Array.from({ length: width }, () => { const byte = Number(remaining & 255n); remaining >>= 8n; return byte; });
}

/** Encode the inner entry payload before the existing WitnessArgs placement
 * and signer. Arguments follow authored witness order; runtime-owned bindings
 * are omitted. Unit accepts an optional null, matching the native codec. */
export function encodeEntryWitness(kind: "action" | "lock", name: string, args: readonly EntryWitnessInput[]): Uint8Array {
  const entry = entryWitnessContracts.entries.find(candidate => candidate.kind === kind && candidate.name === name);
  if (!entry || !Array.isArray(args)) throw new Error("CellScript entry witness has no declared entry or argument array");
  const parts: Uint8Array[] = [];
  let index = 0;
  let consumes = false;
  for (const param of entry.params) {
    if (param.encoding === "runtime") continue;
    consumes = true;
    if (param.encoding === "unsupported") throw new Error("CellScript entry witness unsupported parameter '" + param.name + "'");
    if (param.encoding === "unit") { if (args[index] === null) index += 1; continue; }
    if (index >= args.length) throw new Error("CellScript entry witness missing argument '" + param.name + "'");
    const value = args[index++];
    let bytes: Uint8Array;
    if (param.encoding === "bool") {
      if (typeof value !== "boolean") throw new Error("CellScript entry witness '" + param.name + "' requires a boolean");
      bytes = Uint8Array.of(value ? 1 : 0);
    } else if (param.encoding === "unsigned" || param.encoding === "signed"
      || (param.encoding === "u128" && !(value instanceof Uint8Array) && !(typeof value === "string" && value.startsWith("0x")))) {
      bytes = entryWitnessInteger(value, param.width, param.encoding === "signed", param.name);
    } else {
      bytes = entryWitnessBytes(value, param.name);
      if (param.encoding === "schema") {
        if (bytes.length > 0xffffffff) throw new Error("CellScript entry witness schema exceeds its u32 length");
        const length = new Uint8Array(4);
        new DataView(length.buffer).setUint32(0, bytes.length, true);
        parts.push(length);
      } else if (bytes.length !== param.width) {
        throw new Error("CellScript entry witness '" + param.name + "' has the wrong fixed width");
      }
    }
    parts.push(bytes);
  }
  if (index !== args.length) throw new Error("CellScript entry witness has extra arguments");
  if (!consumes) return new Uint8Array();
  const magic = Uint8Array.from(entryWitnessContracts.magic);
  const output = new Uint8Array(parts.reduce((length, part) => length + part.length, magic.length));
  output.set(magic);
  let offset = magic.length;
  for (const part of parts) { output.set(part, offset); offset += part.length; }
  return output;
}

export function encodeLockEntryWitness(name: string, args: readonly EntryWitnessInput[]): Uint8Array {
  return encodeEntryWitness("lock", name, args);
}
