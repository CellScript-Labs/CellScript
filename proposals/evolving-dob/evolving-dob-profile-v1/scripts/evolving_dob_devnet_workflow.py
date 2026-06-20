#!/usr/bin/env python3
"""Strict DOB-EVO/1 local devnet workflow gate.

This script starts a local CKB integration node, deploys the compiled DOB
artifact as a live code cell, records the resulting deployment facts, and then
exercises the package, registry, action-plan, and generated-builder workflows
against those facts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import socket
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
REPO_ROOT = ROOT.parents[2]  # proposals/evolving-dob/evolving-dob-profile-v1 -> CellScript repo root
CKB_BLAKE2B_PERSONAL = b"ckb-default-hash"
ALWAYS_SUCCESS_CODE_HASH = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5"
ALWAYS_SUCCESS_INDEX = 5
SHANNONS = 100_000_000
FEE = 1_000
ACTIONS = ["initialise_dob_state", "evolve_dob_state", "finalise_dob_state"]


class WorkflowError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    default_ckb_repo = REPO_ROOT.parent / "ckb"
    default_ckb_bin = os.environ.get("CKB_BIN")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ckb-repo", type=pathlib.Path, default=pathlib.Path(os.environ.get("CKB_REPO", default_ckb_repo)))
    parser.add_argument("--ckb-bin", type=pathlib.Path, default=pathlib.Path(default_ckb_bin) if default_ckb_bin else None)
    parser.add_argument("--network", default=os.environ.get("DOB_EVO_DEVNET_NETWORK", "devnet"))
    parser.add_argument("--run-dir", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=ROOT / "target/devnet-workflow/report.json")
    parser.add_argument("--keep-node", action="store_true")
    parser.add_argument("--pretty", action="store_true")
    return parser.parse_args()


def cellc() -> list[str]:
    configured = os.environ.get("CELLC")
    if configured:
        return [configured]
    binary = REPO_ROOT / "target" / "debug" / "cellc"
    if binary.exists():
        return [str(binary)]
    return ["cargo", "run", "--locked", "-p", "cellscript", "--manifest-path", str(REPO_ROOT / "Cargo.toml"), "--bin", "cellc", "--"]


def run_checked(args: list[str], *, cwd: pathlib.Path = ROOT) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if proc.returncode != 0:
        raise WorkflowError(
            "command failed: {}\nSTDOUT:\n{}\nSTDERR:\n{}".format(" ".join(args), proc.stdout[-5000:], proc.stderr[-5000:])
        )
    return proc


def require(condition: bool, message: str) -> None:
    if not condition:
        raise WorkflowError(message)


def hex_u64(value: int) -> str:
    return hex(value)


def out_point(tx_hash: str, index: int) -> dict[str, str]:
    return {"tx_hash": tx_hash, "index": hex_u64(index)}


def ckb_hash_hex(data: bytes) -> str:
    return "0x" + hashlib.blake2b(data, digest_size=32, person=CKB_BLAKE2B_PERSONAL).hexdigest()


def sha256_hex(data: bytes) -> str:
    return "0x" + hashlib.sha256(data).hexdigest()


def read_toml(path: pathlib.Path) -> dict[str, Any]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def pick_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def resolve_ckb_bin(ckb_repo: pathlib.Path, ckb_bin: pathlib.Path | None) -> pathlib.Path:
    if ckb_bin is not None:
        resolved = ckb_bin.expanduser().resolve()
        require(resolved.exists() and os.access(resolved, os.X_OK), f"CKB_BIN is not executable: {resolved}")
        return resolved
    for candidate in [ckb_repo / "target/debug/ckb", ckb_repo / "target/release/ckb"]:
        if candidate.exists() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    raise WorkflowError(f"no CKB executable found under {ckb_repo}; set CKB_BIN or build ckb first")


def prepare_ckb_node(ckb_repo: pathlib.Path, ckb_dir: pathlib.Path, rpc_port: int, p2p_port: int) -> None:
    template = ckb_repo / "test/template"
    require((template / "ckb.toml").exists(), f"CKB test template missing: {template}")
    shutil.copytree(template, ckb_dir)
    config = ckb_dir / "ckb.toml"
    text = config.read_text(encoding="utf-8")
    text = re.sub(r'listen_address = "127\.0\.0\.1:\d+"', f'listen_address = "127.0.0.1:{rpc_port}"', text, count=1)
    text = re.sub(
        r'listen_addresses = \["/ip4/0\.0\.0\.0/tcp/\d+"\]',
        f'listen_addresses = ["/ip4/127.0.0.1/tcp/{p2p_port}"]',
        text,
        count=1,
    )
    config.write_text(text, encoding="utf-8")


def rpc(rpc_url: str, method: str, params: list[Any] | None = None, timeout: int = 20) -> Any:
    body = json.dumps({"id": 42, "jsonrpc": "2.0", "method": method, "params": params or []}).encode("utf-8")
    request = urllib.request.Request(rpc_url, data=body, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError as error:
        raise WorkflowError(f"RPC {method} failed to connect: {error}") from error
    if payload.get("error"):
        raise WorkflowError(f"RPC {method} returned error: {payload['error']}")
    return payload.get("result")


def wait_rpc_ready(rpc_url: str, attempts: int = 160) -> None:
    last_error = ""
    for _ in range(attempts):
        try:
            result = rpc(rpc_url, "get_tip_header", [], timeout=2)
            if result is not None:
                return
        except Exception as error:  # noqa: BLE001 - keep startup diagnostics compact.
            last_error = str(error)
        time.sleep(0.25)
    raise WorkflowError(f"CKB RPC did not become ready at {rpc_url}: {last_error}")


def wait_live_cell(rpc_url: str, tx_hash: str, index: int, attempts: int = 30, delay: float = 0.1) -> dict[str, Any] | None:
    last: dict[str, Any] | None = None
    for _ in range(attempts):
        result = rpc(rpc_url, "get_live_cell", [out_point(tx_hash, index), True])
        last = result
        if result and result.get("status") == "live":
            return result
        time.sleep(delay)
    return last


def get_block_by_number(rpc_url: str, number: int) -> dict[str, Any]:
    block = rpc(rpc_url, "get_block_by_number", [hex_u64(number)])
    require(block is not None, f"block number not found: {number}")
    return block


def always_success_lock(args: str = "0x") -> dict[str, str]:
    return {"code_hash": ALWAYS_SUCCESS_CODE_HASH, "hash_type": "data", "args": args}


def collect_funding_cells(rpc_url: str, required_capacity: int, max_blocks: int = 512) -> tuple[list[dict[str, Any]], int]:
    cells: list[dict[str, Any]] = []
    total = 0
    seen: set[tuple[str, int]] = set()
    for _ in range(max_blocks):
        block_hash = rpc(rpc_url, "generate_block")
        block = rpc(rpc_url, "get_block", [block_hash])
        cellbase = block["transactions"][0]
        for index, output in enumerate(cellbase.get("outputs", [])):
            key = (cellbase["hash"], index)
            if key in seen:
                continue
            seen.add(key)
            live = wait_live_cell(rpc_url, cellbase["hash"], index, attempts=6, delay=0.05)
            if not live or live.get("status") != "live":
                continue
            capacity = int(output["capacity"], 16)
            cells.append({"tx_hash": cellbase["hash"], "index": index, "capacity": capacity})
            total += capacity
            if total >= required_capacity:
                return cells, total
    raise WorkflowError(f"insufficient generated devnet capacity: need {required_capacity}, collected {total}")


def transaction(inputs: list[dict[str, Any]], output: dict[str, Any], output_data: str, cell_deps: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "version": "0x0",
        "cell_deps": cell_deps,
        "header_deps": [],
        "inputs": [
            {
                "previous_output": out_point(cell["tx_hash"], int(cell["index"])),
                "since": "0x0",
            }
            for cell in inputs
        ],
        "outputs": [output],
        "outputs_data": [output_data],
        "witnesses": ["0x0000000000000000"],
    }


def write_deployed_toml(
    *,
    lockfile: dict[str, Any],
    chain_id: str,
    network: str,
    tx_hash: str,
    data_hash: str,
    audit_report_hash: str,
) -> None:
    package = lockfile["package"]
    build = lockfile["package_build"]
    out_point_text = f"{tx_hash}:0"
    text = f'''version = 1
schema = "cellscript-deployed-v0.19"

[package]
name = "{package["name"]}"
version = "{package["version"]}"
source_hash = "{package["source_hash"]}"

[build]
compiler_version = "{build["compiler_version"]}"
artifact_hash = "{build["artifact_hash"]}"
metadata_hash = "{build["metadata_hash"]}"
schema_hash = "{build["schema_hash"]}"
abi_hash = "{build["abi_hash"]}"
constraints_hash = "{build["constraints_hash"]}"

[[deployments]]
name = "evolving-dob-profile-v1-local-devnet"
status = "active"
network = "{network}"
chain_id = "{chain_id}"
tx_hash = "{tx_hash}"
output_index = 0
code_hash = "{data_hash}"
data_hash = "{data_hash}"
hash_type = "data1"
dep_type = "code"
out_point = "{out_point_text}"
artifact_hash = "{build["artifact_hash"]}"
metadata_hash = "{build["metadata_hash"]}"
schema_hash = "{build["schema_hash"]}"
abi_hash = "{build["abi_hash"]}"
constraints_hash = "{build["constraints_hash"]}"
compiler_version = "{build["compiler_version"]}"
audit_report_hash = "{audit_report_hash}"
publisher_signature = "local-devnet-workflow:{tx_hash}"
'''
    (ROOT / "Deployed.toml").write_text(text, encoding="utf-8")


def action_plan_summary(path: pathlib.Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    draft = data.get("transaction_draft") or {}
    ckb = data.get("ckb") or {}
    return {
        "path": str(path),
        "status": data.get("status"),
        "action": data.get("action"),
        "policy": data.get("policy"),
        "artifact_hash": data.get("artifact_hash"),
        "requires_live_cell_resolution": draft.get("requires_live_cell_resolution"),
        "requires_packed_materialization": draft.get("requires_packed_materialization"),
        "dry_run_required_for_production": ckb.get("dry_run_required_for_production"),
        "required_evidence": draft.get("required_evidence"),
    }


def main() -> int:
    args = parse_args()
    run_id = time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"
    run_dir = args.run_dir or (ROOT / "target/devnet-workflow" / run_id)
    run_dir = run_dir.resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    report_path = args.output if args.output.is_absolute() else ROOT / args.output
    report_path.parent.mkdir(parents=True, exist_ok=True)

    report: dict[str, Any] = {
        "schema": "dob-evo-local-devnet-workflow-v1",
        "status": "started",
        "network": args.network,
        "run_dir": str(run_dir),
    }
    ckb_pid: subprocess.Popen[str] | None = None

    try:
        strict_build = cellc() + ["build", "--release", "--target", "riscv64-elf", "--target-profile", "ckb", "--primitive-strict", "0.16"]
        run_checked(strict_build)
        check = run_checked(cellc() + ["check", "--target-profile", "ckb", "--primitive-strict", "0.16", "--json"])
        check_json = json.loads(check.stdout)
        require(check_json.get("status") == "ok", "strict production check did not return ok")
        run_checked(cellc() + ["package", "verify", "--json"])
        run_checked(cellc() + ["publish", "--dry-run"])

        lockfile = read_toml(ROOT / "Cell.lock")
        artifact = ROOT / "build/evolving_dob_type.elf"
        metadata = ROOT / "build/evolving_dob_type.elf.meta.json"
        require(artifact.exists(), f"compiled artifact missing: {artifact}")
        require(metadata.exists(), f"compile metadata missing: {metadata}")
        artifact_bytes = artifact.read_bytes()
        artifact_data_hash = ckb_hash_hex(artifact_bytes)
        require(
            artifact_data_hash.removeprefix("0x") == lockfile["package_build"]["artifact_hash"],
            "artifact data hash does not match Cell.lock artifact_hash",
        )

        action_summaries = []
        recommended_code_capacity = 0
        for action in ACTIONS:
            plan_path = run_dir / f"action-{action}.json"
            run_checked(cellc() + ["action", "build", ".", "--action", action, "--target-profile", "ckb", "--json", "--output", str(plan_path)])
            summary = action_plan_summary(plan_path)
            action_summaries.append(summary)
            plan = json.loads(plan_path.read_text(encoding="utf-8"))
            capacity = ((plan.get("ckb") or {}).get("capacity_evidence_contract") or {}).get("recommended_code_cell_capacity_shannons")
            if isinstance(capacity, int):
                recommended_code_capacity = max(recommended_code_capacity, capacity)

        ckb_repo = args.ckb_repo.expanduser().resolve()
        ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
        ckb_dir = run_dir / "ckb-node"
        rpc_port = pick_port()
        p2p_port = pick_port()
        rpc_url = f"http://127.0.0.1:{rpc_port}"
        prepare_ckb_node(ckb_repo, ckb_dir, rpc_port, p2p_port)
        log_path = run_dir / "ckb.log"
        with log_path.open("w", encoding="utf-8") as log:
            ckb_pid = subprocess.Popen([str(ckb_bin), "-C", str(ckb_dir), "run", "--ba-advanced"], stdout=log, stderr=log, text=True)
        wait_rpc_ready(rpc_url)

        chain_info = rpc(rpc_url, "get_blockchain_info")
        chain_id = str(chain_info.get("chain") or chain_info.get("chain_id") or "ckb-dev")
        genesis = get_block_by_number(rpc_url, 0)
        genesis_cellbase_hash = genesis["transactions"][0]["hash"]
        always_success_dep = {"out_point": out_point(genesis_cellbase_hash, ALWAYS_SUCCESS_INDEX), "dep_type": "code"}

        min_capacity = max(recommended_code_capacity, (artifact.stat().st_size + 1024) * SHANNONS)
        required_capacity = min_capacity + FEE
        funding_cells, total_capacity = collect_funding_cells(rpc_url, required_capacity)
        output_capacity = total_capacity - FEE
        code_output = {"capacity": hex_u64(output_capacity), "lock": always_success_lock(), "type": None}
        deploy_tx = transaction(funding_cells, code_output, "0x" + artifact_bytes.hex(), [always_success_dep])
        estimate_cycles = rpc(rpc_url, "estimate_cycles", [deploy_tx])
        tx_pool_accept = rpc(rpc_url, "test_tx_pool_accept", [deploy_tx, "passthrough"])
        deploy_tx_hash = rpc(rpc_url, "send_transaction", [deploy_tx, "passthrough"])

        commit_status = "unknown"
        live = None
        for _ in range(20):
            rpc(rpc_url, "generate_block")
            live = wait_live_cell(rpc_url, deploy_tx_hash, 0, attempts=5, delay=0.1)
            if live and live.get("status") == "live":
                commit_status = "committed"
                break
            time.sleep(0.2)
        require(commit_status == "committed", f"deploy transaction {deploy_tx_hash} did not produce a live code cell")

        live_data_hash = (((live or {}).get("cell") or {}).get("data") or {}).get("hash")
        require(live_data_hash == artifact_data_hash, "live code cell data hash does not match compiled artifact")
        audit_hash = sha256_hex(
            json.dumps(
                {
                    "schema": "dob-evo-local-devnet-audit-presence-v1",
                    "source_hash": lockfile["package"]["source_hash"],
                    "artifact_hash": lockfile["package_build"]["artifact_hash"],
                    "tx_hash": deploy_tx_hash,
                    "network": args.network,
                },
                sort_keys=True,
            ).encode("utf-8")
        )
        write_deployed_toml(
            lockfile=lockfile,
            chain_id=chain_id,
            network=args.network,
            tx_hash=deploy_tx_hash,
            data_hash=artifact_data_hash,
            audit_report_hash=audit_hash,
        )

        deployed_copy = run_dir / "Deployed.toml"
        shutil.copy2(ROOT / "Deployed.toml", deployed_copy)
        run_checked(strict_build)

        offline_verify = run_checked(
            cellc() + ["registry", "verify", "--json", "--require-publisher-signature", "--require-audit-report"]
        )
        live_verify = run_checked(
            cellc()
            + [
                "registry",
                "verify",
                "--json",
                "--live",
                "--rpc-url",
                rpc_url,
                "--network",
                args.network,
                "--require-publisher-signature",
                "--require-audit-report",
            ]
        )

        builder_dir = run_dir / "generated-builder"
        builder_summary = run_checked(
            cellc()
            + [
                "gen-builder",
                ".",
                "--target",
                "typescript",
                "--metadata",
                str(metadata),
                "--lockfile",
                str(ROOT / "Cell.lock"),
                "--deployed",
                str(ROOT / "Deployed.toml"),
                "--deployment-network",
                args.network,
                "--output",
                str(builder_dir),
                "--package-name",
                "@dob/evolving-dob-profile-v1-builder",
                "--json",
            ]
        )
        run_checked(["npm", "--prefix", str(builder_dir), "install", "--ignore-scripts"], cwd=ROOT)
        run_checked(["npm", "--prefix", str(builder_dir), "test"], cwd=ROOT)

        report.update(
            {
                "status": "passed",
                "package": {
                    "name": lockfile["package"]["name"],
                    "version": lockfile["package"]["version"],
                    "namespace": lockfile["package"].get("namespace"),
                    "source_hash": lockfile["package"].get("source_hash"),
                },
                "artifact": {
                    "path": str(artifact),
                    "size_bytes": artifact.stat().st_size,
                    "data_hash": artifact_data_hash,
                    "cell_lock_artifact_hash": lockfile["package_build"]["artifact_hash"],
                },
                "local_devnet": {
                    "rpc_url": rpc_url,
                    "chain_id": chain_id,
                    "ckb_bin": str(ckb_bin),
                    "ckb_log": str(log_path),
                    "funding_input_count": len(funding_cells),
                    "funding_capacity_shannons": total_capacity,
                    "code_output_capacity_shannons": output_capacity,
                    "fee_shannons": FEE,
                    "estimate_cycles": estimate_cycles,
                    "test_tx_pool_accept": tx_pool_accept,
                    "deploy_tx_hash": deploy_tx_hash,
                    "deploy_out_point": f"{deploy_tx_hash}:0",
                    "live_data_hash": live_data_hash,
                    "commit_status": commit_status,
                },
                "deployed_toml": str(deployed_copy),
                "registry": {
                    "offline": json.loads(offline_verify.stdout),
                    "live": json.loads(live_verify.stdout),
                },
                "action_plans": action_summaries,
                "generated_builder": {
                    "path": str(builder_dir),
                    "summary": json.loads(builder_summary.stdout),
                    "npm_install": "passed",
                    "npm_test": "passed",
                },
            }
        )
    except Exception as error:  # noqa: BLE001 - report and fail closed.
        report.update({"status": "failed", "error": str(error)})
    finally:
        if ckb_pid is not None and ckb_pid.poll() is None and not args.keep_node:
            ckb_pid.terminate()
            try:
                ckb_pid.wait(timeout=10)
            except subprocess.TimeoutExpired:
                ckb_pid.kill()
                ckb_pid.wait(timeout=10)
        if ckb_pid is not None:
            report.setdefault("local_devnet", {})["node_exit_status"] = ckb_pid.poll()

    report_path.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {report_path} status={report['status']}")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
