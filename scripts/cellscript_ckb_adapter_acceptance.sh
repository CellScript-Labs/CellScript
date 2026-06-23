#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

default_ckb_repo() {
  local parent grandparent
  parent="$(cd "$REPO_ROOT/.." && pwd)"
  grandparent="$(cd "$REPO_ROOT/../.." && pwd)"
  if [[ -d "$parent/ckb" ]]; then
    printf '%s\n' "$parent/ckb"
  else
    printf '%s\n' "$grandparent/ckb"
  fi
}

CKB_REPO="${CKB_REPO:-$(default_ckb_repo)}"
CKB_BIN="${CKB_BIN:-}"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$REPO_ROOT/target/ckb-cellscript-adapter-acceptance/$RUN_ID"
CKB_DIR="$RUN_DIR/ckb-node"
CKB_LOG="$RUN_DIR/ckb.log"
REPORT_JSON="$RUN_DIR/cellscript-ckb-adapter-acceptance-report.json"
ACTION_PLAN_JSON="$RUN_DIR/action-plan.json"
CKB_PID=""

usage() {
  cat <<'USAGE'
Usage: scripts/cellscript_ckb_adapter_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>]

Runs a focused local CKB node acceptance check for the CellScript CKB adapter
boundary. This is not a business-flow semantic gate; it proves the adapter path
can produce CKB-facing evidence around action plans, packed transaction shape,
estimate_cycles, and test_tx_pool_accept.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ckb-repo)
      CKB_REPO="${2:?missing value for --ckb-repo}"
      shift 2
      ;;
    --ckb-repo=*)
      CKB_REPO="${1#*=}"
      shift
      ;;
    --ckb-bin)
      CKB_BIN="${2:?missing value for --ckb-bin}"
      shift 2
      ;;
    --ckb-bin=*)
      CKB_BIN="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

pick_port() {
  python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

resolve_ckb_bin() {
  if [[ -n "$CKB_BIN" ]]; then
    if [[ ! -x "$CKB_BIN" ]]; then
      echo "CKB_BIN is not executable: $CKB_BIN" >&2
      exit 1
    fi
    printf '%s\n' "$CKB_BIN"
    return
  fi

  local candidate
  for candidate in "$CKB_REPO/target/debug/ckb" "$CKB_REPO/target/release/ckb"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  echo "No existing CKB executable found; building parent CKB checkout with cargo build --bin ckb" >&2
  (cd "$CKB_REPO" && cargo build --bin ckb)
  candidate="$CKB_REPO/target/debug/ckb"
  if [[ ! -x "$candidate" ]]; then
    echo "CKB build finished but executable was not found at $candidate" >&2
    exit 1
  fi
  printf '%s\n' "$candidate"
}

stop_ckb() {
  if [[ -n "$CKB_PID" ]] && kill -0 "$CKB_PID" >/dev/null 2>&1; then
    kill "$CKB_PID" >/dev/null 2>&1 || true
    wait "$CKB_PID" >/dev/null 2>&1 || true
  fi
  CKB_PID=""
}
trap stop_ckb EXIT

require_cmd cargo
require_cmd curl
require_cmd python3

if [[ ! -d "$CKB_REPO" ]]; then
  echo "CKB repo does not exist: $CKB_REPO" >&2
  exit 1
fi
if [[ ! -f "$CKB_REPO/test/template/ckb.toml" ]]; then
  echo "CKB repo does not contain test/template/ckb.toml: $CKB_REPO" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"

CKB_BIN="$(resolve_ckb_bin)"
CKB_REPO="$(cd "$CKB_REPO" && pwd)"
CKB_BIN="$(cd "$(dirname "$CKB_BIN")" && pwd)/$(basename "$CKB_BIN")"
RPC_PORT="$(pick_port)"
P2P_PORT="$(pick_port)"
RPC_URL="http://127.0.0.1:$RPC_PORT"

mkdir -p "$CKB_DIR"
cp -R "$CKB_REPO/test/template/." "$CKB_DIR/"

python3 - "$CKB_DIR/ckb.toml" "$RPC_PORT" "$P2P_PORT" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
rpc_port = sys.argv[2]
p2p_port = sys.argv[3]
text = path.read_text(encoding="utf-8")
text = re.sub(
    r'listen_address = "127\.0\.0\.1:\d+"',
    f'listen_address = "127.0.0.1:{rpc_port}"',
    text,
    count=1,
)
text = re.sub(
    r'listen_addresses = \["/ip4/0\.0\.0\.0/tcp/\d+"\]',
    f'listen_addresses = ["/ip4/127.0.0.1/tcp/{p2p_port}"]',
    text,
    count=1,
)
path.write_text(text, encoding="utf-8")
PY

cargo run --locked -p cellscript --bin cellc -- action build examples/token.cell --action mint --json >"$ACTION_PLAN_JSON"
cargo test --locked -p cellscript-ckb-adapter materializes_resolved_action_with_ckb_sdk_transaction_builder -- --test-threads=1
cargo test --locked -p cellscript-ckb-adapter builds_deploy_transaction_with_type_id_code_cell -- --test-threads=1

"$CKB_BIN" -C "$CKB_DIR" run --ba-advanced >"$CKB_LOG" 2>&1 &
CKB_PID="$!"

for _ in $(seq 1 120); do
  if curl -sS \
    -H 'content-type: application/json' \
    -d '{"id":1,"jsonrpc":"2.0","method":"get_tip_header","params":[]}' \
    "$RPC_URL" >"$RUN_DIR/rpc-ready.json" 2>/dev/null; then
    break
  fi
  sleep 0.25
done

if ! grep -q '"result"' "$RUN_DIR/rpc-ready.json" 2>/dev/null; then
  echo "CKB RPC did not become ready at $RPC_URL. Log: $CKB_LOG" >&2
  exit 1
fi

python3 - "$RPC_URL" "$ACTION_PLAN_JSON" "$REPORT_JSON" "$CKB_REPO" "$CKB_BIN" "$CKB_LOG" <<'PY'
import hashlib
import json
import pathlib
import sys
import time
import urllib.error
import urllib.request

rpc_url, action_plan_path, report_path, ckb_repo, ckb_bin, ckb_log = sys.argv[1:]
action_plan_path = pathlib.Path(action_plan_path)
report_path = pathlib.Path(report_path)

ALWAYS_SUCCESS_CODE_HASH = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5"
ALWAYS_SUCCESS_INDEX = 5
FEE = 1_000

def rpc(method, params=None):
    body = json.dumps({"id": 42, "jsonrpc": "2.0", "method": method, "params": params or []}).encode("utf-8")
    request = urllib.request.Request(rpc_url, data=body, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError as error:
        raise RuntimeError(f"RPC {method} failed to connect: {error}") from error
    if payload.get("error"):
        raise RuntimeError(f"RPC {method} returned error: {payload['error']}")
    return payload.get("result")

def hex_u64(value):
    return hex(value if isinstance(value, int) else int(value, 16))

def out_point(tx_hash, index):
    return {"tx_hash": tx_hash, "index": hex_u64(index)}

def wait_live_cell(tx_hash, index, attempts=20, delay_seconds=0.05):
    last_result = None
    for _ in range(attempts):
        last_result = rpc("get_live_cell", [out_point(tx_hash, index), True])
        if last_result and last_result.get("status") == "live":
            return last_result
        time.sleep(delay_seconds)
    return last_result

def always_success_lock(args="0x"):
    return {"code_hash": ALWAYS_SUCCESS_CODE_HASH, "hash_type": "data", "args": args}

def get_block_by_number(number):
    block = rpc("get_block_by_number", [hex_u64(number)])
    if block is None:
        raise RuntimeError(f"block number not found: {number}")
    return block

def find_spendable_cellbase(max_blocks=64):
    for _ in range(max_blocks):
        block_hash = rpc("generate_block")
        block = rpc("get_block", [block_hash])
        cellbase = block["transactions"][0]
        for index, output in enumerate(cellbase.get("outputs", [])):
            capacity = int(output["capacity"], 16)
            if capacity <= FEE:
                continue
            live = wait_live_cell(cellbase["hash"], index)
            if live and live.get("status") == "live":
                return {
                    "block_hash": block_hash,
                    "tx_hash": cellbase["hash"],
                    "index": index,
                    "capacity": capacity,
                }
    raise RuntimeError(f"no spendable cellbase output found after {max_blocks} generated blocks")

def transaction(input_cell, output, outputs_data, cell_deps, witnesses=None, header_deps=None):
    return {
        "version": "0x0",
        "cell_deps": cell_deps,
        "header_deps": header_deps or [],
        "inputs": [{
            "previous_output": out_point(input_cell["tx_hash"], input_cell["index"]),
            "since": "0x0",
        }],
        "outputs": [output],
        "outputs_data": outputs_data,
        "witnesses": witnesses or [],
    }

def json_serialized_size_bytes(value):
    return len(json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8"))

def ckb_blake2b(data):
    return "0x" + hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()

action_plan = json.loads(action_plan_path.read_text(encoding="utf-8"))
genesis = get_block_by_number(0)
genesis_cellbase_hash = genesis["transactions"][0]["hash"]
always_success_dep = {
    "out_point": out_point(genesis_cellbase_hash, ALWAYS_SUCCESS_INDEX),
    "dep_type": "code",
}

# ---- Phase 1: Action transaction smoke test ----
funding = find_spendable_cellbase()
output = {
    "capacity": hex_u64(funding["capacity"] - FEE),
    "lock": always_success_lock(),
    "type": None,
}
tx = transaction(funding, output, ["0x"], [always_success_dep])
estimate = rpc("estimate_cycles", [tx])
tx_pool_accept = rpc("test_tx_pool_accept", [tx, "passthrough"])

# ---- Phase 2: Deploy probe with TYPE_ID code cell ----
# Build a deploy transaction that places a pseudo-artifact as a code cell
# with a TYPE_ID type script, exactly as build_deploy_transaction() does.
#
# TYPE_ID args = blake2b(first_input_tx_hash || first_input_index_u64_le || output_index_u64_le)
# where first_input_index is the CellInput.previous_output.index.
#
# The code cell uses hash_type="type" so code_hash = type_script_hash.

deploy_funding = find_spendable_cellbase()

# Pseudo-artifact: 32 bytes of test data.
artifact_data = bytes(range(32))
artifact_data_hex = "0x" + artifact_data.hex()
artifact_data_hash = ckb_blake2b(artifact_data)

# TYPE_ID args = blake2b(first_input_tx_hash || output_index_le)
first_input_tx_hash_bytes = bytes.fromhex(deploy_funding["tx_hash"][2:])
type_id_args_input = first_input_tx_hash_bytes + (0).to_bytes(8, "little") + (0).to_bytes(8, "little")
type_id_args = "0x" + hashlib.blake2b(type_id_args_input, digest_size=32, person=b"ckb-default-hash").hexdigest()

# TYPE_ID type script: For devnet testing we use always_success with hash_type="data"
# since the always_success binary is deployed in genesis with data hash.
# Production TYPE_ID uses hash_type="type" with the TYPE_ID script code_hash.
type_script = {
    "code_hash": ALWAYS_SUCCESS_CODE_HASH,
    "hash_type": "data",
    "args": type_id_args,
}

# Code output: lock = always_success, type = TYPE_ID type script.
# Use a generous capacity (200 CKB = 200_000_000_000 shannons) for the code cell
# to ensure it exceeds the occupied floor regardless of exact molecule overhead.
# The adapter crate's build_deploy_transaction() computes exact occupied capacity;
# here we just need the transaction to pass devnet validation.
code_output_capacity = 200_000_000_000
change_capacity = deploy_funding["capacity"] - code_output_capacity - FEE
if change_capacity < 0:
    raise RuntimeError(f"deploy funding {deploy_funding['capacity']} insufficient for code output {code_output_capacity} + fee {FEE}")

code_output = {
    "capacity": hex_u64(code_output_capacity),
    "lock": always_success_lock(),
    "type": type_script,
}
change_output = {
    "capacity": hex_u64(change_capacity),
    "lock": always_success_lock(),
    "type": None,
}

deploy_tx = {
    "version": "0x0",
    "cell_deps": [always_success_dep],
    "header_deps": [],
    "inputs": [{
        "previous_output": out_point(deploy_funding["tx_hash"], deploy_funding["index"]),
        "since": "0x0",
    }],
    "outputs": [code_output, change_output],
    "outputs_data": [artifact_data_hex, "0x"],
    "witnesses": ["0x0000000000000000"],  # placeholder witness for always_success
}

deploy_estimate = rpc("estimate_cycles", [deploy_tx])
deploy_tx_pool_accept = rpc("test_tx_pool_accept", [deploy_tx, "passthrough"])

# ---- Phase 3: Submit deploy transaction and verify commitment ----
deploy_tx_hash = rpc("send_transaction", [deploy_tx, "passthrough"])
# Generate a block to commit the transaction.
rpc("generate_block")
# Wait for the transaction to be committed: keep generating blocks until the code cell is live.
commit_evidence_status = "unknown"
commit_block_hash = "0x"
for _ in range(10):
    time.sleep(0.5)
    rpc("generate_block")
    commit_live_check = wait_live_cell(deploy_tx_hash, 0, attempts=3)
    if commit_live_check and commit_live_check.get("status") == "live":
        commit_evidence_status = "committed"
        break
if commit_evidence_status != "committed":
    raise RuntimeError(f"deploy transaction {deploy_tx_hash} not committed after 10 generated blocks")
commit_live = commit_live_check
commit_live_output = commit_live["cell"]["output"] if commit_live.get("cell") else {}

report = {
    "schema": "cellscript-ckb-adapter-local-node-acceptance-v0.19",
    "status": "passed",
    "rpc_url": rpc_url,
    "ckb_repo": ckb_repo,
    "ckb_bin": ckb_bin,
    "ckb_log": ckb_log,
    "action_plan": {
        "policy": action_plan.get("policy"),
        "action": action_plan.get("action"),
        "adapter_contract_schema": (action_plan.get("adapter_contract") or {}).get("schema"),
        "can_submit": (action_plan.get("transaction_draft") or {}).get("can_submit"),
        "requires_packed_materialization": (action_plan.get("transaction_draft") or {}).get("requires_packed_materialization"),
    },
    "adapter_materialization": {
        "crate": "crates/cellscript-ckb-adapter",
        "test": "materializes_resolved_action_with_ckb_sdk_transaction_builder",
        "status": "passed",
    },
    "adapter_deploy_probe": {
        "crate": "crates/cellscript-ckb-adapter",
        "test": "builds_deploy_transaction_with_type_id_code_cell",
        "status": "passed",
    },
    "local_node": {
        "estimate_cycles": estimate,
        "test_tx_pool_accept": tx_pool_accept,
        "tx_size_json_bytes": json_serialized_size_bytes(tx),
        "output_capacity_shannons": funding["capacity"] - FEE,
        "fee_shannons": FEE,
        "cell_deps": tx["cell_deps"],
        "header_deps": tx["header_deps"],
        "witnesses": tx["witnesses"],
        "outputs_data_count": len(tx["outputs_data"]),
        "outputs_count": len(tx["outputs"]),
        "lineage": [{
            "from": out_point(funding["tx_hash"], funding["index"]),
            "to_output_index": 0,
            "relation": "adapter-local-node-smoke",
        }],
        "tx_shape_hash": ckb_blake2b(json.dumps(tx, sort_keys=True, separators=(",", ":")).encode("utf-8")),
    },
    "deploy_probe": {
        "status": "passed",
        "type_id_args": type_id_args,
        "artifact_data_hash": artifact_data_hash,
        "code_output_capacity_shannons": code_output_capacity,
        "change_output_capacity_shannons": change_capacity,
        "fee_shannons": FEE,
        "estimate_cycles": deploy_estimate,
        "test_tx_pool_accept": deploy_tx_pool_accept,
        "tx_size_json_bytes": json_serialized_size_bytes(deploy_tx),
        "outputs_count": len(deploy_tx["outputs"]),
        "outputs_data_count": len(deploy_tx["outputs_data"]),
        "cell_deps_count": len(deploy_tx["cell_deps"]),
    },
    "commit_evidence": {
        "status": commit_evidence_status,
        "deploy_tx_hash": deploy_tx_hash,
        "commit_block_hash": commit_block_hash,
        "code_cell_live": True,
        "code_cell_has_type_script": commit_live_output.get("type") is not None,
    },
    "known_limitations": [
        "This focused adapter acceptance proves CKB SDK/RPC materialization boundary evidence, not full CellScript business-flow semantics.",
        "Stateful business-flow semantics remain covered by ckb_cellscript_acceptance.sh and release gates.",
        "No wallet UI, CellFabric intent DAG, external audit, or mainnet-value certification is claimed.",
        "The deploy probe uses always_success with hash_type=data as the type script for devnet acceptance; production TYPE_ID uses hash_type=type with the actual TYPE_ID script code_hash.",
    ],
}
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(report_path)
PY

echo "CellScript CKB adapter acceptance report: $REPORT_JSON"
