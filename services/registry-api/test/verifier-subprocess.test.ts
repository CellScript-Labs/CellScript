import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { canonicalJson, ckbBlake2bHex } from "../src/domain";
import { executeVerifierSubprocess } from "../src/verifier-subprocess";

const verifierBinary = process.env["CELLSCRIPT_REGISTRY_VERIFIER_TEST_BINARY"]?.trim();
const temporaryRoots: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe.skipIf(!verifierBinary)("Rust verifier subprocess contract", () => {
  it("passes a copy-material bundle through the same Node subprocess boundary used by the worker", async () => {
    const root = await mkdtemp(join(tmpdir(), "cellscript-verifier-contract-"));
    temporaryRoots.push(root);
    const source = new TextEncoder().encode("starter artifact");
    const contract = {
      schema: "cellscript-registry-profile-contract-v1",
      artifact_kind: "template",
      profile: "copy_material",
      copy: { format: "file_map_v1", entrypoint: "template.cell" },
    };
    const manifestJson = canonicalJson(contract);
    const bundle = {
      schema: "cellscript-registry-bundle",
      namespace: "cellscript",
      name: "starter",
      release: "1.0.0",
      profile: "copy_material",
      manifest_json: manifestJson,
      objects: [{ role: "source", content_base64: Buffer.from(source).toString("base64") }],
    };
    const snapshotPath = join(root, "artifact.bundle.json");
    await writeFile(snapshotPath, JSON.stringify(bundle));
    const args = verifierArgs(snapshotPath, ckbBlake2bHex(source), ckbBlake2bHex(manifestJson));

    const result = await executeVerifierSubprocess(verifierBinary!, args, {
      cwd: root,
      env: { ...process.env, NO_COLOR: "1" },
      timeoutMs: 30_000,
    });

    expect(result.timedOut).toBe(false);
    expect(result.exitCode).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: "passed",
      verification_level: "hash_bound",
      artifact_format: "copy-material",
    });
  });

  it("preserves the Rust verifier's stable rejection code", async () => {
    const root = await mkdtemp(join(tmpdir(), "cellscript-verifier-contract-"));
    temporaryRoots.push(root);
    const snapshotPath = join(root, "invalid.bundle.json");
    await writeFile(snapshotPath, "not-json");

    const result = await executeVerifierSubprocess(
      verifierBinary!,
      verifierArgs(snapshotPath, "11".repeat(32), "22".repeat(32)),
      { cwd: root, env: { ...process.env, NO_COLOR: "1" }, timeoutMs: 30_000 },
    );

    expect(result.timedOut).toBe(false);
    expect(result.exitCode).toBe(1);
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: "failed",
      error_code: "artifact_bundle_invalid",
    });
  });
});

function verifierArgs(snapshotPath: string, sourceHash: string, manifestHash: string): string[] {
  return [
    "--snapshot", snapshotPath,
    "--namespace", "cellscript",
    "--name", "starter",
    "--version", "1.0.0",
    "--source-hash", sourceHash,
    "--manifest-hash", manifestHash,
    "--artifact-kind", "template",
    "--profile", "copy_material",
  ];
}
