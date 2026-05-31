#!/usr/bin/env python3
"""Local transaction-shape harness for NovaSeal Agreement Profile v0.

This script does not execute CellScript or CKB VM. It checks the deterministic
builder-visible CKB capacity/output shapes implied by the current CKB/CKB
Agreement Profile fixtures.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


CKB = 100_000_000

AGREEMENT_OCCUPIED_CAPACITY = 40 * CKB
RECEIPT_OCCUPIED_CAPACITY = 20 * CKB
PAYOUT_OCCUPIED_CAPACITY = 40 * CKB
BUILDER_FEE_SHANNONS = 100_000

START_TIMEPOINT = 100
EXPIRY_TIMEPOINT = 200

COLLATERAL_AMOUNT = 1_000 * CKB
PRINCIPAL_AMOUNT = 700 * CKB
FIXED_FEE_AMOUNT = 30 * CKB

BORROWER_AUTHORITY = "0x" + "11" * 32
LENDER_AUTHORITY = "0x" + "22" * 32
STRANGER_AUTHORITY = "0x" + "33" * 32

PACKAGE_ROOT = Path(__file__).resolve().parents[1]
FIXTURES_DIR = PACKAGE_ROOT / "fixtures"
DEFAULT_REPORT = PACKAGE_ROOT / "target" / "nova-agreement-tx-shape-report.json"


@dataclass(frozen=True)
class OutputShape:
    role: str
    owner: str
    occupied_capacity_shannons: int
    capacity_shannons: int

    @property
    def economic_value_shannons(self) -> int:
        return self.capacity_shannons - self.occupied_capacity_shannons

    def to_json(self) -> dict[str, Any]:
        return {
            "role": self.role,
            "owner": self.owner,
            "occupied_capacity_shannons": self.occupied_capacity_shannons,
            "capacity_shannons": self.capacity_shannons,
            "economic_value_shannons": self.economic_value_shannons,
        }


@dataclass(frozen=True)
class HarnessCase:
    fixture: str
    action: str
    current_timepoint: int
    actor_authority_hash: str
    outputs: tuple[OutputShape, ...]
    note: str


def output(role: str, owner: str, occupied_capacity: int, economic_value: int) -> OutputShape:
    return OutputShape(
        role=role,
        owner=owner,
        occupied_capacity_shannons=occupied_capacity,
        capacity_shannons=occupied_capacity + economic_value,
    )


def under_capacity(role: str, owner: str, occupied_capacity: int, missing: int) -> OutputShape:
    return OutputShape(
        role=role,
        owner=owner,
        occupied_capacity_shannons=occupied_capacity,
        capacity_shannons=occupied_capacity - missing,
    )


def canonical_cases() -> tuple[HarnessCase, ...]:
    return (
        HarnessCase(
            fixture="originate_valid",
            action="originate_agreement",
            current_timepoint=120,
            actor_authority_hash=BORROWER_AUTHORITY,
            outputs=(
                output(
                    "agreement_collateral",
                    BORROWER_AUTHORITY,
                    AGREEMENT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output(
                    "borrower_principal_payout",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="Borrower locks collateral while lender-funded principal is paid to borrower.",
        ),
        HarnessCase(
            fixture="repay_before_expiry_valid",
            action="repay_before_expiry",
            current_timepoint=180,
            actor_authority_hash=BORROWER_AUTHORITY,
            outputs=(
                output("closed_agreement", BORROWER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_repayment",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                ),
                output(
                    "borrower_collateral_return",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="Borrower repays principal plus fixed fee and receives collateral back.",
        ),
        HarnessCase(
            fixture="claim_after_expiry_valid",
            action="claim_after_expiry",
            current_timepoint=220,
            actor_authority_hash=LENDER_AUTHORITY,
            outputs=(
                output("closed_agreement", LENDER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_default_claim",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", LENDER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="After expiry, lender claims the locked collateral. No extra fixed fee is minted.",
        ),
        HarnessCase(
            fixture="expired_repay_reject",
            action="repay_before_expiry",
            current_timepoint=220,
            actor_authority_hash=BORROWER_AUTHORITY,
            outputs=(
                output("closed_agreement", BORROWER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_repayment",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                ),
                output(
                    "borrower_collateral_return",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="Repayment after expiry must be rejected by the time guard.",
        ),
        HarnessCase(
            fixture="early_claim_reject",
            action="claim_after_expiry",
            current_timepoint=180,
            actor_authority_hash=LENDER_AUTHORITY,
            outputs=(
                output("closed_agreement", LENDER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_default_claim",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", LENDER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="Default claim before expiry must be rejected by the time guard.",
        ),
        HarnessCase(
            fixture="wrong_party_reject",
            action="repay_before_expiry",
            current_timepoint=180,
            actor_authority_hash=STRANGER_AUTHORITY,
            outputs=(
                output("closed_agreement", BORROWER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_repayment",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                ),
                output(
                    "borrower_collateral_return",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="A non-borrower actor cannot exercise the repay path.",
        ),
        HarnessCase(
            fixture="under_capacity_reject",
            action="repay_before_expiry",
            current_timepoint=180,
            actor_authority_hash=BORROWER_AUTHORITY,
            outputs=(
                under_capacity("closed_agreement", BORROWER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, CKB),
                output(
                    "lender_repayment",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT,
                ),
                output(
                    "borrower_collateral_return",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="A terminal agreement output below occupied capacity is invalid.",
        ),
        HarnessCase(
            fixture="wrong_settlement_amount_reject",
            action="repay_before_expiry",
            current_timepoint=180,
            actor_authority_hash=BORROWER_AUTHORITY,
            outputs=(
                output("closed_agreement", BORROWER_AUTHORITY, AGREEMENT_OCCUPIED_CAPACITY, 0),
                output(
                    "lender_repayment",
                    LENDER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT - CKB,
                ),
                output(
                    "borrower_collateral_return",
                    BORROWER_AUTHORITY,
                    PAYOUT_OCCUPIED_CAPACITY,
                    COLLATERAL_AMOUNT,
                ),
                output("receipt", BORROWER_AUTHORITY, RECEIPT_OCCUPIED_CAPACITY, 0),
            ),
            note="The lender repayment output must equal principal plus fixed fee.",
        ),
    )


def load_fixture_expectations(fixtures_dir: Path) -> dict[str, str]:
    expectations: dict[str, str] = {}
    for path in sorted(fixtures_dir.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        fixture = data["fixture"]
        expected = data["expected"]
        if expected not in ("accepted", "rejected"):
            raise ValueError(f"{path} has unsupported expected value {expected!r}")
        expectations[fixture] = expected
    return expectations


def find_output(case: HarnessCase, role: str) -> OutputShape | None:
    for candidate in case.outputs:
        if candidate.role == role:
            return candidate
    return None


def require_output(case: HarnessCase, role: str, failures: list[str]) -> OutputShape | None:
    candidate = find_output(case, role)
    if candidate is None:
        failures.append(f"missing output role {role}")
    return candidate


def require_economic_value(
    case: HarnessCase,
    role: str,
    expected_value: int,
    failures: list[str],
) -> None:
    candidate = require_output(case, role, failures)
    if candidate is None:
        return
    if candidate.economic_value_shannons != expected_value:
        failures.append(
            "output role "
            + role
            + " economic value "
            + str(candidate.economic_value_shannons)
            + " != expected "
            + str(expected_value)
        )


def protocol_input_capacity(case: HarnessCase) -> int:
    if case.action == "originate_agreement":
        return 0
    return AGREEMENT_OCCUPIED_CAPACITY + COLLATERAL_AMOUNT


def evaluate_case(case: HarnessCase, expected: str) -> dict[str, Any]:
    failures: list[str] = []

    for candidate in case.outputs:
        if candidate.capacity_shannons < candidate.occupied_capacity_shannons:
            failures.append(
                "output role "
                + candidate.role
                + " capacity "
                + str(candidate.capacity_shannons)
                + " below occupied capacity "
                + str(candidate.occupied_capacity_shannons)
            )

    if case.action == "originate_agreement":
        if case.current_timepoint < START_TIMEPOINT:
            failures.append("current_timepoint before start_timepoint")
        if case.current_timepoint > EXPIRY_TIMEPOINT:
            failures.append("current_timepoint after expiry_timepoint")
        if case.actor_authority_hash != BORROWER_AUTHORITY:
            failures.append("originator is not borrower")
        require_economic_value(case, "agreement_collateral", COLLATERAL_AMOUNT, failures)
        require_economic_value(case, "borrower_principal_payout", PRINCIPAL_AMOUNT, failures)
        require_economic_value(case, "receipt", 0, failures)
    elif case.action == "repay_before_expiry":
        if case.current_timepoint > EXPIRY_TIMEPOINT:
            failures.append("current_timepoint after expiry_timepoint")
        if case.actor_authority_hash != BORROWER_AUTHORITY:
            failures.append("actor is not borrower")
        require_economic_value(case, "closed_agreement", 0, failures)
        require_economic_value(case, "lender_repayment", PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT, failures)
        require_economic_value(case, "borrower_collateral_return", COLLATERAL_AMOUNT, failures)
        require_economic_value(case, "receipt", 0, failures)
    elif case.action == "claim_after_expiry":
        if case.current_timepoint <= EXPIRY_TIMEPOINT:
            failures.append("current_timepoint not after expiry_timepoint")
        if case.actor_authority_hash != LENDER_AUTHORITY:
            failures.append("actor is not lender")
        require_economic_value(case, "closed_agreement", 0, failures)
        require_economic_value(case, "lender_default_claim", COLLATERAL_AMOUNT, failures)
        require_economic_value(case, "receipt", 0, failures)
    else:
        failures.append("unsupported action " + case.action)

    total_output_capacity = sum(candidate.capacity_shannons for candidate in case.outputs)
    protocol_capacity = protocol_input_capacity(case)
    min_additional_input_capacity = max(
        0,
        total_output_capacity + BUILDER_FEE_SHANNONS - protocol_capacity,
    )

    accepted = not failures
    expected_accepted = expected == "accepted"
    return {
        "fixture": case.fixture,
        "action": case.action,
        "expected": expected,
        "accepted": accepted,
        "matched_expected": accepted == expected_accepted,
        "current_timepoint": case.current_timepoint,
        "actor_authority_hash": case.actor_authority_hash,
        "outputs": [candidate.to_json() for candidate in case.outputs],
        "total_output_capacity_shannons": total_output_capacity,
        "protocol_input_capacity_shannons": protocol_capacity,
        "builder_min_additional_input_capacity_shannons": min_additional_input_capacity,
        "failures": failures,
        "note": case.note,
    }


def build_report(fixtures_dir: Path) -> dict[str, Any]:
    expectations = load_fixture_expectations(fixtures_dir)
    cases = canonical_cases()

    missing = sorted(case.fixture for case in cases if case.fixture not in expectations)
    if missing:
        raise ValueError("missing fixture files for cases: " + ", ".join(missing))

    covered_fixture_names = {case.fixture for case in cases}
    unexecuted_fixture_names = sorted(set(expectations) - covered_fixture_names)

    results = [evaluate_case(case, expectations[case.fixture]) for case in cases]
    mismatches = [case["fixture"] for case in results if not case["matched_expected"]]
    accepted = [case for case in results if case["accepted"]]
    rejected = [case for case in results if not case["accepted"]]

    return {
        "schema": "novaseal-agreement-tx-shape-report-v0.1",
        "package": "novaseal-agreement-profile-v0 0.0.1",
        "classification": "local-transaction-shape-evidence",
        "generated_by": "scripts/nova_agreement_tx_shape_harness.py",
        "constants": {
            "ckb_shannons": CKB,
            "agreement_occupied_capacity_shannons": AGREEMENT_OCCUPIED_CAPACITY,
            "receipt_occupied_capacity_shannons": RECEIPT_OCCUPIED_CAPACITY,
            "payout_occupied_capacity_shannons": PAYOUT_OCCUPIED_CAPACITY,
            "builder_fee_shannons": BUILDER_FEE_SHANNONS,
        },
        "canonical_terms": {
            "collateral_amount_shannons": COLLATERAL_AMOUNT,
            "principal_amount_shannons": PRINCIPAL_AMOUNT,
            "fixed_fee_amount_shannons": FIXED_FEE_AMOUNT,
            "start_timepoint": START_TIMEPOINT,
            "expiry_timepoint": EXPIRY_TIMEPOINT,
            "borrower_authority_hash": BORROWER_AUTHORITY,
            "lender_authority_hash": LENDER_AUTHORITY,
        },
        "summary": {
            "total_cases": len(results),
            "accepted_cases": len(accepted),
            "rejected_cases": len(rejected),
            "matched_expected_cases": len(results) - len(mismatches),
            "mismatched_expected_cases": len(mismatches),
            "capacity_shape_checks_exercised": True,
            "settlement_amount_checks_exercised": True,
            "time_guards_exercised": True,
            "party_guards_exercised": True,
            "covered_fixture_names": sorted(covered_fixture_names),
            "unexecuted_fixture_names": unexecuted_fixture_names,
        },
        "cases": results,
        "limits": [
            "Does not execute generated CellScript in CKB VM.",
            "Does not call ckb-verification.",
            "Does not prove live-chain RPC, deployment, mempool, or miner acceptance.",
            "Does not check typed payout, terms_hash, or receipt_hash output bindings; those are covered by the resolved transaction harness.",
            "Cryptographic borrower/lender authority locks are not implemented in this profile slice.",
            "Native CKB settlement capacity/value shape is checked here as local builder evidence.",
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures-dir", type=Path, default=FIXTURES_DIR)
    parser.add_argument("--out", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--pretty", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report = build_report(args.fixtures_dir)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    summary = report["summary"]
    print(
        "wrote "
        + str(args.out)
        + " total="
        + str(summary["total_cases"])
        + " matched="
        + str(summary["matched_expected_cases"])
        + " mismatched="
        + str(summary["mismatched_expected_cases"])
    )
    return 0 if summary["mismatched_expected_cases"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
