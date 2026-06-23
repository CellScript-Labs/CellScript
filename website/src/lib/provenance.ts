/**
 * View-model helpers for the precompiled provenance data.
 *
 * The website's provenance rail and hero compile-output indicator read
 * `src/data/provenance.json`, which is produced by
 * `website/scripts/regen-provenance.py` from live `cellc metadata`
 * output. These helpers turn that raw JSON into the compact shapes the
 * UI renders, and provide the formatting helpers (KB, cycles, hashes).
 */

import provenance from "../data/provenance.json";

export type EffectClass = "Pure" | "Mutating" | "Creating" | "Destroying" | string;

export interface ProvenanceEntry {
  op: string;
  type: string;
  binding: string;
}

export interface ProvenanceType {
  name: string;
  kind: string;
  capabilities: string[];
  encodedSize: number | null;
  flowStates: string[];
}

export interface ProvenanceAction {
  name: string;
  effectClass: EffectClass;
  consume: ProvenanceEntry[];
  create: ProvenanceEntry[];
  estimatedCycles: number | null;
  parallelizable: boolean | null;
}

export interface ProvenanceView {
  module: string;
  target: string;
  artifactSizeBytes: number;
  artifactHash: string;
  sourceHash: string;
  compilerVersion: string;
  types: ProvenanceType[];
  actions: ProvenanceAction[];
}

export type ProvenanceData = Record<string, ProvenanceView>;

export const provenanceByExample = provenance as ProvenanceData;

/**
 * Resolve a hero/example id to its provenance view. Example ids in
 * site.ts ("amm") don't always match the provenance file stem
 * ("amm_pool"), so this falls back to a prefix match.
 */
export function provenanceForExample(exampleId: string): ProvenanceView | null {
  if (provenanceByExample[exampleId]) return provenanceByExample[exampleId];
  // Try matching by prefix: example id "ammPool" -> provenance key
  // "amm_pool" (split on camelCase boundaries and underscores).
  const normalized = exampleId.replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase();
  const prefixMatch = Object.keys(provenanceByExample).find(
    (key) => normalized === key || normalized === key.split("_")[0],
  );
  return prefixMatch ? provenanceByExample[prefixMatch] : null;
}

/** Distinct effect classes in an example, e.g. ["Pure", "Mutating"]. */
export function distinctEffects(view: ProvenanceView): EffectClass[] {
  const seen = new Set<EffectClass>();
  for (const action of view.actions) seen.add(action.effectClass);
  return [...seen];
}

/** "Pure / Mutating" — effects joined with a slash for the hero indicator. */
export function effectsSummary(view: ProvenanceView): string {
  return distinctEffects(view).join(" / ") || "—";
}

/** Bytes -> "8.2 KB" (human-readable artifact size). */
export function formatBytes(bytes: number): string {
  if (bytes >= 1024) {
    const kb = bytes / 1024;
    return kb >= 1024 ? `${(kb / 1024).toFixed(2)} MB` : `${kb.toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

/** 572 -> "572 cycles"; null -> "—". */
export function formatCycles(cycles: number | null): string {
  return cycles == null ? "—" : `${cycles.toLocaleString()} cycles`;
}

/** Short hash for display: keep the first 12 hex chars. */
export function shortHash(hash: string): string {
  return hash.slice(0, 12);
}
