#!/usr/bin/env python3
"""Matrix-driven CellScript syntax-combination audit runner.

The runner is intentionally token-light: stdout prints a compact summary and the
full command outputs/artifacts stay under target/syntax-combo-audit/<run>.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import textwrap
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "tests" / "syntax_combo" / "matrix.toml"
SEEDS = ROOT / "tests" / "syntax_combo" / "seeds"


@dataclass(frozen=True)
class Expected:
    phase: str
    contains: tuple[str, ...] = ()


@dataclass(frozen=True)
class Oracle:
    action: str | None = None
    consume_bindings: tuple[str, ...] = ()
    create_bindings: tuple[str, ...] = ()
    locked_outputs: tuple[str, ...] = ()
    create_fields: dict[str, tuple[str, ...]] = field(default_factory=dict)
    obligation_contains: tuple[str, ...] = ()


@dataclass(frozen=True)
class AuditCase:
    name: str
    source: str
    expected: Expected
    oracle: Oracle = field(default_factory=Oracle)
    origin: str = "generated"

    @property
    def case_id(self) -> str:
        digest = hashlib.blake2b(
            f"{self.name}\n{self.source}".encode("utf-8"),
            digest_size=6,
        ).hexdigest()
        return digest


def read_matrix() -> dict[str, Any]:
    if not MATRIX.exists():
        return {}
    return tomllib.loads(MATRIX.read_text(encoding="utf-8"))


def compact(text: str, limit: int = 1200) -> str:
    text = text.replace(str(ROOT), "$ROOT")
    if len(text) <= limit:
        return text
    return text[:limit] + "\n...<truncated>..."


def run_cmd(cmd: list[str], *, timeout: int = 30) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )


def require_tool(path_or_name: str) -> str:
    if "/" in path_or_name:
        path = Path(path_or_name)
        if path.exists() and os.access(path, os.X_OK):
            return str(path)
    resolved = shutil.which(path_or_name)
    if not resolved:
        raise SystemExit(f"missing required tool: {path_or_name}")
    return resolved


def cellc_bin() -> str:
    env = os.environ.get("CELLC_BIN")
    if env:
        return require_tool(env)
    target_dir_env = os.environ.get("CARGO_TARGET_DIR")
    target_dir = Path(target_dir_env) if target_dir_env else ROOT / "target"
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    candidate = target_dir / "debug" / "cellc"
    if candidate.exists() and os.access(candidate, os.X_OK):
        return str(candidate)
    build = run_cmd(["cargo", "build", "--locked", "--bin", "cellc"], timeout=120)
    if build.returncode != 0:
        raise SystemExit(compact(build.stdout, 4000))
    return str(candidate)


BASE_TYPES = """\
module cellscript::audit::{module_name}

resource Coin has store, create, consume, replace, burn, relock {{
    amount: u64,
    nonce: u64,
}}

receipt Voucher -> Coin has create, consume, burn {{
    amount: u64,
    nonce: u64,
    holder: Address,
}}

resource Wallet has store, create, consume, replace, burn, relock {{
    owner: Address,
}}
"""


def module_source(module_name: str, body: str) -> str:
    return BASE_TYPES.format(module_name=module_name) + "\n" + textwrap.dedent(body).strip() + "\n"


def matrix_cases(include_deep: bool) -> list[AuditCase]:
    cases: list[AuditCase] = []

    helper_specs = [
        ("preserve_type", "std::cell::preserve_type", ()),
        ("same_lock", "std::cell::same_lock", ("cell-metadata-equality:lock_hash",)),
        ("preserve_lock", "std::cell::preserve_lock", ("cell-metadata-equality:lock_hash",)),
        ("preserve_capacity", "std::cell::preserve_capacity", ("cell-metadata-equality:capacity",)),
        ("conserved", "std::accounting::conserved", ()),
    ]
    for short_name, helper, obligations in helper_specs:
        action = f"matrix_{short_name}"
        cases.append(
            AuditCase(
                name=f"matrix-cell-helper-{short_name}",
                source=module_source(
                    f"matrix_cell_helper_{short_name}",
                    f"""
                    action {action}(coin_before: Coin) -> coin_after: Coin {{
                        verification
                        {helper}(coin_after, coin_before)
                    }}
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(action=action, obligation_contains=obligations),
                origin="matrix:continuity/std-cell",
            )
        )

    cases.extend(
        [
            AuditCase(
                name="matrix-explicit-transfer-branch-require",
                source=module_source(
                    "matrix_explicit_transfer_branch_require",
                    """
                    action branch_keep(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        consume coin

                        create next_coin = Coin {
                            amount: coin.amount,
                            nonce: coin.nonce
                        } with_lock(to)

                        if next_coin.amount == coin.amount {
                            require next_coin.nonce == coin.nonce
                        } else {
                            require next_coin.nonce == coin.nonce
                        }
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="branch_keep",
                    consume_bindings=("coin",),
                    create_bindings=("next_coin",),
                    locked_outputs=("next_coin",),
                    create_fields={"next_coin": ("amount", "nonce")},
                ),
                origin="matrix:lifecycle/proof/control-flow",
            ),
            AuditCase(
                name="matrix-explicit-transfer-let-proof",
                source=module_source(
                    "matrix_explicit_transfer_let_proof",
                    """
                    action let_keep(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        consume coin

                        create next_coin = Coin {
                            amount: coin.amount,
                            nonce: coin.nonce
                        } with_lock(to)

                        let same_amount = next_coin.amount == coin.amount
                        require same_amount
                        require next_coin.nonce == coin.nonce
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="let_keep",
                    consume_bindings=("coin",),
                    create_bindings=("next_coin",),
                    locked_outputs=("next_coin",),
                    create_fields={"next_coin": ("amount", "nonce")},
                ),
                origin="matrix:lifecycle/proof/local-binding",
            ),
            AuditCase(
                name="matrix-stdlib-transfer-require-block",
                source=module_source(
                    "matrix_stdlib_transfer_require_block",
                    """
                    action transfer_with_block(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        std::lifecycle::transfer(coin, next_coin, to) {
                            amount
                            nonce
                        }

                        require {
                            next_coin.amount == coin.amount
                            next_coin.nonce == coin.nonce
                        }
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="transfer_with_block",
                    consume_bindings=("coin",),
                    create_bindings=("next_coin",),
                    locked_outputs=("next_coin",),
                    create_fields={"next_coin": ("amount", "nonce")},
                    obligation_contains=("create-output-lock", "consume-input:Coin:coin"),
                ),
                origin="matrix:stdlib-lifecycle/proof",
            ),
            AuditCase(
                name="matrix-stdlib-transfer-lock-capacity",
                source=module_source(
                    "matrix_stdlib_transfer_lock_capacity",
                    """
                    action transfer_with_metadata(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        std::lifecycle::transfer(coin, next_coin, to) {
                            amount
                            nonce
                        }
                        std::cell::preserve_lock(next_coin, coin)
                        std::cell::preserve_capacity(next_coin, coin)
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="transfer_with_metadata",
                    consume_bindings=("coin",),
                    create_bindings=("next_coin",),
                    locked_outputs=("next_coin",),
                    create_fields={"next_coin": ("amount", "nonce")},
                    obligation_contains=("cell-metadata-equality:lock_hash", "cell-metadata-equality:capacity"),
                ),
                origin="matrix:stdlib-lifecycle/metadata",
            ),
            AuditCase(
                name="matrix-stdlib-claim-require-block",
                source=module_source(
                    "matrix_stdlib_claim_require_block",
                    """
                    action claim_with_block(voucher: Voucher) -> coin: Coin {
                        verification
                        std::receipt::claim(voucher, coin, voucher.holder) {
                            amount
                            nonce
                        }

                        require {
                            coin.amount == voucher.amount
                            coin.nonce == voucher.nonce
                        }
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="claim_with_block",
                    consume_bindings=("voucher",),
                    create_bindings=("coin",),
                    locked_outputs=("coin",),
                    create_fields={"coin": ("amount", "nonce")},
                ),
                origin="matrix:receipt/proof",
            ),
            AuditCase(
                name="matrix-stdlib-settle-preserve-capacity",
                source=module_source(
                    "matrix_stdlib_settle_preserve_capacity",
                    """
                    action settle_with_capacity(voucher: Voucher) -> coin: Coin {
                        verification
                        std::lifecycle::settle(voucher, coin, voucher.holder) {
                            amount
                            nonce
                        }
                        std::cell::preserve_capacity(coin, voucher)
                    }
                    """,
                ),
                expected=Expected("accept"),
                oracle=Oracle(
                    action="settle_with_capacity",
                    consume_bindings=("voucher",),
                    create_bindings=("coin",),
                    locked_outputs=("coin",),
                    create_fields={"coin": ("amount", "nonce")},
                    obligation_contains=("cell-metadata-equality:capacity",),
                ),
                origin="matrix:receipt/metadata",
            ),
            AuditCase(
                name="matrix-lock-protected-only",
                source=module_source(
                    "matrix_lock_protected_only",
                    """
                    lock protected_wallet(protected wallet: Wallet) -> bool {
                        verification
                        require wallet.owner == wallet.owner
                    }
                    """,
                ),
                expected=Expected("accept"),
                origin="matrix:lock/source-qualifier",
            ),
            AuditCase(
                name="matrix-lock-witness-only",
                source=module_source(
                    "matrix_lock_witness_only",
                    """
                    lock witness_owner(witness owner: Address) -> bool {
                        verification
                        require owner == owner
                    }
                    """,
                ),
                expected=Expected("accept"),
                origin="matrix:lock/source-qualifier",
            ),
            AuditCase(
                name="matrix-lock-args-only",
                source=module_source(
                    "matrix_lock_args_only",
                    """
                    lock args_owner(lock_args owner: Address) -> bool {
                        verification
                        require owner == owner
                    }
                    """,
                ),
                expected=Expected("accept"),
                origin="matrix:lock/source-qualifier",
            ),
            AuditCase(
                name="matrix-reject-require-block-assignment",
                source=module_source(
                    "matrix_reject_require_block_assignment",
                    """
                    action hidden_mutation(flag: bool) {
                        verification
                        let mut ok = flag
                        require {
                            ok = false
                        }
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("require block", "assignment")),
                origin="matrix:reject/proof-purity",
            ),
            AuditCase(
                name="matrix-reject-claim-non-receipt",
                source=module_source(
                    "matrix_reject_claim_non_receipt",
                    """
                    action bad_claim(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        std::receipt::claim(coin, next_coin, to) {
                            amount
                            nonce
                        }
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("claim requires a receipt",)),
                origin="matrix:reject/stdlib-lifecycle",
            ),
            AuditCase(
                name="matrix-reject-claim-extra-args",
                source=module_source(
                    "matrix_reject_claim_extra_args",
                    """
                    action bad_claim(voucher: Voucher) -> coin: Coin {
                        verification
                        std::receipt::claim(voucher, coin, voucher.holder, voucher.holder) {
                            amount
                            nonce
                        }
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("claim expects 3 arguments",)),
                origin="matrix:reject/stdlib-lifecycle",
            ),
            AuditCase(
                name="matrix-reject-transfer-extra-args",
                source=module_source(
                    "matrix_reject_transfer_extra_args",
                    """
                    action bad_transfer(coin: Coin, to: Address) -> next_coin: Coin {
                        verification
                        std::lifecycle::transfer(coin, next_coin, to, to) {
                            amount
                            nonce
                        }
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("transfer expects 3 arguments",)),
                origin="matrix:reject/stdlib-lifecycle",
            ),
            AuditCase(
                name="matrix-reject-settle-missing-args",
                source=module_source(
                    "matrix_reject_settle_missing_args",
                    """
                    action bad_settle(voucher: Voucher) -> coin: Coin {
                        verification
                        std::lifecycle::settle(voucher, coin) {
                            amount
                            nonce
                        }
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("settle expects 3 arguments",)),
                origin="matrix:reject/stdlib-lifecycle",
            ),
            AuditCase(
                name="matrix-reject-cell-metadata-non-cell",
                source=module_source(
                    "matrix_reject_cell_metadata_non_cell",
                    """
                    action bad_metadata(amount: u64) -> out: Coin {
                        verification
                        std::cell::preserve_capacity(out, amount)
                    }
                    """,
                ),
                expected=Expected("reject_compile", ("preserve_capacity input must be a cell-backed value",)),
                origin="matrix:reject/metadata",
            ),
        ]
    )

    if include_deep:
        cases.extend(
            [
                AuditCase(
                    name="matrix-deep-reject-transfer-read-param",
                    source=module_source(
                        "matrix_deep_reject_transfer_read_param",
                        """
                        action bad_transfer(read coin: Coin, to: Address) -> next_coin: Coin {
                            verification
                            std::lifecycle::transfer(coin, next_coin, to) {
                                amount
                                nonce
                            }
                        }
                        """,
                    ),
                    expected=Expected("reject_compile", ("cell-backed linear",)),
                    origin="matrix:deep/reject/source-qualifier",
                ),
                AuditCase(
                    name="matrix-deep-reject-require-block-transfer",
                    source=module_source(
                        "matrix_deep_reject_require_block_transfer",
                        """
                        action hidden_transfer(coin: Coin, to: Address) -> next_coin: Coin {
                            verification
                            require {
                                std::lifecycle::transfer(coin, next_coin, to) {
                                    amount
                                    nonce
                                }
                            }
                        }
                        """,
                    ),
                    expected=Expected("reject_compile", ("require block", "verifier-boundary syntax")),
                    origin="matrix:deep/reject/proof-purity",
                ),
                AuditCase(
                    name="matrix-deep-reject-unknown-accounting",
                    source=module_source(
                        "matrix_deep_reject_unknown_accounting",
                        """
                        action bad_accounting(coin_before: Coin) -> coin_after: Coin {
                            verification
                            std::accounting::minted(coin_after, coin_before)
                        }
                        """,
                    ),
                    expected=Expected("reject_compile", ("unknown stdlib pattern",)),
                    origin="matrix:deep/reject/stdlib-namespace",
                ),
            ]
        )

    return cases


def generated_cases() -> list[AuditCase]:
    cases: list[AuditCase] = [
        AuditCase(
            name="explicit-transfer",
            source=module_source(
                "explicit_transfer",
                """
                action transfer_coin(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    consume coin

                    create next_coin = Coin {
                        amount: coin.amount,
                        nonce: coin.nonce
                    } with_lock(to)
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="transfer_coin",
                consume_bindings=("coin",),
                create_bindings=("next_coin",),
                locked_outputs=("next_coin",),
                create_fields={"next_coin": ("amount", "nonce")},
                obligation_contains=("create-output-lock",),
            ),
        ),
        AuditCase(
            name="pure-require-block",
            source=module_source(
                "pure_require_block",
                """
                action keep_fields(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    consume coin

                    create next_coin = Coin {
                        amount: coin.amount,
                        nonce: coin.nonce
                    } with_lock(to)

                    require {
                        next_coin.amount == coin.amount
                        next_coin.nonce == coin.nonce
                    }
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="keep_fields",
                consume_bindings=("coin",),
                create_bindings=("next_coin",),
                locked_outputs=("next_coin",),
                create_fields={"next_coin": ("amount", "nonce")},
            ),
        ),
        AuditCase(
            name="preserve-sugar",
            source=module_source(
                "preserve_sugar",
                """
                action preserve_fields(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    consume coin

                    create next_coin = Coin {
                        amount: coin.amount,
                        nonce: coin.nonce
                    } with_lock(to)

                    preserve next_coin from coin {
                        amount
                        nonce
                    }
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="preserve_fields",
                consume_bindings=("coin",),
                create_bindings=("next_coin",),
                locked_outputs=("next_coin",),
                create_fields={"next_coin": ("amount", "nonce")},
            ),
        ),
        AuditCase(
            name="stdlib-transfer",
            source=module_source(
                "stdlib_transfer",
                """
                action transfer_coin(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    std::lifecycle::transfer(coin, next_coin, to) {
                        amount
                        nonce
                    }
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="transfer_coin",
                consume_bindings=("coin",),
                create_bindings=("next_coin",),
                locked_outputs=("next_coin",),
                create_fields={"next_coin": ("amount", "nonce")},
                obligation_contains=("create-output-lock", "consume-input:Coin:coin"),
            ),
        ),
        AuditCase(
            name="stdlib-claim",
            source=module_source(
                "stdlib_claim",
                """
                action claim_voucher(voucher: Voucher) -> coin: Coin {
                    verification
                    std::receipt::claim(voucher, coin, voucher.holder) {
                        amount
                        nonce
                    }
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="claim_voucher",
                consume_bindings=("voucher",),
                create_bindings=("coin",),
                locked_outputs=("coin",),
                create_fields={"coin": ("amount", "nonce")},
            ),
        ),
        AuditCase(
            name="stdlib-settle",
            source=module_source(
                "stdlib_settle",
                """
                action settle_voucher(voucher: Voucher) -> coin: Coin {
                    verification
                    std::lifecycle::settle(voucher, coin, voucher.holder) {
                        amount
                        nonce
                    }
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="settle_voucher",
                consume_bindings=("voucher",),
                create_bindings=("coin",),
                locked_outputs=("coin",),
                create_fields={"coin": ("amount", "nonce")},
            ),
        ),
        AuditCase(
            name="cell-metadata-helpers",
            source=module_source(
                "cell_metadata_helpers",
                """
                action preserve_boundary(coin_before: Coin) -> coin_after: Coin {
                    verification
                    std::cell::preserve_type(coin_after, coin_before)
                    std::cell::preserve_lock(coin_after, coin_before)
                    std::cell::preserve_capacity(coin_after, coin_before)
                    std::accounting::conserved(coin_after, coin_before)
                }
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action="preserve_boundary",
                obligation_contains=(
                    "cell-metadata-equality:lock_hash",
                    "cell-metadata-equality:capacity",
                ),
            ),
        ),
        AuditCase(
            name="lock-source-qualifiers",
            source=module_source(
                "lock_source_qualifiers",
                """
                lock owner_only(
                    protected wallet: Wallet,
                    lock_args owner: Address,
                    witness claimed_owner: Address
                ) -> bool {
                    verification
                    require wallet.owner == owner
                    require claimed_owner == owner
                }
                """,
            ),
            expected=Expected("accept"),
        ),
        AuditCase(
            name="reject-require-block-lifecycle",
            source=module_source(
                "reject_require_block_lifecycle",
                """
                action bad(voucher: Voucher) -> coin: Coin {
                    verification
                    require {
                        std::receipt::claim(voucher, coin, voucher.holder) {
                            amount
                            nonce
                        }
                    }
                }
                """,
            ),
            expected=Expected("reject_compile", ("require block", "verifier-boundary syntax")),
        ),
        AuditCase(
            name="reject-preserve-type-mismatch",
            source="""\
module cellscript::audit::reject_preserve_type_mismatch

resource Coin has store, create, consume, replace, burn, relock {
    amount: u64,
}

resource BadCoin has store, create, consume, replace, burn, relock {
    amount: bool,
}

action bad(coin: Coin) -> bad_coin: BadCoin {
    verification
    preserve bad_coin from coin {
        amount
    }
}
""",
            expected=Expected("reject_compile", ("type mismatch",)),
        ),
        AuditCase(
            name="reject-transfer-missing-field",
            source=module_source(
                "reject_transfer_missing_field",
                """
                action bad(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    std::lifecycle::transfer(coin, next_coin, to) {
                        amount
                    }
                }
                """,
            ),
            expected=Expected("reject_compile", ("missing nonce",)),
        ),
        AuditCase(
            name="reject-consume-read-param",
            source=module_source(
                "reject_consume_read_param",
                """
                action bad(read coin: Coin) {
                    verification
                    consume coin
                }
                """,
            ),
            expected=Expected("reject_compile", ("cell-backed linear",)),
        ),
        AuditCase(
            name="reject-unknown-stdlib",
            source=module_source(
                "reject_unknown_stdlib",
                """
                action bad(coin_before: Coin) -> coin_after: Coin {
                    verification
                    std::cell::teleport(coin_after, coin_before)
                }
                """,
            ),
            expected=Expected("reject_compile", ("unknown stdlib pattern",)),
        ),
        AuditCase(
            name="reject-claim-without-output-arrow",
            source="""\
module cellscript::audit::reject_claim_without_output_arrow

resource Coin has store, create, consume, replace, burn, relock {
    amount: u64,
    nonce: u64,
}

receipt Voucher has create, consume, burn {
    amount: u64,
    nonce: u64,
    holder: Address,
}

action bad(voucher: Voucher) -> coin: Coin {
    verification
    std::receipt::claim(voucher, coin, voucher.holder) {
        amount
        nonce
    }
}
""",
            expected=Expected("reject_compile", ("declare a claim output type",)),
        ),
        AuditCase(
            name="reject-legacy-transfer-capability-strict",
            source="""\
module cellscript::audit::reject_legacy_transfer_capability_strict

resource Coin has store, transfer {
    amount: u64,
    nonce: u64,
}

action bad(coin: Coin, to: Address) {
    verification
    transfer coin to to
}
""",
            expected=Expected("reject_compile", ("CS0150", "legacy capability 'transfer'", "replace + relock")),
        ),
    ]
    return cases


def seeded_deep_cases(seed: int) -> list[AuditCase]:
    rng = random.Random(seed)
    suffix = f"{seed & 0xffff_ffff:x}"
    field_order = ["amount", "nonce"]
    rng.shuffle(field_order)
    transfer_fields = "\n".join(f"                        {field}" for field in field_order)
    helper = rng.choice(
        [
            "std::cell::preserve_type",
            "std::cell::same_lock",
            "std::cell::preserve_lock",
            "std::cell::preserve_capacity",
        ]
    )
    reject = rng.choice(
        [
            (
                "require_block_lifecycle",
                """
                action seeded_reject_lifecycle_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    require {
                        std::lifecycle::transfer(coin, next_coin, to) {
                            amount
                            nonce
                        }
                    }
                }
                """,
                ("require block", "verifier-boundary syntax"),
            ),
            (
                "unknown_stdlib",
                """
                action seeded_reject_unknown_{suffix}(coin_before: Coin) -> coin_after: Coin {
                    verification
                    std::cell::teleport(coin_after, coin_before)
                }
                """,
                ("unknown stdlib pattern",),
            ),
            (
                "transfer_missing_field",
                """
                action seeded_reject_missing_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {
                    verification
                    std::lifecycle::transfer(coin, next_coin, to) {
                        amount
                    }
                }
                """,
                ("missing nonce",),
            ),
        ]
    )
    reject_name, reject_body, reject_tokens = reject
    return [
        AuditCase(
            name=f"seeded-deep-transfer-{suffix}",
            source=module_source(
                f"seeded_deep_transfer_{suffix}",
                f"""
                action seeded_transfer_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {{
                    verification
                    std::lifecycle::transfer(coin, next_coin, to) {{
{transfer_fields}
                    }}
                }}
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(
                action=f"seeded_transfer_{suffix}",
                consume_bindings=("coin",),
                create_bindings=("next_coin",),
                locked_outputs=("next_coin",),
                create_fields={"next_coin": tuple(field_order)},
                obligation_contains=("create-output-lock", "consume-input:Coin:coin"),
            ),
            origin="seeded:deep/stdlib-lifecycle",
        ),
        AuditCase(
            name=f"seeded-deep-cell-helper-{suffix}",
            source=module_source(
                f"seeded_deep_cell_helper_{suffix}",
                f"""
                action seeded_helper_{suffix}(coin_before: Coin) -> coin_after: Coin {{
                    verification
                    {helper}(coin_after, coin_before)
                }}
                """,
            ),
            expected=Expected("accept"),
            oracle=Oracle(action=f"seeded_helper_{suffix}"),
            origin="seeded:deep/cell-helper",
        ),
        AuditCase(
            name=f"seeded-deep-reject-{reject_name}-{suffix}",
            source=module_source(
                f"seeded_deep_reject_{reject_name}_{suffix}",
                reject_body.replace("{suffix}", suffix),
            ),
            expected=Expected("reject_compile", reject_tokens),
            origin="seeded:deep/reject",
        ),
    ]


def parse_seed(path: Path) -> AuditCase:
    text = path.read_text(encoding="utf-8")
    phase = "accept"
    contains: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped.startswith("// audit:"):
            continue
        payload = stripped.removeprefix("// audit:").strip()
        if "=" not in payload:
            continue
        key, value = [part.strip() for part in payload.split("=", 1)]
        if key == "phase":
            phase = value
        elif key == "contains":
            contains.append(value)
    return AuditCase(
        name=f"seed-{path.stem}",
        source=text,
        expected=Expected(phase, tuple(contains)),
        origin=str(path.relative_to(ROOT)),
    )


def load_cases(mode: str, budget: int | None, seed: int) -> list[AuditCase]:
    include_matrix = mode in {"ci", "deep", "repro"}
    include_deep = mode in {"deep", "repro"}
    cases = generated_cases()
    if include_matrix:
        cases.extend(matrix_cases(include_deep=include_deep))
    if include_deep:
        cases.extend(seeded_deep_cases(seed))

    seed_cases: list[AuditCase] = []
    if SEEDS.exists():
        seed_cases = [parse_seed(path) for path in sorted(SEEDS.glob("*.cell")) if path.is_file()]

    if mode == "quick":
        default_budget = read_matrix().get("mode", {}).get("quick", {}).get("budget", len(cases))
    elif mode == "ci":
        default_budget = read_matrix().get("mode", {}).get("ci", {}).get("budget", len(cases))
    else:
        default_budget = read_matrix().get("mode", {}).get("deep", {}).get("budget", len(cases))
    limit = budget or default_budget or len(cases)
    selected = cases[: min(limit, len(cases))]

    # Regression seeds are never dropped by a small generation budget.
    existing = {case.name for case in selected}
    for seed_case in seed_cases:
        if seed_case.name not in existing:
            selected.append(seed_case)
            existing.add(seed_case.name)
    return selected


def contract_failure(code: str, summary: str) -> dict[str, Any]:
    return {
        "case": "-",
        "name": "mode-contract",
        "origin": str(MATRIX.relative_to(ROOT)),
        "phase": "contract",
        "code": code,
        "summary": summary,
        "shrunk": "",
        "output": "",
    }


def validate_mode_contract(mode: str, report: dict[str, Any]) -> list[dict[str, Any]]:
    if mode == "repro":
        return []
    config = read_matrix().get("mode", {}).get(mode, {})
    failures: list[dict[str, Any]] = []
    numeric_contracts = [
        ("min_cases", "generated", "SCA-CONTRACT-CASES"),
        ("min_accept", "accepted", "SCA-CONTRACT-ACCEPT"),
        ("min_reject", "rejected", "SCA-CONTRACT-REJECT"),
    ]
    for config_key, report_key, code in numeric_contracts:
        expected = config.get(config_key)
        if expected is None:
            continue
        actual = report.get(report_key, 0)
        if actual < expected:
            failures.append(contract_failure(code, f"{mode} {report_key} floor {expected} not met; got {actual}"))

    origins = report.get("origins", {})
    missing_origins = [origin for origin in config.get("required_origins", []) if origin not in origins]
    if missing_origins:
        failures.append(contract_failure("SCA-CONTRACT-ORIGIN", f"{mode} missing required origins: {', '.join(missing_origins)}"))
    return failures


def failure(
    case: AuditCase,
    phase: str,
    code: str,
    summary: str,
    run_dir: Path,
    output: str = "",
) -> dict[str, Any]:
    shrink_dir = run_dir / "shrink"
    shrink_dir.mkdir(parents=True, exist_ok=True)
    shrink_path = shrink_dir / f"{case.case_id}.cell"
    compact_source = "\n".join(
        line for line in case.source.splitlines() if line.strip() and not line.strip().startswith("//")
    )
    shrink_path.write_text(compact_source + "\n", encoding="utf-8")
    return {
        "case": case.case_id,
        "name": case.name,
        "origin": case.origin,
        "phase": phase,
        "code": code,
        "summary": summary,
        "shrunk": str(shrink_path.relative_to(run_dir)),
        "output": compact(output),
    }


def output_matches(text: str, needles: tuple[str, ...]) -> bool:
    if not needles:
        return True
    lowered = text.lower()
    return all(needle.lower() in lowered for needle in needles)


def find_action(metadata: dict[str, Any], name: str) -> dict[str, Any] | None:
    for action in metadata.get("actions", []):
        if action.get("name") == name:
            return action
    return None


def validate_metadata(case: AuditCase, metadata_path: Path, run_dir: Path) -> list[dict[str, Any]]:
    failures: list[dict[str, Any]] = []
    try:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001 - report compact audit failure
        return [failure(case, "metadata", "SCA-META-JSON", f"metadata JSON decode failed: {exc}", run_dir)]

    required_keys = {"actions", "compiler_version", "constraints", "lowering", "runtime", "target_profile"}
    missing = sorted(required_keys - set(metadata))
    if missing:
        failures.append(failure(case, "metadata", "SCA-META-KEYS", f"metadata missing keys: {', '.join(missing)}", run_dir))

    target_profile = metadata.get("target_profile", {})
    if target_profile.get("name") != "ckb":
        failures.append(failure(case, "metadata", "SCA-META-PROFILE", "metadata target_profile.name is not ckb", run_dir))

    oracle = case.oracle
    if oracle.action:
        action = find_action(metadata, oracle.action)
        if action is None:
            failures.append(failure(case, "metadata", "SCA-META-ACTION", f"missing action metadata for {oracle.action}", run_dir))
            return failures

        consume_bindings = tuple(item.get("binding") for item in action.get("consume_set", []))
        if oracle.consume_bindings and consume_bindings != oracle.consume_bindings:
            failures.append(
                failure(
                    case,
                    "metadata",
                    "SCA-META-CONSUME",
                    f"consume bindings {consume_bindings!r} != {oracle.consume_bindings!r}",
                    run_dir,
                )
            )
        if len(consume_bindings) != len(set(consume_bindings)):
            failures.append(failure(case, "metadata", "SCA-META-DUP-CONSUME", "duplicate consume binding", run_dir))

        create_set = action.get("create_set", [])
        create_by_binding = {item.get("binding"): item for item in create_set}
        for binding in oracle.create_bindings:
            if binding not in create_by_binding:
                failures.append(failure(case, "metadata", "SCA-META-CREATE", f"missing create binding {binding}", run_dir))
        for binding in oracle.locked_outputs:
            if not create_by_binding.get(binding, {}).get("has_lock"):
                failures.append(failure(case, "metadata", "SCA-META-LOCK", f"create binding {binding} is not locked", run_dir))
        for binding, fields in oracle.create_fields.items():
            actual = tuple(create_by_binding.get(binding, {}).get("fields", []))
            if actual != fields:
                failures.append(
                    failure(
                        case,
                        "metadata",
                        "SCA-META-FIELDS",
                        f"create fields for {binding} {actual!r} != {fields!r}",
                        run_dir,
                    )
                )

        obligations_text = json.dumps(action.get("verifier_obligations", []), sort_keys=True)
        for needle in oracle.obligation_contains:
            if needle not in obligations_text:
                failures.append(
                    failure(
                        case,
                        "metadata",
                        "SCA-META-OBLIGATION",
                        f"missing obligation containing {needle!r}",
                        run_dir,
                    )
                )

        if action.get("fail_closed_runtime_features"):
            failures.append(
                failure(
                    case,
                    "metadata",
                    "SCA-META-FAIL-CLOSED",
                    "accepted audit case contains fail_closed_runtime_features",
                    run_dir,
                )
            )
    return failures


def audit_case(case: AuditCase, run_dir: Path, cellc: str) -> tuple[str, list[dict[str, Any]]]:
    case_path = run_dir / "cases" / f"{case.case_id}.cell"
    fmt_path = run_dir / "fmt" / f"{case.case_id}.cell"
    asm_path = run_dir / "asm" / f"{case.case_id}.s"
    meta_path = run_dir / "meta" / f"{case.case_id}.json"
    for path in [case_path.parent, fmt_path.parent, asm_path.parent, meta_path.parent]:
        path.mkdir(parents=True, exist_ok=True)
    case_path.write_text(case.source, encoding="utf-8")

    parse = run_cmd([cellc, "--parse", str(case_path)], timeout=20)
    if case.expected.phase == "reject_parse":
        if parse.returncode == 0:
            return "failed", [failure(case, "parse", "SCA-PARSE-ACCEPTED", "expected parse rejection, got success", run_dir, parse.stdout)]
        return "rejected", []
    if parse.returncode != 0:
        return "failed", [failure(case, "parse", "SCA-PARSE-FAILED", "unexpected parse failure", run_dir, parse.stdout)]

    if case.expected.phase == "accept":
        fmt_path.write_text(case.source, encoding="utf-8")
        fmt = run_cmd([cellc, "fmt", "--json", str(fmt_path)], timeout=20)
        if fmt.returncode != 0:
            return "failed", [failure(case, "fmt", "SCA-FMT-FAILED", "formatter failed", run_dir, fmt.stdout)]
        fmt_check = run_cmd([cellc, "fmt", "--check", "--json", str(fmt_path)], timeout=20)
        if fmt_check.returncode != 0:
            return "failed", [failure(case, "fmt", "SCA-FMT-NON-IDEMPOTENT", "formatted source is not idempotent", run_dir, fmt_check.stdout)]
        parse_fmt = run_cmd([cellc, "--parse", str(fmt_path)], timeout=20)
        if parse_fmt.returncode != 0:
            return "failed", [failure(case, "fmt", "SCA-FMT-PARSE", "formatted source does not parse", run_dir, parse_fmt.stdout)]

    compile_cmd = [
        cellc,
        str(case_path),
        "--target",
        "riscv64-asm",
        "--target-profile",
        "ckb",
        "--primitive-strict",
        "0.15",
        "-o",
        str(asm_path),
    ]
    compiled = run_cmd(compile_cmd, timeout=30)
    if case.expected.phase == "reject_compile":
        if compiled.returncode == 0:
            return "failed", [
                failure(case, "compile", "SCA-COMPILE-ACCEPTED", "expected compile rejection, got success", run_dir, compiled.stdout)
            ]
        if not output_matches(compiled.stdout, case.expected.contains):
            return "failed", [
                failure(
                    case,
                    "compile",
                    "SCA-COMPILE-DIAGNOSTIC",
                    f"compile diagnostic missing expected tokens {case.expected.contains!r}",
                    run_dir,
                    compiled.stdout,
                )
            ]
        return "rejected", []
    if compiled.returncode != 0:
        return "failed", [failure(case, "compile", "SCA-COMPILE-FAILED", "unexpected compile failure", run_dir, compiled.stdout)]

    if not asm_path.exists() or asm_path.stat().st_size == 0:
        return "failed", [failure(case, "codegen", "SCA-CODEGEN-EMPTY", "assembly output is missing or empty", run_dir, compiled.stdout)]
    asm_text = asm_path.read_text(encoding="utf-8", errors="replace")
    for obsolete in ("IrTransfer", "IrClaim", "IrSettle"):
        if obsolete in asm_text:
            return "failed", [failure(case, "codegen", "SCA-CODEGEN-OBSOLETE", f"assembly contains obsolete token {obsolete}", run_dir)]

    metadata = run_cmd(
        [
            cellc,
            "metadata",
            str(case_path),
            "--target",
            "riscv64-asm",
            "--target-profile",
            "ckb",
            "-o",
            str(meta_path),
        ],
        timeout=30,
    )
    if metadata.returncode != 0:
        return "failed", [failure(case, "metadata", "SCA-META-FAILED", "metadata command failed", run_dir, metadata.stdout)]
    meta_failures = validate_metadata(case, meta_path, run_dir)
    if meta_failures:
        return "failed", meta_failures
    return "accepted", []


def write_reports(run_dir: Path, report: dict[str, Any], failures: list[dict[str, Any]]) -> None:
    (run_dir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    with (run_dir / "report.jsonl").open("w", encoding="utf-8") as handle:
        for item in failures:
            handle.write(json.dumps(item, sort_keys=True) + "\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Run CellScript syntax-combination audit")
    parser.add_argument("mode", nargs="?", default="quick", choices=["quick", "ci", "deep", "repro"])
    parser.add_argument("--seed", type=int, default=20260503)
    parser.add_argument("--budget", type=int)
    parser.add_argument("--case", help="case name or id for repro mode")
    args = parser.parse_args(argv)

    require_tool("cargo")
    require_tool("python3")
    cellc = cellc_bin()

    timestamp = dt.datetime.now(dt.UTC).strftime("%Y%m%d-%H%M%S")
    run_dir = ROOT / "target" / "syntax-combo-audit" / f"{timestamp}-{args.mode}-{args.seed}"
    run_dir.mkdir(parents=True, exist_ok=True)

    cases = load_cases(args.mode, args.budget, args.seed)
    if args.mode == "repro":
        if not args.case:
            raise SystemExit("repro mode requires --case <name-or-id>")
        cases = [case for case in cases if case.name == args.case or case.case_id == args.case]
        if not cases:
            raise SystemExit(f"unknown repro case: {args.case}")

    failures: list[dict[str, Any]] = []
    accepted = 0
    rejected = 0
    phase_counts: dict[str, dict[str, int]] = {}
    origin_counts: dict[str, int] = {}

    for case in cases:
        origin_counts[case.origin] = origin_counts.get(case.origin, 0) + 1
        status, case_failures = audit_case(case, run_dir, cellc)
        expected_phase = case.expected.phase
        phase_counts.setdefault(expected_phase, {"passed": 0, "failed": 0})
        if case_failures:
            phase_counts[expected_phase]["failed"] += 1
            failures.extend(case_failures)
        else:
            phase_counts[expected_phase]["passed"] += 1
        if status == "accepted":
            accepted += 1
        elif status == "rejected":
            rejected += 1

    report = {
        "status": "passed" if not failures else "failed",
        "mode": args.mode,
        "seed": args.seed,
        "generated": len(cases),
        "accepted": accepted,
        "rejected": rejected,
        "failures_count": len(failures),
        "phases": phase_counts,
        "origins": origin_counts,
        "failures": failures[:10],
    }
    contract_failures = validate_mode_contract(args.mode, report)
    if contract_failures:
        failures.extend(contract_failures)
        report["status"] = "failed"
        report["failures_count"] = len(failures)
        report["failures"] = failures[:10]
    write_reports(run_dir, report, failures)

    print(
        "syntax-combo-audit: "
        f"{report['status']} seed={args.seed} mode={args.mode} "
        f"generated={len(cases)} accepted={accepted} rejected={rejected} failures={len(failures)}"
    )
    print(f"report={run_dir / 'report.json'}")
    if failures:
        print("top:")
        for item in failures[:5]:
            print(f"  {item['code']} {item['summary']} case={item['case']} phase={item['phase']}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
