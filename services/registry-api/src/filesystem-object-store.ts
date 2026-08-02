import { mkdir, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { dirname, resolve, sep } from "node:path";

import { sha256Hex } from "./domain";
import type { RegistryObjectRead, RegistryObjectReader, SnapshotWriter } from "./index";

export class FilesystemObjectStore implements SnapshotWriter, RegistryObjectReader {
  constructor(private readonly root: string) {}

  async put(
    key: string,
    body: Uint8Array,
    _options: { contentType: string; metadata: Record<string, string> },
  ): Promise<void> {
    const path = this.pathFor(key);
    await mkdir(dirname(path), { recursive: true, mode: 0o750 });
    const temporary = `${path}.tmp-${randomUUID()}`;
    try {
      await writeFile(temporary, body, { mode: 0o640, flag: "wx" });
      await rename(temporary, path);
    } catch (error) {
      await unlink(temporary).catch(() => undefined);
      throw error;
    }
  }

  async get(key: string): Promise<RegistryObjectRead | null> {
    try {
      const body = await readFile(this.pathFor(key));
      return {
        body,
        contentType: contentTypeFor(key),
        etag: `"sha256-${await sha256Hex(body)}"`,
      };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
  }

  async delete(key: string): Promise<void> {
    await unlink(this.pathFor(key)).catch((error: NodeJS.ErrnoException) => {
      if (error.code !== "ENOENT") throw error;
    });
  }

  pathFor(key: string): string {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9._/-]{0,1023}$/.test(key) || key.split("/").includes("..")) {
      throw new Error("registry object key is invalid");
    }
    const path = resolve(this.root, key);
    if (path !== this.root && !path.startsWith(`${this.root}${sep}`)) {
      throw new Error("registry object key escapes the configured root");
    }
    return path;
  }
}

function contentTypeFor(key: string): string {
  if (key.endsWith(".json")) return "application/json; charset=utf-8";
  if (key.endsWith(".tar.gz")) return "application/gzip";
  if (key.endsWith(".tar")) return "application/x-tar";
  return "application/octet-stream";
}
