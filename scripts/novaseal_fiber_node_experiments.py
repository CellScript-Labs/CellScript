#!/usr/bin/env python3
"""Build NovaSeal evidence from the cloned Fiber Network Node repository.

The report is deliberately stricter than a source inventory. It records the
exact Fiber clone, checks that the expected devnet/e2e workflow suites exist,
maps each suite back to NovaSeal profiles, and optionally runs selected Bruno
e2e suites against Fiber's own devnet runner.

Without --run-suite or --run-all the report is a discovery contract, not live
execution evidence.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import signal
import subprocess
import time
from dataclasses import dataclass
from typing import Any


SCHEMA = "novaseal-fiber-node-execution-v0.1"


@dataclass(frozen=True)
class FiberWorkflow:
    suite: str
    category: str
    description: str
    mapped_profiles: tuple[str, ...]
    expected_terms: tuple[str, ...]
    requires_lnd: bool = False


REQUIRED_WORKFLOWS: tuple[FiberWorkflow, ...] = (
    FiberWorkflow(
        suite="open-use-close-a-channel",
        category="channel-lifecycle",
        description="single-channel open, TLC add/remove, cooperative shutdown, and closed-state checks",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("open-channel", "add-tlc", "remove-tlc", "shutdown", "list-channel"),
    ),
    FiberWorkflow(
        suite="3-nodes-transfer",
        category="multi-hop-transfer",
        description="three-node channel graph with routed TLC transfer and shutdown",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("connect", "open-channel", "add-tlc", "remove-tlc", "shutdown"),
    ),
    FiberWorkflow(
        suite="router-pay",
        category="multi-hop-payment",
        description="router payment workflow with invoice, keysend, graph, duplicate, and failure paths",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("send-payment", "gen-invoice", "get-payment-status", "list-graph", "will-fail"),
    ),
    FiberWorkflow(
        suite="invoice-ops",
        category="invoice",
        description="invoice generation, duplicate rejection, decode, lookup, and cancellation",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("gen-invoice", "duplicate", "decode", "get-invoice", "cancel"),
    ),
    FiberWorkflow(
        suite="shutdown-force",
        category="force-close",
        description="force shutdown after peer disconnect and closed-channel assertions",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("shutdown-force", "disconnect", "closed-channel", "trigger-check"),
    ),
    FiberWorkflow(
        suite="reestablish",
        category="reconnect",
        description="channel reestablishment after disconnect before TLC removal and shutdown",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("disconnect", "reconnect", "remove-tlc", "shutdown"),
    ),
    FiberWorkflow(
        suite="external-funding-open",
        category="external-funding",
        description="external funding script, signing, submission, channel ready, shutdown, and balance checks",
        mapped_profiles=("fiber-candidate-profile-v0", "btc-transaction-commitment-profile-v0"),
        expected_terms=("funding-script", "external-funding", "sign", "submit", "balance-after"),
    ),
    FiberWorkflow(
        suite="funding-tx-verification",
        category="funding-verification",
        description="funding transaction verification with a shell builder and auto-accepted channel check",
        mapped_profiles=("fiber-candidate-profile-v0", "btc-transaction-commitment-profile-v0"),
        expected_terms=("funding-tx", "verification", "open-channel", "auto-accepted"),
    ),
    FiberWorkflow(
        suite="udt",
        category="udt-channel",
        description="UDT channel open, invoice/TLC flow, invalid open, manual accept, and shutdown",
        mapped_profiles=("fiber-candidate-profile-v0", "fungible-xudt-profile-v0"),
        expected_terms=("udt", "open-channel", "add-tlc", "remove-tlc", "invalid", "shutdown"),
    ),
    FiberWorkflow(
        suite="udt-router-pay",
        category="udt-routing",
        description="multi-hop routed UDT payment including invoice and keysend paths",
        mapped_profiles=("fiber-candidate-profile-v0", "fungible-xudt-profile-v0"),
        expected_terms=("udt", "router", "send-payment", "gen-invoice", "keysend"),
    ),
    FiberWorkflow(
        suite="watchtower/force-close-after-open-channel",
        category="watchtower",
        description="watchtower force-close settlement after opening a channel",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("force-close", "commitment-tx", "settlement", "check-balance"),
    ),
    FiberWorkflow(
        suite="watchtower/force-close-with-pending-tlcs",
        category="watchtower",
        description="force-close with pending TLCs, settlement transaction generation, and balance checks",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("pending-tlcs", "force-close", "settlement", "commitment-tx", "check-balance"),
    ),
    FiberWorkflow(
        suite="watchtower/force-close-with-pending-tlcs-and-udt",
        category="watchtower-udt",
        description="force-close with pending UDT TLCs and CKB/UDT balance checks",
        mapped_profiles=("fiber-candidate-profile-v0", "fungible-xudt-profile-v0"),
        expected_terms=("pending-tlcs", "udt", "force-close", "settlement", "check-balance"),
    ),
    FiberWorkflow(
        suite="watchtower/force-close-preimage-multiple",
        category="watchtower-preimage",
        description="multiple preimage settlement path after force-close",
        mapped_profiles=("fiber-candidate-profile-v0",),
        expected_terms=("preimage", "force-close", "settlement", "check-balance"),
    ),
    FiberWorkflow(
        suite="cross-chain-hub",
        category="cross-chain",
        description="Fiber plus Lightning/BTC hub send and receive order workflow",
        mapped_profiles=(
            "fiber-candidate-profile-v0",
            "btc-transaction-commitment-profile-v0",
            "btc-utxo-seal-profile-v0",
        ),
        expected_terms=("btc", "lnd", "send-payment", "order", "wrapped-btc", "shutdown"),
        requires_lnd=True,
    ),
)


def parse_args() -> argparse.Namespace:
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=pathlib.Path, default=repo_root)
    parser.add_argument("--fiber-repo", type=pathlib.Path, default=repo_root.parent / "fiber")
    parser.add_argument("--output", type=pathlib.Path, default=repo_root / "target/novaseal-fiber-node-experiments.json")
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--run-suite", action="append", choices=[workflow.suite for workflow in REQUIRED_WORKFLOWS])
    parser.add_argument("--run-all", action="store_true")
    parser.add_argument("--assume-nodes-running", action="store_true")
    parser.add_argument("--timeout-seconds", type=int, default=1800)
    return parser.parse_args()


def run_cmd(args: list[str], cwd: pathlib.Path, *, timeout: int | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, text=True, capture_output=True, timeout=timeout)


def git_value(fiber_repo: pathlib.Path, args: list[str]) -> str | None:
    completed = run_cmd(["git", *args], fiber_repo)
    if completed.returncode != 0:
        return None
    return completed.stdout.strip()


def rel(path: pathlib.Path, root: pathlib.Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def suite_dir(fiber_repo: pathlib.Path, suite: str) -> pathlib.Path:
    return fiber_repo / "tests" / "bruno" / "e2e" / suite


def suite_files(fiber_repo: pathlib.Path, suite: str) -> list[pathlib.Path]:
    directory = suite_dir(fiber_repo, suite)
    if not directory.is_dir():
        return []
    return sorted(directory.glob("*.bru"))


def terms_present(files: list[pathlib.Path], expected_terms: tuple[str, ...]) -> dict[str, bool]:
    names = " ".join(str(path).lower() for path in files)
    return {term: term.lower() in names for term in expected_terms}


def extract_rpc_methods(files: list[pathlib.Path]) -> list[str]:
    methods: set[str] = set()
    for path in files:
        try:
            for line in path.read_text(encoding="utf-8").splitlines():
                marker = '"method"'
                if marker not in line:
                    continue
                after = line.split(":", 1)[-1].strip().strip(",").strip()
                if after.startswith('"') and after.endswith('"'):
                    methods.add(after.strip('"'))
        except UnicodeDecodeError:
            continue
    return sorted(methods)


def workflow_report(fiber_repo: pathlib.Path, workflow: FiberWorkflow, execution: dict[str, Any] | None) -> dict[str, Any]:
    files = suite_files(fiber_repo, workflow.suite)
    terms = terms_present(files, workflow.expected_terms)
    present = bool(files) and all(terms.values())
    status = "present" if present else "missing"
    if execution is not None:
        status = execution["status"]
    return {
        "suite": workflow.suite,
        "category": workflow.category,
        "description": workflow.description,
        "mapped_profiles": list(workflow.mapped_profiles),
        "requires_lnd": workflow.requires_lnd,
        "status": status,
        "present": present,
        "step_count": len(files),
        "expected_terms": terms,
        "rpc_methods": extract_rpc_methods(files),
        "evidence_files": [rel(path, fiber_repo) for path in files],
        "execution": execution,
    }


def write_text(path: pathlib.Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value, encoding="utf-8")


def run_workflow(args: argparse.Namespace, workflow: FiberWorkflow) -> dict[str, Any]:
    fiber_repo = args.fiber_repo.resolve()
    suite_arg = f"e2e/{workflow.suite}"
    log_dir = args.output.resolve().parent / "novaseal-fiber-node-experiments" / workflow.suite.replace("/", "__")
    log_dir.mkdir(parents=True, exist_ok=True)
    started_node = False
    node_process: subprocess.Popen[str] | None = None
    started_at = time.time()
    try:
        if not args.assume_nodes_running:
            node_log = log_dir / "start-node.log"
            node_process = subprocess.Popen(
                ["./tests/nodes/start.sh", suite_arg],
                cwd=fiber_repo,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                preexec_fn=os.setsid if hasattr(os, "setsid") else None,
            )
            started_node = True
            wait = run_cmd(["./tests/nodes/wait.sh"], fiber_repo, timeout=args.timeout_seconds)
            write_text(log_dir / "wait.stdout", wait.stdout)
            write_text(log_dir / "wait.stderr", wait.stderr)
            if wait.returncode != 0:
                return {
                    "status": "failed",
                    "started_node": started_node,
                    "command": ["./tests/nodes/start.sh", suite_arg],
                    "failure": "fiber node wait failed",
                    "wait_returncode": wait.returncode,
                    "duration_seconds": round(time.time() - started_at, 3),
                }
        command = ["npm", "exec", "--", "@usebruno/cli", "run", suite_arg, "-r", "--env", "test"]
        completed = run_cmd(command, fiber_repo / "tests" / "bruno", timeout=args.timeout_seconds)
        write_text(log_dir / "bruno.stdout", completed.stdout)
        write_text(log_dir / "bruno.stderr", completed.stderr)
        return {
            "status": "passed" if completed.returncode == 0 else "failed",
            "started_node": started_node,
            "command": command,
            "returncode": completed.returncode,
            "stdout_log": rel(log_dir / "bruno.stdout", args.repo_root.resolve()),
            "stderr_log": rel(log_dir / "bruno.stderr", args.repo_root.resolve()),
            "duration_seconds": round(time.time() - started_at, 3),
        }
    finally:
        if node_process is not None and node_process.poll() is None:
            if hasattr(os, "killpg"):
                os.killpg(os.getpgid(node_process.pid), signal.SIGTERM)
            else:
                node_process.terminate()
            try:
                stdout, _ = node_process.communicate(timeout=20)
            except subprocess.TimeoutExpired:
                node_process.kill()
                stdout, _ = node_process.communicate(timeout=20)
            write_text(log_dir / "start-node.log", stdout or "")


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    fiber_repo = args.fiber_repo.resolve()
    run_suites = {workflow.suite for workflow in REQUIRED_WORKFLOWS} if args.run_all else set(args.run_suite or [])

    executions: dict[str, dict[str, Any]] = {}
    for workflow in REQUIRED_WORKFLOWS:
        if workflow.suite in run_suites:
            executions[workflow.suite] = run_workflow(args, workflow)

    workflows = [workflow_report(fiber_repo, workflow, executions.get(workflow.suite)) for workflow in REQUIRED_WORKFLOWS]
    present_count = sum(1 for row in workflows if row["present"])
    executed_count = sum(1 for row in workflows if row["execution"] is not None)
    passed_execution_count = sum(1 for row in workflows if row["execution"] is not None and row["execution"]["status"] == "passed")
    all_present = present_count == len(REQUIRED_WORKFLOWS)
    all_executed = executed_count == len(REQUIRED_WORKFLOWS)
    all_executed_passed = all_executed and passed_execution_count == len(REQUIRED_WORKFLOWS)
    runnable_contract_present = all(
        (fiber_repo / path).is_file()
        for path in (
            "tests/nodes/start.sh",
            "tests/nodes/wait.sh",
            "package.json",
            "tests/bruno/bruno.json",
            "docs/dev/README.md",
            "Cargo.lock",
        )
    )
    if not fiber_repo.is_dir():
        status = "missing_fiber_clone"
    elif all_executed_passed:
        status = "passed"
    elif executed_count > 0 and passed_execution_count != executed_count:
        status = "failed"
    elif all_present and runnable_contract_present:
        status = "discovery_ready_live_not_run"
    else:
        status = "incomplete"

    mapped_profiles = sorted({profile for workflow in REQUIRED_WORKFLOWS for profile in workflow.mapped_profiles})
    return {
        "schema": SCHEMA,
        "status": status,
        "generated_at_unix": int(time.time()),
        "classification": "fiber_node_execution_v0",
        "fiber_repo": {
            "path": fiber_repo.as_posix(),
            "origin": git_value(fiber_repo, ["remote", "get-url", "origin"]),
            "branch": git_value(fiber_repo, ["branch", "--show-current"]),
            "commit": git_value(fiber_repo, ["rev-parse", "HEAD"]),
            "dirty": bool(git_value(fiber_repo, ["status", "--short"])),
        },
        "devnet_contract": {
            "runnable_devnet_contract_present": runnable_contract_present,
            "start_command": "./tests/nodes/start.sh e2e/<suite>",
            "wait_command": "./tests/nodes/wait.sh",
            "bruno_command": "cd tests/bruno && npm exec -- @usebruno/cli run e2e/<suite> -r --env test",
            "source_docs": "docs/dev/README.md",
        },
        "workflow_coverage": {
            "required_count": len(REQUIRED_WORKFLOWS),
            "present_count": present_count,
            "executed_count": executed_count,
            "passed_execution_count": passed_execution_count,
            "all_required_workflows_present": all_present,
            "all_required_workflows_executed": all_executed,
            "all_required_workflows_executed_passed": all_executed_passed,
        },
        "profiles_covered": mapped_profiles,
        "workflows": workflows,
        "acceptance_boundary": {
            "discovery_ready_live_not_run": "the Fiber clone exposes the expected devnet/e2e workflow surface, but no live Fiber node execution is claimed",
            "passed": "all required Fiber workflow suites were executed through Fiber's devnet node runner and Bruno e2e harness",
            "novaseal_mapping": "NovaSeal consumes this as external Fiber-node evidence; it does not replace NovaSeal's own CKB stateful profile reports",
        },
        "generated_by": {
            "script": "scripts/novaseal_fiber_node_experiments.py",
            "implementation": "cellscript::scripts::novaseal_fiber_node_experiments",
        },
        "tooling": {
            "npm": shutil.which("npm"),
            "cargo": shutil.which("cargo"),
            "ckb": shutil.which("ckb"),
        },
    }


def main() -> int:
    args = parse_args()
    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")
    print(args.output)
    return 0 if report["status"] not in {"missing_fiber_clone", "incomplete", "failed"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
