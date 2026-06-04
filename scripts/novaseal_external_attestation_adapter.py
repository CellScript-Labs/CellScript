#!/usr/bin/env python3
"""Generate NovaSeal external attestation adapter requests.

This report packages the public/shared CellDep and external BIP340 TCB review
requests from the current templates and local TCB review. It is deliberately
not an attestation; production still requires the real public/shared CellDep
attestation and external reviewer acceptance files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TCB_REVIEW = ROOT / "target/novaseal-bip340-tcb-review.json"
DEFAULT_PUBLIC_TEMPLATE = ROOT / "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json"
DEFAULT_EXTERNAL_TEMPLATE = ROOT / "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-external-attestation-adapter.json"

REPORT_PERSON = b"NovaExtAttReqV0"


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def report_hash(label: str, value: Any) -> str:
    h = hashlib.blake2b(digest_size=32, person=REPORT_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(canonical_json(value))
    return hex0x(h.digest())


def is_present(value: Any) -> bool:
    return value is not None and value != "" and value != [] and value != {}


def public_celldep_case(template: dict[str, Any], tcb: dict[str, Any]) -> dict[str, Any]:
    verifier = template.get("runtime_verifier", {})
    release = template.get("release", {})
    runtime = tcb.get("runtime_artifact", {})
    request = {
        "attestation_type": "public_shared_cell_dep_attestation",
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json",
        "template_schema": template.get("schema"),
        "template_hash": report_hash("public_celldep_template", template),
        "required_public_fields": [
            "network",
            "attested_at",
            "attestor",
            "release.package",
            "release.version",
            "release.manifest_commit",
            "runtime_verifier.verifier_id",
            "runtime_verifier.ipc_abi",
            "runtime_verifier.out_point",
            "runtime_verifier.data_hash",
            "runtime_verifier.dep_type",
            "runtime_verifier.hash_type",
            "runtime_verifier.artifact_hash",
            "request_handoff.bundle",
            "request_handoff.bundle_hash",
            "request_handoff.bundle_hash_algorithm",
            "request_handoff.group",
        ],
        "field_constraints": {
            "network": "public CKB network name; must not be local-devnet",
            "attested_at": "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form",
            "attestor": "real release signer or deployer identity; placeholder tokens are rejected",
            "release.manifest_commit": "40-character hex source commit matching the reviewed TCB repo_commit",
            "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
        },
        "verifier_id": verifier.get("verifier_id"),
        "ipc_abi": verifier.get("ipc_abi"),
        "expected_artifact_hash": runtime.get("artifact_hash") or verifier.get("artifact_hash"),
        "expected_release_manifest_commit": tcb.get("repo_commit"),
        "template_artifact_hash": verifier.get("artifact_hash"),
        "required_status": "attested",
        "network_must_not_equal": "local-devnet",
    }
    checks = {
        "template_schema_current": request["template_schema"] == "novaseal-public-shared-cell-dep-attestation-v0.1",
        "template_status_attested": template.get("status") == "attested",
        "release_fields_current": isinstance(release, dict) and set(release) == {"package", "version", "manifest_commit"},
        "release_package_current": release.get("package") == "novaseal" if isinstance(release, dict) else False,
        "release_manifest_commit_present": is_present(release.get("manifest_commit")) if isinstance(release, dict) else False,
        "expected_release_manifest_commit_present": is_present(request["expected_release_manifest_commit"]),
        "verifier_id_current": request["verifier_id"] == "btc.bip340.v0",
        "ipc_abi_current": request["ipc_abi"] == "cellscript-btc-bip340-ipc-v0",
        "artifact_hash_matches_tcb": request["template_artifact_hash"] == request["expected_artifact_hash"],
        "required_fields_complete": len(request["required_public_fields"]) == 17,
    }
    return {
        "name": "public_shared_cell_dep_attestation",
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "request": request,
    }


def external_tcb_case(template: dict[str, Any], tcb: dict[str, Any]) -> dict[str, Any]:
    runtime = tcb.get("runtime_artifact", {})
    source = tcb.get("source_inventory", {})
    request = {
        "attestation_type": "external_bip340_tcb_review_attestation",
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json",
        "template_schema": template.get("schema"),
        "template_hash": report_hash("external_tcb_template", template),
        "required_public_fields": [
            "reviewer",
            "review_date",
            "review_scope",
            "verifier_id",
            "ipc_abi",
            "artifact_hash",
            "artifact_hash_algorithm",
            "source_tree_sha256",
            "report_uri",
            "request_handoff.bundle",
            "request_handoff.bundle_hash",
            "request_handoff.bundle_hash_algorithm",
            "request_handoff.group",
        ],
        "field_constraints": {
            "reviewer": "real external reviewer identity; placeholder tokens are rejected",
            "review_date": "UTC date in YYYY-MM-DD form",
            "artifact_hash_algorithm": "sha256",
            "report_uri": "HTTPS URI for the public review report or source-controlled review commit",
            "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
        },
        "verifier_id": template.get("verifier_id"),
        "ipc_abi": template.get("ipc_abi"),
        "expected_artifact_hash": runtime.get("artifact_hash"),
        "template_artifact_hash": template.get("artifact_hash"),
        "expected_artifact_hash_algorithm": runtime.get("artifact_hash_algorithm"),
        "template_artifact_hash_algorithm": template.get("artifact_hash_algorithm"),
        "expected_source_tree_sha256": source.get("source_tree_sha256"),
        "template_source_tree_sha256": template.get("source_tree_sha256"),
        "required_status": "accepted",
    }
    checks = {
        "template_schema_current": request["template_schema"] == "novaseal-bip340-external-tcb-review-attestation-v0.1",
        "template_status_accepted": template.get("status") == "accepted",
        "verifier_id_current": request["verifier_id"] == "btc.bip340.v0",
        "ipc_abi_current": request["ipc_abi"] == "cellscript-btc-bip340-ipc-v0",
        "artifact_hash_matches_tcb": is_present(request["expected_artifact_hash"])
        and request["template_artifact_hash"] == request["expected_artifact_hash"],
        "artifact_hash_algorithm_current": template.get("artifact_hash_algorithm") == "sha256",
        "artifact_hash_algorithm_matches_tcb": is_present(request["expected_artifact_hash_algorithm"])
        and request["template_artifact_hash_algorithm"] == request["expected_artifact_hash_algorithm"],
        "source_tree_hash_matches_tcb": is_present(request["expected_source_tree_sha256"])
        and request["template_source_tree_sha256"] == request["expected_source_tree_sha256"],
        "review_scope_present": bool(template.get("review_scope")),
        "required_fields_complete": len(request["required_public_fields"]) == 13,
    }
    return {
        "name": "external_bip340_tcb_review_attestation",
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "request": request,
    }


def build_report(public_template: dict[str, Any], external_template: dict[str, Any], tcb: dict[str, Any]) -> dict[str, Any]:
    cases = [public_celldep_case(public_template, tcb), external_tcb_case(external_template, tcb)]
    status = "passed" if all(case["status"] == "passed" for case in cases) else "failed"
    return {
        "schema": "novaseal-external-attestation-adapter-v0.1",
        "status": status,
        "adapter_status": "request_ready_external_attestations_required",
        "source_tcb_review": str(DEFAULT_TCB_REVIEW.relative_to(ROOT)),
        "source_tcb_review_hash": report_hash("tcb_review", tcb),
        "source_public_cell_dep_template": str(DEFAULT_PUBLIC_TEMPLATE.relative_to(ROOT)),
        "source_public_cell_dep_template_hash": report_hash("public_celldep_template", public_template),
        "source_external_tcb_template": str(DEFAULT_EXTERNAL_TEMPLATE.relative_to(ROOT)),
        "source_external_tcb_template_hash": report_hash("external_tcb_template", external_template),
        "production_boundary": "This adapter proves the attestation request package is complete; it does not prove public CellDep deployment or independent external TCB review.",
        "summary": {
            "total": len(cases),
            "matched": len([case for case in cases if case["status"] == "passed"]),
            "required_attestations": [case["name"] for case in cases],
        },
        "cases": cases,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tcb-review", type=Path, default=DEFAULT_TCB_REVIEW)
    parser.add_argument("--public-template", type=Path, default=DEFAULT_PUBLIC_TEMPLATE)
    parser.add_argument("--external-template", type=Path, default=DEFAULT_EXTERNAL_TEMPLATE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    tcb = json.loads(args.tcb_review.read_text(encoding="utf-8"))
    public_template = json.loads(args.public_template.read_text(encoding="utf-8"))
    external_template = json.loads(args.external_template.read_text(encoding="utf-8"))
    report = build_report(public_template, external_template, tcb)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"attestations={report['summary']['matched']}/{report['summary']['total']}"
        )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
