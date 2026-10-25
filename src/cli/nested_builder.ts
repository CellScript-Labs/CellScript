/** One canonical nested value. The existing entry-witness and transaction
 * adapters still own framing, final size limits, capacity and signing. */
export interface NestedElementInput {
  readonly fixedFields: readonly Uint8Array[];
  readonly payload: Uint8Array;
}

export type NestedCellFieldInput = Uint8Array | readonly NestedElementInput[];

interface NestedCellWireContract {
  readonly schema_hash: string;
  readonly max_encoded_bytes: number;
  readonly fields: readonly { readonly name: string; readonly ty: string; readonly fixed_width: number | null }[];
}

/** Complete canonical Cell data for a declared bounded schema. Supply exact
 * fixed-field bytes and nested element inputs by field name. Pass the result
 * to the normal transaction adapter with actual Lock/Type scripts; that adapter
 * must calculate occupied capacity and preserve the existing identity checks. */
export function encodeNestedCellData(type: string, fields: Readonly<Record<string, NestedCellFieldInput>>): Uint8Array {
  const contracts: Readonly<Record<string, NestedCellWireContract>> = nestedCellContracts;
  if (!Object.hasOwn(contracts, type)) throw new Error("Nested Cell type is not declared by this artifact");
  const contract = contracts[type];
  if (fields === null || typeof fields !== "object" || Array.isArray(fields)
      || Reflect.ownKeys(fields).length !== contract.fields.length) {
    throw new Error("Nested Cell fields must exactly match the source declaration");
  }
  const header = 4 * (contract.fields.length + 1);
  let total = header;
  const encoded: Uint8Array[] = [];
  for (const field of contract.fields) {
    if (!Object.hasOwn(fields, field.name)) throw new Error("Nested Cell is missing a declared field");
    const input = fields[field.name];
    let bytes: Uint8Array;
    if (field.fixed_width === null) {
      if (!Array.isArray(input)) throw new Error("Nested Cell field requires nested element inputs");
      bytes = encodeNestedValues(field.ty, input);
    } else {
      if (!(input instanceof Uint8Array) || input.length !== field.fixed_width) {
        throw new Error("Nested Cell fixed field differs from its declared width");
      }
      bytes = input;
    }
    total += bytes.length;
    if (!Number.isSafeInteger(total) || total > contract.max_encoded_bytes || total > 0xffffffff) {
      throw new Error("Nested Cell exceeds its declared total byte bound");
    }
    encoded.push(bytes);
  }
  const data = new Uint8Array(total);
  const view = new DataView(data.buffer);
  view.setUint32(0, total, true);
  let cursor = header;
  encoded.forEach((bytes, index) => {
    view.setUint32(4 * (index + 1), cursor, true);
    data.set(bytes, cursor);
    cursor += bytes.length;
  });
  return data;
}

interface NestedValueWireContract {
  readonly profile: string;
  readonly element: { readonly kind: string; readonly fixed_field_widths?: readonly number[] };
  readonly max_items: number;
  readonly max_payload_bytes: number;
  readonly max_total_bytes: number;
}

/** Encode only a type declared by this generated artifact. Fixed fields are
 * little-endian unsigned integers or exact byte arrays in declaration order.
 * This helper does not authenticate the metadata or admit the experimental ELF. */
export function encodeNestedValues(type: string, items: readonly NestedElementInput[]): Uint8Array {
  const contracts: Readonly<Record<string, NestedValueWireContract>> = nestedValueContracts;
  if (!Object.hasOwn(contracts, type)) throw new Error("Nested type is not declared by this artifact");
  const contract = contracts[type];
  const bound = (value: number, low: number, high: number): boolean => Number.isInteger(value) && value >= low && value <= high;
  if (contract.profile !== "cellscript-bounded-nested-values-v1" || !bound(contract.max_items, 0, 32)
      || !bound(contract.max_payload_bytes, 0, 256) || !bound(contract.max_total_bytes, 4, 4096)) {
    throw new Error("Invalid nested source contract");
  }
  const widths = contract.element.kind === "bytes" ? [] : contract.element.fixed_field_widths;
  if (!widths || (contract.element.kind !== "bytes" && contract.element.kind !== "record")
      || (contract.element.kind === "record" && (widths.length < 1 || widths.length > 8))
      || widths.some(width => !bound(width, 1, 256))) throw new Error("Invalid nested record layout");
  if (!Array.isArray(items) || items.length > contract.max_items) throw new Error("Nested item count exceeds the declared bound");
  const recordHeader = widths.length === 0 ? 0 : 4 * (widths.length + 2);
  const fixedSize = widths.reduce((sum, width) => sum + width, 0);
  let total = 4 * (items.length + 1);
  for (const item of items) {
    if (!item || !Array.isArray(item.fixedFields) || item.fixedFields.length !== widths.length) {
      throw new Error("Nested fixed fields differ from the declared record layout");
    }
    for (let i = 0; i < widths.length; i++) {
      const field: unknown = item.fixedFields[i];
      if (!(field instanceof Uint8Array) || field.length !== widths[i]) {
        throw new Error("Nested fixed fields differ from the declared record layout");
      }
    }
    if (!(item.payload instanceof Uint8Array) || item.payload.length > contract.max_payload_bytes) {
      throw new Error("Nested payload exceeds the declared bound");
    }
    total += recordHeader + fixedSize + 4 + item.payload.length;
    if (total > contract.max_total_bytes) throw new Error("Nested encoding exceeds the declared total byte bound");
  }
  if (total > contract.max_total_bytes) throw new Error("Nested header exceeds the declared total byte bound");
  const encoded = new Uint8Array(total);
  const view = new DataView(encoded.buffer);
  const put = (at: number, value: number): void => view.setUint32(at, value, true);
  put(0, total);
  let cursor = 4 * (items.length + 1);
  items.forEach((item: NestedElementInput, index: number) => {
    put(4 * (index + 1), cursor);
    if (recordHeader !== 0) {
      put(cursor, recordHeader + fixedSize + 4 + item.payload.length);
      let offset = recordHeader;
      item.fixedFields.forEach((field, fieldIndex) => {
        put(cursor + 4 * (fieldIndex + 1), offset);
        encoded.set(field, cursor + offset);
        offset += field.length;
      });
      put(cursor + 4 * (widths.length + 1), offset);
      cursor += offset;
    }
    put(cursor, item.payload.length);
    encoded.set(item.payload, cursor + 4);
    cursor += 4 + item.payload.length;
  });
  return encoded;
}
