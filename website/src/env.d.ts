/// <reference path="../.astro/types.d.ts" />

// Minimal declarations for Node builtins used in build-time data
// loading (src/data/site.ts reads example source files). We avoid a
// full @types/node dependency since only readFileSync/path are used.
declare module "node:fs" {
  export function readFileSync(path: string, encoding: string): string;
}
declare module "node:url" {
  export function fileURLToPath(url: string): string;
}
declare module "node:path" {
  export function dirname(p: string): string;
  export function resolve(...paths: string[]): string;
}

// The WASM bundle is built by wasm-pack and served as a static asset
// from public/wasm/. TypeScript needs a module declaration so the
// playground's import resolves at check time. The wildcard covers the
// absolute-path import used in the client script.
declare module "*/wasm/cellscript_wasm.js" {
  export function compile_metadata_json(source: string, target: string | null): string;
  export function version(): string;
  export default function init(): Promise<void>;
}

declare module "/wasm/cellscript_wasm.js" {
  export function compile_metadata_json(source: string, target: string | null): string;
  export function version(): string;
  export default function init(): Promise<void>;
}
