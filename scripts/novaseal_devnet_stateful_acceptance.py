#!/usr/bin/env python3
"""Fail-closed NovaSeal live-devnet stateful acceptance gate.

This script exists to prevent local CKB-VM/ResolvedTransaction evidence from
being mistaken for a real devnet lifecycle. It intentionally reports the
current blockers until NovaSeal has a stable type-script dispatcher/bootstrap
surface that can carry a live Cell from one transaction into the next.
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from novaseal_devnet_stateful_live import file_sha256_hex, git_commit, source_tree_hash


CORE_ROOT = Path("proposals/novaseal/v0-mvp-skeleton")
AGREEMENT_ROOT = Path("proposals/novaseal/agreement-profile-v0")
VERIFIER_ROOT = CORE_ROOT / "verifier/novaseal_btc_verifier"
CORE_STATEFUL_SOURCE_PATHS = [
    CORE_ROOT / "Cell.toml",
    CORE_ROOT / "src",
    CORE_ROOT / "schemas",
    VERIFIER_ROOT,
    Path("scripts/novaseal_devnet_stateful_live.py"),
]
AGREEMENT_STATEFUL_SOURCE_PATHS = [
    AGREEMENT_ROOT / "Cell.toml",
    AGREEMENT_ROOT / "src",
    AGREEMENT_ROOT / "schemas",
    VERIFIER_ROOT,
    Path("scripts/novaseal_agreement_devnet_stateful_live.py"),
    Path("scripts/novaseal_devnet_stateful_live.py"),
]


@dataclass(frozen=True)
class ActionSurface:
    name: str
    params: str

    @property
    def consumes_resource(self) -> bool:
        return "NovaSealCellV0" in self.params or "NovaAgreementCellV0" in self.params


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, default=Path("target/novaseal-devnet-stateful-acceptance.json"))
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument(
        "--report-only",
        action="store_true",
        help="write the fail-closed report but exit 0 even when stateful acceptance is blocked",
    )
    return parser.parse_args()


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def read_cell_sources(root: Path) -> str:
    src_root = root / "src"
    parts: list[str] = []
    for path in sorted(src_root.glob("*.cell")):
        if not path.is_file():
            continue
        parts.append(f"\n// source-unit: {path.name}\n")
        parts.append(read_text(path))
    return "\n".join(parts)


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except json.JSONDecodeError as exc:
        return {"_invalid_json": str(exc)}
    return value if isinstance(value, dict) else {"_invalid_json": "top-level value is not an object"}


def find_actions(source: str) -> list[ActionSurface]:
    actions: list[ActionSurface] = []
    for match in re.finditer(r"(?m)^action\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)", source):
        actions.append(ActionSurface(name=match.group(1), params=match.group(2)))
    return actions


def has_dispatcher_surface(source: str, root: Path) -> bool:
    names = {action.name for action in find_actions(source)}
    manifest = read_text(root / "Cell.toml")
    return (
        bool(names & {"dispatch", "dispatch_agreement", "novaseal_dispatch", "agreement_dispatch"})
        or "stateful_dispatcher" in manifest
        or "dispatcher" in manifest and "entry" in manifest
    )


def has_core_bootstrap_surface(source: str) -> bool:
    lifecycle_names = {action.name for action in find_actions(source)}
    if "novaseal_lifecycle" in lifecycle_names and "OP_BOOTSTRAP" in source:
        return True
    for action in find_actions(source):
        lowered = action.name.lower()
        if any(word in lowered for word in ("bootstrap", "genesis", "seed", "initialize", "originate")) and not action.consumes_resource:
            return True
    return False


def has_agreement_origination_surface(source: str) -> bool:
    if "nova_agreement_lifecycle" in {action.name for action in find_actions(source)} and "PATH_ORIGINATE" in source:
        return True
    return any(action.name == "originate_agreement" and not action.consumes_resource for action in find_actions(source))


def summary_from_report(report: dict[str, Any] | None, summary_keys: list[str]) -> dict[str, Any]:
    if report is None:
        return {"present": False}
    if "_invalid_json" in report:
        return {"present": True, "valid_json": False, "error": report["_invalid_json"]}
    summary = report.get("summary")
    if not isinstance(summary, dict):
        return {"present": True, "valid_json": True, "summary_present": False}
    out: dict[str, Any] = {"present": True, "valid_json": True, "summary_present": True}
    for key in summary_keys:
        out[key] = summary.get(key)
    return out


def provenance_summary(report: dict[str, Any] | None, repo_root: Path, source_paths: list[Path]) -> dict[str, Any]:
    if report is None:
        return {"present": False, "freshness_matched": False}
    if "_invalid_json" in report:
        return {"present": False, "freshness_matched": False, "error": report["_invalid_json"]}
    provenance = report.get("provenance")
    if not isinstance(provenance, dict):
        return {"present": False, "freshness_matched": False}

    recorded_source = provenance.get("source_tree") if isinstance(provenance.get("source_tree"), dict) else {}
    current_source = source_tree_hash(repo_root, source_paths)
    source_hash_matches = recorded_source.get("sha256") == current_source.get("sha256")

    artifact_checks: dict[str, Any] = {}
    recorded_artifacts = provenance.get("artifacts") if isinstance(provenance.get("artifacts"), dict) else {}
    for name in ("verifier", "lifecycle"):
        artifact = recorded_artifacts.get(name) if isinstance(recorded_artifacts.get(name), dict) else {}
        raw_path = artifact.get("path")
        path = Path(raw_path) if isinstance(raw_path, str) else None
        if path is not None and not path.is_absolute():
            path = repo_root / path
        exists = path.is_file() if path is not None else False
        current_sha = file_sha256_hex(path) if exists and path is not None else None
        artifact_checks[name] = {
            "present": bool(artifact),
            "path": raw_path,
            "exists": exists,
            "sha256_matches": current_sha == artifact.get("sha256"),
            "recorded_sha256": artifact.get("sha256"),
            "current_sha256": current_sha,
        }
    artifact_hashes_match = all(row["present"] and row["exists"] and row["sha256_matches"] for row in artifact_checks.values())

    return {
        "present": True,
        "freshness_matched": source_hash_matches and artifact_hashes_match,
        "repo_commit": provenance.get("repo_commit"),
        "current_repo_commit": git_commit(repo_root),
        "repo_commit_matches": provenance.get("repo_commit") == git_commit(repo_root),
        "source_hash_matches": source_hash_matches,
        "recorded_source_hash": recorded_source.get("sha256"),
        "current_source_hash": current_source.get("sha256"),
        "recorded_file_count": recorded_source.get("file_count"),
        "current_file_count": current_source.get("file_count"),
        "artifact_hashes_match": artifact_hashes_match,
        "artifacts": artifact_checks,
    }


def negative_case_matched(report: dict[str, Any], key: str) -> bool | None:
    negative = report.get("negative_cases") if isinstance(report.get("negative_cases"), dict) else {}
    row = negative.get(key) if isinstance(negative.get(key), dict) else None
    if row is None:
        return None
    return row.get("status") == "rejected" and row.get("matched_expected") is True


def live_core_summary(report: dict[str, Any] | None, repo_root: Path) -> dict[str, Any]:
    if report is None:
        return {"present": False}
    if "_invalid_json" in report:
        return {"present": True, "valid_json": False, "error": report["_invalid_json"]}
    transition = report.get("transition") if isinstance(report.get("transition"), dict) else {}
    provenance = provenance_summary(report, repo_root, CORE_STATEFUL_SOURCE_PATHS)
    return {
        "present": True,
        "valid_json": True,
        "status": report.get("status"),
        "live_devnet_rpc_executed": report.get("live_devnet_rpc_executed") is True,
        "stateful_lifecycle_executed": report.get("stateful_lifecycle_executed") is True,
        "provenance": provenance,
        "provenance_freshness_matched": provenance.get("freshness_matched") is True,
        "bootstrap_tx_hash": ((report.get("bootstrap") or {}).get("commit") or {}).get("tx_hash")
        if isinstance(report.get("bootstrap"), dict)
        else None,
        "transition_tx_hash": (transition.get("commit") or {}).get("tx_hash") if isinstance(transition.get("commit"), dict) else None,
        "old_state_not_live": transition.get("old_state_not_live"),
        "new_state_live": transition.get("new_state_live"),
        "receipt_live": transition.get("receipt_live"),
        "wrong_signature_rejected": negative_case_matched(report, "wrong_signature_dry_run"),
    }


def live_agreement_summary(report: dict[str, Any] | None, repo_root: Path) -> dict[str, Any]:
    if report is None:
        return {"present": False}
    if "_invalid_json" in report:
        return {"present": True, "valid_json": False, "error": report["_invalid_json"]}
    originate = report.get("originate") if isinstance(report.get("originate"), dict) else {}
    claim_originate = report.get("claim_originate") if isinstance(report.get("claim_originate"), dict) else {}
    repay = report.get("repay") if isinstance(report.get("repay"), dict) else {}
    claim = report.get("claim") if isinstance(report.get("claim"), dict) else {}
    provenance = provenance_summary(report, repo_root, AGREEMENT_STATEFUL_SOURCE_PATHS)
    return {
        "present": True,
        "valid_json": True,
        "status": report.get("status"),
        "live_devnet_rpc_executed": report.get("live_devnet_rpc_executed") is True,
        "stateful_lifecycle_executed": report.get("stateful_lifecycle_executed") is True,
        "provenance": provenance,
        "provenance_freshness_matched": provenance.get("freshness_matched") is True,
        "originate_tx_hash": (originate.get("commit") or {}).get("tx_hash")
        if isinstance(originate.get("commit"), dict)
        else None,
        "repay_tx_hash": (repay.get("commit") or {}).get("tx_hash") if isinstance(repay.get("commit"), dict) else None,
        "claim_originate_tx_hash": (claim_originate.get("commit") or {}).get("tx_hash")
        if isinstance(claim_originate.get("commit"), dict)
        else None,
        "claim_tx_hash": (claim.get("commit") or {}).get("tx_hash") if isinstance(claim.get("commit"), dict) else None,
        "origin_active_live": originate.get("active_live"),
        "origin_principal_payout_live": originate.get("principal_payout_live"),
        "origin_receipt_live": originate.get("receipt_live"),
        "claim_origin_active_live": claim_originate.get("active_live"),
        "claim_origin_principal_payout_live": claim_originate.get("principal_payout_live"),
        "claim_origin_receipt_live": claim_originate.get("receipt_live"),
        "repay_old_active_not_live": repay.get("old_active_not_live"),
        "repay_closed_live": repay.get("closed_live"),
        "repay_lender_repayment_live": repay.get("lender_repayment_live"),
        "repay_borrower_collateral_return_live": repay.get("borrower_collateral_return_live"),
        "repay_receipt_live": repay.get("receipt_live"),
        "claim_old_active_not_live": claim.get("old_active_not_live"),
        "claim_closed_live": claim.get("closed_live"),
        "claim_lender_default_claim_live": claim.get("lender_default_claim_live"),
        "claim_receipt_live": claim.get("receipt_live"),
        "wrong_lender_signature_rejected": negative_case_matched(report, "wrong_lender_signature_dry_run"),
        "non_ckb_asset_kind_rejected": negative_case_matched(report, "non_ckb_asset_kind_dry_run"),
        "wrong_borrower_signature_rejected": negative_case_matched(report, "wrong_borrower_signature_dry_run"),
        "repay_payout_capacity_short_rejected": negative_case_matched(report, "repay_payout_capacity_short_dry_run"),
        "repay_payout_lock_args_mismatch_rejected": negative_case_matched(
            report,
            "repay_payout_lock_args_mismatch_dry_run",
        ),
        "repay_wrong_payout_amount_rejected": negative_case_matched(report, "repay_wrong_payout_amount_dry_run"),
        "early_claim_rejected": negative_case_matched(report, "early_claim_dry_run"),
        "wrong_lender_claim_signature_rejected": negative_case_matched(report, "wrong_lender_claim_signature_dry_run"),
        "post_negative_active_still_live": (report.get("negative_cases") or {}).get("post_negative_active_still_live")
        if isinstance(report.get("negative_cases"), dict)
        else None,
        "post_claim_negative_active_still_live": (report.get("negative_cases") or {}).get(
            "post_claim_negative_active_still_live"
        )
        if isinstance(report.get("negative_cases"), dict)
        else None,
    }


def blocker(text: str, *, required_for: str) -> dict[str, str]:
    return {"blocker": text, "required_for": required_for}


def build_report(repo_root: Path) -> dict[str, Any]:
    core_root = repo_root / CORE_ROOT
    agreement_root = repo_root / AGREEMENT_ROOT
    core_source = read_cell_sources(core_root)
    agreement_source = read_cell_sources(agreement_root)
    core_actions = find_actions(core_source)
    agreement_actions = find_actions(agreement_source)
    core_combined = load_json(core_root / "target/novaseal-combined-tx-report.json")
    agreement_tx = load_json(agreement_root / "target/nova-agreement-ckb-tx-report.json")
    live_core = live_core_summary(load_json(repo_root / "target/novaseal-devnet-stateful-live.json"), repo_root)
    live_agreement = live_agreement_summary(
        load_json(repo_root / "target/novaseal-agreement-devnet-stateful-live.json"),
        repo_root,
    )
    core_live_passed = (
        live_core.get("status") == "passed"
        and live_core.get("live_devnet_rpc_executed") is True
        and live_core.get("stateful_lifecycle_executed") is True
        and live_core.get("provenance_freshness_matched") is True
        and live_core.get("old_state_not_live") is True
        and live_core.get("new_state_live") is True
        and live_core.get("receipt_live") is True
        and live_core.get("wrong_signature_rejected") is True
    )
    agreement_live_passed = (
        live_agreement.get("status") == "passed"
        and live_agreement.get("live_devnet_rpc_executed") is True
        and live_agreement.get("stateful_lifecycle_executed") is True
        and live_agreement.get("provenance_freshness_matched") is True
        and live_agreement.get("origin_active_live") is True
        and live_agreement.get("origin_principal_payout_live") is True
        and live_agreement.get("origin_receipt_live") is True
        and live_agreement.get("claim_origin_active_live") is True
        and live_agreement.get("claim_origin_principal_payout_live") is True
        and live_agreement.get("claim_origin_receipt_live") is True
        and live_agreement.get("repay_old_active_not_live") is True
        and live_agreement.get("repay_closed_live") is True
        and live_agreement.get("repay_lender_repayment_live") is True
        and live_agreement.get("repay_borrower_collateral_return_live") is True
        and live_agreement.get("repay_receipt_live") is True
        and live_agreement.get("claim_old_active_not_live") is True
        and live_agreement.get("claim_closed_live") is True
        and live_agreement.get("claim_lender_default_claim_live") is True
        and live_agreement.get("claim_receipt_live") is True
        and live_agreement.get("wrong_lender_signature_rejected") is True
        and live_agreement.get("non_ckb_asset_kind_rejected") is True
        and live_agreement.get("wrong_borrower_signature_rejected") is True
        and live_agreement.get("repay_payout_capacity_short_rejected") is True
        and live_agreement.get("repay_payout_lock_args_mismatch_rejected") is True
        and live_agreement.get("repay_wrong_payout_amount_rejected") is True
        and live_agreement.get("early_claim_rejected") is True
        and live_agreement.get("wrong_lender_claim_signature_rejected") is True
        and live_agreement.get("post_negative_active_still_live") is True
        and live_agreement.get("post_claim_negative_active_still_live") is True
    )

    core_blockers: list[dict[str, str]] = []
    if not has_core_bootstrap_surface(core_source):
        core_blockers.append(
            blocker(
                "NovaSeal core has key_auth_transition but no bootstrap/genesis/seed action that can create the first live NovaSealCellV0.",
                required_for="creating an initial live state cell on devnet before the first transition",
            )
        )
    if not has_dispatcher_surface(core_source, core_root):
        core_blockers.append(
            blocker(
                "NovaSeal core is still compiled as a single entry action/lock surface, not a stable lifecycle dispatcher type script.",
                required_for="preserving one script identity across create, transition, and future terminal paths",
            )
        )

    agreement_blockers: list[dict[str, str]] = []
    agreement_action_names = {action.name for action in agreement_actions}
    expected_agreement_actions = {"originate_agreement", "repay_before_expiry", "claim_after_expiry"}
    if expected_agreement_actions <= agreement_action_names and not has_dispatcher_surface(agreement_source, agreement_root):
        agreement_blockers.append(
            blocker(
                "Agreement Profile compiles originate/repay/claim as separate entry-action ELFs; a live CKB Cell cannot move from originate ELF identity to repay/claim ELF identity.",
                required_for="originate -> repay or originate -> claim live-cell lifecycle",
            )
        )
    if not has_agreement_origination_surface(agreement_source):
        agreement_blockers.append(
            blocker(
                "Agreement Profile has no output-only origination action suitable for creating the initial agreement cell.",
                required_for="first live agreement cell creation",
            )
        )

    scenarios = [
        {
            "name": "novaseal_core_key_auth_transition",
            "status": "blocked" if core_blockers else ("passed" if core_live_passed else "ready_to_wire_live_devnet"),
            "live_devnet_rpc_executed": core_live_passed,
            "stateful_lifecycle_executed": core_live_passed,
            "actions": [action.name for action in core_actions],
            "blockers": core_blockers,
            "live_devnet_evidence": live_core,
            "existing_local_evidence": summary_from_report(
                core_combined,
                [
                    "combined_full_transaction_executed",
                    "ckb_node_verification_stack_executed",
                    "total_cases",
                    "matched_expected",
                    "node_stack_matched_expected",
                    "lock_and_type_script_groups_present",
                ],
            ),
        },
        {
            "name": "agreement_profile_originate_to_terminal",
            "status": "blocked" if agreement_blockers else ("passed" if agreement_live_passed else "ready_to_wire_live_devnet"),
            "live_devnet_rpc_executed": agreement_live_passed,
            "stateful_lifecycle_executed": agreement_live_passed,
            "actions": [action.name for action in agreement_actions],
            "blockers": agreement_blockers,
            "live_devnet_evidence": live_agreement,
            "existing_local_evidence": summary_from_report(
                agreement_tx,
                [
                    "resolved_transaction_harness_executed",
                    "ckb_node_verification_stack_executed",
                    "total_cases",
                    "script_matched_expected",
                    "node_matched_expected",
                    "fixture_files_not_executed_by_tx_harness",
                ],
            ),
        },
    ]

    all_blockers = [item for scenario in scenarios for item in scenario["blockers"]]
    if all_blockers:
        status = "blocked"
    elif all(scenario["status"] == "passed" for scenario in scenarios):
        status = "passed"
    elif core_live_passed and not agreement_live_passed:
        status = "core_live_devnet_passed_agreement_pending"
    elif agreement_live_passed and not core_live_passed:
        status = "agreement_live_devnet_passed_core_pending"
    else:
        status = "ready_to_run_live_devnet"
    return {
        "schema": "novaseal-devnet-stateful-acceptance-v0.1",
        "classification": "live_devnet_stateful_release_gate",
        "status": status,
        "production_ready": False,
        "live_devnet_rpc_executed": all(scenario["live_devnet_rpc_executed"] for scenario in scenarios),
        "stateful_lifecycle_executed": all(scenario["stateful_lifecycle_executed"] for scenario in scenarios),
        "repo_root": str(repo_root),
        "requirements": [
            "deploy runtime verifier and protocol artifacts as live CellDeps",
            "submit transactions through CKB RPC, not only in-memory ResolvedTransaction",
            "commit each valid step and verify old inputs are dead plus new state/receipt/payout outputs are live",
            "verify live output capacity/lock/type/data and reject stale source/artifact provenance",
            "prove negative dry-runs fail from the expected lifecycle script and artifact hash",
            "use one stable type-script identity for a lifecycle, or an explicitly audited dispatcher/bootstrap surface",
            "run negative cases as dry-run/send-test rejections without mutating live state",
        ],
        "scenarios": scenarios,
        "blocker_count": len(all_blockers),
        "blockers": all_blockers,
        "next_engineering_step": (
            "Stateful live-devnet acceptance is complete; production readiness is now governed by public CellDep "
            "pinning, wallet/Molecule vectors, and external verifier TCB attestation."
            if status == "passed"
            else "Re-run the live core/agreement devnet runners after source or artifact changes; this gate fails closed "
            "until both reports have fresh provenance, strict output checks, and matched negative dry-run errors."
        ),
    }


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    output = args.output if args.output.is_absolute() else repo_root / args.output
    report = build_report(repo_root)
    output.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(report, indent=2 if args.pretty else None, sort_keys=True)
    output.write_text(payload + "\n", encoding="utf-8")
    print(
        f"wrote {output} status={report['status']} "
        f"live_devnet_rpc_executed={report['live_devnet_rpc_executed']} blockers={report['blocker_count']}"
    )
    if report["status"] != "passed" and not args.report_only:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
