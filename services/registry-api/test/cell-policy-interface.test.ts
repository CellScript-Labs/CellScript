import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { validateInterfaceUpgrade } from "../src/domain";

type NativeInterface = Record<string, unknown> & { types: Record<string, unknown>[] };
const fixture = JSON.parse(readFileSync(new URL("./cell-policy-interface-fixture.json", import.meta.url), "utf8")) as {
  source: string;
  interface: NativeInterface;
};
const native = fixture.interface;

function changed(name: string, update: (item: Record<string, unknown>) => void): NativeInterface {
  const candidate = structuredClone(native);
  const item = candidate.types.find((item) => item["name"] === name);
  if (!item) throw new Error(`missing actual compiler type ${name}`);
  update(item);
  return candidate;
}

describe("current declaration-owned Cell policies", () => {
  it("admits the actual native v7 interface for value/resource/shared/receipt declarations", () => {
    expect(native["schema"]).toBe("cellscript-package-interface-v7");
    expect(() => validateInterfaceUpgrade(native, native)).not.toThrow();
  });

  for (const policy of ["ckb-type-id", "script-args", "singleton-type"]) {
    it(`retains the declaration policy ${policy}`, () => {
      const candidate = changed("Unique", (item) => { item["identity_policy"] = policy; });
      expect(() => validateInterfaceUpgrade(candidate, candidate)).not.toThrow();
    });
  }

  it("uses the native 512-byte UTF-8 boundary for opaque declaration references", () => {
    const result = changed("Voucher", (item) => { item["claim_output"] = `${"界".repeat(170)}ab`; });
    expect(() => validateInterfaceUpgrade(result, result)).not.toThrow();
    const field = changed("Unique", (item) => { item["identity_policy"] = `field:id.${"界".repeat(169)}ab`; });
    expect(() => validateInterfaceUpgrade(field, field)).not.toThrow();
  });

  it("rejects an identity-policy change even when all layouts and capabilities agree", () => {
    const candidate = changed("Unique", (item) => { item["identity_policy"] = "none"; });
    expect(() => validateInterfaceUpgrade(candidate, candidate)).not.toThrow();
    expect(() => validateInterfaceUpgrade(native, candidate)).toThrow(/changed incompatibly/);
    expect(() => validateInterfaceUpgrade(candidate, native)).toThrow(/changed incompatibly/);
  });

  it("rejects a declared receipt-result change independently of its physical layout", () => {
    const candidate = changed("Voucher", (item) => { item["claim_output"] = "B"; });
    expect(() => validateInterfaceUpgrade(candidate, candidate)).not.toThrow();
    expect(() => validateInterfaceUpgrade(native, candidate)).toThrow(/changed incompatibly/);
    expect(() => validateInterfaceUpgrade(candidate, native)).toThrow(/changed incompatibly/);
    const omitted = changed("Voucher", (item) => { item["claim_output"] = null; });
    expect(() => validateInterfaceUpgrade(omitted, omitted)).not.toThrow();
    expect(() => validateInterfaceUpgrade(native, omitted)).toThrow(/changed incompatibly/);
  });

  const invalid: [string, string, (item: Record<string, unknown>) => void][] = [
    ["missing resource identity", "Unique", (item) => { delete item["identity_policy"]; }],
    ["unknown identity policy", "Unique", (item) => { item["identity_policy"] = "future-policy"; }],
    ["undeclared identity field", "Unique", (item) => { item["identity_policy"] = "field:missing"; }],
    ["non-string identity", "Unique", (item) => { item["identity_policy"] = 1; }],
    ["identity on a value", "Value", (item) => { item["identity_policy"] = "none"; }],
    ["claim result on a resource", "A", (item) => { item["claim_output"] = "B"; }],
    ["empty receipt result", "Voucher", (item) => { item["claim_output"] = ""; }],
    ["non-string receipt result", "Voucher", (item) => { item["claim_output"] = []; }],
    ["UTF-8 result budget", "Voucher", (item) => { item["claim_output"] = "界".repeat(171); }],
    ["UTF-8 identity-field budget", "Unique", (item) => { item["identity_policy"] = `field:id.${"界".repeat(170)}`; }],
  ];
  for (const [name, type, update] of invalid) {
    it(`rejects ${name} before upgrade comparison`, () => {
      const candidate = changed(type, update);
      expect(() => validateInterfaceUpgrade(candidate, candidate)).toThrow(/Cell policies/);
    });
  }
});
