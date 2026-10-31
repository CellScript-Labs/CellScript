import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ApiError, validateInterfaceUpgrade } from "../src/domain";
import { callableEffectFacts, effectFactsAccept, readEffectFacts } from "../src/effect-facts";
import { publicInterface, callable } from "./interface-fixtures";

const vectors = JSON.parse(readFileSync(new URL("../../../crates/cellscript-artifact-checker/src/effect_facts_vectors.json", import.meta.url), "utf8")) as {
  valid: { name: string; value: unknown }[];
  invalid: { name: string; value: unknown }[];
  comparisons: { name: string; required: unknown; candidate: unknown; accepts: boolean }[];
};

describe("current inferred interface effects", () => {
  it("agrees with actual compiler source interfaces and the Rust comparison vectors", () => {
    const emitted = JSON.parse(readFileSync(new URL("../../../tests/fixtures/effect_facts_interfaces.json", import.meta.url), "utf8")) as {
      cases: { name: string; interface: unknown }[];
      comparisons: { required: string; candidate: string; accepts: boolean }[];
    };
    const interfaces = new Map(emitted.cases.map((item) => [item.name, item.interface]));
    for (const item of emitted.cases) expect(() => validateInterfaceUpgrade(item.interface, item.interface)).not.toThrow();
    for (const comparison of emitted.comparisons) {
      const check = () => validateInterfaceUpgrade(interfaces.get(comparison.required), interfaces.get(comparison.candidate));
      if (comparison.accepts) expect(check).not.toThrow();
      else expect(check).toThrow(/inferred effect facts increased/);
    }
  });

  for (const { name, value } of vectors.valid) {
    it(`accepts canonical shared vector: ${name}`, () => expect(readEffectFacts(value)).toEqual(value));
  }
  for (const { name, value } of vectors.invalid) {
    it(`rejects shared vector: ${name}`, () => expect(() => readEffectFacts(value)).toThrow());
  }
  for (const { name, required, candidate, accepts } of vectors.comparisons) {
    it(`uses the product relation in Registry admission: ${name}`, () => {
      expect(effectFactsAccept(readEffectFacts(required), readEffectFacts(candidate))).toBe(accepts);
      const old = { ...publicInterface(), callables: [{ ...callable(), effects: required }] };
      const next = { ...publicInterface(), callables: [{ ...callable(), effects: candidate }] };
      if (accepts) expect(() => validateInterfaceUpgrade(old, next)).not.toThrow();
      else expect(() => validateInterfaceUpgrade(old, next)).toThrow(/inferred effect facts increased/);
    });
  }

  it("requires facts and rejects mixed callable generations", () => {
    const current: Record<string, unknown> = callable();
    expect(callableEffectFacts(current)).toEqual(current.effects);
    for (const field of ["effect", "effect_class", "declared_effect", "inferred_effect_class"]) {
      expect(() => callableEffectFacts({ ...current, [field]: "Pure" })).toThrow();
    }
    delete current.effects;
    expect(() => callableEffectFacts(current)).toThrow();
    expect(() => callableEffectFacts({ ...callable(), effects: null })).toThrow();
  });

  it("retains explicit unavailable template evidence without defaulting", () => {
    const template = { ...callable(), type_parameters: [{ name: "T", phantom: false, constraints: [] }], effects: null };
    expect(callableEffectFacts(template)).toBeNull();
    expect(() => callableEffectFacts({ ...template, effects: callable().effects })).toThrow();
    expect(() => validateInterfaceUpgrade({ ...publicInterface(), callables: [template] }, { ...publicInterface(), callables: [template] })).not.toThrow();
  });

  it("rejects every retired interface generation on both sides of an upgrade", () => {
    for (const version of [2, 3, 4]) {
      const old = { ...publicInterface(), schema: `cellscript-package-interface-v${version}`, version };
      for (const [required, candidate] of [[old, publicInterface()], [publicInterface(), old]]) {
        try {
          validateInterfaceUpgrade(required, candidate);
          throw new Error("retired interface admitted");
        } catch (error) {
          expect(error).toBeInstanceOf(ApiError);
          expect((error as ApiError).code).toBe("unsupported_public_interface");
        }
      }
    }
  });

  it("enforces the source-fact set budget before comparison", () => {
    expect(() => readEffectFacts({ ...callable().effects, process_ops: Array(4097).fill("wait") })).toThrow(/bounded/);
  });
});
