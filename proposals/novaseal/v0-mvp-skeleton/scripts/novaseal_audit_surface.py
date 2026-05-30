#!/usr/bin/env python3
"""Extract a narrow NovaSeal v0 audit surface from a CellScript audit bundle.

This script is intentionally conservative: it only summarises evidence already
present in `cellc audit-bundle --json` output, plus exact source-guard snippets
from the MVP state action. It does not upgrade source guards into generated
ProofPlan coverage.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-audit-surface-v0.1"

DEFAULT_BUNDLE = Path("target/cellscript-audit-bundle/audit-bundle.json")
DEFAULT_SOURCE = Path("src/nova_state_type.cell")
DEFAULT_OUTPUT = Path("target/novaseal-audit-surface.json")

FIELD_GUARDS = [
    {
        "criterion": 3,
        "field": "state_hash",
        "source_snippets": [
            "require intent.old_state_hash == old_cell.state_hash",
            "let actual_state_hash_commitment = hash_blake2b(intent.new_state_hash)",
            "require actual_state_hash_commitment == state_hash_commitment",
            "state_hash: intent.new_state_hash",
        ],
    },
    {
        "criterion": 4,
        "field": "nonce",
        "source_snippets": [
            "require intent.nonce == old_cell.nonce + 1",
            "nonce: intent.nonce",
        ],
    },
    {
        "criterion": 5,
        "field": "expiry",
        "source_snippets": [
            "require now <= intent.expiry",
            "expiry: intent.expiry",
        ],
    },
    {
        "criterion": 7,
        "field": "policy_hash",
        "source_snippets": [
            "require intent.policy_hash == old_cell.policy_hash",
            "policy_hash: old_cell.policy_hash",
        ],
    },
    {
        "criterion": 8,
        "field": "receipt_hash",
        "source_snippets": [
            "require receipt_hash == intent.receipt_hash",
        ],
    },
]


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"missing audit bundle: {path}") from None
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from None


def read_text_if_present(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def compact_record(record: dict[str, Any], keys: list[str]) -> dict[str, Any]:
    return {key: record.get(key) for key in keys if key in record}


def proof_plan_records(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    records = bundle.get("proof_plan", [])
    if not isinstance(records, list):
        return []
    return [record for record in records if isinstance(record, dict)]


def actions(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    records = bundle.get("actions", [])
    if not isinstance(records, list):
        return []
    return [record for record in records if isinstance(record, dict)]


def assumptions(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    records = bundle.get("builder_assumptions", [])
    if not isinstance(records, list):
        return []
    return [record for record in records if isinstance(record, dict)]


def source_units(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    records = bundle.get("source_units", [])
    if not isinstance(records, list):
        return []
    return [record for record in records if isinstance(record, dict)]


def generated_hits_for_field(records: list[dict[str, Any]], field: str) -> list[dict[str, Any]]:
    hits = []
    for record in records:
        haystack = json.dumps(record, sort_keys=True)
        if field in haystack:
            hits.append(
                compact_record(
                    record,
                    [
                        "name",
                        "feature",
                        "status",
                        "codegen_coverage_status",
                        "on_chain_checked",
                        "origin",
                        "input_output_relation_checks",
                    ],
                )
            )
    return hits


def field_guard_visibility(bundle: dict[str, Any], source_text: str) -> list[dict[str, Any]]:
    records = proof_plan_records(bundle)
    result = []
    for guard in FIELD_GUARDS:
        snippets = guard["source_snippets"]
        missing = [snippet for snippet in snippets if snippet not in source_text]
        hits = generated_hits_for_field(records, guard["field"])
        if missing:
            classification = "missing_source_guard"
        elif hits:
            classification = "generated_visible"
        else:
            classification = "source_guard_only"
        result.append(
            {
                "criterion": guard["criterion"],
                "field": guard["field"],
                "source_guard_present": not missing,
                "missing_source_snippets": missing,
                "generated_named_obligation": bool(hits),
                "generated_hits": hits,
                "classification": classification,
            }
        )
    return result


def strict_mode_predictions(bundle: dict[str, Any]) -> list[dict[str, str]]:
    predictions = []
    for record in proof_plan_records(bundle):
        status = str(record.get("status", ""))
        coverage = str(record.get("codegen_coverage_status", ""))
        feature = str(record.get("feature", ""))
        origin = str(record.get("origin", ""))
        on_chain_checked = bool(record.get("on_chain_checked", False))
        if status == "checked-runtime" and not on_chain_checked:
            predictions.append(
                {
                    "code": "PP0103",
                    "feature": feature,
                    "origin": origin,
                    "reason": "checked ProofPlan status is not reflected in on_chain_checked",
                }
            )
        if status == "runtime-required" or coverage == "gap:metadata-only":
            predictions.append(
                {
                    "code": "PP0150",
                    "feature": feature,
                    "origin": origin,
                    "reason": "strict v0.16 ProofPlan mode rejects metadata-only or runtime-required obligations",
                }
            )
    return predictions


def runtime_gaps(bundle: dict[str, Any]) -> list[dict[str, Any]]:
    gaps = []
    for record in proof_plan_records(bundle):
        status = str(record.get("status", ""))
        coverage = str(record.get("codegen_coverage_status", ""))
        if status == "runtime-required" or coverage.startswith("gap:"):
            gaps.append(
                compact_record(
                    record,
                    [
                        "name",
                        "feature",
                        "status",
                        "codegen_coverage_status",
                        "detail",
                        "origin",
                        "on_chain_checked",
                        "input_output_relation_checks",
                    ],
                )
            )
    return gaps


def capacity_evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    ckb = bundle.get("constraints", {}).get("ckb", {})
    if not isinstance(ckb, dict):
        return {}
    contract = ckb.get("capacity_evidence_contract", {})
    if not isinstance(contract, dict):
        contract = {}
    return {
        "capacity_status": ckb.get("capacity_status"),
        "cycles_status": ckb.get("cycles_status"),
        "estimated_cycles": ckb.get("estimated_cycles"),
        "measured_cycles": ckb.get("measured_cycles"),
        "tx_size_status": ckb.get("tx_size_status"),
        "tx_size_bytes": ckb.get("tx_size_bytes"),
        "occupied_capacity_measurement_required": ckb.get("occupied_capacity_measurement_required"),
        "tx_size_measurement_required": ckb.get("tx_size_measurement_required"),
        "capacity_evidence_contract": compact_record(
            contract,
            [
                "status",
                "required",
                "measured_occupied_capacity_shannons",
                "measured_tx_size_bytes",
                "recommended_code_cell_capacity_shannons",
            ],
        ),
    }


def build_surface(bundle: dict[str, Any], bundle_path: Path, source_path: Path, source_text: str) -> dict[str, Any]:
    plan = proof_plan_records(bundle)
    lock_records = bundle.get("locks", [])
    if not isinstance(lock_records, list):
        lock_records = []

    generated_actions = [
        compact_record(action, ["name", "proof_plan_records", "estimated_cycles", "runtime_accesses"])
        for action in actions(bundle)
    ]
    generated_plan = [
        compact_record(
            record,
            [
                "name",
                "feature",
                "category",
                "status",
                "codegen_coverage_status",
                "on_chain_checked",
                "origin",
                "input_output_relation_checks",
                "reads",
                "scope",
                "trigger",
                "detail",
            ],
        )
        for record in plan
    ]
    generated_assumptions = [
        compact_record(
            assumption,
            [
                "assumption_id",
                "feature",
                "origin",
                "kind",
                "proof_plan_status",
                "capacity_policy",
                "change_policy",
                "signature_policy",
                "failure_mode",
            ],
        )
        for assumption in assumptions(bundle)
    ]

    gaps = runtime_gaps(bundle)
    strict_predictions = strict_mode_predictions(bundle)
    field_visibility = field_guard_visibility(bundle, source_text)
    plan_features = [str(record.get("feature", "")) for record in proof_plan_records(bundle)]

    production_blockers = []
    if gaps:
        production_blockers.append("runtime-required ProofPlan gaps remain")
    if not any(lock.get("name") == "btc_authority" for lock in lock_records):
        production_blockers.append("generated locks[] does not include btc_authority")
    if any(not item["generated_named_obligation"] for item in field_visibility):
        production_blockers.append("state/nonce/expiry/policy/receipt guards are source-visible but not named generated ProofPlan obligations")
    if not any("spawn" in feature or "bip340" in feature or "btc-verifier" in feature for feature in plan_features):
        production_blockers.append("btc_authority has no generated spawn/IPC wiring")
    if not any(feature.startswith("create-output:ProofReceiptV0") for feature in plan_features):
        production_blockers.append("ProofReceiptV0 output cell materialisation is not generated")
    if capacity_evidence(bundle).get("measured_cycles") is None:
        production_blockers.append("cycles, tx size, and occupied capacity are not measured")

    return {
        "schema": SCHEMA,
        "generated_from": str(bundle_path),
        "source_checked": str(source_path),
        "module": bundle.get("module"),
        "compiler_version": bundle.get("compiler_version"),
        "target_profile": bundle.get("target_profile"),
        "audit_bundle_status": bundle.get("status"),
        "summary": {
            "actions": len(generated_actions),
            "locks": len(lock_records),
            "proof_plan_records": len(plan),
            "runtime_gaps": len(gaps),
            "builder_assumptions": len(generated_assumptions),
            "source_units": len(source_units(bundle)),
            "strict_prediction_errors": len(strict_predictions),
            "classification": "non_production_audit_surface",
        },
        "actions": generated_actions,
        "locks": lock_records,
        "source_units": source_units(bundle),
        "proof_plan_soundness": bundle.get("proof_plan_soundness", {}),
        "proof_plan": generated_plan,
        "runtime_gaps": gaps,
        "strict_mode_predictions": strict_predictions,
        "field_guard_visibility": field_visibility,
        "capacity_evidence": capacity_evidence(bundle),
        "builder_assumptions": generated_assumptions,
        "production_blockers": production_blockers,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit-bundle", type=Path, default=DEFAULT_BUNDLE)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true", help="Pretty-print JSON output")
    args = parser.parse_args()

    bundle = load_json(args.audit_bundle)
    source_text = read_text_if_present(args.source)
    surface = build_surface(bundle, args.audit_bundle, args.source, source_text)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(surface, indent=indent, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"actions={surface['summary']['actions']} "
        f"locks={surface['summary']['locks']} "
        f"proof_plan_records={surface['summary']['proof_plan_records']} "
        f"runtime_gaps={surface['summary']['runtime_gaps']} "
        f"strict_prediction_errors={surface['summary']['strict_prediction_errors']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
