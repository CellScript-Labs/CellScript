import { spawn, type ChildProcess } from "node:child_process";

export interface VerifierSubprocessResult {
  exitCode: number | null;
  timedOut: boolean;
  stdout: string;
  stderr: string;
}

export interface VerifierSubprocessOptions {
  cwd: string;
  env: NodeJS.ProcessEnv;
  timeoutMs: number;
  maximumOutputBytes?: number;
  onSpawn?: (child: ChildProcess) => void;
}

export async function executeVerifierSubprocess(
  binary: string,
  args: string[],
  options: VerifierSubprocessOptions,
): Promise<VerifierSubprocessResult> {
  const child = spawn(binary, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  options.onSpawn?.(child);
  return collectVerifierSubprocess(child, options.timeoutMs, options.maximumOutputBytes ?? 1024 * 1024);
}

async function collectVerifierSubprocess(
  child: ChildProcess,
  timeoutMs: number,
  maximumOutputBytes: number,
): Promise<VerifierSubprocessResult> {
  let stdout = "";
  let stderr = "";
  let overflow = false;
  child.stdout?.setEncoding("utf8");
  child.stderr?.setEncoding("utf8");
  child.stdout?.on("data", (chunk: string) => {
    if (overflow) return;
    if (Buffer.byteLength(stdout) + Buffer.byteLength(chunk) > maximumOutputBytes) {
      overflow = true;
      child.kill("SIGKILL");
      return;
    }
    stdout += chunk;
  });
  child.stderr?.on("data", (chunk: string) => {
    if (overflow) return;
    if (Buffer.byteLength(stderr) + Buffer.byteLength(chunk) > maximumOutputBytes) {
      overflow = true;
      child.kill("SIGKILL");
      return;
    }
    stderr += chunk;
  });
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    child.kill("SIGKILL");
  }, timeoutMs);
  const exitCode = await new Promise<number | null>((resolveExit, reject) => {
    child.once("error", reject);
    child.once("close", (code) => resolveExit(code));
  }).finally(() => clearTimeout(timer));
  if (overflow) throw new Error("CellScript verifier output exceeded the configured limit");
  return { exitCode, timedOut, stdout, stderr };
}
