interface FixedCellWireContract {
  readonly schema_hash: string;
  readonly encoded_bytes: number;
  readonly fields: readonly { readonly name: string; readonly ty: string; readonly width: number }[];
}

/** Encode a supported flat fixed Cell schema from exact named field bytes.
 * Scalars use little-endian bytes; bool is exactly 0 or 1. This neither signs
 * the transaction nor authenticates metadata or experimental artifact proofs. */
export function encodeFixedCellData(type: string, fields: Readonly<Record<string, Uint8Array>>): Uint8Array {
  const contracts: Readonly<Record<string, FixedCellWireContract>> = fixedCellContracts;
  if (!Object.hasOwn(contracts, type)) throw new Error("Fixed Cell type is not supported by this artifact");
  const contract = contracts[type];
  if (fields === null || typeof fields !== "object" || Array.isArray(fields)
      || Reflect.ownKeys(fields).length !== contract.fields.length) {
    throw new Error("Fixed Cell fields must exactly match the source declaration");
  }
  const values: Uint8Array[] = [];
  for (const field of contract.fields) {
    if (!Object.hasOwn(fields, field.name)) throw new Error("Fixed Cell is missing a declared field");
    const value = fields[field.name];
    if (!(value instanceof Uint8Array) || value.length !== field.width || (field.ty === "bool" && value[0] > 1)) {
      throw new Error("Fixed Cell field width or boolean value is not canonical");
    }
    // Snapshot caller-owned bytes before reading later fields or getters.
    values.push(value.slice());
  }
  const data = new Uint8Array(contract.encoded_bytes);
  let cursor = 0;
  for (const value of values) { data.set(value, cursor); cursor += value.length; }
  return data;
}
