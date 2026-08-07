import type { Env } from "./index";

type NodeCkbRpcEnv = Pick<
  Env,
  | "CKB_MAINNET_RPC_URL"
  | "CKB_RPC_URL"
  | "CKB_RPC_TIMEOUT_MS"
  | "CKB_RPC_MAX_RESPONSE_BYTES"
  | "CKB_DEP_GROUP_MAX_MEMBERS"
>;

export function nodeCkbRpcEnv(
  processEnv: Readonly<Record<string, string | undefined>>,
): Partial<NodeCkbRpcEnv> {
  return {
    ...(processEnv["CKB_MAINNET_RPC_URL"] ? { CKB_MAINNET_RPC_URL: processEnv["CKB_MAINNET_RPC_URL"] } : {}),
    ...(processEnv["CKB_RPC_URL"] ? { CKB_RPC_URL: processEnv["CKB_RPC_URL"] } : {}),
    ...(processEnv["CKB_RPC_TIMEOUT_MS"] ? { CKB_RPC_TIMEOUT_MS: processEnv["CKB_RPC_TIMEOUT_MS"] } : {}),
    ...(processEnv["CKB_RPC_MAX_RESPONSE_BYTES"]
      ? { CKB_RPC_MAX_RESPONSE_BYTES: processEnv["CKB_RPC_MAX_RESPONSE_BYTES"] }
      : {}),
    ...(processEnv["CKB_DEP_GROUP_MAX_MEMBERS"]
      ? { CKB_DEP_GROUP_MAX_MEMBERS: processEnv["CKB_DEP_GROUP_MAX_MEMBERS"] }
      : {}),
  };
}
