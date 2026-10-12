import { lstatSync, readFileSync } from 'node:fs';
import { compareLifecycle, readLifecyclePackagePlan } from './lifecycle.ts';

function readBounded(path: string, limit: number): Buffer {
  const metadata = lstatSync(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > limit) throw new Error('invalid lifecycle input file');
  const bytes = readFileSync(path);
  if (bytes.length > limit) throw new Error('lifecycle input grew beyond budget');
  return bytes;
}
const [oldPath, oldDigest, newPath, newDigest, packagePath, ...extra] = process.argv.slice(2);
if (!oldPath || !oldDigest || !newPath || !newDigest || extra.length) {
  throw new Error('usage: npm run lifecycle -- OLD.json TRUSTED_OLD_SHA256 NEW.json TRUSTED_NEW_SHA256 [PACKAGE_PLAN.json]');
}
const plan = packagePath ? readLifecyclePackagePlan(readBounded(packagePath, 8 * 1024 * 1024)) : undefined;
process.stdout.write(`${JSON.stringify(compareLifecycle(readBounded(oldPath, 32768), oldDigest,
  readBounded(newPath, 32768), newDigest, plan), null, 2)}\n`);
