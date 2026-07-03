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
RUN_ONCHAIN=1
RUN_STATEFUL_SCENARIOS="${RUN_STATEFUL_SCENARIOS:-0}"
KEEP_NODE_LOGS=1
ACCEPTANCE_MODE="production"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$REPO_ROOT/target/ckb-cellscript-acceptance/$RUN_ID"
CKB_DIR="$RUN_DIR/ckb-node"
CKB_LOG="$RUN_DIR/ckb.log"
REPORT_JSON="$RUN_DIR/ckb-cellscript-acceptance-report.json"
CKB_PID=""

usage() {
  cat <<'USAGE'
Usage: scripts/ckb_cellscript_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>] [--compile-only] [--stateful-scenarios] [--production|--bounded]

Runs CellScript CKB compatibility acceptance against a local CKB integration
devnet from the parent CKB repository. The default mode is the production gate:
it fails closed if any CKB coverage still depends on synthetic harnesses,
expected fail-closed entries, or non-original artifacts.

Options:
  --ckb-repo <path>   Parent CKB checkout. Defaults to ../ckb.
  --ckb-bin <path>    Existing CKB executable. Defaults to target/debug/ckb,
                      building `cargo build --bin ckb` in --ckb-repo if needed.
  --compile-only      Compile and verify the CKB-profile CellScript artifacts,
                      but skip local CKB node deployment/spend checks. This
                      mode does not require a CKB checkout or executable.
  --stateful-scenarios
                      After the production action/lock matrix, run additional
                      local CKB transactions that feed live outputs from one
                      action into the next action.
  --production        Enforce the production gate. This is the default.
  --bounded           Run the bounded development coverage matrix. This keeps
                      bounded harnesses visible, but it is not a
                      production-readiness claim.
  -h, --help          Show this help.
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
    --compile-only)
      RUN_ONCHAIN=0
      shift
      ;;
    --stateful-scenarios)
      RUN_STATEFUL_SCENARIOS=1
      shift
      ;;
    --production)
      ACCEPTANCE_MODE="production"
      shift
      ;;
    --bounded)
      ACCEPTANCE_MODE="bounded"
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

cleanup() {
  stop_ckb
  if [[ "$KEEP_NODE_LOGS" != "1" && -f "$CKB_LOG" ]]; then
    rm -f "$CKB_LOG"
  fi
}
trap cleanup EXIT

require_cmd cargo
require_cmd python3
if [[ "$RUN_ONCHAIN" == "1" ]]; then
  require_cmd curl
fi

mkdir -p "$RUN_DIR"

RPC_URL=""
if [[ "$RUN_ONCHAIN" == "1" ]]; then
  if [[ ! -d "$CKB_REPO" ]]; then
    echo "CKB repo does not exist: $CKB_REPO" >&2
    exit 1
  fi
  if [[ ! -f "$CKB_REPO/test/template/ckb.toml" ]]; then
    echo "CKB repo does not contain test/template/ckb.toml: $CKB_REPO" >&2
    exit 1
  fi

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
else
  if [[ -d "$CKB_REPO" ]]; then
    CKB_REPO="$(cd "$CKB_REPO" && pwd)"
  fi
  if [[ -n "$CKB_BIN" && -e "$CKB_BIN" ]]; then
    CKB_BIN="$(cd "$(dirname "$CKB_BIN")" && pwd)/$(basename "$CKB_BIN")"
  fi
fi

CELLC_BUILD_JSON="$RUN_DIR/cellc-build.jsonl"
if ! cargo build --locked --manifest-path "$REPO_ROOT/Cargo.toml" --bin cellc --message-format=json-render-diagnostics >"$CELLC_BUILD_JSON"; then
  cat "$CELLC_BUILD_JSON" >&2
  exit 1
fi
CELLC_BIN="$(python3 - "$CELLC_BUILD_JSON" <<'PY'
import json
import pathlib
import sys

for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        continue
    if message.get("reason") != "compiler-artifact":
        continue
    target = message.get("target") or {}
    if target.get("name") != "cellc" or "bin" not in target.get("kind", []):
        continue
    executable = message.get("executable")
    if executable:
        print(executable)
        break
PY
)"
if [[ -z "$CELLC_BIN" || ! -x "$CELLC_BIN" ]]; then
  cat "$CELLC_BUILD_JSON" >&2
  echo "cellc build finished but Cargo did not report an executable artifact" >&2
  exit 1
fi

python3 - "$CELLC_BIN" "$REPO_ROOT" "$RUN_DIR" "$REPORT_JSON" "$ACCEPTANCE_MODE" <<'PY'
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys

cellc = pathlib.Path(sys.argv[1])
repo_root = pathlib.Path(sys.argv[2])
run_dir = pathlib.Path(sys.argv[3])
report_path = pathlib.Path(sys.argv[4])
acceptance_mode = sys.argv[5]

SOURCE_PROVENANCE_SCHEMA = "cellscript-ckb-acceptance-source-provenance-v0.1"
BUILD_REPORT_SCHEMA = "cellscript-ckb-build-report-v0.20"
SOURCE_PROVENANCE_PATHS = [
    "Cargo.lock",
    "Cargo.toml",
    "src",
    "examples",
    "scripts/cellscript_gate.sh",
    "scripts/cellscript_ckb_release_gate.sh",
    "scripts/ckb_cellscript_acceptance.sh",
    "scripts/validate_ckb_cellscript_production_evidence.py",
]

EXAMPLES = [
    "amm_pool.cell",
    "launch.cell",
    "multisig.cell",
    "nft.cell",
    "timelock.cell",
    "token.cell",
    "vesting.cell",
]
NON_PRODUCTION_EXAMPLES = [
    # 0.13 bounded collection helper coverage. This is intentionally exercised
    # by broader CellScript tooling tests, not by the CKB production
    # bundled-contract matrix.
    "registry.cell",
    # 0.21 business-flow examples. These illustrate flow-edge validation,
    # state transitions, and cross-module composition for auditing and docs.
    # They are not part of the production bundled-contract deployment matrix.
    "atomic_swap.cell",
    "multi_phase_dao.cell",
]
LANGUAGE_EXAMPLES = [
    "canonical_style.cell",
    "order_book.cell",
    "registry.cell",
    "stdlib.cell",
    "v0_14_capacity_time.cell",
    "v0_14_ckb_type_id_create.cell",
    "v0_14_delegate_verify.cell",
    "v0_14_hash_blake2b.cell",
    "v0_14_multi_step_pipeline.cell",
    "v0_14_witness_source.cell",
    "v0_15_identity_lifecycle.cell",
    "v0_15_scoped_invariant.cell",
]
EXAMPLE_SCOPE = {
    "production_bundled_examples": EXAMPLES,
    "non_production_top_level_examples": NON_PRODUCTION_EXAMPLES,
    "non_production_language_examples": LANGUAGE_EXAMPLES,
    "production_scope_note": (
        "Only production_bundled_examples are deployed and action-exercised by this CKB production "
        "acceptance report. non_production_top_level_examples and non_production_language_examples are "
        "covered by compiler/tooling tests unless they are promoted into production_bundled_examples."
    ),
}
LOCK_ACCEPTANCE_SCOPE = {
    "strict_compile_only": True,
    "onchain_lock_spend_matrix": False,
    "pending_onchain_lock_spend_matrix": {
        "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
        "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
        "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "not_expired", "emergency_approved"],
        "vesting.cell": ["vesting_admin"],
    },
    "required_cases_per_lock_when_promoted": ["valid_spend", "invalid_spend"],
    "scope_note": (
        "Scoped lock entries are strict-compiled under the CKB profile and counted as strict lock coverage. "
        "They are not counted as builder-backed on-chain lock spend/deny-spend transactions."
    ),
}
LOCK_BEHAVIOR_ACCEPTANCE_SCOPE = {
    "strict_compile_only": False,
    "onchain_lock_spend_matrix": True,
    "onchain_lock_spend_matrix_scope": {
        "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
        "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
        "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "not_expired", "emergency_approved"],
        "vesting.cell": ["vesting_admin"],
    },
    "required_cases_per_lock": ["valid_spend", "invalid_spend"],
    "scope_note": (
        "Scoped lock entries are strict-compiled under the CKB profile and each lock is exercised "
        "through builder-backed local CKB valid-spend and invalid-spend transactions."
    ),
}
TRUNCATE = 12000
UNEXPECTED_PROFILE_TRAILER = bytes.fromhex("53504f5241424900")
ELF_ENTRY_ABI_SCHEMA = "cellscript-ckb-elf-entry-abi-v0.20"
ELF64_HEADER_SIZE = 64
ELF64_PROGRAM_HEADER_SIZE = 56
ELF_PT_LOAD = 1
ELF_PF_X = 1
ELF_PF_W = 2
ELF_PF_R = 4
ELF_EM_RISCV = 243
ENTRY_TRAMPOLINE_SIZE = 20
CRITICAL_0_20_DEVNET_EXAMPLES = ["launch.cell", "token.cell", "amm_pool.cell"]

examples_dir = repo_root / "examples"
language_examples_dir = examples_dir / "language"

def production_example_path(name):
    return examples_dir / name

def production_example_build_path(name):
    """Return the path to pass to cellc for building. Uses the workspace package directory when available."""
    pkg_dir = examples_dir / name.replace(".cell", "")
    if (pkg_dir / "Cell.toml").is_file():
        return pkg_dir
    return examples_dir / name

def language_example_path(name):
    source = language_examples_dir / name
    if source.is_file():
        return source
    return examples_dir / name

def language_example_build_path(name):
    """Return the path to pass to cellc for building. Uses the workspace package directory when available."""
    pkg_dir = examples_dir / "language"
    if (pkg_dir / "Cell.toml").is_file():
        return pkg_dir
    return language_example_path(name)

actual_flat_examples = sorted(
    path.name
    for path in examples_dir.glob("*.cell")
    if path.is_file() and path.name not in NON_PRODUCTION_EXAMPLES
)
if actual_flat_examples != sorted(EXAMPLES):
    raise SystemExit(f"canonical bundled examples changed: expected {sorted(EXAMPLES)}, found {actual_flat_examples}")
actual_non_production_examples = sorted(
    path.name
    for path in examples_dir.glob("*.cell")
    if path.is_file() and path.name in NON_PRODUCTION_EXAMPLES
)
if actual_non_production_examples != sorted(NON_PRODUCTION_EXAMPLES):
    raise SystemExit(
        f"non-production top-level examples changed: expected {sorted(NON_PRODUCTION_EXAMPLES)}, "
        f"found {actual_non_production_examples}"
    )
actual_language_examples = sorted(path.name for path in language_examples_dir.glob("*.cell") if path.is_file())
if actual_language_examples != sorted(LANGUAGE_EXAMPLES):
    raise SystemExit(f"language examples changed: expected {sorted(LANGUAGE_EXAMPLES)}, found {actual_language_examples}")
for stale_dir in ("business", "acceptance"):
    stale_path = examples_dir / stale_dir
    if stale_path.exists():
        raise SystemExit(f"stale checked-in example mirror directory must be removed: {stale_path.relative_to(repo_root)}")
for name in NON_PRODUCTION_EXAMPLES:
    if not (examples_dir / name).is_file():
        raise SystemExit(f"missing non-production top-level example: {name}")
for name in LANGUAGE_EXAMPLES:
    if not (language_examples_dir / name).is_file():
        raise SystemExit(f"missing non-production language example: {name}")

source_root = run_dir / "generated-sources"
baseline_source_root = source_root / "baseline"
token_action_source_root = source_root / "token-actions"
nft_action_source_root = source_root / "nft-actions"
timelock_action_source_root = source_root / "timelock-actions"
amm_action_source_root = source_root / "amm-actions"
multisig_action_source_root = source_root / "multisig-actions"
launch_action_source_root = source_root / "launch-actions"
artifact_root = run_dir / "artifacts"
strict_root = run_dir / "strict-original-ckb"
for path in (
    baseline_source_root,
    token_action_source_root,
    nft_action_source_root,
    timelock_action_source_root,
    amm_action_source_root,
    multisig_action_source_root,
    launch_action_source_root,
    artifact_root,
    strict_root,
):
    path.mkdir(parents=True, exist_ok=True)

baseline_source = baseline_source_root / "ckb_noop.cell"
baseline_source.write_text(
    """module acceptance::ckb_noop

action main() -> u64 {
    verification
    0
}
""",
    encoding="utf-8",
)

TOKEN_TYPES_SOURCE = """resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

resource MintAuthority has store, create, replace {
    token_symbol: [u8; 8]
    max_supply: u64
    minted: u64
}
"""

TOKEN_ACTION_SOURCES = {
    "mint_with_authority": """
action mint_with_authority(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token) {
    verification
    require auth_before.minted + amount <= auth_before.max_supply

    require auth_after.token_symbol == auth_before.token_symbol
    require auth_after.max_supply == auth_before.max_supply
    require auth_after.minted == auth_before.minted + amount

    create token = Token {
        amount: amount,
        symbol: auth_before.token_symbol
    } with_lock(to)
}
""",
    "transfer_token": """
action transfer_token(token: Token, to: Address) -> next_token: Token {
    verification
    consume token
    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
}
""",
    "burn": """
action burn(token: Token) {
    verification
    require token.amount > 0
    destroy token
}
""",
    "merge": """
action merge(a: Token, b: Token, to: Address) -> merged: Token {
    verification
    require a.symbol == b.symbol
    let total = a.amount + b.amount
    consume a
    consume b

    create merged = Token {
        amount: total,
        symbol: a.symbol
    } with_lock(to)
}
""",
}

for action, source in TOKEN_ACTION_SOURCES.items():
    (token_action_source_root / f"token_{action}.cell").write_text(
        f"module acceptance::token_{action}\n\n" + TOKEN_TYPES_SOURCE + "\n" + source,
        encoding="utf-8",
    )

NFT_TYPES_SOURCE = """resource NFT has store, create, consume, replace, burn, relock, read_ref {
    token_id: u64
    owner: Address
    metadata_hash: Hash
    royalty_recipient: Address
    royalty_bps: u16
}

resource Collection has store, create, replace {
    creator: Address
    total_supply: u64
    max_supply: u64
}

receipt Listing has create, consume, burn {
    token_id: u64
    seller: Address
    price: u64
    created_at: u64
}

receipt Offer has create, consume, burn {
    token_id: u64
    buyer: Address
    price: u64
    expires_at: u64
}

receipt RoyaltyPayment has create {
    token_id: u64
    recipient: Address
    amount: u64
}
"""

NFT_ACTION_SOURCES = {
    "create_collection": """
action create_collection(creator: Address, max_supply: u64) -> collection: Collection {
    verification
    require max_supply > 0, "max supply must be positive"
    require max_supply <= 10000, "max supply too high"

    create collection = Collection {
        creator: creator,
        total_supply: 0,
        max_supply: max_supply
    } with_lock(creator)
}
""",
    "mint": """
action mint(collection_before: Collection, to: Address, metadata_hash: Hash) -> (collection_after: Collection, nft: NFT) {
    verification
    require collection_before.total_supply < collection_before.max_supply
    let token_id = collection_before.total_supply + 1

    require collection_after.creator == collection_before.creator
    require collection_after.max_supply == collection_before.max_supply
    require collection_after.total_supply == token_id

    create nft = NFT {
        token_id: token_id,
        owner: to,
        metadata_hash: metadata_hash,
        royalty_recipient: collection_before.creator,
        royalty_bps: 250
    }
}
""",
    "transfer": """
action transfer(nft_before: NFT, to: Address) -> nft_after: NFT {
    verification
    require nft_before.owner != to
    require nft_after.token_id == nft_before.token_id
    require nft_after.owner == to
    require nft_after.metadata_hash == nft_before.metadata_hash
    require nft_after.royalty_recipient == nft_before.royalty_recipient
    require nft_after.royalty_bps == nft_before.royalty_bps
}
""",
    "create_listing": """
action create_listing(read nft: NFT, price: u64, current_time: u64) -> listing: Listing {
    verification
    require price > 0
    create listing = Listing {
        token_id: nft.token_id,
        seller: nft.owner,
        price: price,
        created_at: current_time
    }
}
""",
    "cancel_listing": """
action cancel_listing(listing: Listing) {
    verification
    destroy listing
}
""",
    "buy_from_listing": """
action buy_from_listing(nft_before: NFT, listing: Listing, buyer: Address, seller: Address, payment: u64) -> (nft_after: NFT, royalty_payment: RoyaltyPayment, seller_payment: RoyaltyPayment) {
    verification
    require payment >= listing.price

    let royalty_amount = payment * nft_before.royalty_bps / 10000
    let seller_amount = payment - royalty_amount

    require nft_after.token_id == nft_before.token_id
    require nft_after.owner == buyer
    require nft_after.metadata_hash == nft_before.metadata_hash
    require nft_after.royalty_recipient == nft_before.royalty_recipient
    require nft_after.royalty_bps == nft_before.royalty_bps

    destroy listing

    create royalty_payment = RoyaltyPayment {
        token_id: nft_before.token_id,
        recipient: nft_before.royalty_recipient,
        amount: royalty_amount
    }

    create seller_payment = RoyaltyPayment {
        token_id: nft_before.token_id,
        recipient: seller,
        amount: seller_amount
    }
}
""",
    "create_offer": """
action create_offer(token_id: u64, buyer: Address, price: u64, expires_at: u64) -> offer: Offer {
    verification
    require price > 0
    require expires_at > 0
    create offer = Offer {
        token_id: token_id,
        buyer: buyer,
        price: price,
        expires_at: expires_at
    }
}
""",
    "accept_offer": """
action accept_offer(nft_before: NFT, offer: Offer, buyer: Address, seller: Address, price: u64, current_time: u64) -> (nft_after: NFT, royalty_payment: RoyaltyPayment, seller_payment: RoyaltyPayment) {
    verification
    require current_time < offer.expires_at

    let royalty_amount = price * nft_before.royalty_bps / 10000
    let seller_amount = price - royalty_amount

    require nft_after.token_id == nft_before.token_id
    require nft_after.owner == buyer
    require nft_after.metadata_hash == nft_before.metadata_hash
    require nft_after.royalty_recipient == nft_before.royalty_recipient
    require nft_after.royalty_bps == nft_before.royalty_bps

    destroy offer

    create royalty_payment = RoyaltyPayment {
        token_id: nft_before.token_id,
        recipient: nft_before.royalty_recipient,
        amount: royalty_amount
    }

    create seller_payment = RoyaltyPayment {
        token_id: nft_before.token_id,
        recipient: seller,
        amount: seller_amount
    }
}
""",
    "burn": """
action burn(nft: NFT) {
    verification
    destroy nft
}
""",
    "batch_mint": """
action batch_mint(
    collection_before: Collection,
    recipients: [Address; 4],
    metadata_hashes: [Hash; 4],
) -> (collection_after: Collection, nft0: NFT, nft1: NFT, nft2: NFT, nft3: NFT) {
    verification
    require collection_before.total_supply + 4 <= collection_before.max_supply
    let first_token_id = collection_before.total_supply + 1

    require collection_after.creator == collection_before.creator
    require collection_after.max_supply == collection_before.max_supply
    require collection_after.total_supply == collection_before.total_supply + 4

    create nft0 = NFT {
        token_id: first_token_id,
        owner: recipients[0],
        metadata_hash: metadata_hashes[0],
        royalty_recipient: collection_before.creator,
        royalty_bps: 250
    }
    create nft1 = NFT {
        token_id: first_token_id + 1,
        owner: recipients[1],
        metadata_hash: metadata_hashes[1],
        royalty_recipient: collection_before.creator,
        royalty_bps: 250
    }
    create nft2 = NFT {
        token_id: first_token_id + 2,
        owner: recipients[2],
        metadata_hash: metadata_hashes[2],
        royalty_recipient: collection_before.creator,
        royalty_bps: 250
    }
    create nft3 = NFT {
        token_id: first_token_id + 3,
        owner: recipients[3],
        metadata_hash: metadata_hashes[3],
        royalty_recipient: collection_before.creator,
        royalty_bps: 250
    }
}
""",
}

for action, source in NFT_ACTION_SOURCES.items():
    (nft_action_source_root / f"nft_{action}.cell").write_text(
        f"module acceptance::nft_{action}\n\n" + NFT_TYPES_SOURCE + "\n" + source,
        encoding="utf-8",
    )

TIMELOCK_TYPES_SOURCE = """resource TimeLock has store, create, consume, replace, burn, read_ref {
    owner: Address
    lock_type: u8
    unlock_height: u64
    created_at: u64
}

resource LockedAsset has store, create, consume, burn {
    amount: u64
    lock_hash: Hash
}

receipt ReleaseRequest has create, consume, burn {
    lock_hash: Hash
    requester: Address
    requested_at: u64
}

receipt EmergencyRelease has create, consume, replace, burn {
    lock_hash: Hash
    requester: Address
    requested_at: u64
    approvals: u8
}

receipt ReleaseRecord has create {
    lock_hash: Hash
    released_at: u64
    released_by: Address
}
"""

TIMELOCK_ACTION_SOURCES = {
    "create_absolute_lock": """
action create_absolute_lock(owner: Address, unlock_height: u64, current_height: u64) -> created_lock: TimeLock {
    verification
    require unlock_height > current_height + 10
    require unlock_height <= current_height + 2628000
    create created_lock = TimeLock {
        owner: owner,
        lock_type: 0,
        unlock_height: unlock_height,
        created_at: current_height
    }
}
""",
    "create_relative_lock": """
action create_relative_lock(owner: Address, lock_period: u64, current_height: u64) -> created_lock: TimeLock {
    verification
    require lock_period >= 10
    require lock_period <= 2628000
    create created_lock = TimeLock {
        owner: owner,
        lock_type: 1,
        unlock_height: current_height + lock_period,
        created_at: current_height
    }
}
""",
    "lock_asset": """
action lock_asset(read time_lock: TimeLock, lock_hash: Hash, amount: u64) -> locked: LockedAsset {
    verification
    require amount > 0
    create locked = LockedAsset {
        amount: amount,
        lock_hash: lock_hash
    }
}
""",
    "request_release": """
action request_release(read time_lock: TimeLock, lock_hash: Hash, requester: Address, current_height: u64) -> request: ReleaseRequest {
    verification
    require current_height >= time_lock.unlock_height
    create request = ReleaseRequest {
        lock_hash: lock_hash,
        requester: requester,
        requested_at: current_height
    }
}
""",
    "request_emergency_release": """
action request_emergency_release(read time_lock: TimeLock, lock_hash: Hash, requester: Address, current_height: u64) -> emergency: EmergencyRelease {
    verification
    require time_lock.owner == requester
    require current_height < time_lock.unlock_height
    create emergency = EmergencyRelease {
        lock_hash: lock_hash,
        requester: requester,
        requested_at: current_height,
        approvals: 0
    }
}
""",
    "approve_emergency_release": """
action approve_emergency_release(emergency_before: EmergencyRelease, approver: Address, required_approvals: u8) -> emergency_after: EmergencyRelease {
    verification
    require emergency_before.approvals < required_approvals
    require emergency_after.lock_hash == emergency_before.lock_hash
    require emergency_after.requester == emergency_before.requester
    require emergency_after.requested_at == emergency_before.requested_at
    require emergency_after.approvals == emergency_before.approvals + 1
}
""",
    "extend_lock": """
action extend_lock(time_lock_before: TimeLock, additional_period: u64, owner: Address, current_height: u64) -> time_lock_after: TimeLock {
    verification
    require time_lock_before.owner == owner
    require current_height < time_lock_before.unlock_height

    let new_unlock_height = time_lock_before.unlock_height + additional_period
    require new_unlock_height <= current_height + 2628000

    require time_lock_after.owner == time_lock_before.owner
    require time_lock_after.lock_type == time_lock_before.lock_type
    require time_lock_after.unlock_height == new_unlock_height
    require time_lock_after.created_at == time_lock_before.created_at
}
""",
    "execute_release": """
action execute_release(
    time_lock: TimeLock,
    locked_asset: LockedAsset,
    request: ReleaseRequest,
    executor: Address
) -> record: ReleaseRecord {
    verification
    require time_lock.owner == executor
    require locked_asset.lock_hash == request.lock_hash

    create record = ReleaseRecord {
        lock_hash: request.lock_hash,
        released_at: 125,
        released_by: executor
    }

    destroy time_lock
    destroy locked_asset
    destroy request
}
""",
    "execute_emergency_release": """
action execute_emergency_release(
    time_lock: TimeLock,
    locked_asset: LockedAsset,
    emergency: EmergencyRelease,
    executor: Address,
    required_approvals: u8
) -> record: ReleaseRecord {
    verification
    require time_lock.owner == executor
    require emergency.approvals >= required_approvals
    require locked_asset.lock_hash == emergency.lock_hash

    create record = ReleaseRecord {
        lock_hash: emergency.lock_hash,
        released_at: 125,
        released_by: executor
    }

    destroy time_lock
    destroy locked_asset
    destroy emergency
}
""",
    "batch_create_locks": """
action batch_create_locks(
    owners: [Address; 4],
    unlock_heights: [u64; 4],
    current_height: u64,
) -> (lock0: TimeLock, lock1: TimeLock, lock2: TimeLock, lock3: TimeLock) {
    verification
    require unlock_heights[0] > current_height + 10
    require unlock_heights[1] > current_height + 10
    require unlock_heights[2] > current_height + 10
    require unlock_heights[3] > current_height + 10
    require unlock_heights[0] <= current_height + 2628000
    require unlock_heights[1] <= current_height + 2628000
    require unlock_heights[2] <= current_height + 2628000
    require unlock_heights[3] <= current_height + 2628000

    create lock0 = TimeLock {
        owner: owners[0],
        lock_type: 0,
        unlock_height: unlock_heights[0],
        created_at: current_height
    }
    create lock1 = TimeLock {
        owner: owners[1],
        lock_type: 0,
        unlock_height: unlock_heights[1],
        created_at: current_height
    }
    create lock2 = TimeLock {
        owner: owners[2],
        lock_type: 0,
        unlock_height: unlock_heights[2],
        created_at: current_height
    }
    create lock3 = TimeLock {
        owner: owners[3],
        lock_type: 0,
        unlock_height: unlock_heights[3],
        created_at: current_height
    }
}
""",
}

for action, source in TIMELOCK_ACTION_SOURCES.items():
    (timelock_action_source_root / f"timelock_{action}.cell").write_text(
        f"module acceptance::timelock_{action}\n\n" + TIMELOCK_TYPES_SOURCE + "\n" + source,
        encoding="utf-8",
    )

AMM_ACTION_SOURCES = {
    "seed_pool": """
resource Token has store, create, consume {
    amount: u64
    symbol: [u8; 8]
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

receipt LPReceipt has store, create, consume {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

action seed_pool(token_a: Token, token_b: Token, fee_rate_bps: u16, provider: Address) -> (pool: Pool, receipt: LPReceipt) {
    verification
    require token_a.symbol != token_b.symbol
    require token_a.amount > 0 && token_b.amount > 0
    require fee_rate_bps <= 10000

    let initial_lp = isqrt(token_a.amount * token_b.amount)

    consume token_a
    consume token_b

    create pool = Pool {
        token_a_symbol: token_a.symbol,
        token_b_symbol: token_b.symbol,
        reserve_a: token_a.amount,
        reserve_b: token_b.amount,
        total_lp: initial_lp,
        fee_rate_bps: fee_rate_bps
    }

    create receipt = LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: initial_lp,
        provider: provider
    } with_lock(provider)
}

action isqrt(n: u64) -> u64 {
    verification
    if n == 0 {
        return 0
    }

    let mut x = n
    let mut y = (x + 1) / 2

    while y < x {
        x = y
        y = (x + n / x) / 2
    }

    x
}
""",
    "add_liquidity": """
resource Token has store, create, consume {
    amount: u64
    symbol: [u8; 8]
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

receipt LPReceipt has store, create, consume {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

action add_liquidity(pool_before: Pool, token_a: Token, token_b: Token, provider: Address) -> (pool_after: Pool, receipt: LPReceipt) {
    verification
    require token_a.symbol == pool_before.token_a_symbol
    require token_b.symbol == pool_before.token_b_symbol

    let lp_from_a = token_a.amount * pool_before.total_lp / pool_before.reserve_a
    let lp_from_b = token_b.amount * pool_before.total_lp / pool_before.reserve_b
    let lp_amount = min(lp_from_a, lp_from_b)

    consume token_a
    consume token_b

    require pool_after.token_a_symbol == pool_before.token_a_symbol
    require pool_after.token_b_symbol == pool_before.token_b_symbol
    require pool_after.reserve_a == pool_before.reserve_a + token_a.amount
    require pool_after.reserve_b == pool_before.reserve_b + token_b.amount
    require pool_after.total_lp == pool_before.total_lp + lp_amount
    require pool_after.fee_rate_bps == pool_before.fee_rate_bps

    create receipt = LPReceipt {
        pool_id: pool_before.type_hash(),
        lp_amount: lp_amount,
        provider: provider
    } with_lock(provider)
}

action min(a: u64, b: u64) -> u64 {
    verification
    if a < b { a } else { b }
}
""",
    "swap_a_for_b": """
resource Token has store, create, consume {
    amount: u64
    symbol: [u8; 8]
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

action swap_a_for_b(pool_before: Pool, input: Token, min_output: u64, to: Address) -> (pool_after: Pool, token_out: Token) {
    verification
    require input.symbol == pool_before.token_a_symbol

    let fee = input.amount * pool_before.fee_rate_bps as u64 / 10000
    let net_input = input.amount - fee

    let amount_out = pool_before.reserve_b * net_input / (pool_before.reserve_a + net_input)

    require amount_out >= min_output
    require amount_out < pool_before.reserve_b

    consume input

    require pool_after.token_a_symbol == pool_before.token_a_symbol
    require pool_after.token_b_symbol == pool_before.token_b_symbol
    require pool_after.reserve_a == pool_before.reserve_a + input.amount
    require pool_after.reserve_b == pool_before.reserve_b - amount_out
    require pool_after.total_lp == pool_before.total_lp
    require pool_after.fee_rate_bps == pool_before.fee_rate_bps

    create token_out = Token {
        amount: amount_out,
        symbol: pool_before.token_b_symbol
    } with_lock(to)
}
""",
    "remove_liquidity": """
resource Token has store, create, consume {
    amount: u64
    symbol: [u8; 8]
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

receipt LPReceipt has store, create, consume {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

action remove_liquidity(pool_before: Pool, receipt: LPReceipt, provider: Address) -> (pool_after: Pool, token_a_out: Token, token_b_out: Token) {
    verification
    require receipt.pool_id == pool_before.type_hash()

    let amount_a = receipt.lp_amount * pool_before.reserve_a / pool_before.total_lp
    let amount_b = receipt.lp_amount * pool_before.reserve_b / pool_before.total_lp

    consume receipt

    require pool_after.token_a_symbol == pool_before.token_a_symbol
    require pool_after.token_b_symbol == pool_before.token_b_symbol
    require pool_after.reserve_a == pool_before.reserve_a - amount_a
    require pool_after.reserve_b == pool_before.reserve_b - amount_b
    require pool_after.total_lp == pool_before.total_lp - receipt.lp_amount
    require pool_after.fee_rate_bps == pool_before.fee_rate_bps

    create token_a_out = Token {
        amount: amount_a,
        symbol: pool_before.token_a_symbol
    } with_lock(provider)

    create token_b_out = Token {
        amount: amount_b,
        symbol: pool_before.token_b_symbol
    } with_lock(provider)
}
""",
    "isqrt": """
action isqrt(n: u64) -> u64 {
    verification
    if n == 0 {
        return 0
    }

    let mut x = n
    let mut y = (x + 1) / 2

    while y < x {
        x = y
        y = (x + n / x) / 2
    }

    x
}
""",
    "min": """
action min(a: u64, b: u64) -> u64 {
    verification
    if a < b { a } else { b }
}
""",
}

for action, source in AMM_ACTION_SOURCES.items():
    (amm_action_source_root / f"amm_{action}.cell").write_text(
        f"module acceptance::amm_{action}\n\n" + source,
        encoding="utf-8",
    )

MULTISIG_TYPES_SOURCE = """resource MultisigWallet has store, create, replace, read_ref {
    wallet_id: Hash
    signer_a: Address
    signer_b: Address
    threshold: u8
    nonce: u64
    created_at: u64
}

receipt Proposal has create, consume, replace, burn {
    wallet_id: Hash
    proposal_id: u64
    proposer: Address
    operation: u8
    target: Address
    amount: u64
    required_signatures: u8
    signature_count: u8
    created_at: u64
    expires_at: u64
}

receipt SignatureConfirmation has create {
    proposal_id: u64
    signer: Address
    timestamp: u64
}

receipt ExecutionRecord has create {
    proposal_id: u64
    executor: Address
    executed_at: u64
    success: u8
}
"""

MULTISIG_ACTION_SOURCES = {
    "create_wallet": """
action create_wallet(wallet_id: Hash, signer_a: Address, signer_b: Address, threshold: u8, current_time: u64) -> wallet: MultisigWallet {
    verification
    require signer_a != signer_b
    require threshold >= 2
    require threshold <= 2

    create wallet = MultisigWallet {
        wallet_id: wallet_id,
        signer_a: signer_a,
        signer_b: signer_b,
        threshold: threshold,
        nonce: 0,
        created_at: current_time
    }
}
""",
    "propose_transfer": """
action propose_transfer(wallet_before: MultisigWallet, proposer: Address, target: Address, amount: u64, current_time: u64) -> (wallet_after: MultisigWallet, proposal: Proposal) {
    verification
    require proposer == wallet_before.signer_a
    require amount > 0

    let proposal_id = wallet_before.nonce + 1

    require wallet_after.wallet_id == wallet_before.wallet_id
    require wallet_after.signer_a == wallet_before.signer_a
    require wallet_after.signer_b == wallet_before.signer_b
    require wallet_after.threshold == wallet_before.threshold
    require wallet_after.nonce == proposal_id
    require wallet_after.created_at == wallet_before.created_at

    create proposal = Proposal {
        wallet_id: wallet_before.wallet_id,
        proposal_id: proposal_id,
        proposer: proposer,
        operation: 0,
        target: target,
        amount: amount,
        required_signatures: wallet_before.threshold,
        signature_count: 0,
        created_at: current_time,
        expires_at: current_time + 1440
    }
}
""",
    "add_signature": """
action add_signature(proposal_before: Proposal, signer: Address, current_time: u64) -> (proposal_after: Proposal, confirmation: SignatureConfirmation) {
    verification
    require current_time < proposal_before.expires_at
    require proposal_before.signature_count < proposal_before.required_signatures

    require proposal_after.wallet_id == proposal_before.wallet_id
    require proposal_after.proposal_id == proposal_before.proposal_id
    require proposal_after.proposer == proposal_before.proposer
    require proposal_after.operation == proposal_before.operation
    require proposal_after.target == proposal_before.target
    require proposal_after.amount == proposal_before.amount
    require proposal_after.required_signatures == proposal_before.required_signatures
    require proposal_after.signature_count == proposal_before.signature_count + 1
    require proposal_after.created_at == proposal_before.created_at
    require proposal_after.expires_at == proposal_before.expires_at

    create confirmation = SignatureConfirmation {
        proposal_id: proposal_before.proposal_id,
        signer: signer,
        timestamp: current_time
    }
}
""",
    "propose_add_signer": """
action propose_add_signer(wallet_before: MultisigWallet, proposer: Address, new_signer: Address, current_time: u64) -> (wallet_after: MultisigWallet, proposal: Proposal) {
    verification
    require proposer == wallet_before.signer_a
    require new_signer != wallet_before.signer_a
    require new_signer != wallet_before.signer_b

    let proposal_id = wallet_before.nonce + 1

    require wallet_after.wallet_id == wallet_before.wallet_id
    require wallet_after.signer_a == wallet_before.signer_a
    require wallet_after.signer_b == wallet_before.signer_b
    require wallet_after.threshold == wallet_before.threshold
    require wallet_after.nonce == proposal_id
    require wallet_after.created_at == wallet_before.created_at

    create proposal = Proposal {
        wallet_id: wallet_before.wallet_id,
        proposal_id: proposal_id,
        proposer: proposer,
        operation: 1,
        target: new_signer,
        amount: 0,
        required_signatures: wallet_before.threshold,
        signature_count: 0,
        created_at: current_time,
        expires_at: current_time + 1440
    }
}
""",
    "propose_remove_signer": """
action propose_remove_signer(wallet_before: MultisigWallet, proposer: Address, signer_to_remove: Address, current_time: u64) -> (wallet_after: MultisigWallet, proposal: Proposal) {
    verification
    require proposer == wallet_before.signer_a
    require signer_to_remove == wallet_before.signer_b
    require wallet_before.threshold <= 1

    let proposal_id = wallet_before.nonce + 1

    require wallet_after.wallet_id == wallet_before.wallet_id
    require wallet_after.signer_a == wallet_before.signer_a
    require wallet_after.signer_b == wallet_before.signer_b
    require wallet_after.threshold == wallet_before.threshold
    require wallet_after.nonce == proposal_id
    require wallet_after.created_at == wallet_before.created_at

    create proposal = Proposal {
        wallet_id: wallet_before.wallet_id,
        proposal_id: proposal_id,
        proposer: proposer,
        operation: 2,
        target: signer_to_remove,
        amount: 0,
        required_signatures: wallet_before.threshold,
        signature_count: 0,
        created_at: current_time,
        expires_at: current_time + 1440
    }
}
""",
    "propose_change_threshold": """
action propose_change_threshold(wallet_before: MultisigWallet, proposer: Address, new_threshold: u8, current_time: u64) -> (wallet_after: MultisigWallet, proposal: Proposal) {
    verification
    require proposer == wallet_before.signer_a
    require new_threshold >= 1
    require new_threshold <= 2

    let proposal_id = wallet_before.nonce + 1

    require wallet_after.wallet_id == wallet_before.wallet_id
    require wallet_after.signer_a == wallet_before.signer_a
    require wallet_after.signer_b == wallet_before.signer_b
    require wallet_after.threshold == wallet_before.threshold
    require wallet_after.nonce == proposal_id
    require wallet_after.created_at == wallet_before.created_at

    create proposal = Proposal {
        wallet_id: wallet_before.wallet_id,
        proposal_id: proposal_id,
        proposer: proposer,
        operation: 3,
        target: Address::zero(),
        amount: new_threshold as u64,
        required_signatures: wallet_before.threshold,
        signature_count: 0,
        created_at: current_time,
        expires_at: current_time + 1440
    }
}
""",
    "execute_proposal": """
action execute_proposal(proposal: Proposal, executor: Address, current_time: u64) -> record: ExecutionRecord {
    verification
    require current_time < proposal.expires_at
    require proposal.signature_count >= proposal.required_signatures

    create record = ExecutionRecord {
        proposal_id: proposal.proposal_id,
        executor: executor,
        executed_at: current_time,
        success: 1
    }

    destroy proposal
}
""",
    "cancel_proposal": """
action cancel_proposal(proposal: Proposal, canceller: Address) {
    verification
    require proposal.proposer == canceller
    destroy proposal
}
""",
}

for action, source in MULTISIG_ACTION_SOURCES.items():
    (multisig_action_source_root / f"multisig_{action}.cell").write_text(
        f"module acceptance::multisig_{action}\n\n" + MULTISIG_TYPES_SOURCE + "\n" + source,
        encoding="utf-8",
    )

LAUNCH_TYPES_SOURCE = """const U64_MAX: u64 = 18446744073709551615

resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

resource MintAuthority has store, create, replace {
    token_symbol: [u8; 8]
    max_supply: u64
    minted: u64
}

receipt LPReceipt has store, create, consume {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}
"""

LAUNCH_ACTION_SOURCES = {
    "launch_token": """
action launch_token(symbol: [u8; 8], max_supply: u64, initial_mint: u64, pool_seed_amount: u64, pool_paired_token: Token, fee_rate_bps: u16, creator: Address, distribution: [(Address, u64); 4]) -> (auth: MintAuthority, dist0: Token, dist1: Token, dist2: Token, dist3: Token, pool: Pool, lp_receipt: LPReceipt, change: Token) {
    verification
    require initial_mint <= max_supply, "initial exceeds max"
    require pool_seed_amount > 0, "zero pool seed"
    require pool_paired_token.amount > 0, "zero paired seed"
    require symbol != pool_paired_token.symbol, "same token"
    require fee_rate_bps <= 10000, "fee too high"
    require pool_seed_amount <= initial_mint, "pool seed exceeds mint"
    require distribution[1].1 <= U64_MAX - distribution[0].1, "distribution overflow"
    let dist01 = distribution[0].1 + distribution[1].1
    require distribution[2].1 <= U64_MAX - dist01, "distribution overflow"
    let dist012 = dist01 + distribution[2].1
    require distribution[3].1 <= U64_MAX - dist012, "distribution overflow"
    let dist_total = dist012 + distribution[3].1
    require pool_seed_amount <= U64_MAX - dist_total, "allocation overflow"
    require dist_total + pool_seed_amount <= initial_mint, "allocation exceeds mint"

    create auth = MintAuthority {
        token_symbol: symbol,
        max_supply: max_supply,
        minted: initial_mint
    } with_lock(creator)
    create dist0 = Token { amount: distribution[0].1, symbol: symbol } with_lock(distribution[0].0)
    create dist1 = Token { amount: distribution[1].1, symbol: symbol } with_lock(distribution[1].0)
    create dist2 = Token { amount: distribution[2].1, symbol: symbol } with_lock(distribution[2].0)
    create dist3 = Token { amount: distribution[3].1, symbol: symbol } with_lock(distribution[3].0)

    let initial_lp = pool_seed_amount
    consume pool_paired_token
    create pool = Pool {
        token_a_symbol: symbol,
        token_b_symbol: pool_paired_token.symbol,
        reserve_a: pool_seed_amount,
        reserve_b: pool_paired_token.amount,
        total_lp: initial_lp,
        fee_rate_bps: fee_rate_bps
    }
    create lp_receipt = LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: initial_lp,
        provider: creator
    } with_lock(creator)
    let remaining = initial_mint - dist_total - pool_seed_amount
    create change = Token { amount: remaining, symbol: symbol } with_lock(creator)
}
""",
    "bootstrap_token": """
action bootstrap_token(symbol: [u8; 8], max_supply: u64, initial_mint: u64, creator: Address, recipients: [(Address, u64); 2]) -> (auth: MintAuthority, rec0: Token, rec1: Token, change: Token) {
    verification
    require initial_mint <= max_supply, "initial exceeds max"
    require recipients[1].1 <= U64_MAX - recipients[0].1, "distribution overflow"
    let total_distributed = recipients[0].1 + recipients[1].1
    require total_distributed <= initial_mint, "distribution exceeds mint"

    create auth = MintAuthority {
        token_symbol: symbol,
        max_supply: max_supply,
        minted: initial_mint
    } with_lock(creator)
    create rec0 = Token { amount: recipients[0].1, symbol: symbol } with_lock(recipients[0].0)
    create rec1 = Token { amount: recipients[1].1, symbol: symbol } with_lock(recipients[1].0)
    let remaining = initial_mint - total_distributed
    create change = Token { amount: remaining, symbol: symbol } with_lock(creator)
}
""",
}

for action, source in LAUNCH_ACTION_SOURCES.items():
    (launch_action_source_root / f"launch_{action}.cell").write_text(
        f"module acceptance::launch_{action}\n\n" + LAUNCH_TYPES_SOURCE + "\n" + source,
        encoding="utf-8",
    )

ORIGINAL_SCOPED_ACTIONS = {
    "nft.cell": [
        "create_collection",
        "mint",
        "transfer",
        "create_listing",
        "cancel_listing",
        "buy_from_listing",
        "create_offer",
        "accept_offer",
        "burn",
        "batch_mint",
    ],
    "timelock.cell": [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "request_emergency_release",
        "approve_emergency_release",
        "execute_release",
        "execute_emergency_release",
        "extend_lock",
        "batch_create_locks",
    ],
    "multisig.cell": [
        "create_wallet",
        "propose_transfer",
        "add_signature",
        "propose_add_signer",
        "propose_change_threshold",
        "propose_remove_signer",
        "execute_proposal",
        "cancel_proposal",
    ],
    "vesting.cell": ["create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"],
    "token.cell": ["mint_with_authority", "transfer_token", "burn", "merge"],
    "amm_pool.cell": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity", "isqrt", "min"],
    "launch.cell": ["launch_token", "bootstrap_token"],
}

ORIGINAL_SCOPED_LOCKS = {
    "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
    "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "not_expired", "emergency_approved"],
    "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
    "vesting.cell": ["vesting_admin"],
}

ORIGINAL_SCOPED_ACTION_FAIL_CLOSED = {}

ORIGINAL_SCOPED_LOCK_FAIL_CLOSED = {}

EXPECTED_SOURCE_ACTIONS = {
    "token.cell": ["mint_with_authority", "transfer_token", "burn", "merge"],
    "nft.cell": [
        "create_collection",
        "mint",
        "transfer",
        "create_listing",
        "cancel_listing",
        "buy_from_listing",
        "create_offer",
        "accept_offer",
        "burn",
        "batch_mint",
    ],
    "timelock.cell": [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "execute_release",
        "request_emergency_release",
        "approve_emergency_release",
        "execute_emergency_release",
        "extend_lock",
        "batch_create_locks",
    ],
    "multisig.cell": [
        "create_wallet",
        "propose_transfer",
        "add_signature",
        "execute_proposal",
        "cancel_proposal",
        "propose_add_signer",
        "propose_remove_signer",
        "propose_change_threshold",
    ],
    "vesting.cell": ["create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"],
    "amm_pool.cell": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity", "isqrt", "min"],
    "launch.cell": ["launch_token", "bootstrap_token"],
}

EXPECTED_SOURCE_LOCKS = {
    "token.cell": [],
    "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
    "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "emergency_approved", "not_expired"],
    "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
    "vesting.cell": ["vesting_admin"],
    "amm_pool.cell": [],
    "launch.cell": [],
}

CKB_ONCHAIN_ACTION_HARNESSES = {
    "token.cell": list(TOKEN_ACTION_SOURCES.keys()),
    "nft.cell": list(NFT_ACTION_SOURCES.keys()),
    "timelock.cell": list(TIMELOCK_ACTION_SOURCES.keys()),
    "multisig.cell": list(MULTISIG_ACTION_SOURCES.keys()),
    "vesting.cell": ["create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"],
    "amm_pool.cell": list(AMM_ACTION_SOURCES.keys()),
    "launch.cell": ["launch_token", "bootstrap_token"],
}

def clipped(text):
    if len(text) <= TRUNCATE:
        return text
    return text[:TRUNCATE] + f"\n... truncated {len(text) - TRUNCATE} bytes ..."

def run(args, *, env=None, timeout=180):
    completed = subprocess.run(args, env=env, text=True, capture_output=True, timeout=timeout)
    return {
        "command": [str(arg) for arg in args],
        "returncode": completed.returncode,
        "stdout": clipped(completed.stdout),
        "stderr": clipped(completed.stderr),
    }

def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))

def git_stdout(args):
    return subprocess.check_output(["git", *args], cwd=repo_root, text=True).strip()

def tracked_source_files():
    output = git_stdout(["ls-files", "--", *SOURCE_PROVENANCE_PATHS])
    return [
        line
        for line in output.splitlines()
        if line and (repo_root / line).is_file()
    ]

def file_sha256(path):
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def sha256_hex(data):
    return "0x" + hashlib.sha256(data).hexdigest()

def ckb_data_hash_hex(data):
    return "0x" + hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()

def tracked_source_sha256(files):
    h = hashlib.sha256()
    for rel in files:
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(file_sha256(repo_root / rel).encode("ascii"))
        h.update(b"\n")
    return "0x" + h.hexdigest()

def source_provenance_report():
    files = tracked_source_files()
    return {
        "schema": SOURCE_PROVENANCE_SCHEMA,
        "generated_at_utc": datetime.datetime.now(datetime.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "repo_commit": git_stdout(["rev-parse", "HEAD"]),
        "git_dirty": bool(git_stdout(["status", "--porcelain", "--untracked-files=no"])),
        "tracked_source_paths": SOURCE_PROVENANCE_PATHS,
        "tracked_source_files": files,
        "tracked_source_file_count": len(files),
        "tracked_source_sha256": tracked_source_sha256(files),
        "acceptance_script_sha256": "0x" + file_sha256(repo_root / "scripts/ckb_cellscript_acceptance.sh"),
        "validator_script_sha256": "0x" + file_sha256(repo_root / "scripts/validate_ckb_cellscript_production_evidence.py"),
    }

def source_entries(name, keyword):
    text = production_example_path(name).read_text(encoding="utf-8")
    pattern = re.compile(rf"^\s*{keyword}\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.MULTILINE)
    return pattern.findall(text)

def validate_source_coverage_matrix():
    action_mismatches = {}
    lock_mismatches = {}
    for name in EXAMPLES:
        actual_actions = source_entries(name, "action")
        expected_actions = EXPECTED_SOURCE_ACTIONS.get(name, [])
        if actual_actions != expected_actions:
            action_mismatches[name] = {
                "expected": expected_actions,
                "actual": actual_actions,
            }
        actual_locks = source_entries(name, "lock")
        expected_locks = EXPECTED_SOURCE_LOCKS.get(name, [])
        if actual_locks != expected_locks:
            lock_mismatches[name] = {
                "expected": expected_locks,
                "actual": actual_locks,
            }
    if action_mismatches or lock_mismatches:
        raise RuntimeError(
            "source coverage matrix is stale: "
            + json.dumps(
                {
                    "action_mismatches": action_mismatches,
                    "lock_mismatches": lock_mismatches,
                },
                sort_keys=True,
            )
        )

def build_ckb_business_coverage(onchain_actions=None):
    onchain_actions = onchain_actions or {}
    rows = []
    for example in EXAMPLES:
        source_actions = EXPECTED_SOURCE_ACTIONS.get(example, [])
        source_locks = EXPECTED_SOURCE_LOCKS.get(example, [])
        strict_actions = ORIGINAL_SCOPED_ACTIONS.get(example, [])
        strict_locks = ORIGINAL_SCOPED_LOCKS.get(example, [])
        fail_closed_actions = ORIGINAL_SCOPED_ACTION_FAIL_CLOSED.get(example, [])
        fail_closed_locks = ORIGINAL_SCOPED_LOCK_FAIL_CLOSED.get(example, [])
        ckb_onchain_actions = onchain_actions.get(example, [])

        missing_strict_actions = sorted(set(source_actions) - set(strict_actions) - set(fail_closed_actions))
        missing_strict_locks = sorted(set(source_locks) - set(strict_locks) - set(fail_closed_locks))
        missing_onchain_actions = sorted(set(strict_actions) - set(ckb_onchain_actions))

        rows.append({
            "example": example,
            "source_actions": source_actions,
            "source_locks": source_locks,
            "strict_ckb_actions": strict_actions,
            "strict_ckb_locks": strict_locks,
            "expected_fail_closed_actions": fail_closed_actions,
            "expected_fail_closed_locks": fail_closed_locks,
            "ckb_onchain_actions": ckb_onchain_actions,
            "missing_strict_ckb_actions": missing_strict_actions,
            "missing_strict_ckb_locks": missing_strict_locks,
            "missing_ckb_onchain_actions": missing_onchain_actions,
            "strict_action_coverage_complete": not missing_strict_actions,
            "strict_lock_coverage_complete": not missing_strict_locks,
            "ckb_onchain_action_coverage_complete": not missing_onchain_actions,
        })

    strict_complete = all(
        row["strict_action_coverage_complete"] and row["strict_lock_coverage_complete"]
        for row in rows
    )
    onchain_complete = all(row["ckb_onchain_action_coverage_complete"] for row in rows)
    return {
        "status": "complete" if strict_complete and onchain_complete else "incomplete",
        "strict_compile_coverage_complete": strict_complete,
        "onchain_action_coverage_complete": onchain_complete,
        "source_action_count": sum(len(row["source_actions"]) for row in rows),
        "source_lock_count": sum(len(row["source_locks"]) for row in rows),
        "strict_ckb_action_count": sum(len(row["strict_ckb_actions"]) for row in rows),
        "strict_ckb_lock_count": sum(len(row["strict_ckb_locks"]) for row in rows),
        "expected_fail_closed_action_count": sum(len(row["expected_fail_closed_actions"]) for row in rows),
        "expected_fail_closed_lock_count": sum(len(row["expected_fail_closed_locks"]) for row in rows),
        "ckb_onchain_action_count": sum(len(row["ckb_onchain_actions"]) for row in rows),
        "missing_strict_ckb_actions": {
            row["example"]: row["missing_strict_ckb_actions"]
            for row in rows
            if row["missing_strict_ckb_actions"]
        },
        "missing_strict_ckb_locks": {
            row["example"]: row["missing_strict_ckb_locks"]
            for row in rows
            if row["missing_strict_ckb_locks"]
        },
        "missing_ckb_onchain_actions": {
            row["example"]: row["missing_ckb_onchain_actions"]
            for row in rows
            if row["missing_ckb_onchain_actions"]
        },
        "rows": rows,
    }

def verify_artifact(artifact):
    completed = subprocess.run(
        [cellc, "verify-artifact", artifact, "--expect-target-profile", "ckb", "--json"],
        text=True,
        capture_output=True,
        timeout=180,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"verify-artifact failed for {artifact}: {clipped(completed.stderr)}")
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"verify-artifact did not return JSON for {artifact}: {clipped(completed.stdout)}") from error

def internal_assembler_env():
    env = os.environ.copy()
    for key in ("CELLSCRIPT_RISCV_CC", "CELLSCRIPT_RISCV_AS", "CELLSCRIPT_RISCV_LD"):
        env.pop(key, None)
    return env

def read_u16_le(data, offset):
    return struct.unpack_from("<H", data, offset)[0]

def read_u32_le(data, offset):
    return struct.unpack_from("<I", data, offset)[0]

def read_u64_le(data, offset):
    return struct.unpack_from("<Q", data, offset)[0]

def audit_ckb_elf_entry_abi(name, artifact_bytes):
    if len(artifact_bytes) < ELF64_HEADER_SIZE or not artifact_bytes.startswith(b"\x7fELF"):
        raise RuntimeError(f"{name} artifact is not a complete ELF64 file")
    if artifact_bytes[4] != 2:
        raise RuntimeError(f"{name} artifact is not ELFCLASS64")
    if artifact_bytes[5] != 1:
        raise RuntimeError(f"{name} artifact is not little-endian ELF")
    if read_u16_le(artifact_bytes, 18) != ELF_EM_RISCV:
        raise RuntimeError(f"{name} artifact is not RISC-V ELF")

    entry = read_u64_le(artifact_bytes, 24)
    program_header_offset = read_u64_le(artifact_bytes, 32)
    program_header_entry_size = read_u16_le(artifact_bytes, 54)
    program_header_count = read_u16_le(artifact_bytes, 56)
    if program_header_entry_size < ELF64_PROGRAM_HEADER_SIZE:
        raise RuntimeError(f"{name} ELF program header entry size is too small: {program_header_entry_size}")
    if program_header_offset + program_header_entry_size * program_header_count > len(artifact_bytes):
        raise RuntimeError(f"{name} ELF program headers exceed artifact size")

    executable_headers = []
    for index in range(program_header_count):
        offset = program_header_offset + index * program_header_entry_size
        p_type = read_u32_le(artifact_bytes, offset)
        flags = read_u32_le(artifact_bytes, offset + 4)
        if p_type != ELF_PT_LOAD or flags & ELF_PF_X == 0:
            continue
        file_offset = read_u64_le(artifact_bytes, offset + 8)
        virtual_address = read_u64_le(artifact_bytes, offset + 16)
        file_size = read_u64_le(artifact_bytes, offset + 32)
        memory_size = read_u64_le(artifact_bytes, offset + 40)
        executable_headers.append({
            "index": index,
            "flags": flags,
            "file_offset": file_offset,
            "virtual_address": virtual_address,
            "file_size": file_size,
            "memory_size": memory_size,
        })

    if not executable_headers:
        raise RuntimeError(f"{name} ELF does not contain an executable PT_LOAD segment")

    header = executable_headers[0]
    flags = header["flags"]
    if flags != (ELF_PF_R | ELF_PF_X):
        raise RuntimeError(f"{name} executable PT_LOAD flags must be RX-only, got 0x{flags:x}")
    if flags & ELF_PF_W:
        raise RuntimeError(f"{name} executable PT_LOAD segment must not be writable")
    if header["file_size"] != header["memory_size"]:
        raise RuntimeError(
            f"{name} executable PT_LOAD must not fake stack memory: "
            f"filesz={header['file_size']} memsz={header['memory_size']}"
        )
    if not (header["virtual_address"] <= entry < header["virtual_address"] + header["file_size"]):
        raise RuntimeError(f"{name} ELF entry point is outside the executable PT_LOAD segment")

    entry_file_offset = header["file_offset"] + (entry - header["virtual_address"])
    if entry_file_offset + ENTRY_TRAMPOLINE_SIZE > len(artifact_bytes):
        raise RuntimeError(f"{name} ELF entry trampoline exceeds artifact size")
    first_instruction = read_u32_le(artifact_bytes, entry_file_offset)
    first_opcode = first_instruction & 0x7f
    first_rd = (first_instruction >> 7) & 0x1f
    if first_opcode != 0x17 or first_rd != 1:
        raise RuntimeError(
            f"{name} ELF entry trampoline must start with auipc ra, not instruction 0x{first_instruction:08x}"
        )

    return {
        "schema": ELF_ENTRY_ABI_SCHEMA,
        "status": "passed",
        "entry_point": f"0x{entry:x}",
        "executable_load_segment": {
            "index": header["index"],
            "flags": flags,
            "flags_symbolic": "R|X",
            "writable": False,
            "file_offset": header["file_offset"],
            "virtual_address": f"0x{header['virtual_address']:x}",
            "file_size": header["file_size"],
            "memory_size": header["memory_size"],
            "file_size_equals_memory_size": True,
        },
        "trampoline": {
            "size_bytes": ENTRY_TRAMPOLINE_SIZE,
            "entry_file_offset": entry_file_offset,
            "first_instruction_le_hex": f"0x{first_instruction:08x}",
            "first_instruction_opcode": "auipc",
            "first_instruction_rd": "ra",
            "calls_entry_with_ra": True,
            "preserves_ckb_vm_stack_pointer": True,
            "forbidden_sp_initialisation": False,
        },
    }

def compile_artifact(name, kind, source, artifact, *, entry_args=None):
    entry_args = entry_args or []
    env = internal_assembler_env()
    result = run([cellc, source, "--target-profile", "ckb", "--target", "riscv64-elf", *entry_args, "-o", artifact], env=env)
    if result["returncode"] != 0:
        raise RuntimeError(f"CKB artifact compile failed for {name}: {result['stderr']}")
    if not artifact.exists():
        raise RuntimeError(f"CKB artifact compile did not produce artifact for {name}: {artifact}")

    metadata_path = pathlib.Path(str(artifact) + ".meta.json")
    if not metadata_path.exists():
        raise RuntimeError(f"CKB artifact compile did not produce metadata sidecar for {name}: {metadata_path}")

    artifact_bytes = artifact.read_bytes()
    artifact_has_unexpected_profile_trailer = UNEXPECTED_PROFILE_TRAILER in artifact_bytes[-64:]
    if not artifact_bytes.startswith(b"\x7fELF"):
        raise RuntimeError(f"{name} artifact is not an ELF")
    if artifact_has_unexpected_profile_trailer:
        raise RuntimeError(f"{name} CKB artifact still contains an unexpected non-CKB ABI trailer")
    elf_entry_abi = audit_ckb_elf_entry_abi(name, artifact_bytes)

    metadata = load_json(metadata_path)
    verify = verify_artifact(artifact)
    if metadata.get("target_profile", {}).get("name") != "ckb" or verify.get("target_profile") != "ckb":
        raise RuntimeError(f"{name} metadata/verify did not pin target_profile=ckb")

    return {
        "name": name,
        "kind": kind,
        "source": str(source),
        "artifact": str(artifact),
        "metadata": str(metadata_path),
        "artifact_size_bytes": len(artifact_bytes),
        "artifact_starts_with_elf_magic": True,
        "artifact_has_unexpected_profile_trailer": False,
        "elf_entry_abi": elf_entry_abi,
        "target_profile": "ckb",
        "artifact_packaging": metadata.get("target_profile", {}).get("artifact_packaging"),
        "entry_args": [str(arg) for arg in entry_args],
        "compile": result,
        "verify": verify,
    }

validate_source_coverage_matrix()

def strict_policy_fail_closed(stderr):
    return (
        "target profile policy failed for 'ckb'" in stderr
        or (
            "ProofPlan soundness check failed" in stderr
            and "PP0150" in stderr
            and "strict v0.16 ProofPlan mode rejects metadata-only or runtime-required obligations" in stderr
        )
    )

def strict_original_compile(name):
    source = production_example_build_path(name)
    artifact = strict_root / f"{name}.strict.elf"
    result = run(
        [cellc, source, "--target-profile", "ckb", "--target", "riscv64-elf", "--primitive-strict", "0.16", "-o", artifact],
        env=internal_assembler_env(),
    )
    policy_fail_closed = result["returncode"] != 0 and strict_policy_fail_closed(result["stderr"])
    unexpected_failure = result["returncode"] != 0 and not policy_fail_closed
    verify = None
    elf_entry_abi = None
    if result["returncode"] == 0:
        verify = verify_artifact(artifact)
        elf_entry_abi = audit_ckb_elf_entry_abi(name, artifact.read_bytes())
    return {
        "source": str(source),
        "artifact": str(artifact),
        "status": "passed" if result["returncode"] == 0 else "failed",
        "policy_fail_closed": policy_fail_closed,
        "unexpected_failure": unexpected_failure,
        "verify": verify,
        "elf_entry_abi": elf_entry_abi,
        "returncode": result["returncode"],
        "stdout": result["stdout"],
        "stderr": result["stderr"],
    }

def strict_scoped_compile(name, source, entry_flag, entry_name):
    artifact = strict_root / f"{name}.{entry_name}.strict-scoped.elf"
    result = run(
        [cellc, source, "--target-profile", "ckb", "--target", "riscv64-elf", "--primitive-strict", "0.16", entry_flag, entry_name, "-o", artifact],
        env=internal_assembler_env(),
    )
    policy_fail_closed = result["returncode"] != 0 and strict_policy_fail_closed(result["stderr"])
    unexpected_failure = result["returncode"] != 0 and not policy_fail_closed
    verify = None
    elf_entry_abi = None
    if result["returncode"] == 0:
        verify = verify_artifact(artifact)
        elf_entry_abi = audit_ckb_elf_entry_abi(name, artifact.read_bytes())
    return {
        "source": str(source),
        "artifact": str(artifact),
        "entry_flag": entry_flag,
        "entry": entry_name,
        "status": "passed" if result["returncode"] == 0 else "failed",
        "policy_fail_closed": policy_fail_closed,
        "unexpected_failure": unexpected_failure,
        "verify": verify,
        "elf_entry_abi": elf_entry_abi,
        "returncode": result["returncode"],
        "stdout": result["stdout"],
        "stderr": result["stderr"],
    }

artifacts = []
baseline = compile_artifact(
    "ckb_noop.cell",
    "pure-baseline",
    baseline_source,
    artifact_root / "ckb_noop.elf",
)
artifacts.append(baseline)

bundled_examples = []
bundled_example_deployment_artifacts = []
for name in EXAMPLES:
    strict = strict_original_compile(name)
    if strict["unexpected_failure"]:
        raise RuntimeError(
            f"primitive-strict original CKB compile for {name} failed for a non-policy reason: {strict['stderr']}"
        )
    record = {
        "name": name,
        "kind": "bundled-example-strict-original",
        "source": str(production_example_path(name)),
        "strict_original_ckb_compile": strict,
    }
    bundled_examples.append(record)
    if strict["status"] == "passed":
        bundled_example_deployment_artifacts.append({
            "name": name,
            "kind": "bundled-example-strict-original",
            "source": str(production_example_path(name)),
            "artifact": strict["artifact"],
        })

token_action_artifacts = []
for action in TOKEN_ACTION_SOURCES:
    source = token_action_source_root / f"token_{action}.cell"
    record = compile_artifact(
        f"token.{action}.cell",
        "token-action-strict",
        source,
        artifact_root / f"token_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("token.cell"))
    token_action_artifacts.append(record)

nft_action_artifacts = []
for action in NFT_ACTION_SOURCES:
    source = nft_action_source_root / f"nft_{action}.cell"
    record = compile_artifact(
        f"nft.{action}.cell",
        "nft-action-strict",
        source,
        artifact_root / f"nft_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("nft.cell"))
    nft_action_artifacts.append(record)

timelock_action_artifacts = []
for action in TIMELOCK_ACTION_SOURCES:
    source = timelock_action_source_root / f"timelock_{action}.cell"
    record = compile_artifact(
        f"timelock.{action}.cell",
        "timelock-action-strict",
        source,
        artifact_root / f"timelock_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("timelock.cell"))
    timelock_action_artifacts.append(record)

amm_action_artifacts = []
for action in AMM_ACTION_SOURCES:
    source = amm_action_source_root / f"amm_{action}.cell"
    record = compile_artifact(
        f"amm.{action}.cell",
        "amm-action-strict",
        source,
        artifact_root / f"amm_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("amm_pool.cell"))
    amm_action_artifacts.append(record)

multisig_action_artifacts = []
for action in MULTISIG_ACTION_SOURCES:
    source = multisig_action_source_root / f"multisig_{action}.cell"
    record = compile_artifact(
        f"multisig.{action}.cell",
        "multisig-action-strict",
        source,
        artifact_root / f"multisig_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("multisig.cell"))
    multisig_action_artifacts.append(record)

launch_action_artifacts = []
for action in LAUNCH_ACTION_SOURCES:
    source = launch_action_source_root / f"launch_{action}.cell"
    record = compile_artifact(
        f"launch.{action}.cell",
        "launch-action-strict",
        source,
        artifact_root / f"launch_{action}.elf",
    )
    record["action"] = action
    record["original_source"] = str(production_example_path("launch.cell"))
    launch_action_artifacts.append(record)

original_scoped_action_artifacts = []
for example_name, actions in ORIGINAL_SCOPED_ACTIONS.items():
    for action in actions:
        record = compile_artifact(
            f"{example_name}:{action}",
            "original-scoped-action-strict",
            production_example_build_path(example_name),
            artifact_root / f"original_{example_name.removesuffix('.cell')}_{action}.elf",
            entry_args=["--primitive-strict", "0.16", "--entry-action", action],
        )
        record["example"] = example_name
        record["action"] = action
        record["original_source"] = str(production_example_path(example_name))
        original_scoped_action_artifacts.append(record)

def original_scoped_action_or(record, example_name):
    return next(
        (
            original
            for original in original_scoped_action_artifacts
            if original["example"] == example_name and original["action"] == record["action"]
        ),
        record,
    )

launch_action_artifacts = [
    original_scoped_action_or(record, "launch.cell")
    for record in launch_action_artifacts
]

token_action_artifacts = [
    original_scoped_action_or(record, "token.cell")
    for record in token_action_artifacts
]

nft_action_artifacts = [
    original_scoped_action_or(record, "nft.cell")
    for record in nft_action_artifacts
]

timelock_action_artifacts = [
    next(
        (
            original
            for original in original_scoped_action_artifacts
            if original["example"] == "timelock.cell" and original["action"] == record["action"]
        ),
        record,
    )
    if record["action"] in (
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "request_emergency_release",
        "approve_emergency_release",
        "execute_release",
        "execute_emergency_release",
        "extend_lock",
        "batch_create_locks",
    ) else record
    for record in timelock_action_artifacts
]

amm_action_artifacts = [
    original_scoped_action_or(record, "amm_pool.cell")
    for record in amm_action_artifacts
]

multisig_action_artifacts = [
    next(
        (
            original
            for original in original_scoped_action_artifacts
            if original["example"] == "multisig.cell" and original["action"] == record["action"]
        ),
        record,
    )
    if record["action"] in (
        "create_wallet",
        "propose_transfer",
        "add_signature",
        "propose_add_signer",
        "propose_remove_signer",
        "propose_change_threshold",
        "execute_proposal",
        "cancel_proposal",
    ) else record
    for record in multisig_action_artifacts
]

original_scoped_lock_artifacts = []
for example_name, locks in ORIGINAL_SCOPED_LOCKS.items():
    for lock in locks:
        record = compile_artifact(
            f"{example_name}:{lock}",
            "original-scoped-lock-strict",
            production_example_build_path(example_name),
            artifact_root / f"original_{example_name.removesuffix('.cell')}_{lock}.elf",
            entry_args=["--primitive-strict", "0.16", "--entry-lock", lock],
        )
        record["example"] = example_name
        record["lock"] = lock
        record["original_source"] = str(production_example_path(example_name))
        original_scoped_lock_artifacts.append(record)

original_scoped_action_fail_closed = []
for example_name, actions in ORIGINAL_SCOPED_ACTION_FAIL_CLOSED.items():
    for action in actions:
        record = strict_scoped_compile(
            f"{example_name}:{action}",
            production_example_build_path(example_name),
            "--entry-action",
            action,
        )
        record["example"] = example_name
        record["action"] = action
        record["original_source"] = str(production_example_path(example_name))
        original_scoped_action_fail_closed.append(record)

original_scoped_lock_fail_closed = []
for example_name, locks in ORIGINAL_SCOPED_LOCK_FAIL_CLOSED.items():
    for lock in locks:
        record = strict_scoped_compile(
            f"{example_name}:{lock}",
            production_example_build_path(example_name),
            "--entry-lock",
            lock,
        )
        record["example"] = example_name
        record["lock"] = lock
        record["original_source"] = str(production_example_path(example_name))
        original_scoped_lock_fail_closed.append(record)

expected_original_scoped_action_count = sum(len(actions) for actions in ORIGINAL_SCOPED_ACTIONS.values())
expected_original_scoped_lock_count = sum(len(locks) for locks in ORIGINAL_SCOPED_LOCKS.values())
expected_original_scoped_action_fail_closed_count = sum(
    len(actions) for actions in ORIGINAL_SCOPED_ACTION_FAIL_CLOSED.values()
)
expected_original_scoped_lock_fail_closed_count = sum(
    len(locks) for locks in ORIGINAL_SCOPED_LOCK_FAIL_CLOSED.values()
)
if len(original_scoped_action_artifacts) != expected_original_scoped_action_count:
    raise RuntimeError(
        f"original scoped action coverage mismatch: expected {expected_original_scoped_action_count}, "
        f"compiled {len(original_scoped_action_artifacts)}"
    )
if len(original_scoped_lock_artifacts) != expected_original_scoped_lock_count:
    raise RuntimeError(
        f"original scoped lock coverage mismatch: expected {expected_original_scoped_lock_count}, "
        f"compiled {len(original_scoped_lock_artifacts)}"
    )
if len(original_scoped_action_fail_closed) != expected_original_scoped_action_fail_closed_count:
    raise RuntimeError(
        "original scoped action fail-closed coverage mismatch: "
        f"expected {expected_original_scoped_action_fail_closed_count}, "
        f"checked {len(original_scoped_action_fail_closed)}"
    )
if len(original_scoped_lock_fail_closed) != expected_original_scoped_lock_fail_closed_count:
    raise RuntimeError(
        "original scoped lock fail-closed coverage mismatch: "
        f"expected {expected_original_scoped_lock_fail_closed_count}, "
        f"checked {len(original_scoped_lock_fail_closed)}"
    )

unexpected_scoped_admissions = [
    f"{record['example']}:{record.get('action') or record.get('lock')}"
    for record in [*original_scoped_action_fail_closed, *original_scoped_lock_fail_closed]
    if record["status"] == "passed"
]
if unexpected_scoped_admissions:
    raise RuntimeError(
        "expected fail-closed original scoped entries were admitted; "
        "move them into the strict scoped pass matrix only after reviewing coverage: "
        + ", ".join(unexpected_scoped_admissions)
    )

unexpected_scoped_failures = [
    f"{record['example']}:{record.get('action') or record.get('lock')}"
    for record in [*original_scoped_action_fail_closed, *original_scoped_lock_fail_closed]
    if record["unexpected_failure"]
]
if unexpected_scoped_failures:
    raise RuntimeError(
        "expected fail-closed original scoped entries failed for non-policy reasons: "
        + ", ".join(unexpected_scoped_failures)
    )

non_policy_fail_closed = [
    f"{record['example']}:{record.get('action') or record.get('lock')}"
    for record in [*original_scoped_action_fail_closed, *original_scoped_lock_fail_closed]
    if not record["policy_fail_closed"]
]
if non_policy_fail_closed:
    raise RuntimeError(
        "expected fail-closed original scoped entries were not rejected by strict CKB/ProofPlan policy: "
        + ", ".join(non_policy_fail_closed)
    )

strict_original_policy_fail_closed = [
    record["name"]
    for record in bundled_examples
    if record["strict_original_ckb_compile"]["policy_fail_closed"]
]
strict_original_unexpected_failures = [
    record["name"]
    for record in bundled_examples
    if record["strict_original_ckb_compile"]["unexpected_failure"]
]

def elf_entry_abi_source_example(record):
    example = record.get("example")
    if isinstance(example, str) and example:
        return example
    original_source = record.get("original_source") or record.get("source")
    if isinstance(original_source, str):
        source_name = pathlib.Path(original_source).name
        if source_name in EXAMPLES:
            return source_name
    return None

def collect_elf_entry_abi_gate():
    rows = []
    seen_artifacts = set()

    def add_record(record, *, fallback_name=None, fallback_kind=None, source_example=None):
        artifact = record.get("artifact")
        if not artifact or artifact in seen_artifacts:
            return
        seen_artifacts.add(artifact)
        audit = record.get("elf_entry_abi")
        row = {
            "name": record.get("name") or fallback_name or pathlib.Path(artifact).name,
            "kind": record.get("kind") or fallback_kind or "unknown",
            "source": record.get("source"),
            "original_source": record.get("original_source"),
            "example": source_example or elf_entry_abi_source_example(record),
            "artifact": artifact,
            "status": audit.get("status") if isinstance(audit, dict) else "missing",
            "preserves_ckb_vm_stack_pointer": False,
            "entry_trampoline_calls_with_ra": False,
            "executable_segment_rx_only": False,
            "executable_segment_file_size_equals_memory_size": False,
        }
        if isinstance(audit, dict):
            trampoline = audit.get("trampoline") or {}
            executable = audit.get("executable_load_segment") or {}
            row.update({
                "preserves_ckb_vm_stack_pointer": trampoline.get("preserves_ckb_vm_stack_pointer") is True,
                "entry_trampoline_calls_with_ra": trampoline.get("calls_entry_with_ra") is True,
                "executable_segment_rx_only": executable.get("flags_symbolic") == "R|X" and executable.get("writable") is False,
                "executable_segment_file_size_equals_memory_size": executable.get("file_size_equals_memory_size") is True,
                "first_instruction_le_hex": trampoline.get("first_instruction_le_hex"),
                "entry_point": audit.get("entry_point"),
            })
        rows.append(row)

    for record in artifacts:
        add_record(record)
    for record in bundled_examples:
        strict = record["strict_original_ckb_compile"]
        if strict["status"] == "passed":
            strict = {**strict, "name": record["name"], "kind": "bundled-example-strict-original", "source": record["source"], "example": record["name"]}
            add_record(strict, source_example=record["name"])
    for group in (
        token_action_artifacts,
        nft_action_artifacts,
        timelock_action_artifacts,
        amm_action_artifacts,
        multisig_action_artifacts,
        launch_action_artifacts,
        original_scoped_action_artifacts,
        original_scoped_lock_artifacts,
    ):
        for record in group:
            add_record(record)

    failures = [
        row["name"]
        for row in rows
        if row["status"] != "passed"
        or not row["preserves_ckb_vm_stack_pointer"]
        or not row["entry_trampoline_calls_with_ra"]
        or not row["executable_segment_rx_only"]
        or not row["executable_segment_file_size_equals_memory_size"]
    ]

    critical = {}
    for example in CRITICAL_0_20_DEVNET_EXAMPLES:
        example_rows = [row for row in rows if row.get("example") == example]
        missing = not example_rows
        failed = [row["name"] for row in example_rows if row["status"] != "passed"]
        critical[example] = {
            "status": "passed" if example_rows and not failed else "failed",
            "artifact_count": len(example_rows),
            "audited_artifacts": [row["name"] for row in example_rows],
            "missing": missing,
            "failures": failed,
        }
        if missing:
            failures.append(f"{example}:missing")
        failures.extend(f"{example}:{name}" for name in failed)

    unique_failures = sorted(set(failures))
    return {
        "schema": "cellscript-ckb-elf-entry-abi-gate-v0.20",
        "status": "passed" if not unique_failures else "failed",
        "requires_ckb_vm_stack_pointer_preserved": True,
        "requires_entry_trampoline_call_sequence": True,
        "requires_rx_only_executable_segment": True,
        "requires_no_fake_stack_load_segment": True,
        "critical_examples": CRITICAL_0_20_DEVNET_EXAMPLES,
        "critical_example_gate": critical,
        "audited_artifact_count": len(rows),
        "failures": unique_failures,
        "rows": rows,
    }

def collect_build_reports():
    rows = []
    seen_artifacts = set()

    def add_record(record, *, fallback_name=None, fallback_kind=None, source_example=None):
        artifact = record.get("artifact")
        if not artifact or artifact in seen_artifacts:
            return
        seen_artifacts.add(artifact)
        artifact_path = pathlib.Path(artifact)
        artifact_bytes = artifact_path.read_bytes()
        verify = record.get("verify") or {}
        elf_entry_abi = record.get("elf_entry_abi") or {}
        metadata_sidecar = record.get("metadata")
        row = {
            "schema": BUILD_REPORT_SCHEMA,
            "name": record.get("name") or fallback_name or artifact_path.name,
            "kind": record.get("kind") or fallback_kind or "unknown",
            "source": record.get("source"),
            "original_source": record.get("original_source"),
            "example": source_example or elf_entry_abi_source_example(record),
            "entry_flag": record.get("entry_flag"),
            "entry": record.get("entry"),
            "target_profile": "ckb",
            "vm_profile": "ckb-vm",
            "artifact_format": "riscv64-elf",
            "artifact_path": str(artifact_path),
            "metadata_sidecar": metadata_sidecar,
            "artifact_packaging": record.get("artifact_packaging"),
            "artifact_size_bytes": len(artifact_bytes),
            "artifact_hash_algorithm": "ckb-blake2b256",
            "deployable_elf_hash": ckb_data_hash_hex(artifact_bytes),
            "artifact_sha256": sha256_hex(artifact_bytes),
            "deployment_hash_type_used_by_gate": "data1",
            "verify_artifact_status": "passed" if isinstance(verify, dict) else "missing",
            "verify_target_profile": verify.get("target_profile") if isinstance(verify, dict) else None,
            "elf_entry_abi_status": elf_entry_abi.get("status") if isinstance(elf_entry_abi, dict) else "missing",
            "abi_trailer_stripped": UNEXPECTED_PROFILE_TRAILER not in artifact_bytes[-64:],
            "onchain_deployments": [],
        }
        rows.append(row)

    for record in artifacts:
        add_record(record)
    for record in bundled_examples:
        strict = record["strict_original_ckb_compile"]
        if strict["status"] == "passed":
            strict = {
                **strict,
                "name": record["name"],
                "kind": "bundled-example-strict-original",
                "source": record["source"],
                "example": record["name"],
            }
            add_record(strict, source_example=record["name"])
    for group in (
        token_action_artifacts,
        nft_action_artifacts,
        timelock_action_artifacts,
        amm_action_artifacts,
        multisig_action_artifacts,
        launch_action_artifacts,
        original_scoped_action_artifacts,
        original_scoped_lock_artifacts,
    ):
        for record in group:
            add_record(record)

    return {
        "schema": "cellscript-ckb-build-report-index-v0.20",
        "status": "passed",
        "artifact_count": len(rows),
        "artifact_hash_algorithm": "ckb-blake2b256",
        "artifact_format": "riscv64-elf",
        "target_profile": "ckb",
        "vm_profile": "ckb-vm",
        "requires_exact_artifact_hash": True,
        "requires_elf_entry_abi_gate": True,
        "requires_live_code_cell_data_hash_match": True,
        "reports": rows,
    }

ckb_elf_entry_abi_gate = collect_elf_entry_abi_gate()
if ckb_elf_entry_abi_gate["status"] != "passed":
    raise RuntimeError("CKB ELF entry ABI gate failed: " + json.dumps(ckb_elf_entry_abi_gate["failures"], sort_keys=True))
build_reports = collect_build_reports()

report = {
    "status": "artifact-verified",
    "acceptance_mode": acceptance_mode,
    "ckb_acceptance_scope": (
        "Production mode is a hard gate and must not depend on synthetic harnesses, "
        "expected fail-closed entries, or non-original artifacts. Bounded mode is a development coverage matrix only."
    ),
    "cellc": str(cellc),
    "source_provenance": source_provenance_report(),
    "bundled_examples_exact_order": EXAMPLES,
    "bundled_examples_count": len(EXAMPLES),
    "non_production_examples": NON_PRODUCTION_EXAMPLES,
    "language_examples_exact_order": LANGUAGE_EXAMPLES,
    "language_examples_count": len(LANGUAGE_EXAMPLES),
    "example_scope": EXAMPLE_SCOPE,
    "example_source_layout": {
        "canonical_bundled_examples": str(examples_dir),
        "language_examples": str(language_examples_dir),
        "canonical_examples_note": (
            "Production acceptance compiles the checked-in top-level examples/*.cell directly. "
            "examples/business and examples/acceptance are intentionally not part of the checked-in source layout."
        ),
    },
    "lock_acceptance_scope": LOCK_ACCEPTANCE_SCOPE,
    "ckb_elf_entry_abi_gate": ckb_elf_entry_abi_gate,
    "cellscript_build_reports": build_reports,
    "bundled_examples_strict_admitted": [
        record["name"]
        for record in bundled_examples
        if record["strict_original_ckb_compile"]["status"] == "passed"
    ],
    "strict_original_ckb_compile_policy_fail_closed": strict_original_policy_fail_closed,
    "strict_original_ckb_compile_unexpected_failures": strict_original_unexpected_failures,
    "pure_baseline": baseline,
    "bundled_examples": bundled_examples,
    "bundled_example_deployment_artifacts": bundled_example_deployment_artifacts,
    "token_action_artifacts": token_action_artifacts,
    "nft_action_artifacts": nft_action_artifacts,
    "timelock_action_artifacts": timelock_action_artifacts,
    "amm_action_artifacts": amm_action_artifacts,
    "multisig_action_artifacts": multisig_action_artifacts,
    "launch_action_artifacts": launch_action_artifacts,
    "original_scoped_actions_expected": ORIGINAL_SCOPED_ACTIONS,
    "original_scoped_locks_expected": ORIGINAL_SCOPED_LOCKS,
    "original_scoped_action_fail_closed_expected": ORIGINAL_SCOPED_ACTION_FAIL_CLOSED,
    "original_scoped_lock_fail_closed_expected": ORIGINAL_SCOPED_LOCK_FAIL_CLOSED,
    "original_scoped_action_count": len(original_scoped_action_artifacts),
    "original_scoped_lock_count": len(original_scoped_lock_artifacts),
    "original_scoped_action_fail_closed_count": len(original_scoped_action_fail_closed),
    "original_scoped_lock_fail_closed_count": len(original_scoped_lock_fail_closed),
    "original_scoped_action_artifacts": original_scoped_action_artifacts,
    "original_scoped_lock_artifacts": original_scoped_lock_artifacts,
    "original_scoped_action_fail_closed": original_scoped_action_fail_closed,
    "original_scoped_lock_fail_closed": original_scoped_lock_fail_closed,
    "ckb_business_coverage": build_ckb_business_coverage(),
    "production_ready": False,
    "artifacts": artifacts,
}

def production_gate_failures(report):
    failures = []
    if report.get("strict_original_ckb_compile_policy_fail_closed"):
        failures.append(
            "primitive-strict original bundled examples still fail strict CKB/ProofPlan policy: "
            + ", ".join(report["strict_original_ckb_compile_policy_fail_closed"])
        )
    if report.get("strict_original_ckb_compile_unexpected_failures"):
        failures.append(
            "primitive-strict original bundled examples have unexpected compile failures: "
            + ", ".join(report["strict_original_ckb_compile_unexpected_failures"])
        )
    fail_closed_actions = [
        f"{record['example']}:{record.get('action')}"
        for record in report.get("original_scoped_action_fail_closed", [])
    ]
    fail_closed_locks = [
        f"{record['example']}:{record.get('lock')}"
        for record in report.get("original_scoped_lock_fail_closed", [])
    ]
    if fail_closed_actions or fail_closed_locks:
        failures.append(
            "original scoped entries still intentionally fail closed: "
            + ", ".join([*fail_closed_actions, *fail_closed_locks])
        )
    non_original_harnesses = [
        record["name"]
        for key in (
            "token_action_artifacts",
            "nft_action_artifacts",
            "timelock_action_artifacts",
            "amm_action_artifacts",
            "multisig_action_artifacts",
            "launch_action_artifacts",
        )
        for record in report.get(key, [])
        if record.get("kind") != "original-scoped-action-strict"
    ]
    if non_original_harnesses:
        failures.append(
            "on-chain action harnesses still use synthetic or non-original sources: "
            + ", ".join(non_original_harnesses)
        )
    coverage = report.get("ckb_business_coverage") or {}
    if coverage.get("expected_fail_closed_action_count", 0) or coverage.get("expected_fail_closed_lock_count", 0):
        failures.append(
            "source coverage matrix still includes expected fail-closed entries"
        )
    return failures

production_failures = production_gate_failures(report)
report["production_gate"] = {
    "status": "passed" if not production_failures else "failed",
    "failures": production_failures,
    "requires_original_scoped_harnesses": True,
    "requires_no_expected_fail_closed_entries": True,
    "requires_all_bundled_examples_strict_original_ckb": True,
    "requires_ckb_elf_entry_abi_gate": True,
    "requires_cellscript_build_reports": True,
}
if acceptance_mode == "production" and production_failures:
    report["status"] = "failed-production-gate"
    report["production_ready"] = False
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    raise SystemExit(
        "CKB production gate failed; rerun with --bounded only for development coverage. "
        + "Failures: "
        + " | ".join(production_failures)
    )
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

if [[ "$RUN_ONCHAIN" != "1" ]]; then
  python3 - "$REPORT_JSON" "$CKB_REPO" "$CKB_BIN" "$RPC_URL" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
report = json.loads(report_path.read_text(encoding="utf-8"))
report.update({
    "status": "passed",
    "ckb_repo": sys.argv[2],
    "ckb_bin": sys.argv[3],
    "rpc_url": sys.argv[4],
    "onchain": {"status": "skipped", "reason": "compile-only"},
})
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  if [[ "$ACCEPTANCE_MODE" == "production" ]]; then
    python3 "$REPO_ROOT/scripts/validate_ckb_cellscript_production_evidence.py" "$REPORT_JSON" --compile-only
    echo "CKB compile-only production evidence is not sufficient for external release; run without --compile-only for final hardening." >&2
  fi
  echo "CKB CellScript $ACCEPTANCE_MODE compile-only acceptance passed: $REPORT_JSON"
  exit 0
fi

"$CKB_BIN" -C "$CKB_DIR" run --ba-advanced > "$CKB_LOG" 2>&1 &
CKB_PID="$!"

ready=0
for _ in $(seq 1 120); do
  if curl -sS --noproxy '*' \
    -H 'Content-Type: application/json' \
    -d '{"id":1,"jsonrpc":"2.0","method":"get_tip_header","params":[]}' \
    "$RPC_URL" > "$RUN_DIR/rpc-ready.json" 2>/dev/null; then
    if python3 - "$RUN_DIR/rpc-ready.json" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
raise SystemExit(0 if payload.get("result") and not payload.get("error") else 1)
PY
    then
      ready=1
      break
    fi
  fi
  if ! kill -0 "$CKB_PID" >/dev/null 2>&1; then
    echo "CKB process exited before RPC became ready. Log: $CKB_LOG" >&2
    tail -100 "$CKB_LOG" >&2 || true
    exit 1
  fi
  sleep 1
done

if [[ "$ready" != "1" ]]; then
  echo "CKB RPC did not become ready at $RPC_URL. Log: $CKB_LOG" >&2
  tail -100 "$CKB_LOG" >&2 || true
  exit 1
fi

python3 - "$RPC_URL" "$REPORT_JSON" "$CKB_REPO" "$CKB_BIN" "$CKB_LOG" "$REPO_ROOT" "$RUN_STATEFUL_SCENARIOS" <<'PY'
import hashlib
import json
import os
import pathlib
import re
import shutil
import sys
import time
import urllib.error
import urllib.request

rpc_url, report_path, ckb_repo, ckb_bin, ckb_log, repo_root, run_stateful_scenarios = sys.argv[1:]
report_path = pathlib.Path(report_path)
ckb_repo = pathlib.Path(ckb_repo)
repo_root = pathlib.Path(repo_root)
run_stateful_scenarios = run_stateful_scenarios == "1"

ALWAYS_SUCCESS_CODE_HASH = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5"
ALWAYS_SUCCESS_INDEX = "0x5"
UNEXPECTED_PROFILE_TRAILER = bytes.fromhex("53504f5241424900")
LOCK_BEHAVIOR_ACCEPTANCE_SCOPE = {
    "strict_compile_only": False,
    "onchain_lock_spend_matrix": True,
    "onchain_lock_spend_matrix_scope": {
        "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
        "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
        "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "not_expired", "emergency_approved"],
        "vesting.cell": ["vesting_admin"],
    },
    "required_cases_per_lock": ["valid_spend", "invalid_spend"],
    "scope_note": (
        "Scoped lock entries are strict-compiled under the CKB profile and each lock is exercised "
        "through builder-backed local CKB valid-spend and invalid-spend transactions."
    ),
}

report = json.loads(report_path.read_text(encoding="utf-8"))
artifacts = report.get("artifacts", [])
if not artifacts:
    raise RuntimeError("acceptance report does not contain artifacts")
bundled_example_deployment_artifacts = report.get("bundled_example_deployment_artifacts", [])
token_action_artifacts = report.get("token_action_artifacts", [])
nft_action_artifacts = report.get("nft_action_artifacts", [])
timelock_action_artifacts = report.get("timelock_action_artifacts", [])
amm_action_artifacts = report.get("amm_action_artifacts", [])
multisig_action_artifacts = report.get("multisig_action_artifacts", [])
vesting_action_artifacts = [
    record
    for record in report.get("original_scoped_action_artifacts", [])
    if record.get("example") == "vesting.cell"
    and record.get("action") in {"create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"}
]
launch_action_artifacts = report.get("launch_action_artifacts", [])
original_scoped_lock_artifacts = report.get("original_scoped_lock_artifacts", [])

report.update({
    "status": "running-onchain",
    "lock_acceptance_scope": LOCK_BEHAVIOR_ACCEPTANCE_SCOPE,
    "ckb_repo": str(ckb_repo),
    "ckb_bin": ckb_bin,
    "ckb_log": ckb_log,
    "rpc_url": rpc_url,
    "onchain": {
        "status": "running",
        "chain_template": "ckb/test/template integration devnet",
        "always_success_system_cell_index": ALWAYS_SUCCESS_INDEX,
        "artifact_runs": [],
        "bundled_example_deployment_runs": [],
        "token_action_runs": [],
        "nft_action_runs": [],
        "timelock_action_runs": [],
        "multisig_action_runs": [],
        "vesting_action_runs": [],
        "amm_action_runs": [],
        "launch_action_runs": [],
        "lock_spend_matrix_runs": [],
        "stateful_scenario_runs": [],
    },
})

def refresh_build_report_deployments():
    build_index = report.get("cellscript_build_reports") or {}
    reports = build_index.get("reports") or []
    by_artifact = {
        row.get("artifact_path"): row
        for row in reports
        if isinstance(row, dict) and isinstance(row.get("artifact_path"), str)
    }
    for row in reports:
        if isinstance(row, dict):
            row["onchain_deployments"] = []

    unexpected_artifacts = []

    def add_deployment(run, *, name=None, kind=None, code=None):
        code = code or run
        artifact = code.get("artifact")
        row = by_artifact.get(artifact)
        if row is None:
            unexpected_artifacts.append(artifact)
            return
        deploy = code.get("code_cell_deploy") or {}
        code_dep = code.get("code_cell_dep") or {}
        out_point_value = code_dep.get("out_point")
        artifact_hash = code.get("artifact_ckb_data_hash_blake2b")
        live_hash = code.get("live_code_cell_data_hash")
        row["onchain_deployments"].append({
            "run_name": name or run.get("name") or row.get("name"),
            "run_kind": kind or run.get("kind") or row.get("kind"),
            "out_point": out_point_value,
            "tx_hash": deploy.get("tx_hash"),
            "output_index": "0x0",
            "artifact_ckb_data_hash_blake2b": artifact_hash,
            "live_code_cell_data_hash": live_hash,
            "live_code_cell_data_hash_matches_artifact": live_hash == artifact_hash,
            "code_cell_live": code.get("code_cell_live") is True,
        })

    for run in report["onchain"].get("artifact_runs", []):
        add_deployment(run, kind="artifact-spend")
    for run in report["onchain"].get("bundled_example_deployment_runs", []):
        add_deployment(run, kind="bundled-example-deployment")
    for key in (
        "token_action_runs",
        "nft_action_runs",
        "timelock_action_runs",
        "multisig_action_runs",
        "vesting_action_runs",
        "amm_action_runs",
        "launch_action_runs",
        "lock_spend_matrix_runs",
    ):
        for run in report["onchain"].get(key, []):
            code = run.get("code")
            if isinstance(code, dict):
                add_deployment(run, kind=key.removesuffix("_runs"), code=code)

    missing = [
        row.get("name")
        for row in reports
        if isinstance(row, dict) and not row.get("onchain_deployments")
    ]
    mismatches = [
        f"{row.get('name')}:{deployment.get('run_name')}"
        for row in reports
        if isinstance(row, dict)
        for deployment in row.get("onchain_deployments", [])
        if deployment.get("live_code_cell_data_hash_matches_artifact") is not True
        or deployment.get("code_cell_live") is not True
    ]
    build_index.update({
        "onchain_deployed_artifact_count": sum(
            1 for row in reports if isinstance(row, dict) and row.get("onchain_deployments")
        ),
        "live_code_cell_data_hash_match_count": sum(
            1
            for row in reports
            if isinstance(row, dict)
            and row.get("onchain_deployments")
            and all(
                deployment.get("live_code_cell_data_hash_matches_artifact") is True
                and deployment.get("code_cell_live") is True
                for deployment in row.get("onchain_deployments", [])
            )
        ),
        "missing_onchain_deployments": missing,
        "live_code_cell_data_hash_mismatches": mismatches,
        "unexpected_onchain_artifacts": [value for value in unexpected_artifacts if value],
        "status": "passed" if not missing and not mismatches and not unexpected_artifacts else "failed",
    })
    return build_index

def write_report():
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def update_ckb_business_coverage(onchain_actions):
    coverage = report.get("ckb_business_coverage") or {}
    rows = coverage.get("rows") or []
    for row in rows:
        example = row["example"]
        strict_actions = row.get("strict_ckb_actions") or []
        ckb_onchain_actions = onchain_actions.get(example, [])
        row["ckb_onchain_actions"] = ckb_onchain_actions
        row["missing_ckb_onchain_actions"] = sorted(set(strict_actions) - set(ckb_onchain_actions))
        row["ckb_onchain_action_coverage_complete"] = not row["missing_ckb_onchain_actions"]

    strict_complete = all(
        row.get("strict_action_coverage_complete") and row.get("strict_lock_coverage_complete")
        for row in rows
    )
    onchain_complete = all(row.get("ckb_onchain_action_coverage_complete") for row in rows)
    coverage.update({
        "status": "complete" if strict_complete and onchain_complete else "incomplete",
        "strict_compile_coverage_complete": strict_complete,
        "onchain_action_coverage_complete": onchain_complete,
        "ckb_onchain_action_count": sum(len(row.get("ckb_onchain_actions") or []) for row in rows),
        "missing_ckb_onchain_actions": {
            row["example"]: row["missing_ckb_onchain_actions"]
            for row in rows
            if row.get("missing_ckb_onchain_actions")
        },
        "rows": rows,
    })
    report["ckb_business_coverage"] = coverage
    report["production_ready"] = (
        report.get("acceptance_mode") == "production"
        and coverage["status"] == "complete"
        and (report.get("production_gate") or {}).get("status") == "passed"
    )

RPC_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))

def rpc(method, params=None):
    body = json.dumps({"id": 42, "jsonrpc": "2.0", "method": method, "params": params or []}).encode("utf-8")
    last_error = None
    for attempt in range(6):
        request = urllib.request.Request(rpc_url, data=body, headers={"Content-Type": "application/json"})
        try:
            with RPC_OPENER.open(request, timeout=20) as response:
                payload = json.loads(response.read().decode("utf-8"))
                break
        except urllib.error.HTTPError as error:
            if error.code not in {502, 503, 504}:
                raise RuntimeError(f"RPC {method} failed to connect: {error}") from error
            last_error = error
        except urllib.error.URLError as error:
            last_error = error
        if attempt == 5:
            raise RuntimeError(f"RPC {method} failed to connect after retries: {last_error}") from last_error
        time.sleep(0.25 * (attempt + 1))
    if payload.get("error"):
        raise RuntimeError(f"RPC {method} returned error: {payload['error']}")
    return payload.get("result")

def hex_u64(value):
    if isinstance(value, str):
        value = int(value, 16)
    return hex(value)

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

def data_hash(data):
    return "0x" + hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()

def live_cell_data_hash(live_cell):
    cell = (live_cell or {}).get("cell") or {}
    data = cell.get("data") or {}
    if isinstance(data, dict):
        reported_hash = data.get("hash")
        if isinstance(reported_hash, str) and reported_hash.startswith("0x"):
            return reported_hash
        content = data.get("content")
    else:
        content = data
    if isinstance(content, str) and content.startswith("0x"):
        return data_hash(bytes.fromhex(content[2:]))
    raise RuntimeError(f"live cell does not expose code data hash/content: {live_cell}")

def ckb_hash(data):
    return hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").digest()

def molecule_u32(value):
    return int(value).to_bytes(4, "little")

def molecule_bytes(data):
    return molecule_u32(len(data)) + data

def molecule_string_witness(data):
    return molecule_bytes(molecule_bytes(data))

def molecule_fixvec(items):
    out = bytearray(molecule_u32(len(items)))
    for item in items:
        out.extend(item)
    return bytes(out)

def molecule_table(fields):
    header_size = 4 + 4 * len(fields)
    offsets = []
    cursor = header_size
    for field in fields:
        offsets.append(cursor)
        cursor += len(field)
    out = bytearray()
    out.extend(molecule_u32(cursor))
    for offset in offsets:
        out.extend(molecule_u32(offset))
    for field in fields:
        out.extend(field)
    return bytes(out)

def hash_type_byte(hash_type):
    values = {"data": 0, "type": 1, "data1": 2, "data2": 4}
    if hash_type not in values:
        raise RuntimeError(f"unsupported hash_type for packed Script hash: {hash_type}")
    return bytes([values[hash_type]])

def decode_hex(value, expected_len=None):
    if not isinstance(value, str) or not value.startswith("0x"):
        raise RuntimeError(f"expected 0x-prefixed hex string, got {value!r}")
    data = bytes.fromhex(value[2:])
    if expected_len is not None and len(data) != expected_len:
        raise RuntimeError(f"expected {expected_len} bytes, got {len(data)}")
    return data

def script_molecule(script):
    return molecule_table([
        decode_hex(script["code_hash"], 32),
        hash_type_byte(script["hash_type"]),
        molecule_bytes(decode_hex(script.get("args", "0x"))),
    ])

def script_hash(script):
    return "0x" + ckb_hash(script_molecule(script)).hex()

def token_data(amount, symbol=b"TOKEN001"):
    if len(symbol) != 8:
        raise RuntimeError(f"token symbol must be exactly 8 bytes, got {len(symbol)}")
    return amount.to_bytes(8, "little") + symbol

def pool_data(token_a_symbol, token_b_symbol, reserve_a, reserve_b, total_lp, fee_rate_bps):
    return token_a_symbol + token_b_symbol + reserve_a.to_bytes(8, "little") + reserve_b.to_bytes(8, "little") + total_lp.to_bytes(8, "little") + fee_rate_bps.to_bytes(2, "little")

def lp_receipt_data(pool_id, lp_amount, provider):
    return pool_id + lp_amount.to_bytes(8, "little") + provider

def mint_authority_data(token_symbol=b"TOKEN001", max_supply=1000, minted=0):
    if len(token_symbol) != 8:
        raise RuntimeError(f"mint authority symbol must be exactly 8 bytes, got {len(token_symbol)}")
    return token_symbol + max_supply.to_bytes(8, "little") + minted.to_bytes(8, "little")

def fixed_recipient_tuple_array(recipients):
    if len(recipients) != 2:
        raise RuntimeError(f"launch recipients must contain exactly 2 entries, got {len(recipients)}")
    out = bytearray()
    for address, amount in recipients:
        if len(address) != 32:
            raise RuntimeError(f"launch recipient address must be exactly 32 bytes, got {len(address)}")
        out.extend(address)
        out.extend(int(amount).to_bytes(8, "little"))
    return bytes(out)

def fixed_recipient_tuple_array4(recipients):
    if len(recipients) != 4:
        raise RuntimeError(f"launch recipients must contain exactly 4 entries, got {len(recipients)}")
    out = bytearray()
    for address, amount in recipients:
        if len(address) != 32:
            raise RuntimeError(f"launch recipient address must be exactly 32 bytes, got {len(address)}")
        out.extend(address)
        out.extend(int(amount).to_bytes(8, "little"))
    return bytes(out)

def fixed_address_array4(addresses):
    if len(addresses) != 4:
        raise RuntimeError(f"address array must contain exactly 4 entries, got {len(addresses)}")
    out = bytearray()
    for address in addresses:
        if len(address) != 32:
            raise RuntimeError(f"address array entry must be exactly 32 bytes, got {len(address)}")
        out.extend(address)
    return bytes(out)

def fixed_hash_array4(hashes):
    if len(hashes) != 4:
        raise RuntimeError(f"hash array must contain exactly 4 entries, got {len(hashes)}")
    out = bytearray()
    for value in hashes:
        if len(value) != 32:
            raise RuntimeError(f"hash array entry must be exactly 32 bytes, got {len(value)}")
        out.extend(value)
    return bytes(out)

def fixed_u64_array4(values):
    if len(values) != 4:
        raise RuntimeError(f"u64 array must contain exactly 4 entries, got {len(values)}")
    out = bytearray()
    for value in values:
        out.extend(int(value).to_bytes(8, "little"))
    return bytes(out)

def nft_data(token_id, owner, metadata_hash, royalty_recipient, royalty_bps):
    if len(owner) != 32:
        raise RuntimeError(f"NFT owner must be exactly 32 bytes, got {len(owner)}")
    if len(metadata_hash) != 32:
        raise RuntimeError(f"NFT metadata hash must be exactly 32 bytes, got {len(metadata_hash)}")
    if len(royalty_recipient) != 32:
        raise RuntimeError(f"NFT royalty recipient must be exactly 32 bytes, got {len(royalty_recipient)}")
    return (
        token_id.to_bytes(8, "little")
        + owner
        + metadata_hash
        + royalty_recipient
        + royalty_bps.to_bytes(2, "little")
    )

def collection_data(creator, total_supply, max_supply):
    if len(creator) != 32:
        raise RuntimeError(f"Collection creator must be exactly 32 bytes, got {len(creator)}")
    return creator + total_supply.to_bytes(8, "little") + max_supply.to_bytes(8, "little")

def collection_molecule_data(creator, total_supply, max_supply, name=b"Acceptance Collection", symbol=b"ACPT", base_uri=b"ckb://cellscript/nft/"):
    if len(creator) != 32:
        raise RuntimeError(f"Collection creator must be exactly 32 bytes, got {len(creator)}")
    return molecule_table([
        molecule_bytes(name),
        molecule_bytes(symbol),
        creator,
        total_supply.to_bytes(8, "little"),
        max_supply.to_bytes(8, "little"),
        molecule_bytes(base_uri),
    ])

def listing_data(token_id, seller, price, created_at, state=None):
    if len(seller) != 32:
        raise RuntimeError(f"Listing seller must be exactly 32 bytes, got {len(seller)}")
    if state is not None and not 0 <= state <= 255:
        raise RuntimeError(f"Listing state must fit in u8, got {state}")
    payload = token_id.to_bytes(8, "little") + seller + price.to_bytes(8, "little") + created_at.to_bytes(8, "little")
    return payload if state is None else payload + bytes([state])

def offer_data(token_id, buyer, price, expires_at, state=None):
    if len(buyer) != 32:
        raise RuntimeError(f"Offer buyer must be exactly 32 bytes, got {len(buyer)}")
    if state is not None and not 0 <= state <= 255:
        raise RuntimeError(f"Offer state must fit in u8, got {state}")
    payload = token_id.to_bytes(8, "little") + buyer + price.to_bytes(8, "little") + expires_at.to_bytes(8, "little")
    return payload if state is None else payload + bytes([state])

def royalty_payment_data(token_id, recipient, amount):
    if len(recipient) != 32:
        raise RuntimeError(f"RoyaltyPayment recipient must be exactly 32 bytes, got {len(recipient)}")
    return token_id.to_bytes(8, "little") + recipient + amount.to_bytes(8, "little")

def timelock_data(owner, lock_type, unlock_height, created_at, lock_id=None):
    if lock_id is not None and len(lock_id) != 32:
        raise RuntimeError(f"TimeLock lock_id must be exactly 32 bytes, got {len(lock_id)}")
    if len(owner) != 32:
        raise RuntimeError(f"TimeLock owner must be exactly 32 bytes, got {len(owner)}")
    if not 0 <= lock_type <= 255:
        raise RuntimeError(f"TimeLock lock_type must fit in u8, got {lock_type}")
    payload = owner + bytes([lock_type]) + unlock_height.to_bytes(8, "little") + created_at.to_bytes(8, "little")
    return payload if lock_id is None else lock_id + payload

def locked_asset_data(amount, lock_hash):
    if len(lock_hash) != 32:
        raise RuntimeError(f"LockedAsset lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    return amount.to_bytes(8, "little") + lock_hash

def locked_asset_molecule_data(asset_type, amount, lock_hash):
    if len(lock_hash) != 32:
        raise RuntimeError(f"LockedAsset lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    return molecule_table([
        asset_type,
        amount.to_bytes(8, "little"),
        lock_hash,
    ])

def release_request_data(lock_hash, requester, requested_at, state=None):
    if len(lock_hash) != 32:
        raise RuntimeError(f"ReleaseRequest lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    if len(requester) != 32:
        raise RuntimeError(f"ReleaseRequest requester must be exactly 32 bytes, got {len(requester)}")
    if state is not None and not 0 <= state <= 255:
        raise RuntimeError(f"ReleaseRequest state must fit in u8, got {state}")
    payload = lock_hash + requester + requested_at.to_bytes(8, "little")
    return payload if state is None else payload + bytes([state])

def emergency_release_data(lock_hash, requester, requested_at, approvals):
    if len(lock_hash) != 32:
        raise RuntimeError(f"EmergencyRelease lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    if len(requester) != 32:
        raise RuntimeError(f"EmergencyRelease requester must be exactly 32 bytes, got {len(requester)}")
    if not 0 <= approvals <= 255:
        raise RuntimeError(f"EmergencyRelease approvals must fit in u8, got {approvals}")
    return lock_hash + requester + requested_at.to_bytes(8, "little") + bytes([approvals])

def emergency_release_molecule_data(lock_hash, requester, reason, requested_at, approvers, state=0):
    if len(lock_hash) != 32:
        raise RuntimeError(f"EmergencyRelease lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    if len(requester) != 32:
        raise RuntimeError(f"EmergencyRelease requester must be exactly 32 bytes, got {len(requester)}")
    if not 0 <= state <= 255:
        raise RuntimeError(f"EmergencyRelease state must fit in u8, got {state}")
    for approver in approvers:
        if len(approver) != 32:
            raise RuntimeError(f"EmergencyRelease approver must be exactly 32 bytes, got {len(approver)}")
    return molecule_table([
        lock_hash,
        requester,
        reason,
        requested_at.to_bytes(8, "little"),
        molecule_fixvec(approvers),
        bytes([state]),
    ])

def release_record_data(lock_hash, released_at, released_by):
    if len(lock_hash) != 32:
        raise RuntimeError(f"ReleaseRecord lock_hash must be exactly 32 bytes, got {len(lock_hash)}")
    if len(released_by) != 32:
        raise RuntimeError(f"ReleaseRecord released_by must be exactly 32 bytes, got {len(released_by)}")
    return lock_hash + released_at.to_bytes(8, "little") + released_by

def multisig_wallet_data(wallet_id, signer_a, signer_b, threshold, nonce, created_at):
    if len(wallet_id) != 32:
        raise RuntimeError(f"MultisigWallet wallet_id must be exactly 32 bytes, got {len(wallet_id)}")
    if len(signer_a) != 32:
        raise RuntimeError(f"MultisigWallet signer_a must be exactly 32 bytes, got {len(signer_a)}")
    if len(signer_b) != 32:
        raise RuntimeError(f"MultisigWallet signer_b must be exactly 32 bytes, got {len(signer_b)}")
    if not 0 <= threshold <= 255:
        raise RuntimeError(f"MultisigWallet threshold must fit in u8, got {threshold}")
    return wallet_id + signer_a + signer_b + bytes([threshold]) + nonce.to_bytes(8, "little") + created_at.to_bytes(8, "little")

def multisig_wallet_molecule_data(wallet_id, signers, threshold, nonce, created_at):
    if len(wallet_id) != 32:
        raise RuntimeError(f"MultisigWallet wallet_id must be exactly 32 bytes, got {len(wallet_id)}")
    if len(signers) < 2:
        raise RuntimeError(f"MultisigWallet signers must contain at least 2 entries, got {len(signers)}")
    for signer in signers:
        if len(signer) != 32:
            raise RuntimeError(f"MultisigWallet signer must be exactly 32 bytes, got {len(signer)}")
    if not 0 <= threshold <= 255:
        raise RuntimeError(f"MultisigWallet threshold must fit in u8, got {threshold}")
    return molecule_table([
        wallet_id,
        molecule_fixvec(signers),
        bytes([threshold]),
        nonce.to_bytes(8, "little"),
        created_at.to_bytes(8, "little"),
    ])

def multisig_proposal_molecule_data(wallet_id, proposal_id, proposer, operation, target, amount, data, signatures, required_signatures, created_at, expires_at, state=0):
    if len(wallet_id) != 32:
        raise RuntimeError(f"Proposal wallet_id must be exactly 32 bytes, got {len(wallet_id)}")
    if len(proposer) != 32:
        raise RuntimeError(f"Proposal proposer must be exactly 32 bytes, got {len(proposer)}")
    if len(target) != 32:
        raise RuntimeError(f"Proposal target must be exactly 32 bytes, got {len(target)}")
    if not 0 <= operation <= 255:
        raise RuntimeError(f"Proposal operation must fit in u8, got {operation}")
    if not 0 <= required_signatures <= 255:
        raise RuntimeError(f"Proposal required_signatures must fit in u8, got {required_signatures}")
    if not 0 <= state <= 255:
        raise RuntimeError(f"Proposal state must fit in u8, got {state}")
    encoded_signatures = []
    for signer, signature in signatures:
        if len(signer) != 32:
            raise RuntimeError(f"Proposal signature signer must be exactly 32 bytes, got {len(signer)}")
        if len(signature) != 64:
            raise RuntimeError(f"Proposal signature bytes must be exactly 64 bytes, got {len(signature)}")
        encoded_signatures.append(signer + signature)
    return molecule_table([
        wallet_id,
        proposal_id.to_bytes(8, "little"),
        proposer,
        bytes([operation]),
        target,
        amount.to_bytes(8, "little"),
        molecule_fixvec([bytes([byte]) for byte in data]),
        bytes([required_signatures]),
        molecule_fixvec(encoded_signatures),
        created_at.to_bytes(8, "little"),
        expires_at.to_bytes(8, "little"),
        bytes([state]),
    ])

def multisig_proposal_data(wallet_id, proposal_id, proposer, operation, target, amount, required_signatures, signature_count, created_at, expires_at):
    if len(wallet_id) != 32:
        raise RuntimeError(f"Proposal wallet_id must be exactly 32 bytes, got {len(wallet_id)}")
    if len(proposer) != 32:
        raise RuntimeError(f"Proposal proposer must be exactly 32 bytes, got {len(proposer)}")
    if len(target) != 32:
        raise RuntimeError(f"Proposal target must be exactly 32 bytes, got {len(target)}")
    if not 0 <= operation <= 255:
        raise RuntimeError(f"Proposal operation must fit in u8, got {operation}")
    if not 0 <= required_signatures <= 255:
        raise RuntimeError(f"Proposal required_signatures must fit in u8, got {required_signatures}")
    if not 0 <= signature_count <= 255:
        raise RuntimeError(f"Proposal signature_count must fit in u8, got {signature_count}")
    return (
        wallet_id
        + proposal_id.to_bytes(8, "little")
        + proposer
        + bytes([operation])
        + target
        + amount.to_bytes(8, "little")
        + bytes([required_signatures])
        + bytes([signature_count])
        + created_at.to_bytes(8, "little")
        + expires_at.to_bytes(8, "little")
    )

def signature_confirmation_data(proposal_id, signer, timestamp):
    if len(signer) != 32:
        raise RuntimeError(f"SignatureConfirmation signer must be exactly 32 bytes, got {len(signer)}")
    return proposal_id.to_bytes(8, "little") + signer + timestamp.to_bytes(8, "little")

def execution_record_data(proposal_id, executor, executed_at, success):
    if len(executor) != 32:
        raise RuntimeError(f"ExecutionRecord executor must be exactly 32 bytes, got {len(executor)}")
    if not 0 <= success <= 255:
        raise RuntimeError(f"ExecutionRecord success must fit in u8, got {success}")
    return proposal_id.to_bytes(8, "little") + executor + executed_at.to_bytes(8, "little") + bytes([success])

def vesting_config_data(admin, symbol, cliff_period, total_period, revocable):
    if len(admin) != 32:
        raise RuntimeError(f"VestingConfig admin must be exactly 32 bytes, got {len(admin)}")
    if len(symbol) != 8:
        raise RuntimeError(f"VestingConfig token_symbol must be exactly 8 bytes, got {len(symbol)}")
    if revocable not in (0, 1, False, True):
        raise RuntimeError(f"VestingConfig revocable must be boolean-like, got {revocable!r}")
    return (
        admin
        + symbol
        + cliff_period.to_bytes(8, "little")
        + total_period.to_bytes(8, "little")
        + bytes([1 if revocable else 0])
    )

def vesting_grant_data(state, beneficiary, total_amount, claimed_amount, grant_timepoint, cliff_timepoint, end_timepoint, symbol):
    if len(beneficiary) != 32:
        raise RuntimeError(f"VestingGrant beneficiary must be exactly 32 bytes, got {len(beneficiary)}")
    if len(symbol) != 8:
        raise RuntimeError(f"VestingGrant token_symbol must be exactly 8 bytes, got {len(symbol)}")
    return (
        bytes([state])
        + beneficiary
        + total_amount.to_bytes(8, "little")
        + claimed_amount.to_bytes(8, "little")
        + grant_timepoint.to_bytes(8, "little")
        + cliff_timepoint.to_bytes(8, "little")
        + end_timepoint.to_bytes(8, "little")
        + symbol
    )

def entry_witness(*args):
    out = bytearray(b"CSARGv1\0")
    for arg in args:
        if isinstance(arg, int):
            out.extend(arg.to_bytes(8, "little"))
        elif isinstance(arg, bytes):
            out.extend(arg)
        else:
            raise RuntimeError(f"unsupported entry witness arg: {arg!r}")
    return "0x" + bytes(out).hex()

def get_block(block_hash, attempts=20, delay_seconds=0.05):
    block = None
    for _ in range(attempts):
        block = rpc("get_block", [block_hash])
        if block is not None:
            return block
        time.sleep(delay_seconds)
    raise RuntimeError(f"block not found: {block_hash}")

def get_block_by_number(number, attempts=20, delay_seconds=0.05):
    block = None
    for _ in range(attempts):
        block = rpc("get_block_by_number", [hex_u64(number)])
        if block is not None:
            return block
        time.sleep(delay_seconds)
    raise RuntimeError(f"block number not found: {number}")

def epoch_number_from_header(header):
    return int(header["epoch"], 16) & ((1 << 24) - 1)

def wait_header_epoch_at_least(min_epoch, max_blocks=1200):
    last_header = None
    for generated in range(max_blocks + 1):
        last_header = rpc("get_tip_header")
        epoch_number = epoch_number_from_header(last_header)
        if epoch_number >= min_epoch:
            return {
                "hash": last_header["hash"],
                "epoch": last_header["epoch"],
                "epoch_number": epoch_number,
                "generated_blocks": generated,
            }
        if generated < max_blocks:
            rpc("generate_block")
            time.sleep(0.01)
    raise RuntimeError(
        f"tip epoch did not reach {min_epoch} after {max_blocks} generated blocks; "
        f"last_header={last_header}"
    )

RESERVED_SPENDABLE_OUTPOINTS = set()

def spendable_outpoint_key(tx_hash, index):
    return (tx_hash, int(index))

def reserve_spendable_outpoint(tx_hash, index):
    key = spendable_outpoint_key(tx_hash, index)
    if key in RESERVED_SPENDABLE_OUTPOINTS:
        return False
    RESERVED_SPENDABLE_OUTPOINTS.add(key)
    return True

def find_spendable_cellbase(max_blocks=64):
    generated = []
    for _ in range(max_blocks):
        block_hash = rpc("generate_block")
        generated.append(block_hash)
        block = get_block(block_hash)
        cellbase = block["transactions"][0]
        outputs = cellbase.get("outputs", [])
        if outputs:
            for index, output in enumerate(outputs):
                capacity = int(output["capacity"], 16)
                if capacity > 0:
                    if spendable_outpoint_key(cellbase["hash"], index) in RESERVED_SPENDABLE_OUTPOINTS:
                        continue
                    live_status = wait_live_cell(cellbase["hash"], index)
                    if (
                        live_status
                        and live_status.get("status") == "live"
                        and reserve_spendable_outpoint(cellbase["hash"], index)
                    ):
                        return {
                            "block_hash": block_hash,
                            "tx_hash": cellbase["hash"],
                            "index": index,
                            "capacity": capacity,
                            "generated_blocks": generated,
                        }
    raise RuntimeError(f"no spendable cellbase output found after {max_blocks} generated blocks")

def collect_spendable_cellbases(min_capacity, max_cells=256):
    cells = []
    total_capacity = 0
    generated_blocks = []
    while total_capacity < min_capacity and len(cells) < max_cells:
        cell = find_spendable_cellbase()
        cells.append(cell)
        total_capacity += cell["capacity"]
        generated_blocks.extend(cell["generated_blocks"])
    if total_capacity < min_capacity:
        raise RuntimeError(
            f"collected {total_capacity:#x} capacity from {len(cells)} cellbase cells, "
            f"need at least {min_capacity:#x}"
        )
    return {
        "cells": cells,
        "total_capacity": total_capacity,
        "generated_blocks": generated_blocks,
    }

def transaction(input_cells, outputs, outputs_data, cell_deps, witnesses=None, header_deps=None):
    if isinstance(input_cells, dict) and "cells" in input_cells:
        input_cells = input_cells["cells"]
    elif isinstance(input_cells, dict):
        input_cells = [input_cells]
    return {
        "version": "0x0",
        "cell_deps": cell_deps,
        "header_deps": header_deps or [],
        "inputs": [
            {
                "previous_output": out_point(input_cell["tx_hash"], input_cell["index"]),
                "since": "0x0",
            }
            for input_cell in input_cells
        ],
        "outputs": outputs,
        "outputs_data": outputs_data,
        "witnesses": witnesses or [],
    }

def cell_dep_for(cell):
    return {"out_point": out_point(cell["tx_hash"], cell["index"]), "dep_type": "code"}

def parse_hex_u64(value):
    if value is None:
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        return int(value, 16) if value.startswith("0x") else int(value)
    raise RuntimeError(f"unsupported numeric value: {value!r}")

def json_serialized_size_bytes(value):
    return len(json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8"))

def ensure_ckb_tx_measure_bin():
    import pathlib
    import subprocess
    helper_root = report_path.parent / "ckb-tx-measure-helper"
    tx_measure_manifest = helper_root / "Cargo.toml"
    tx_measure_lock = helper_root / "Cargo.lock"
    tx_measure_target = helper_root / "target"
    tx_measure_bin = tx_measure_target / "debug" / "cellscript-ckb-tx-measure"
    if tx_measure_bin.exists():
        return tx_measure_bin
    cargo_env = os.environ.copy()
    toolchain_file = ckb_repo / "rust-toolchain.toml"
    if toolchain_file.exists():
        match = re.search(r'channel\s*=\s*"([^"]+)"', toolchain_file.read_text(encoding="utf-8"))
        if match:
            cargo_env["RUSTUP_TOOLCHAIN"] = match.group(1)
    helper_root.mkdir(parents=True, exist_ok=True)
    source_bin = repo_root / "src" / "bin" / "ckb_tx_measure.rs"
    lock_src = repo_root / "tools" / "ckb-tx-measure" / "Cargo.lock"
    shutil.copyfile(lock_src, tx_measure_lock)
    tx_measure_manifest.write_text(
        f"""[package]
name = "cellscript-ckb-tx-measure"
version = "0.1.0"
edition = "2021"
rust-version = "1.92.0"
publish = false

[workspace]

[[bin]]
name = "cellscript-ckb-tx-measure"
path = "{source_bin.as_posix()}"

[dependencies]
ckb-jsonrpc-types = {{ path = "{(ckb_repo / "util" / "jsonrpc-types").as_posix()}" }}
ckb-types = {{ path = "{(ckb_repo / "util" / "types").as_posix()}" }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
""",
        encoding="utf-8",
    )
    subprocess.run(
        [
            "cargo",
            "generate-lockfile",
            "--manifest-path",
            str(tx_measure_manifest),
        ],
        check=True,
        cwd=helper_root,
        env=cargo_env,
    )
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            str(tx_measure_manifest),
            "--target-dir",
            str(tx_measure_target),
        ],
        check=True,
        cwd=helper_root,
        env=cargo_env,
    )
    if not tx_measure_bin.exists():
        raise RuntimeError(f"ckb tx measure helper was not built at {tx_measure_bin}")
    return tx_measure_bin

def measure_ckb_transaction_shape(valid_tx):
    import json
    import subprocess
    helper = ensure_ckb_tx_measure_bin()
    proc = subprocess.run(
        [str(helper)],
        input=json.dumps(valid_tx, separators=(",", ":")),
        text=True,
        capture_output=True,
    )
    if proc.returncode != 0:
        stderr = (proc.stderr or "").strip()
        stdout = (proc.stdout or "").strip()
        raise RuntimeError(
            f"cellscript-ckb-tx-measure failed with exit {proc.returncode}; stderr={stderr!r}; stdout={stdout!r}"
        )
    return json.loads(proc.stdout)

def measure_release_constraints(valid_tx, valid_dry_run):
    outputs = valid_tx.get("outputs") or []
    outputs_data = valid_tx.get("outputs_data") or []
    witnesses = valid_tx.get("witnesses") or []
    input_count = len(valid_tx.get("inputs") or [])
    cell_dep_count = len(valid_tx.get("cell_deps") or [])
    header_dep_count = len(valid_tx.get("header_deps") or [])
    output_capacity_shannons = sum(parse_hex_u64(output.get("capacity")) or 0 for output in outputs)
    output_data_bytes = sum(len(decode_hex(data)) for data in outputs_data)
    witness_bytes = sum(len(decode_hex(witness)) for witness in witnesses)
    measured_cycles = None
    cycles_status = "dry-run-missing-cycles"
    if isinstance(valid_dry_run, dict):
        measured_cycles = parse_hex_u64(valid_dry_run.get("cycles"))
        if measured_cycles is not None:
            cycles_status = "dry-run-measured"
    tx_shape = None
    tx_size_status = "not-measured-by-acceptance"
    occupied_capacity_status = "not-derived-by-acceptance"
    tx_measure_error = None
    try:
        tx_shape = measure_ckb_transaction_shape(valid_tx)
        tx_size_status = "measured-by-cellscript-ckb-tx-measure"
        occupied_capacity_status = "derived-by-cellscript-ckb-tx-measure"
    except Exception as error:
        tx_shape = None
        tx_measure_error = str(error)

    return {
        "measured_cycles": measured_cycles,
        "cycles_status": cycles_status,
        "consensus_serialized_tx_size_bytes": None if tx_shape is None else tx_shape.get("consensus_serialized_tx_size_bytes"),
        "tx_size_status": tx_size_status,
        "tx_measure_error": tx_measure_error,
        "json_envelope_size_bytes": json_serialized_size_bytes(valid_tx),
        "witness_bytes": witness_bytes,
        "output_capacity_shannons": output_capacity_shannons,
        "output_data_bytes": output_data_bytes,
        "occupied_capacity_shannons": None if tx_shape is None else tx_shape.get("occupied_capacity_shannons"),
        "output_occupied_capacity_shannons": [] if tx_shape is None else tx_shape.get("output_occupied_capacity_shannons", []),
        "measured_output_capacity_shannons": [] if tx_shape is None else tx_shape.get("output_capacity_shannons", []),
        "capacity_is_sufficient": None if tx_shape is None else tx_shape.get("capacity_is_sufficient"),
        "under_capacity_output_indexes": [] if tx_shape is None else tx_shape.get("under_capacity_output_indexes", []),
        "occupied_capacity_status": occupied_capacity_status,
        "input_count": input_count,
        "output_count": len(outputs),
        "cell_dep_count": cell_dep_count,
        "header_dep_count": header_dep_count,
        "witness_count": len(witnesses),
    }

def submit_and_commit(tx, label, max_blocks=64):
    tx_hash = rpc("send_test_transaction", [tx, "passthrough"])
    last_status = None
    for generated in range(max_blocks + 1):
        status = rpc("get_transaction", [tx_hash])
        tx_status = (status or {}).get("tx_status", {})
        last_status = tx_status
        if tx_status.get("status") == "committed":
            return {"tx_hash": tx_hash, "generated_blocks_after_submit": generated, "status": tx_status}
        if tx_status.get("status") == "rejected":
            raise RuntimeError(f"{label} was rejected while waiting for commit: {tx_hash}; last_status={tx_status}")
        rpc("generate_block")
        time.sleep(0.05)
    raise RuntimeError(f"{label} was not committed after {max_blocks} generated blocks: {tx_hash}; last_status={last_status}")

def expect_dry_run_rejected(tx, label, expected_fragments):
    try:
        estimate = rpc("dry_run_transaction", [tx])
    except RuntimeError as error:
        message = str(error)
        if not any(fragment in message for fragment in expected_fragments):
            raise RuntimeError(f"{label} was rejected for an unexpected reason: {message}") from error
        forbidden_fragments = (
            "InsufficientCellCapacity",
            "ExceededMaximumAncestorsCount",
            "ExceededMaximumCycles",
            "MaxBlockCycles",
            "MaxBlockBytes",
            "Duplicated",
            "PoolIsFull",
        )
        if any(fragment in message for fragment in forbidden_fragments):
            raise RuntimeError(f"{label} was rejected by a policy/capacity reason: {message}") from error
        return {
            "status": "rejected",
            "check": "dry_run_transaction",
            "reason": message,
            "expected_reason_matched": True,
            "policy_or_capacity_reason": False,
        }
    raise RuntimeError(f"{label} was unexpectedly accepted by dry-run: {estimate}")

def assert_live(tx_hash, index, label):
    result = wait_live_cell(tx_hash, index)
    if not result or result.get("status") != "live":
        raise RuntimeError(f"{label} is not live: {result}")
    return result

def is_transient_dead_outpoint_error(error):
    message = str(error)
    return (
        "Resolve failed Dead(OutPoint" in message
        or "Dead(OutPoint" in message
        or "Resolve failed Unknown(OutPoint" in message
        or "Unknown(OutPoint" in message
    )

def code_cell_deploy_transaction(deploy_input, artifact, always_success_dep):
    return transaction(
        deploy_input,
        [
            {
                "capacity": hex_u64(deploy_input["total_capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x" + artifact.hex()],
        [always_success_dep],
    )

def submit_code_cell_deploy_with_fresh_funding(
    name,
    artifact,
    always_success_dep,
    label_suffix,
    measure_dry_run=False,
    max_attempts=4,
):
    deploy_min_capacity = (len(artifact) + 1_000) * 100_000_000
    last_error = None
    for attempt in range(1, max_attempts + 1):
        deploy_input = collect_spendable_cellbases(deploy_min_capacity)
        deploy_tx = code_cell_deploy_transaction(deploy_input, artifact, always_success_dep)
        try:
            valid_deploy_dry_run = rpc("dry_run_transaction", [deploy_tx]) if measure_dry_run else None
            deploy_result = submit_and_commit(deploy_tx, f"{name} {label_suffix}")
            return {
                "deploy_input": deploy_input,
                "deploy_tx": deploy_tx,
                "valid_deploy_dry_run": valid_deploy_dry_run,
                "code_cell_deploy": deploy_result,
                "deploy_attempts": attempt,
            }
        except RuntimeError as error:
            last_error = error
            if is_transient_dead_outpoint_error(error):
                continue
            raise
    raise RuntimeError(f"{name} {label_suffix} failed after {max_attempts} attempts: {last_error}")

def run_artifact(artifact_record, always_success_dep):
    name = artifact_record["name"]
    artifact_path = pathlib.Path(artifact_record["artifact"])
    artifact = artifact_path.read_bytes()
    artifact_ckb_data_hash = data_hash(artifact)

    result = {
        "name": name,
        "kind": artifact_record["kind"],
        "harness_origin": "handwritten-python-transaction",
        "builder_backed": False,
        "artifact": str(artifact_path),
        "artifact_size_bytes": len(artifact),
        "artifact_ckb_data_hash_blake2b": artifact_ckb_data_hash,
        "artifact_has_unexpected_profile_trailer": UNEXPECTED_PROFILE_TRAILER in artifact[-64:],
    }
    if result["artifact_has_unexpected_profile_trailer"]:
        raise RuntimeError(f"{name} CKB artifact still contains an unexpected non-CKB ABI trailer")

    deploy = submit_code_cell_deploy_with_fresh_funding(name, artifact, always_success_dep, "code-cell deploy")
    deploy_input = deploy["deploy_input"]
    deploy_result = deploy["code_cell_deploy"]
    deploy_live = assert_live(deploy_result["tx_hash"], 0, f"{name} code cell")
    live_data_hash = live_cell_data_hash(deploy_live)
    code_dep = {"out_point": out_point(deploy_result["tx_hash"], 0), "dep_type": "code"}
    result.update({
        "deploy_input": deploy_input,
        "code_cell_deploy": deploy_result,
        "code_cell_live": deploy_live.get("status") == "live",
        "live_code_cell_data_hash": live_data_hash,
        "live_code_cell_data_hash_matches_artifact": live_data_hash == artifact_ckb_data_hash,
        "code_cell_dep": code_dep,
        "deploy_attempts": deploy["deploy_attempts"],
    })
    if not result["live_code_cell_data_hash_matches_artifact"]:
        raise RuntimeError(
            f"{name} live code cell data hash mismatch: "
            f"live={live_data_hash} artifact={artifact_ckb_data_hash}"
        )

    create_input = collect_spendable_cellbases(100 * 100_000_000, max_cells=1)
    cellscript_lock = {"code_hash": artifact_ckb_data_hash, "hash_type": "data1", "args": "0x"}
    create_tx = transaction(
        create_input,
        [
            {
                "capacity": hex_u64(create_input["total_capacity"]),
                "lock": cellscript_lock,
                "type": None,
            }
        ],
        ["0x"],
        [always_success_dep],
    )
    create_result = submit_and_commit(create_tx, f"{name} locked-cell create")
    create_live = assert_live(create_result["tx_hash"], 0, f"{name} locked cell")
    result.update({
        "create_input": create_input,
        "locked_cell_create": create_result,
        "locked_cell_live": create_live.get("status") == "live",
    })

    spend_input = {"tx_hash": create_result["tx_hash"], "index": 0, "capacity": create_input["total_capacity"]}
    missing_dep_spend_tx = transaction(
        spend_input,
        [
            {
                "capacity": hex_u64(spend_input["capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x"],
        [],
    )
    missing_dep_rejection = expect_dry_run_rejected(
        missing_dep_spend_tx,
        f"{name} locked-cell spend without code cell dep",
        ("Resolve", "resolve", "Script", "script", "CellDep", "cell_dep", "code hash"),
    )
    still_live_after_reject = assert_live(create_result["tx_hash"], 0, f"{name} locked cell after malformed spend")
    result.update({
        "malformed_spend_without_code_dep": missing_dep_rejection,
        "locked_cell_live_after_malformed_spend": still_live_after_reject.get("status") == "live",
    })

    spend_tx = transaction(
        spend_input,
        [
            {
                "capacity": hex_u64(spend_input["capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x"],
        [code_dep],
    )
    valid_spend_dry_run = rpc("dry_run_transaction", [spend_tx])
    spend_result = submit_and_commit(spend_tx, f"{name} locked-cell spend")
    spend_live = assert_live(spend_result["tx_hash"], 0, f"{name} spend recipient")
    result.update({
        "valid_spend_dry_run": valid_spend_dry_run,
        "measured_constraints": measure_release_constraints(spend_tx, valid_spend_dry_run),
        "locked_cell_spend": spend_result,
        "spend_recipient_live": spend_live.get("status") == "live",
        "status": "passed",
    })
    return result

def run_bundled_example_deployment(artifact_record, always_success_dep):
    name = artifact_record["name"]
    artifact_path = pathlib.Path(artifact_record["artifact"])
    artifact = artifact_path.read_bytes()
    artifact_ckb_data_hash = data_hash(artifact)

    result = {
        "name": name,
        "kind": artifact_record["kind"],
        "source": artifact_record["source"],
        "artifact": str(artifact_path),
        "artifact_size_bytes": len(artifact),
        "artifact_ckb_data_hash_blake2b": artifact_ckb_data_hash,
        "artifact_has_unexpected_profile_trailer": UNEXPECTED_PROFILE_TRAILER in artifact[-64:],
    }
    if result["artifact_has_unexpected_profile_trailer"]:
        raise RuntimeError(f"{name} CKB artifact still contains an unexpected non-CKB ABI trailer")

    deploy = submit_code_cell_deploy_with_fresh_funding(
        name,
        artifact,
        always_success_dep,
        "bundled-example code-cell deploy",
        measure_dry_run=True,
    )
    deploy_result = deploy["code_cell_deploy"]
    deploy_live = assert_live(deploy_result["tx_hash"], 0, f"{name} bundled-example code cell")
    live_data_hash = live_cell_data_hash(deploy_live)
    result.update({
        "deploy_input": deploy["deploy_input"],
        "valid_deploy_dry_run": deploy["valid_deploy_dry_run"],
        "measured_constraints": measure_release_constraints(deploy["deploy_tx"], deploy["valid_deploy_dry_run"]),
        "code_cell_deploy": deploy_result,
        "code_cell_live": deploy_live.get("status") == "live",
        "live_code_cell_data_hash": live_data_hash,
        "live_code_cell_data_hash_matches_artifact": live_data_hash == artifact_ckb_data_hash,
        "code_cell_dep": {"out_point": out_point(deploy_result["tx_hash"], 0), "dep_type": "code"},
        "deploy_attempts": deploy["deploy_attempts"],
        "status": "passed",
    })
    if not result["live_code_cell_data_hash_matches_artifact"]:
        raise RuntimeError(
            f"{name} live bundled-example code cell data hash mismatch: "
            f"live={live_data_hash} artifact={artifact_ckb_data_hash}"
        )
    return result

def deploy_code_cell(name, artifact_path, always_success_dep):
    artifact = pathlib.Path(artifact_path).read_bytes()
    artifact_ckb_data_hash = data_hash(artifact)
    deploy = submit_code_cell_deploy_with_fresh_funding(name, artifact, always_success_dep, "action code-cell deploy")
    deploy_result = deploy["code_cell_deploy"]
    deploy_live = assert_live(deploy_result["tx_hash"], 0, f"{name} action code cell")
    live_data_hash = live_cell_data_hash(deploy_live)
    result = {
        "artifact": str(artifact_path),
        "artifact_size_bytes": len(artifact),
        "artifact_ckb_data_hash_blake2b": artifact_ckb_data_hash,
        "deploy_input": deploy["deploy_input"],
        "code_cell_deploy": deploy_result,
        "code_cell_live": deploy_live.get("status") == "live",
        "live_code_cell_data_hash": live_data_hash,
        "live_code_cell_data_hash_matches_artifact": live_data_hash == artifact_ckb_data_hash,
        "code_cell_dep": {"out_point": out_point(deploy_result["tx_hash"], 0), "dep_type": "code"},
        "deploy_attempts": deploy["deploy_attempts"],
    }
    if not result["live_code_cell_data_hash_matches_artifact"]:
        raise RuntimeError(
            f"{name} live action code cell data hash mismatch: "
            f"live={live_data_hash} artifact={artifact_ckb_data_hash}"
        )
    return result

def create_script_locked_cells(label, cells, cell_deps, max_attempts=4):
    total_capacity = sum(cell["capacity"] for cell in cells)
    create_fee_capacity = 10 * 100_000_000
    last_error = None
    for attempt in range(1, max_attempts + 1):
        funding = collect_spendable_cellbases(total_capacity + create_fee_capacity)
        tx = transaction(
            funding,
            [
                {
                    "capacity": hex_u64(cell["capacity"]),
                    "lock": cell["lock"],
                    "type": cell.get("type"),
                }
                for cell in cells
            ],
            ["0x" + cell.get("data", b"").hex() for cell in cells],
            cell_deps,
        )
        try:
            result = submit_and_commit(tx, f"{label} input-cell create")
            break
        except RuntimeError as error:
            last_error = error
            if is_transient_dead_outpoint_error(error):
                continue
            raise
    else:
        raise RuntimeError(f"{label} input-cell create failed after {max_attempts} attempts: {last_error}")
    live = [assert_live(result["tx_hash"], index, f"{label} input cell {index}").get("status") == "live" for index in range(len(cells))]
    return {
        "create_input": funding,
        "create_fee_capacity": create_fee_capacity,
        "create_tx": result,
        "created_cells_live": live,
        "cells": [
            {
                "tx_hash": result["tx_hash"],
                "index": index,
                "capacity": cell["capacity"],
                "lock": cell["lock"],
                "type": cell.get("type"),
                "data_hex": "0x" + cell.get("data", b"").hex(),
            }
            for index, cell in enumerate(cells)
        ],
    }

SCRIPT_REJECTION_FRAGMENTS = (
    "Script",
    "script",
    "ValidationFailure",
    "error code",
    "VM",
    "Run result",
    "Invalid",
)
LOCK_PREDICATE_REJECTION_FRAGMENTS = (
    "TransactionFailedToVerify: Script(",
    "source: Inputs[0].Lock",
    "ValidationFailure",
    "error code 5",
)

def lock_spend_case_specs(example, lock_name, lock_script):
    addr_a = bytes([0x11]) * 32
    addr_b = bytes([0x22]) * 32
    addr_c = bytes([0x33]) * 32
    hash_a = bytes([0x44]) * 32
    hash_b = bytes([0x55]) * 32
    zero_hash = bytes(32)
    signature_a = bytes([0xaa]) * 64
    signature_b = bytes([0xbb]) * 64
    cell_capacity = 1_000 * 100_000_000

    def cell(data):
        return {
            "capacity": cell_capacity,
            "lock": lock_script,
            "type": None,
            "data": data,
        }

    proposal_valid = multisig_proposal_molecule_data(
        hash_a, 1, addr_a, 0, addr_c, 500, b"", [(addr_a, signature_a), (addr_b, signature_b)], 2, 10, 2000
    )
    proposal_missing_signature = multisig_proposal_molecule_data(
        hash_a, 1, addr_a, 0, addr_c, 500, b"", [(addr_a, signature_a)], 2, 10, 2000
    )
    nft_valid = nft_data(1, addr_a, hash_a, addr_b, 250)
    asset_type_native = molecule_table([bytes([0])])
    time_lock_valid = timelock_data(addr_a, 0, 100, 10, lock_id=hash_a)
    lock_seed = bytes([0x66]) * 32
    committed_lock_id = hashlib.blake2b(lock_seed, digest_size=32, person=b"ckb-default-hash").digest()
    time_lock_committed = timelock_data(addr_a, 0, 100, 10, lock_id=committed_lock_id)
    emergency_valid = emergency_release_molecule_data(hash_a, addr_a, b"operator review", 10, [addr_a, addr_b])

    cases = {
        ("multisig.cell", "is_signer_lock"): {
            "valid_cells": [cell(multisig_wallet_molecule_data(hash_a, [addr_a, addr_b], 2, 0, 10))],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(multisig_wallet_molecule_data(hash_a, [addr_a, addr_b], 2, 0, 10))],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("multisig.cell", "can_execute"): {
            "valid_cells": [cell(proposal_valid)],
            "valid_witnesses": [entry_witness(100)],
            "invalid_cells": [cell(proposal_valid)],
            "invalid_witnesses": [entry_witness(2500)],
        },
        ("multisig.cell", "can_cancel"): {
            "valid_cells": [cell(proposal_valid)],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(proposal_valid)],
            "invalid_witnesses": [entry_witness(addr_b)],
        },
        ("multisig.cell", "has_enough_signatures"): {
            "valid_cells": [cell(proposal_valid)],
            "valid_witnesses": [entry_witness()],
            "invalid_cells": [cell(proposal_missing_signature)],
            "invalid_witnesses": [entry_witness()],
        },
        ("multisig.cell", "not_expired"): {
            "valid_cells": [cell(proposal_valid)],
            "valid_witnesses": [entry_witness(100)],
            "invalid_cells": [cell(proposal_valid)],
            "invalid_witnesses": [entry_witness(2500)],
        },
        ("nft.cell", "nft_ownership"): {
            "valid_cells": [cell(nft_valid)],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(nft_valid)],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("nft.cell", "listing_seller"): {
            "valid_cells": [cell(listing_data(1, addr_a, 500, 10, state=0))],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(listing_data(1, addr_a, 500, 10, state=0))],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("nft.cell", "offer_buyer"): {
            "valid_cells": [cell(offer_data(1, addr_b, 500, 2000, state=0))],
            "valid_witnesses": [entry_witness(addr_b)],
            "invalid_cells": [cell(offer_data(1, addr_b, 500, 2000, state=0))],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("nft.cell", "valid_royalty"): {
            "valid_cells": [cell(nft_valid)],
            "valid_witnesses": [entry_witness()],
            "invalid_cells": [cell(nft_data(1, addr_a, hash_a, addr_b, 1001))],
            "invalid_witnesses": [entry_witness()],
        },
        ("nft.cell", "collection_creator"): {
            "valid_cells": [cell(collection_molecule_data(addr_a, 1, 1000))],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(collection_molecule_data(addr_a, 1, 1000))],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("timelock.cell", "can_unlock_lock"): {
            "valid_cells": [cell(time_lock_valid)],
            "valid_witnesses": [entry_witness(101)],
            "invalid_cells": [cell(time_lock_valid)],
            "invalid_witnesses": [entry_witness(99)],
        },
        ("timelock.cell", "is_owner"): {
            "valid_cells": [cell(time_lock_valid)],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(time_lock_valid)],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
        ("timelock.cell", "lock_id_commitment"): {
            "valid_cells": [cell(time_lock_committed)],
            "valid_witnesses": [entry_witness(lock_seed)],
            "invalid_cells": [cell(time_lock_committed)],
            "invalid_witnesses": [entry_witness(hash_b)],
        },
        ("timelock.cell", "asset_matches"): {
            "valid_cells": [cell(locked_asset_molecule_data(asset_type_native, 100, hash_a))],
            "valid_read_deps": [cell(time_lock_valid)],
            "valid_witnesses": [entry_witness(), "0x"],
            "invalid_cells": [cell(locked_asset_molecule_data(asset_type_native, 100, hash_b))],
            "invalid_read_deps": [cell(time_lock_valid)],
            "invalid_witnesses": [entry_witness(), "0x"],
        },
        ("timelock.cell", "not_expired"): {
            "valid_cells": [cell(time_lock_valid)],
            "valid_witnesses": [entry_witness(99)],
            "invalid_cells": [cell(time_lock_valid)],
            "invalid_witnesses": [entry_witness(100)],
        },
        ("timelock.cell", "emergency_approved"): {
            "valid_cells": [cell(emergency_valid)],
            "valid_witnesses": [entry_witness(bytes([2]))],
            "invalid_cells": [cell(emergency_valid)],
            "invalid_witnesses": [entry_witness(bytes([3]))],
        },
        ("vesting.cell", "vesting_admin"): {
            "valid_cells": [cell(vesting_config_data(addr_a, b"VEST0001", 10, 100, True))],
            "valid_witnesses": [entry_witness(addr_a)],
            "invalid_cells": [cell(vesting_config_data(addr_a, b"VEST0001", 10, 100, True))],
            "invalid_witnesses": [entry_witness(addr_c)],
        },
    }
    try:
        return cases[(example, lock_name)]
    except KeyError as exc:
        raise RuntimeError(f"missing lock spend matrix case for {example}:{lock_name}") from exc

def run_lock_spend_case(label, cells, witnesses, cell_deps, commit_valid, read_deps=None):
    read_deps = read_deps or []
    initial = create_script_locked_cells(label, cells + read_deps, cell_deps)
    input_cells = initial["cells"][:len(cells)]
    dep_cells = initial["cells"][len(cells):]
    action_cell_deps = [cell_dep_for(cell) for cell in dep_cells] + cell_deps
    total_capacity = sum(cell["capacity"] for cell in input_cells)
    tx = transaction(
        input_cells,
        [
            {
                "capacity": hex_u64(total_capacity),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x"],
        action_cell_deps,
        witnesses,
    )
    if not commit_valid:
        rejection = expect_dry_run_rejected(tx, f"{label} invalid lock spend", LOCK_PREDICATE_REJECTION_FRAGMENTS)
        live_after_reject = [
            assert_live(cell["tx_hash"], cell["index"], f"{label} invalid input {index} after rejection").get("status") == "live"
            for index, cell in enumerate(initial["cells"])
        ]
        return {
            "input_create": initial,
            "tx": tx,
            "rejection": rejection,
            "input_cells_live_after_rejection": live_after_reject,
            "status": "rejected",
        }

    valid_dry_run = rpc("dry_run_transaction", [tx])
    commit = submit_and_commit(tx, f"{label} valid lock spend")
    output_live = assert_live(commit["tx_hash"], 0, f"{label} valid spend output").get("status") == "live"
    return {
        "input_create": initial,
        "tx": tx,
        "dry_run": valid_dry_run,
        "commit": commit,
        "output_live": output_live,
        "measured_constraints": measure_release_constraints(tx, valid_dry_run),
        "status": "passed",
    }

def run_lock_spend_matrix(lock_record, always_success_dep):
    example = lock_record["example"]
    lock_name = lock_record["lock"]
    name = lock_record["name"]
    code = deploy_code_cell(name, lock_record["artifact"], always_success_dep)
    lock_script = {
        "code_hash": code["artifact_ckb_data_hash_blake2b"],
        "hash_type": "data1",
        "args": "0x",
    }
    cell_deps = [always_success_dep, code["code_cell_dep"]]
    specs = lock_spend_case_specs(example, lock_name, lock_script)
    invalid_spend = run_lock_spend_case(
        f"{name} invalid-spend",
        specs["invalid_cells"],
        specs["invalid_witnesses"],
        cell_deps,
        False,
        specs.get("invalid_read_deps"),
    )
    valid_spend = run_lock_spend_case(
        f"{name} valid-spend",
        specs["valid_cells"],
        specs["valid_witnesses"],
        cell_deps,
        True,
        specs.get("valid_read_deps"),
    )
    return {
        "name": name,
        "example": example,
        "lock": lock_name,
        "kind": lock_record["kind"],
        "harness_origin": "builder-backed-local-ckb-lock-spend-matrix",
        "builder_backed": True,
        "builder_name": "cellscript-lock-spend-matrix-builder-v1",
        "source": lock_record["source"],
        "artifact": lock_record["artifact"],
        "code": code,
        "valid_spend": valid_spend,
        "invalid_spend": invalid_spend,
        "measured_constraints": valid_spend["measured_constraints"],
        "status": "passed",
    }

def build_token_action_case(action, cellscript_lock, cellscript_type, destination_lock, destination_lock_hash, token_symbol, cell_deps):
    def normalized_outputs(outputs):
        return [
            {
                "capacity": hex_u64(output["capacity"]),
                "lock": output["lock"],
                "type": output.get("type"),
            }
            for output in outputs
        ]

    if action == "mint_with_authority":
        initial_specs = [
            {
                "capacity": 1000 * 100_000_000,
                "lock": cellscript_lock,
                "type": cellscript_type,
                "data": mint_authority_data(token_symbol, 1000, 10),
            }
        ]
        valid_outputs = [
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type},
            {"capacity": 100 * 100_000_000, "lock": destination_lock, "type": cellscript_type},
        ]
        valid_outputs_data = [
            "0x" + mint_authority_data(token_symbol, 1000, 15).hex(),
            "0x" + token_data(5, token_symbol).hex(),
        ]
        malformed_outputs = valid_outputs
        malformed_outputs_data = [
            "0x" + mint_authority_data(token_symbol, 1000, 15).hex(),
            "0x" + token_data(6, token_symbol).hex(),
        ]
        witnesses = [entry_witness(destination_lock_hash, 5)]
    elif action == "transfer_token":
        initial_specs = [
            {
                "capacity": 200 * 100_000_000,
                "lock": cellscript_lock,
                "type": cellscript_type,
                "data": token_data(42, token_symbol),
            }
        ]
        valid_outputs = [{"capacity": 200 * 100_000_000, "lock": destination_lock, "type": cellscript_type}]
        valid_outputs_data = ["0x" + token_data(42, token_symbol).hex()]
        malformed_outputs = valid_outputs
        malformed_outputs_data = ["0x" + token_data(41, token_symbol).hex()]
        witnesses = [entry_witness(destination_lock_hash)]
    elif action == "burn":
        initial_specs = [
            {
                "capacity": 100 * 100_000_000,
                "lock": cellscript_lock,
                "type": cellscript_type,
                "data": token_data(7, token_symbol),
            }
        ]
        valid_outputs = [{"capacity": 100 * 100_000_000, "lock": cellscript_lock, "type": None}]
        valid_outputs_data = ["0x"]
        malformed_outputs = [{"capacity": 100 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type}]
        malformed_outputs_data = ["0x" + token_data(7, token_symbol).hex()]
        witnesses = [entry_witness()]
    elif action == "merge":
        initial_specs = [
            {
                "capacity": 300 * 100_000_000,
                "lock": cellscript_lock,
                "type": cellscript_type,
                "data": token_data(40, token_symbol),
            },
            {
                "capacity": 150 * 100_000_000,
                "lock": cellscript_lock,
                "type": cellscript_type,
                "data": token_data(2, token_symbol),
            },
        ]
        valid_outputs = [{"capacity": 300 * 100_000_000, "lock": destination_lock, "type": cellscript_type}]
        valid_outputs_data = ["0x" + token_data(42, token_symbol).hex()]
        malformed_outputs = valid_outputs
        malformed_outputs_data = ["0x" + token_data(41, token_symbol).hex()]
        witnesses = [entry_witness(destination_lock_hash), "0x"]
    else:
        raise RuntimeError(f"unsupported token action harness: {action}")

    initial = create_script_locked_cells(f"token.{action}", initial_specs, cell_deps)
    inputs = initial["cells"] if action == "merge" else initial["cells"][0]
    return {
        "builder_name": "token-action-builder-v1",
        "initial": initial,
        "valid_tx": transaction(
            inputs,
            normalized_outputs(valid_outputs),
            valid_outputs_data,
            cell_deps,
            witnesses,
        ),
        "malformed_tx": transaction(
            inputs,
            normalized_outputs(malformed_outputs),
            malformed_outputs_data,
            cell_deps,
            witnesses,
        ),
    }

def run_token_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    cellscript_type = always_success_lock()
    destination_lock = always_success_lock()
    destination_lock_hash = decode_hex(script_hash(destination_lock), 32)
    token_symbol = b"TOKEN001"
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "token-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
        "destination_lock_hash": "0x" + destination_lock_hash.hex(),
    }
    token_case = build_token_action_case(
        action,
        cellscript_lock,
        cellscript_type,
        destination_lock,
        destination_lock_hash,
        token_symbol,
        cell_deps,
    )
    initial = token_case["initial"]
    valid_tx = token_case["valid_tx"]
    malformed_tx = token_case["malformed_tx"]
    result["builder_name"] = token_case["builder_name"]

    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, cell in enumerate(initial["cells"]):
        assert_live(cell["tx_hash"], cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live" for index in range(len(valid_tx["outputs"]))]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def build_nft_action_case(action_record, cellscript_lock, cellscript_type, destination_lock, current_owner, destination_owner, metadata_hash, royalty_recipient, nft_type, listing_type, offer_type, royalty_payment_type, cell_deps):
    action = action_record["action"]
    original_scoped = action_record.get("kind") == "original-scoped-action-strict"
    flow_state = 0 if original_scoped else None

    if action == "create_collection":
        name = b"Acceptance Collection"
        symbol = b"ACPT"
        base_uri = b"ckb://cellscript/nft/"
        max_supply = 200
        valid_collection_payload = (
            collection_molecule_data(current_owner, 0, max_supply, name, symbol, base_uri)
            if original_scoped
            else collection_data(current_owner, 0, max_supply)
        )
        malformed_collection_payload = (
            collection_molecule_data(current_owner, 1, max_supply, name, symbol, base_uri)
            if original_scoped
            else collection_data(current_owner, 1, max_supply)
        )
        witness = (
            entry_witness(current_owner, max_supply, molecule_string_witness(name), molecule_string_witness(symbol), molecule_string_witness(base_uri))
            if original_scoped
            else entry_witness(current_owner, max_supply)
        )
        initial = create_script_locked_cells(
            "nft.create_collection",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}]
        valid_tx = transaction(input_cell, outputs, ["0x" + valid_collection_payload.hex()], cell_deps, [witness])
        malformed_tx = transaction(input_cell, outputs, ["0x" + malformed_collection_payload.hex()], cell_deps, [witness])
    elif action == "mint":
        input_collection_payload = (
            collection_molecule_data(current_owner, 10, 1000)
            if original_scoped
            else collection_data(current_owner, 10, 1000)
        )
        output_collection_payload = (
            collection_molecule_data(current_owner, 11, 1000)
            if original_scoped
            else collection_data(current_owner, 11, 1000)
        )
        initial = create_script_locked_cells(
            "nft.mint",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type, "data": input_collection_payload}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": destination_lock, "type": cellscript_type},
        ]
        witness = [entry_witness(destination_owner, metadata_hash)]
        valid_tx = transaction(
            input_cell,
            outputs,
            [
                "0x" + output_collection_payload.hex(),
                "0x" + nft_data(11, destination_owner, metadata_hash, current_owner, 250).hex(),
            ],
            cell_deps,
            witness,
        )
        malformed_tx = transaction(
            input_cell,
            outputs,
            [
                "0x" + output_collection_payload.hex(),
                "0x" + nft_data(12, destination_owner, metadata_hash, current_owner, 250).hex(),
            ],
            cell_deps,
            witness,
        )
    elif action == "transfer":
        initial = create_script_locked_cells(
            "nft.transfer",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type, "data": nft_data(1, current_owner, metadata_hash, royalty_recipient, 250)}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}]
        witness = [entry_witness(destination_owner)]
        valid_tx = transaction(input_cell, outputs, ["0x" + nft_data(1, destination_owner, metadata_hash, royalty_recipient, 250).hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + nft_data(1, current_owner, metadata_hash, royalty_recipient, 250).hex()], cell_deps, witness)
    elif action == "create_listing":
        price = 100
        current_time = 55
        token_id = 3
        nft_payload = nft_data(token_id, current_owner, metadata_hash, royalty_recipient, 250)
        initial = create_script_locked_cells(
            "nft.create_listing",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""},
                {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": nft_type, "data": nft_payload},
            ],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": listing_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ]
        witness = [entry_witness(price, current_time)]
        valid_tx = transaction(input_cell, outputs, ["0x" + listing_data(token_id, current_owner, price, current_time, state=flow_state).hex(), "0x"], action_cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + listing_data(token_id, current_owner, price + 1, current_time, state=flow_state).hex(), "0x"], action_cell_deps, witness)
    elif action == "cancel_listing":
        token_id = 4
        price = 120
        created_at = 60
        initial = create_script_locked_cells(
            "nft.cancel_listing",
            [{"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": listing_type, "data": listing_data(token_id, current_owner, price, created_at, state=flow_state)}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": None}]
        witness = [entry_witness()]
        valid_tx = transaction(input_cell, outputs, ["0x"], cell_deps, witness)
        malformed_tx = transaction(input_cell, [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": listing_type}], ["0x" + listing_data(token_id, current_owner, price, created_at, state=flow_state).hex()], cell_deps, witness)
    elif action == "buy_from_listing":
        token_id = 6
        price = 10_000
        payment = 10_000
        royalty_amount = 250
        seller_amount = payment - royalty_amount
        created_at = 70
        nft_payload = nft_data(token_id, current_owner, metadata_hash, royalty_recipient, 250)
        initial = create_script_locked_cells(
            "nft.buy_from_listing",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": nft_type, "data": nft_payload},
                {"capacity": 500 * 100_000_000, "lock": cellscript_lock, "type": listing_type, "data": listing_data(token_id, current_owner, price, created_at, state=flow_state)},
            ],
            cell_deps,
        )
        outputs = [
            {"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": cellscript_lock, "type": royalty_payment_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": cellscript_lock, "type": royalty_payment_type},
        ]
        witness = [entry_witness(destination_owner, current_owner, payment), "0x"]
        valid_tx = transaction(initial["cells"], outputs, [
            "0x" + nft_data(token_id, destination_owner, metadata_hash, royalty_recipient, 250).hex(),
            "0x" + royalty_payment_data(token_id, royalty_recipient, royalty_amount).hex(),
            "0x" + royalty_payment_data(token_id, current_owner, seller_amount).hex(),
        ], cell_deps, witness)
        malformed_tx = transaction(initial["cells"], outputs, [
            "0x" + nft_data(token_id, destination_owner, metadata_hash, royalty_recipient, 250).hex(),
            "0x" + royalty_payment_data(token_id, royalty_recipient, royalty_amount).hex(),
            "0x" + royalty_payment_data(token_id, current_owner, seller_amount + 1).hex(),
        ], cell_deps, witness)
    elif action == "create_offer":
        token_id = 5
        price = 150
        expires_at = 200
        initial = create_script_locked_cells(
            "nft.create_offer",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": offer_type}]
        witness = [entry_witness(token_id, destination_owner, price, expires_at)]
        valid_tx = transaction(input_cell, outputs, ["0x" + offer_data(token_id, destination_owner, price, expires_at, state=flow_state).hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + offer_data(token_id, destination_owner, price + 1, expires_at, state=flow_state).hex()], cell_deps, witness)
    elif action == "accept_offer":
        token_id = 7
        price = 10_000
        royalty_amount = 250
        seller_amount = price - royalty_amount
        expires_at = 200
        current_time = 100
        nft_payload = nft_data(token_id, current_owner, metadata_hash, royalty_recipient, 250)
        initial = create_script_locked_cells(
            "nft.accept_offer",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": nft_type, "data": nft_payload},
                {"capacity": 500 * 100_000_000, "lock": cellscript_lock, "type": offer_type, "data": offer_data(token_id, destination_owner, price, expires_at, state=flow_state)},
            ],
            cell_deps,
        )
        outputs = [
            {"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": cellscript_lock, "type": royalty_payment_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": cellscript_lock, "type": royalty_payment_type},
        ]
        witness = [entry_witness(destination_owner, current_owner, price, current_time), "0x"]
        valid_tx = transaction(initial["cells"], outputs, [
            "0x" + nft_data(token_id, destination_owner, metadata_hash, royalty_recipient, 250).hex(),
            "0x" + royalty_payment_data(token_id, royalty_recipient, royalty_amount).hex(),
            "0x" + royalty_payment_data(token_id, current_owner, seller_amount).hex(),
        ], cell_deps, witness)
        malformed_tx = transaction(initial["cells"], outputs, [
            "0x" + nft_data(token_id, destination_owner, metadata_hash, royalty_recipient, 250).hex(),
            "0x" + royalty_payment_data(token_id, royalty_recipient, royalty_amount).hex(),
            "0x" + royalty_payment_data(token_id, current_owner, seller_amount + 1).hex(),
        ], cell_deps, witness)
    elif action == "burn":
        initial = create_script_locked_cells(
            "nft.burn",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type, "data": nft_data(2, current_owner, metadata_hash, royalty_recipient, 250)}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        witness = [entry_witness()]
        valid_tx = transaction(input_cell, [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": None}], ["0x"], cell_deps, witness)
        malformed_tx = transaction(input_cell, [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}], ["0x" + nft_data(2, current_owner, metadata_hash, royalty_recipient, 250).hex()], cell_deps, witness)
    elif action == "batch_mint":
        collection_type = always_success_lock("0x25")
        recipients = [destination_owner, bytes([0x31]) * 32, bytes([0x32]) * 32, bytes([0x33]) * 32]
        metadata_hashes = [bytes(range(32)), bytes([0x41]) * 32, bytes([0x42]) * 32, bytes([0x43]) * 32]
        input_collection_payload = collection_molecule_data(current_owner, 20, 1000)
        output_collection_payload = collection_molecule_data(current_owner, 24, 1000)
        initial = create_script_locked_cells(
            "nft.batch_mint",
            [{"capacity": 2500 * 100_000_000, "lock": cellscript_lock, "type": collection_type, "data": input_collection_payload}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [
            {"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": collection_type},
            {"capacity": hex_u64(250 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
            {"capacity": hex_u64(250 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
            {"capacity": hex_u64(250 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
            {"capacity": hex_u64(250 * 100_000_000), "lock": cellscript_lock, "type": nft_type},
        ]
        outputs_data = [
            "0x" + output_collection_payload.hex(),
            "0x" + nft_data(21, recipients[0], metadata_hashes[0], current_owner, 250).hex(),
            "0x" + nft_data(22, recipients[1], metadata_hashes[1], current_owner, 250).hex(),
            "0x" + nft_data(23, recipients[2], metadata_hashes[2], current_owner, 250).hex(),
            "0x" + nft_data(24, recipients[3], metadata_hashes[3], current_owner, 250).hex(),
        ]
        witness = [entry_witness(fixed_address_array4(recipients), fixed_hash_array4(metadata_hashes))]
        valid_tx = transaction(input_cell, outputs, outputs_data, cell_deps, witness)
        malformed_outputs_data = list(outputs_data)
        malformed_outputs_data[3] = "0x" + nft_data(99, recipients[2], metadata_hashes[2], current_owner, 250).hex()
        malformed_tx = transaction(input_cell, outputs, malformed_outputs_data, cell_deps, witness)
    else:
        raise RuntimeError(f"unsupported NFT action harness: {action}")

    return {
        "builder_name": "nft-action-builder-v1",
        "initial": initial,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
    }

def run_nft_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    cellscript_type = always_success_lock()
    destination_lock = always_success_lock()
    current_owner = decode_hex(script_hash(cellscript_lock), 32)
    destination_owner = decode_hex(script_hash(destination_lock), 32)
    metadata_hash = bytes(range(32))
    royalty_recipient = bytes(reversed(range(32)))
    nft_type = always_success_lock("0x21")
    listing_type = always_success_lock("0x22")
    offer_type = always_success_lock("0x23")
    royalty_payment_type = always_success_lock("0x24")
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "nft-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
        "destination_owner": "0x" + destination_owner.hex(),
    }
    nft_case = build_nft_action_case(
        action_record,
        cellscript_lock,
        cellscript_type,
        destination_lock,
        current_owner,
        destination_owner,
        metadata_hash,
        royalty_recipient,
        nft_type,
        listing_type,
        offer_type,
        royalty_payment_type,
        cell_deps,
    )
    initial = nft_case["initial"]
    valid_tx = nft_case["valid_tx"]
    malformed_tx = nft_case["malformed_tx"]
    result["builder_name"] = nft_case["builder_name"]

    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, cell in enumerate(initial["cells"]):
        assert_live(cell["tx_hash"], cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def run_amm_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    destination_lock = always_success_lock()
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "amm-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
    }
    amm_case = build_amm_action_case(action_record, cellscript_lock, destination_lock, cell_deps)
    initial = amm_case["initial"]
    input_cells_to_check = amm_case["input_cells_to_check"]
    valid_tx = amm_case["valid_tx"]
    malformed_tx = amm_case["malformed_tx"]
    result["builder_name"] = amm_case["builder_name"]
    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, input_cell in enumerate(input_cells_to_check):
        assert_live(input_cell["tx_hash"], input_cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def build_amm_action_case(action_record, cellscript_lock, destination_lock, cell_deps):
    action = action_record["action"]

    if action == "seed_pool":
        token_a_symbol = b"AMMA0001"
        token_b_symbol = b"AMMB0001"
        token_a_amount = 4
        token_b_amount = 9
        fee_rate_bps = 30
        initial_lp = 6
        provider_lock = always_success_lock("0x61")
        provider = decode_hex(script_hash(provider_lock), 32)
        token_a_type = always_success_lock("0x62")
        token_b_type = always_success_lock("0x63")
        pool_type = always_success_lock("0x64")
        lp_type = always_success_lock("0x65")
        pool_id = decode_hex(script_hash(pool_type), 32)
        initial = create_script_locked_cells("amm.seed_pool", [
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": token_a_type, "data": token_data(token_a_amount, token_a_symbol)},
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": token_b_type, "data": token_data(token_b_amount, token_b_symbol)},
        ], cell_deps)
        valid_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(200 * 100_000_000), "lock": destination_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": lp_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, token_a_amount, token_b_amount, initial_lp, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, initial_lp, provider).hex(),
        ], cell_deps, [entry_witness(fee_rate_bps.to_bytes(2, "little"), provider), "0x"])
        malformed_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(200 * 100_000_000), "lock": destination_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": lp_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, token_a_amount + 1, token_b_amount, initial_lp, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, initial_lp, provider).hex(),
        ], cell_deps, [entry_witness(fee_rate_bps.to_bytes(2, "little"), provider), "0x"])
        input_cells_to_check = initial["cells"]
    elif action == "swap_a_for_b":
        token_a_symbol = b"AMMA0001"
        token_b_symbol = b"AMMB0001"
        pool_reserve_a = 10_000
        pool_reserve_b = 20_000
        pool_total_lp = 10_000
        input_amount = 1_000
        fee_rate_bps = 30
        fee = input_amount * fee_rate_bps // 10_000
        net_input = input_amount - fee
        output_amount = pool_reserve_b * net_input // (pool_reserve_a + net_input)
        min_output = output_amount - 1
        to_lock = always_success_lock("0x70")
        to = decode_hex(script_hash(to_lock), 32)
        token_a_type = always_success_lock("0x71")
        token_b_type = always_success_lock("0x72")
        pool_type = always_success_lock("0x73")
        initial = create_script_locked_cells("amm.swap_a_for_b", [
            {"capacity": 400 * 100_000_000, "lock": cellscript_lock, "type": pool_type, "data": pool_data(token_a_symbol, token_b_symbol, pool_reserve_a, pool_reserve_b, pool_total_lp, fee_rate_bps)},
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": token_a_type, "data": token_data(input_amount, token_a_symbol)},
        ], cell_deps)
        valid_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": to_lock, "type": token_b_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a + input_amount, pool_reserve_b - output_amount, pool_total_lp, fee_rate_bps).hex(),
            "0x" + token_data(output_amount, token_b_symbol).hex(),
        ], cell_deps, [entry_witness(min_output, to), "0x"])
        malformed_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": to_lock, "type": token_b_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a + input_amount, pool_reserve_b - output_amount, pool_total_lp, fee_rate_bps).hex(),
            "0x" + token_data(output_amount + 1, token_b_symbol).hex(),
        ], cell_deps, [entry_witness(min_output, to), "0x"])
        input_cells_to_check = initial["cells"]
    elif action == "add_liquidity":
        token_a_symbol = b"AMMA0001"
        token_b_symbol = b"AMMB0001"
        pool_reserve_a = 100
        pool_reserve_b = 200
        pool_total_lp = 1000
        token_a_amount = 10
        token_b_amount = 20
        minted_lp = 100
        fee_rate_bps = 30
        provider_lock = always_success_lock("0x66")
        provider = decode_hex(script_hash(provider_lock), 32)
        token_a_type = always_success_lock("0x67")
        token_b_type = always_success_lock("0x68")
        pool_type = always_success_lock("0x69")
        lp_type = always_success_lock("0x6a")
        pool_id = decode_hex(script_hash(pool_type), 32)
        initial = create_script_locked_cells("amm.add_liquidity", [
            {"capacity": 400 * 100_000_000, "lock": cellscript_lock, "type": pool_type, "data": pool_data(token_a_symbol, token_b_symbol, pool_reserve_a, pool_reserve_b, pool_total_lp, fee_rate_bps)},
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": token_a_type, "data": token_data(token_a_amount, token_a_symbol)},
            {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": token_b_type, "data": token_data(token_b_amount, token_b_symbol)},
        ], cell_deps)
        valid_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": lp_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a + token_a_amount, pool_reserve_b + token_b_amount, pool_total_lp + minted_lp, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, minted_lp, provider).hex(),
        ], cell_deps, [entry_witness(provider), "0x", "0x"])
        malformed_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": lp_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a + token_a_amount, pool_reserve_b + token_b_amount, pool_total_lp + minted_lp, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, minted_lp + 1, provider).hex(),
        ], cell_deps, [entry_witness(provider), "0x", "0x"])
        input_cells_to_check = initial["cells"]
    elif action == "remove_liquidity":
        token_a_symbol = b"AMMA0001"
        token_b_symbol = b"AMMB0001"
        pool_reserve_a = 100
        pool_reserve_b = 200
        pool_total_lp = 1000
        burned_lp = 100
        withdrawn_a = 10
        withdrawn_b = 20
        fee_rate_bps = 30
        provider_lock = always_success_lock("0x6b")
        provider = decode_hex(script_hash(provider_lock), 32)
        token_a_type = always_success_lock("0x6c")
        token_b_type = always_success_lock("0x6d")
        pool_type = always_success_lock("0x6e")
        lp_type = always_success_lock("0x6f")
        pool_id = decode_hex(script_hash(pool_type), 32)
        initial = create_script_locked_cells("amm.remove_liquidity", [
            {"capacity": 400 * 100_000_000, "lock": cellscript_lock, "type": pool_type, "data": pool_data(token_a_symbol, token_b_symbol, pool_reserve_a, pool_reserve_b, pool_total_lp, fee_rate_bps)},
            {"capacity": 600 * 100_000_000, "lock": cellscript_lock, "type": lp_type, "data": lp_receipt_data(pool_id, burned_lp, provider)},
        ], cell_deps)
        valid_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_a_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_b_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a - withdrawn_a, pool_reserve_b - withdrawn_b, pool_total_lp - burned_lp, fee_rate_bps).hex(),
            "0x" + token_data(withdrawn_a, token_a_symbol).hex(),
            "0x" + token_data(withdrawn_b, token_b_symbol).hex(),
        ], cell_deps, [entry_witness(provider), "0x"])
        malformed_tx = transaction(initial["cells"], [
            {"capacity": hex_u64(400 * 100_000_000), "lock": cellscript_lock, "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_a_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_b_type},
        ], [
            "0x" + pool_data(token_a_symbol, token_b_symbol, pool_reserve_a - withdrawn_a, pool_reserve_b - withdrawn_b, pool_total_lp - burned_lp, fee_rate_bps).hex(),
            "0x" + token_data(withdrawn_a + 1, token_a_symbol).hex(),
            "0x" + token_data(withdrawn_b, token_b_symbol).hex(),
        ], cell_deps, [entry_witness(provider), "0x"])
        input_cells_to_check = initial["cells"]
    else:
        initial = create_script_locked_cells(
            f"amm.{action}",
            [{"capacity": 100 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        if action == "isqrt":
            valid_witness = entry_witness(0)
            malformed_witness = entry_witness(4)
        elif action == "min":
            valid_witness = entry_witness(0, 0)
            malformed_witness = entry_witness(1, 2)
        else:
            raise RuntimeError(f"unsupported AMM action harness: {action}")
        valid_tx = transaction(input_cell, [{"capacity": hex_u64(100 * 100_000_000), "lock": destination_lock, "type": None}], ["0x"], cell_deps, [valid_witness])
        malformed_tx = transaction(input_cell, [{"capacity": hex_u64(100 * 100_000_000), "lock": destination_lock, "type": None}], ["0x"], cell_deps, [malformed_witness])
        input_cells_to_check = [input_cell]

    return {
        "builder_name": "amm-action-builder-v1",
        "initial": initial,
        "input_cells_to_check": input_cells_to_check,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
    }

def run_multisig_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    cellscript_type = always_success_lock()
    wallet_type = always_success_lock("0x51")
    proposal_type = always_success_lock("0x52")
    confirmation_type = always_success_lock("0x53")
    execution_type = always_success_lock("0x54")
    signer_a = decode_hex(script_hash(cellscript_lock), 32)
    signer_b = decode_hex(script_hash(always_success_lock("0x55")), 32)
    signer_c = decode_hex(script_hash(always_success_lock("0x56")), 32)
    target = decode_hex(script_hash(always_success_lock("0x57")), 32)
    wallet_id = decode_hex(script_hash(always_success_lock("0x58")), 32)
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "multisig-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
    }
    multisig_case = build_multisig_action_case(
        action_record,
        cellscript_lock,
        wallet_type,
        proposal_type,
        confirmation_type,
        execution_type,
        signer_a,
        signer_b,
        signer_c,
        target,
        wallet_id,
        cell_deps,
    )
    initial = multisig_case["initial"]
    valid_tx = multisig_case["valid_tx"]
    malformed_tx = multisig_case["malformed_tx"]
    result["builder_name"] = multisig_case["builder_name"]

    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, cell in enumerate(initial["cells"]):
        assert_live(cell["tx_hash"], cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def build_multisig_action_case(action_record, cellscript_lock, wallet_type, proposal_type, confirmation_type, execution_type, signer_a, signer_b, signer_c, target, wallet_id, cell_deps):
    action = action_record["action"]
    original_scoped = action_record.get("kind") == "original-scoped-action-strict"

    if action == "create_wallet":
        current_time = 10
        signers = [signer_a, signer_b]
        signers_payload = molecule_fixvec(signers)
        wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, 2, 0, current_time) if original_scoped else multisig_wallet_data(wallet_id, signer_a, signer_b, 2, 0, current_time)
        malformed_wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, 1, 0, current_time) if original_scoped else multisig_wallet_data(wallet_id, signer_a, signer_b, 1, 0, current_time)
        witness = entry_witness(wallet_id, molecule_bytes(signers_payload), bytes([2]), current_time) if original_scoped else entry_witness(wallet_id, signer_a, signer_b, bytes([2]), current_time)
        initial = create_script_locked_cells(
            "multisig.create_wallet",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": wallet_type}]
        valid_tx = transaction(input_cell, outputs, ["0x" + wallet_payload.hex()], cell_deps, [witness])
        malformed_tx = transaction(input_cell, outputs, ["0x" + malformed_wallet_payload.hex()], cell_deps, [witness])
    elif action in ("propose_transfer", "propose_add_signer", "propose_remove_signer", "propose_change_threshold"):
        current_time = 20
        threshold = 1 if action == "propose_remove_signer" else 2
        initial_nonce = 0
        proposal_id = 1
        signers = [signer_a, signer_b]
        wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, threshold, initial_nonce, 10) if original_scoped else multisig_wallet_data(wallet_id, signer_a, signer_b, threshold, initial_nonce, 10)
        initial = create_script_locked_cells(
            f"multisig.{action}",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": wallet_type, "data": wallet_payload}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        if action == "propose_transfer":
            operation = 0
            proposal_target = target
            amount = 500
            data_payload = b""
            witness = entry_witness(signer_a, target, amount, current_time)
            malformed_witness = entry_witness(signer_b, target, 0, current_time)
        elif action == "propose_add_signer":
            operation = 1
            proposal_target = signer_c
            amount = 0
            data_payload = signer_c
            witness = entry_witness(signer_a, signer_c, current_time)
            malformed_witness = entry_witness(signer_a, signer_a, current_time)
        elif action == "propose_remove_signer":
            operation = 2
            proposal_target = signer_b
            amount = 0
            data_payload = b""
            witness = entry_witness(signer_a, signer_b, current_time)
            malformed_witness = entry_witness(signer_a, signer_c, current_time)
        else:
            operation = 3
            proposal_target = bytes(32)
            new_threshold = 2 if original_scoped else 1
            amount = new_threshold
            data_payload = bytes([new_threshold])
            witness = entry_witness(signer_a, bytes([new_threshold]), current_time)
            malformed_witness = entry_witness(signer_a, bytes([3]), current_time)
        output_wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, threshold, proposal_id, 10) if original_scoped else multisig_wallet_data(wallet_id, signer_a, signer_b, threshold, proposal_id, 10)
        proposal_payload = (
            multisig_proposal_molecule_data(wallet_id, proposal_id, signer_a, operation, proposal_target, amount, data_payload, [], threshold, current_time, current_time + 1440)
            if original_scoped
            else multisig_proposal_data(wallet_id, proposal_id, signer_a, operation, proposal_target, amount, threshold, 0, current_time, current_time + 1440)
        )
        outputs = [
            {"capacity": hex_u64(700 * 100_000_000), "lock": cellscript_lock, "type": wallet_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": proposal_type},
        ]
        outputs_data = ["0x" + output_wallet_payload.hex(), "0x" + proposal_payload.hex()]
        valid_tx = transaction(input_cell, outputs, outputs_data, cell_deps, [witness])
        malformed_tx = transaction(input_cell, outputs, outputs_data, cell_deps, [malformed_witness])
    elif action == "add_signature":
        current_time = 30
        proposal_id = 7
        signature_a = bytes([0xA5]) * 64
        signature_b = bytes([0xB6]) * 64
        signers = [signer_a, signer_b]
        wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, 2, 0, 10)
        proposal_payload = (
            multisig_proposal_molecule_data(wallet_id, proposal_id, signer_a, 0, target, 500, b"", [(signer_a, signature_a)], 2, 20, 2000)
            if original_scoped
            else multisig_proposal_data(wallet_id, proposal_id, signer_a, 0, target, 500, 2, 1, 20, 2000)
        )
        output_proposal_payload = (
            multisig_proposal_molecule_data(wallet_id, proposal_id, signer_a, 0, target, 500, b"", [(signer_a, signature_a), (signer_b, signature_b)], 2, 20, 2000)
            if original_scoped
            else multisig_proposal_data(wallet_id, proposal_id, signer_a, 0, target, 500, 2, 2, 20, 2000)
        )
        input_cells = [
            {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": proposal_type, "data": proposal_payload},
            {"capacity": 500 * 100_000_000, "lock": always_success_lock(), "type": wallet_type, "data": wallet_payload},
        ]
        initial = create_script_locked_cells("multisig.add_signature", input_cells, cell_deps)
        inputs = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [
            {"capacity": hex_u64(600 * 100_000_000), "lock": cellscript_lock, "type": proposal_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": confirmation_type},
        ]
        valid_tx = transaction(inputs, outputs, ["0x" + output_proposal_payload.hex(), "0x" + signature_confirmation_data(proposal_id, signer_b, current_time).hex()], action_cell_deps, [entry_witness(signer_b, signature_b, current_time)])
        malformed_tx = transaction(inputs, outputs, ["0x" + proposal_payload.hex(), "0x" + signature_confirmation_data(proposal_id, signer_b, current_time).hex()], action_cell_deps, [entry_witness(signer_b, signature_b, current_time)])
    elif action == "execute_proposal":
        current_time = 40
        proposal_id = 8
        signature_a = bytes([0xA5]) * 64
        signature_b = bytes([0xB6]) * 64
        signers = [signer_a, signer_b]
        wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, 2, 0, 10)
        proposal_payload = (
            multisig_proposal_molecule_data(wallet_id, proposal_id, signer_a, 0, target, 500, b"", [(signer_a, signature_a), (signer_b, signature_b)], 2, 20, 2000)
            if original_scoped
            else multisig_proposal_data(wallet_id, proposal_id, signer_a, 0, target, 500, 2, 2, 20, 2000)
        )
        input_cells = [
            {"capacity": 500 * 100_000_000, "lock": cellscript_lock, "type": proposal_type, "data": proposal_payload},
            {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": wallet_type, "data": wallet_payload},
        ]
        initial = create_script_locked_cells("multisig.execute_proposal", input_cells, cell_deps)
        inputs = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [{"capacity": hex_u64(200 * 100_000_000), "lock": cellscript_lock, "type": execution_type}]
        valid_tx = transaction(inputs, outputs, ["0x" + execution_record_data(proposal_id, signer_a, current_time, 1).hex()], action_cell_deps, [entry_witness(signer_a, current_time)])
        malformed_tx = transaction(inputs, outputs, ["0x" + execution_record_data(proposal_id, signer_a, current_time + 1, 1).hex()], action_cell_deps, [entry_witness(signer_a, current_time)])
    elif action == "cancel_proposal":
        proposal_id = 9
        signers = [signer_a, signer_b]
        wallet_payload = multisig_wallet_molecule_data(wallet_id, signers, 2, 0, 10)
        proposal_payload = multisig_proposal_molecule_data(wallet_id, proposal_id, signer_a, 0, target, 500, b"", [], 2, 20, 2000) if original_scoped else multisig_proposal_data(wallet_id, proposal_id, signer_a, 0, target, 500, 2, 0, 20, 2000)
        input_cells = [
            {"capacity": 500 * 100_000_000, "lock": cellscript_lock, "type": proposal_type, "data": proposal_payload},
            {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": wallet_type, "data": wallet_payload},
        ]
        initial = create_script_locked_cells("multisig.cancel_proposal", input_cells, cell_deps)
        inputs = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [{"capacity": hex_u64(490 * 100_000_000), "lock": cellscript_lock, "type": None}]
        valid_tx = transaction(inputs, outputs, ["0x"], action_cell_deps, [entry_witness(signer_a)])
        malformed_tx = transaction(inputs, outputs, ["0x"], action_cell_deps, [entry_witness(signer_b)])
    else:
        raise RuntimeError(f"unsupported multisig action harness: {action}")

    return {
        "builder_name": "multisig-action-builder-v1",
        "initial": initial,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
    }

def run_launch_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    if action != "bootstrap_token":
        if action != "launch_token":
            raise RuntimeError(f"unsupported launch action harness: {action}")
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    auth_type = always_success_lock("0x61")
    token_type = always_success_lock("0x62")
    pool_paired_type = always_success_lock("0x63")
    pool_type = always_success_lock("0x64")
    lp_type = always_success_lock("0x65")
    symbol = b"LAUNCH01"
    max_supply = 10_000
    initial_mint = 1_000
    pool_seed_amount = 500
    paired_amount = 250
    paired_symbol = b"PAIR0001"
    fee_rate_bps = 30
    creator_lock = always_success_lock("0x60")
    recipient_count = 4 if action == "launch_token" else 2
    recipient_locks = [always_success_lock("0x7" + format(index, "x")) for index in range(recipient_count)]
    creator = decode_hex(script_hash(creator_lock), 32)
    recipients = [
        (decode_hex(script_hash(lock), 32), amount)
        for lock, amount in zip(recipient_locks, [10, 20, 30, 40] if action == "launch_token" else [10, 20])
    ]
    recipient_payload = fixed_recipient_tuple_array4(recipients) if action == "launch_token" else fixed_recipient_tuple_array(recipients)
    total_distributed = sum(amount for _, amount in recipients)
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "launch-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
    }
    launch_case = build_launch_action_case(
        action_record,
        cellscript_lock,
        auth_type,
        token_type,
        pool_paired_type,
        pool_type,
        lp_type,
        symbol,
        max_supply,
        initial_mint,
        pool_seed_amount,
        paired_amount,
        paired_symbol,
        fee_rate_bps,
        creator_lock,
        creator,
        recipient_locks,
        recipients,
        recipient_payload,
        total_distributed,
        cell_deps,
    )
    initial = launch_case["initial"]
    input_cell = launch_case["input_cell"]
    valid_tx = launch_case["valid_tx"]
    malformed_tx = launch_case["malformed_tx"]
    result["builder_name"] = launch_case["builder_name"]

    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    assert_live(input_cell["tx_hash"], input_cell["index"], f"{name} input cell after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def build_launch_action_case(action_record, cellscript_lock, auth_type, token_type, pool_paired_type, pool_type, lp_type, symbol, max_supply, initial_mint, pool_seed_amount, paired_amount, paired_symbol, fee_rate_bps, creator_lock, creator, recipient_locks, recipients, recipient_payload, total_distributed, cell_deps):
    action = action_record["action"]
    if action == "launch_token":
        initial_lp = pool_seed_amount
        remaining = initial_mint - total_distributed - pool_seed_amount
        pool_id = decode_hex(script_hash(pool_type), 32)
        initial = create_script_locked_cells(
            "launch.launch_token",
            [{"capacity": 4000 * 100_000_000, "lock": cellscript_lock, "type": pool_paired_type, "data": token_data(paired_amount, paired_symbol)}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(400 * 100_000_000), "lock": creator_lock, "type": auth_type}]
        outputs_data = ["0x" + mint_authority_data(symbol, max_supply, initial_mint).hex()]
        for recipient_lock, (_, amount) in zip(recipient_locks, recipients):
            outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": recipient_lock, "type": token_type})
            outputs_data.append("0x" + token_data(amount, symbol).hex())
        outputs.append({"capacity": hex_u64(400 * 100_000_000), "lock": creator_lock, "type": pool_type})
        outputs_data.append("0x" + pool_data(symbol, paired_symbol, pool_seed_amount, paired_amount, initial_lp, fee_rate_bps).hex())
        outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": creator_lock, "type": lp_type})
        outputs_data.append("0x" + lp_receipt_data(pool_id, initial_lp, creator).hex())
        outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": creator_lock, "type": token_type})
        outputs_data.append("0x" + token_data(remaining, symbol).hex())
        witness = entry_witness(symbol, max_supply, initial_mint, pool_seed_amount, bytes([fee_rate_bps & 0xff, fee_rate_bps >> 8]), creator, recipient_payload)
        valid_tx = transaction(input_cell, outputs, outputs_data, cell_deps, [witness])
        malformed_outputs_data = list(outputs_data)
        malformed_outputs_data[-1] = "0x" + token_data(remaining - 1, symbol).hex()
        malformed_tx = transaction(input_cell, outputs, malformed_outputs_data, cell_deps, [witness])
    else:
        initial = create_script_locked_cells(
            "launch.bootstrap_token",
            [{"capacity": 4000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(400 * 100_000_000), "lock": creator_lock, "type": auth_type}]
        outputs_data = ["0x" + mint_authority_data(symbol, max_supply, initial_mint).hex()]
        for recipient_lock, (_, amount) in zip(recipient_locks, recipients):
            outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": recipient_lock, "type": token_type})
            outputs_data.append("0x" + token_data(amount, symbol).hex())
        outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": creator_lock, "type": token_type})
        outputs_data.append("0x" + token_data(initial_mint - total_distributed, symbol).hex())
        witness = entry_witness(symbol, max_supply, initial_mint, creator, recipient_payload)
        valid_tx = transaction(input_cell, outputs, outputs_data, cell_deps, [witness])
        malformed_outputs_data = list(outputs_data)
        malformed_outputs_data[-1] = "0x" + token_data(initial_mint - total_distributed - 1, symbol).hex()
        malformed_tx = transaction(input_cell, outputs, malformed_outputs_data, cell_deps, [witness])
    return {
        "builder_name": "launch-action-builder-v1",
        "initial": initial,
        "input_cell": input_cell,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
    }

def run_vesting_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    admin_lock = always_success_lock()
    config_type = always_success_lock("0x41")
    admin = decode_hex(script_hash(admin_lock), 32)
    symbol = b"VEST0001"
    cliff_period = 10
    total_period = 100
    revocable = True
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    if action not in {"create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"}:
        raise RuntimeError(f"unsupported vesting action harness: {action}")

    result = {
        "action": action,
        "name": name,
        "harness_origin": "vesting-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
        "admin_lock_hash": "0x" + admin.hex(),
    }
    vesting_case = build_vesting_action_case(
        action_record,
        cellscript_lock,
        admin_lock,
        config_type,
        admin,
        symbol,
        cliff_period,
        total_period,
        revocable,
        cell_deps,
    )
    initial = vesting_case["initial"]
    input_cells_to_check = vesting_case["input_cells_to_check"]
    valid_tx = vesting_case["valid_tx"]
    malformed_tx = vesting_case["malformed_tx"]
    result["builder_name"] = vesting_case["builder_name"]
    if vesting_case.get("timepoint_header") is not None:
        result["timepoint_header"] = vesting_case["timepoint_header"]
    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, input_cell in enumerate(input_cells_to_check):
        assert_live(input_cell["tx_hash"], input_cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def build_vesting_action_case(action_record, cellscript_lock, admin_lock, config_type, admin, symbol, cliff_period, total_period, revocable, cell_deps):
    action = action_record["action"]
    timepoint_header = None

    if action == "create_vesting_config":
        initial = create_script_locked_cells(
            "vesting.create_vesting_config",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cells_to_check = [initial["cells"][0]]
        valid_tx = transaction(
            initial["cells"][0],
            [{"capacity": hex_u64(300 * 100_000_000), "lock": admin_lock, "type": config_type}],
            ["0x" + vesting_config_data(admin, symbol, cliff_period, total_period, revocable).hex()],
            cell_deps,
            [entry_witness(admin, symbol, cliff_period, total_period, bytes([1]))],
        )
        malformed_tx = transaction(
            initial["cells"][0],
            [{"capacity": hex_u64(300 * 100_000_000), "lock": admin_lock, "type": config_type}],
            ["0x" + vesting_config_data(admin, symbol, cliff_period, total_period + 1, revocable).hex()],
            cell_deps,
            [entry_witness(admin, symbol, cliff_period, total_period, bytes([1]))],
        )
    elif action == "grant_vesting":
        beneficiary_lock = always_success_lock("0x42")
        beneficiary = decode_hex(script_hash(beneficiary_lock), 32)
        grant_type = always_success_lock("0x43")
        amount = 77
        now = 0
        header_dep = get_block_by_number(0)["header"]["hash"]
        initial = create_script_locked_cells(
            "vesting.grant_vesting",
            [
                {"capacity": 200 * 100_000_000, "lock": cellscript_lock, "type": always_success_lock("0x44"), "data": token_data(amount, symbol)},
                {"capacity": 200 * 100_000_000, "lock": admin_lock, "type": config_type, "data": vesting_config_data(admin, symbol, cliff_period, total_period, revocable)},
            ],
            cell_deps,
        )
        funding_input = find_spendable_cellbase()
        change_capacity = initial["cells"][0]["capacity"] + funding_input["capacity"] - (300 * 100_000_000)
        input_cells_to_check = initial["cells"] + [funding_input]
        config_dep = {"out_point": out_point(initial["cells"][1]["tx_hash"], initial["cells"][1]["index"]), "dep_type": "code"}
        action_cell_deps = [config_dep] + cell_deps
        valid_tx = transaction(
            [initial["cells"][0], funding_input],
            [
                {"capacity": hex_u64(300 * 100_000_000), "lock": beneficiary_lock, "type": grant_type},
                {"capacity": hex_u64(change_capacity), "lock": always_success_lock(), "type": None},
            ],
            [
                "0x" + vesting_grant_data(0, beneficiary, amount, 0, now, now + cliff_period, now + total_period, symbol).hex(),
                "0x",
            ],
            action_cell_deps,
            [entry_witness(beneficiary)],
            [header_dep],
        )
        malformed_tx = transaction(
            [initial["cells"][0], funding_input],
            [
                {"capacity": hex_u64(300 * 100_000_000), "lock": beneficiary_lock, "type": grant_type},
                {"capacity": hex_u64(change_capacity), "lock": always_success_lock(), "type": None},
            ],
            [
                "0x" + vesting_grant_data(0, beneficiary, amount + 1, 0, now, now + cliff_period, now + total_period, symbol).hex(),
                "0x",
            ],
            action_cell_deps,
            [entry_witness(beneficiary)],
            [header_dep],
        )
    elif action == "claim_vested":
        beneficiary_lock = cellscript_lock
        beneficiary = decode_hex(script_hash(beneficiary_lock), 32)
        grant_type = always_success_lock("0x43")
        token_type = always_success_lock("0x45")
        total_amount = 100
        claimed_amount = 20
        timepoint_header = wait_header_epoch_at_least(1)
        claimable = total_amount - claimed_amount
        grant_timepoint = 0
        cliff_timepoint = 0
        end_timepoint = timepoint_header["epoch_number"]
        header_dep = timepoint_header["hash"]
        initial = create_script_locked_cells(
            "vesting.claim_vested",
            [{"capacity": 500 * 100_000_000, "lock": beneficiary_lock, "type": grant_type, "data": vesting_grant_data(1, beneficiary, total_amount, claimed_amount, grant_timepoint, cliff_timepoint, end_timepoint, symbol)}],
            cell_deps,
        )
        input_cells_to_check = initial["cells"]
        valid_tx = transaction(
            initial["cells"],
            [
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": token_type},
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": grant_type},
            ],
            [
                "0x" + token_data(claimable, symbol).hex(),
                "0x" + vesting_grant_data(2, beneficiary, total_amount, total_amount, grant_timepoint, cliff_timepoint, end_timepoint, symbol).hex(),
            ],
            cell_deps,
            [entry_witness()],
            [header_dep],
        )
        malformed_tx = transaction(
            initial["cells"],
            [
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": token_type},
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": grant_type},
            ],
            [
                "0x" + token_data(claimable - 1, symbol).hex(),
                "0x" + vesting_grant_data(2, beneficiary, total_amount, total_amount, grant_timepoint, cliff_timepoint, end_timepoint, symbol).hex(),
            ],
            cell_deps,
            [entry_witness()],
            [header_dep],
        )
    else:
        beneficiary_lock = always_success_lock("0x42")
        beneficiary = decode_hex(script_hash(beneficiary_lock), 32)
        grant_type = always_success_lock("0x43")
        token_type = always_success_lock("0x45")
        total_amount = 100
        claimed_amount = 20
        timepoint_header = wait_header_epoch_at_least(1)
        grant_timepoint = 0
        cliff_timepoint = 0
        end_timepoint = timepoint_header["epoch_number"]
        header_dep = timepoint_header["hash"]
        unclaimed_vested = total_amount - claimed_amount
        unvested = 0
        initial = create_script_locked_cells(
            "vesting.revoke_grant",
            [
                {"capacity": 500 * 100_000_000, "lock": cellscript_lock, "type": grant_type, "data": vesting_grant_data(1, beneficiary, total_amount, claimed_amount, grant_timepoint, cliff_timepoint, end_timepoint, symbol)},
                {"capacity": 200 * 100_000_000, "lock": admin_lock, "type": config_type, "data": vesting_config_data(admin, symbol, cliff_period, total_period, revocable)},
            ],
            cell_deps,
        )
        input_cells_to_check = initial["cells"]
        config_dep = {"out_point": out_point(initial["cells"][1]["tx_hash"], initial["cells"][1]["index"]), "dep_type": "code"}
        action_cell_deps = [config_dep] + cell_deps
        valid_tx = transaction(
            initial["cells"][0],
            [
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": token_type},
                {"capacity": hex_u64(200 * 100_000_000), "lock": admin_lock, "type": token_type},
            ],
            [
                "0x" + token_data(unclaimed_vested, symbol).hex(),
                "0x" + token_data(unvested, symbol).hex(),
            ],
            action_cell_deps,
            [entry_witness(admin), "0x"],
            [header_dep],
        )
        malformed_tx = transaction(
            initial["cells"][0],
            [
                {"capacity": hex_u64(200 * 100_000_000), "lock": beneficiary_lock, "type": token_type},
                {"capacity": hex_u64(200 * 100_000_000), "lock": admin_lock, "type": token_type},
            ],
            [
                "0x" + token_data(unclaimed_vested - 1, symbol).hex(),
                "0x" + token_data(unvested, symbol).hex(),
            ],
            action_cell_deps,
            [entry_witness(admin), "0x"],
            [header_dep],
        )

    return {
        "builder_name": "vesting-action-builder-v1",
        "initial": initial,
        "input_cells_to_check": input_cells_to_check,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
        "timepoint_header": timepoint_header,
    }

def build_timelock_action_case(action_record, cellscript_lock, cellscript_type, owner, cell_deps):
    action = action_record["action"]
    original_scoped = action_record.get("kind") == "original-scoped-action-strict"
    flow_state = 0 if original_scoped else None
    lock_id = decode_hex(script_hash(cellscript_type), 32)

    def scoped_lock_id():
        return lock_id if original_scoped else bytes(32)

    def scoped_timelock_data(owner_value, lock_type, unlock_height, created_at):
        return timelock_data(
            owner_value,
            lock_type,
            unlock_height,
            created_at,
            lock_id=lock_id if original_scoped else None,
        )

    if action == "create_absolute_lock":
        current_height = 50
        unlock_height = 100
        initial = create_script_locked_cells(
            "timelock.create_absolute_lock",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        witness = [entry_witness(lock_id, owner, unlock_height, current_height)] if original_scoped else [entry_witness(owner, unlock_height, current_height)]
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}]
        valid_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 0, unlock_height, current_height).hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 0, unlock_height + 1, current_height).hex()], cell_deps, witness)
    elif action == "create_relative_lock":
        current_height = 50
        lock_period = 25
        initial = create_script_locked_cells(
            "timelock.create_relative_lock",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        witness = [entry_witness(lock_id, owner, lock_period, current_height)] if original_scoped else [entry_witness(owner, lock_period, current_height)]
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}]
        valid_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 1, current_height + lock_period, current_height).hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 1, current_height + lock_period + 1, current_height).hex()], cell_deps, witness)
    elif action == "lock_asset":
        unlock_height = 500
        created_at = 1
        amount = 42
        asset_type_payload = molecule_bytes(bytes([0]))
        lock_hash = scoped_lock_id()
        locked_asset_payload = locked_asset_molecule_data(asset_type_payload, amount, lock_hash) if original_scoped else locked_asset_data(amount, lock_hash)
        malformed_locked_asset_payload = locked_asset_molecule_data(asset_type_payload, amount + 1, lock_hash) if original_scoped else locked_asset_data(amount + 1, lock_hash)
        locked_asset_type = always_success_lock("0x20")
        initial = create_script_locked_cells(
            "timelock.lock_asset",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""},
                {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": cellscript_type, "data": scoped_timelock_data(owner, 0, unlock_height, created_at)},
            ],
            cell_deps,
        )
        inputs = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": locked_asset_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ]
        witness = [entry_witness(molecule_bytes(asset_type_payload), amount)] if original_scoped else [entry_witness(lock_hash, amount)]
        valid_tx = transaction(inputs, outputs, ["0x" + locked_asset_payload.hex(), "0x"], action_cell_deps, witness)
        malformed_tx = transaction(inputs, outputs, ["0x" + malformed_locked_asset_payload.hex(), "0x"], action_cell_deps, witness)
    elif action == "request_release":
        unlock_height = 100
        current_height = 125
        created_at = 1
        lock_hash = scoped_lock_id()
        request_type = always_success_lock("0x21")
        initial = create_script_locked_cells(
            "timelock.request_release",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""},
                {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": cellscript_type, "data": scoped_timelock_data(owner, 0, unlock_height, created_at)},
            ],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": request_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ]
        witness = [entry_witness(owner, current_height)] if original_scoped else [entry_witness(lock_hash, owner, current_height)]
        valid_tx = transaction(input_cell, outputs, ["0x" + release_request_data(lock_hash, owner, current_height, state=flow_state).hex(), "0x"], action_cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + release_request_data(lock_hash, owner, current_height + 1, state=flow_state).hex(), "0x"], action_cell_deps, witness)
    elif action == "request_emergency_release":
        unlock_height = 500
        current_height = 125
        created_at = 1
        lock_hash = scoped_lock_id()
        reason_payload = molecule_bytes(b"emergency release")
        emergency_type = always_success_lock("0x22")
        initial = create_script_locked_cells(
            "timelock.request_emergency_release",
            [
                {"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""},
                {"capacity": 300 * 100_000_000, "lock": always_success_lock(), "type": cellscript_type, "data": scoped_timelock_data(owner, 0, unlock_height, created_at)},
            ],
            cell_deps,
        )
        emergency_payload = emergency_release_molecule_data(lock_hash, owner, reason_payload, current_height, []) if original_scoped else emergency_release_data(lock_hash, owner, current_height, 0)
        malformed_emergency_payload = emergency_release_molecule_data(lock_hash, owner, reason_payload, current_height + 1, []) if original_scoped else emergency_release_data(lock_hash, owner, current_height, 1)
        inputs = initial["cells"][0]
        action_cell_deps = [cell_dep_for(initial["cells"][1])] + cell_deps
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": emergency_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ]
        witness = [entry_witness(owner, molecule_bytes(reason_payload), current_height)] if original_scoped else [entry_witness(lock_hash, owner, current_height)]
        valid_tx = transaction(inputs, outputs, ["0x" + emergency_payload.hex(), "0x"], action_cell_deps, witness)
        malformed_tx = transaction(inputs, outputs, ["0x" + malformed_emergency_payload.hex(), "0x"], action_cell_deps, witness)
    elif action == "approve_emergency_release":
        lock_hash = scoped_lock_id()
        requester = owner
        requested_at = 120
        initial_approvals = 1
        required_approvals = 3
        existing_approver = bytes([0x42]) * 32
        reason_payload = molecule_bytes(b"emergency release")
        emergency_type = always_success_lock("0x23")
        input_payload = emergency_release_molecule_data(lock_hash, requester, reason_payload, requested_at, [existing_approver]) if original_scoped else emergency_release_data(lock_hash, requester, requested_at, initial_approvals)
        output_payload = emergency_release_molecule_data(lock_hash, requester, reason_payload, requested_at, [existing_approver, owner]) if original_scoped else emergency_release_data(lock_hash, requester, requested_at, initial_approvals + 1)
        malformed_output_payload = emergency_release_molecule_data(lock_hash, requester, reason_payload, requested_at, [existing_approver]) if original_scoped else emergency_release_data(lock_hash, requester, requested_at, initial_approvals)
        initial = create_script_locked_cells(
            "timelock.approve_emergency_release",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": emergency_type, "data": input_payload}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": emergency_type}]
        witness = [entry_witness(owner, bytes([required_approvals]) if original_scoped else required_approvals)]
        valid_tx = transaction(input_cell, outputs, ["0x" + output_payload.hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + malformed_output_payload.hex()], cell_deps, witness)
    elif action == "extend_lock":
        current_height = 50
        initial_unlock_height = 100
        additional_period = 10
        created_at = 1
        initial = create_script_locked_cells(
            "timelock.extend_lock",
            [{"capacity": 1000 * 100_000_000, "lock": cellscript_lock, "type": cellscript_type, "data": scoped_timelock_data(owner, 0, initial_unlock_height, created_at)}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [{"capacity": hex_u64(1000 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type}]
        witness = [entry_witness(additional_period, owner, current_height)]
        valid_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 0, initial_unlock_height + additional_period, created_at).hex()], cell_deps, witness)
        malformed_tx = transaction(input_cell, outputs, ["0x" + scoped_timelock_data(owner, 0, initial_unlock_height + additional_period + 1, created_at).hex()], cell_deps, witness)
    elif action == "execute_release":
        unlock_height = 100
        current_height = 125
        created_at = 1
        lock_hash = scoped_lock_id()
        time_lock_type = always_success_lock("0x01")
        locked_asset_type = always_success_lock("0x02")
        release_request_type = always_success_lock("0x03")
        release_record_type = always_success_lock("0x04")
        asset_type_payload = bytes([0])
        locked_asset_payload = locked_asset_molecule_data(asset_type_payload, 42, lock_hash) if original_scoped else locked_asset_data(42, lock_hash)
        initial = create_script_locked_cells(
            "timelock.execute_release",
            [
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": time_lock_type, "data": scoped_timelock_data(owner, 0, unlock_height, created_at)},
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": locked_asset_type, "data": locked_asset_payload},
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": release_request_type, "data": release_request_data(lock_hash, owner, 120, state=flow_state)},
            ],
            cell_deps,
        )
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": release_record_type}]
        witness = [entry_witness(owner, current_height) if original_scoped else entry_witness(owner), "0x", "0x"]
        valid_tx = transaction(initial["cells"], outputs, ["0x" + release_record_data(lock_hash, 125, owner).hex()], cell_deps, witness)
        malformed_tx = transaction(initial["cells"], outputs, ["0x" + release_record_data(lock_hash, 126, owner).hex()], cell_deps, witness)
    elif action == "execute_emergency_release":
        unlock_height = 500
        current_height = 125
        created_at = 1
        lock_hash = scoped_lock_id()
        time_lock_type = always_success_lock("0x11")
        locked_asset_type = always_success_lock("0x12")
        emergency_type = always_success_lock("0x13")
        release_record_type = always_success_lock("0x14")
        required_approvals = 2
        asset_type_payload = bytes([0])
        reason_payload = molecule_bytes(b"emergency release")
        locked_asset_payload = locked_asset_molecule_data(asset_type_payload, 42, lock_hash) if original_scoped else locked_asset_data(42, lock_hash)
        emergency_payload = emergency_release_molecule_data(lock_hash, owner, reason_payload, 120, [owner, bytes([0x42]) * 32]) if original_scoped else emergency_release_data(lock_hash, owner, 120, 3)
        initial = create_script_locked_cells(
            "timelock.execute_emergency_release",
            [
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": time_lock_type, "data": scoped_timelock_data(owner, 0, unlock_height, created_at)},
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": locked_asset_type, "data": locked_asset_payload},
                {"capacity": 300 * 100_000_000, "lock": cellscript_lock, "type": emergency_type, "data": emergency_payload},
            ],
            cell_deps,
        )
        outputs = [{"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": release_record_type}]
        witness = [entry_witness(owner, bytes([required_approvals]), current_height) if original_scoped else entry_witness(owner, required_approvals), "0x", "0x"]
        valid_tx = transaction(initial["cells"], outputs, ["0x" + release_record_data(lock_hash, 125, owner).hex()], cell_deps, witness)
        malformed_tx = transaction(initial["cells"], outputs, ["0x" + release_record_data(lock_hash, 126, owner).hex()], cell_deps, witness)
    elif action == "batch_create_locks":
        current_height = 50
        owners = [owner, bytes([0x51]) * 32, bytes([0x52]) * 32, bytes([0x53]) * 32]
        lock_ids = [lock_id, bytes([0x61]) * 32, bytes([0x62]) * 32, bytes([0x63]) * 32]
        unlock_heights = [100, 110, 120, 130]
        initial = create_script_locked_cells(
            "timelock.batch_create_locks",
            [{"capacity": 1500 * 100_000_000, "lock": cellscript_lock, "type": None, "data": b""}],
            cell_deps,
        )
        input_cell = initial["cells"][0]
        outputs = [
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": cellscript_lock, "type": cellscript_type},
        ]
        outputs_data = [
            "0x" + timelock_data(owners[0], 0, unlock_heights[0], current_height, lock_id=lock_ids[0] if original_scoped else None).hex(),
            "0x" + timelock_data(owners[1], 0, unlock_heights[1], current_height, lock_id=lock_ids[1] if original_scoped else None).hex(),
            "0x" + timelock_data(owners[2], 0, unlock_heights[2], current_height, lock_id=lock_ids[2] if original_scoped else None).hex(),
            "0x" + timelock_data(owners[3], 0, unlock_heights[3], current_height, lock_id=lock_ids[3] if original_scoped else None).hex(),
        ]
        witness = [entry_witness(fixed_hash_array4(lock_ids), fixed_address_array4(owners), fixed_u64_array4(unlock_heights), current_height)] if original_scoped else [entry_witness(fixed_address_array4(owners), fixed_u64_array4(unlock_heights), current_height)]
        valid_tx = transaction(input_cell, outputs, outputs_data, cell_deps, witness)
        malformed_outputs_data = list(outputs_data)
        malformed_outputs_data[1] = "0x" + timelock_data(owners[1], 0, unlock_heights[1] + 1, current_height, lock_id=lock_ids[1] if original_scoped else None).hex()
        malformed_tx = transaction(input_cell, outputs, malformed_outputs_data, cell_deps, witness)
    else:
        raise RuntimeError(f"unsupported TimeLock action harness: {action}")

    return {
        "builder_name": "timelock-action-builder-v1",
        "initial": initial,
        "valid_tx": valid_tx,
        "malformed_tx": malformed_tx,
    }

def run_timelock_action(action_record, always_success_dep):
    action = action_record["action"]
    name = action_record["name"]
    code = deploy_code_cell(name, action_record["artifact"], always_success_dep)
    cellscript_lock = {"code_hash": code["artifact_ckb_data_hash_blake2b"], "hash_type": "data1", "args": "0x"}
    cellscript_type = always_success_lock()
    owner = decode_hex(script_hash(cellscript_lock), 32)
    cell_deps = [always_success_dep, code["code_cell_dep"]]

    result = {
        "action": action,
        "name": name,
        "harness_origin": "timelock-action-builder-v1",
        "builder_backed": True,
        "artifact": action_record["artifact"],
        "code": code,
        "cellscript_lock_hash": script_hash(cellscript_lock),
        "owner": "0x" + owner.hex(),
    }
    timelock_case = build_timelock_action_case(action_record, cellscript_lock, cellscript_type, owner, cell_deps)
    initial = timelock_case["initial"]
    valid_tx = timelock_case["valid_tx"]
    malformed_tx = timelock_case["malformed_tx"]
    result["builder_name"] = timelock_case["builder_name"]

    malformed_rejection = expect_dry_run_rejected(
        malformed_tx,
        f"{name} malformed action transaction",
        ("Script", "script", "ValidationFailure", "error code", "VM", "Run result", "Invalid"),
    )
    for index, cell in enumerate(initial["cells"]):
        assert_live(cell["tx_hash"], cell["index"], f"{name} input cell {index} after malformed transaction")

    valid_dry_run = rpc("dry_run_transaction", [valid_tx])
    commit = submit_and_commit(valid_tx, f"{name} valid action transaction")
    output_live = [
        assert_live(commit["tx_hash"], index, f"{name} valid output {index}").get("status") == "live"
        for index in range(len(valid_tx["outputs"]))
    ]
    result.update({
        "initial_cells": initial,
        "malformed_transaction": malformed_rejection,
        "valid_dry_run": valid_dry_run,
        "measured_constraints": measure_release_constraints(valid_tx, valid_dry_run),
        "valid_commit": commit,
        "valid_outputs_live": output_live,
        "status": "passed",
    })
    return result

def action_record_by(records, action):
    for record in records:
        if record.get("action") == action:
            return record
    raise RuntimeError(f"missing action artifact for stateful scenario: {action}")

def deploy_stateful_action(record, always_success_dep):
    code = deploy_code_cell(f"stateful.{record['name']}", record["artifact"], always_success_dep)
    lock_script = {
        "code_hash": code["artifact_ckb_data_hash_blake2b"],
        "hash_type": "data1",
        "args": "0x",
    }
    return {
        "action": record["action"],
        "name": record["name"],
        "record": record,
        "code": code,
        "lock": lock_script,
        "lock_hash": decode_hex(script_hash(lock_script), 32),
        "cell_deps": [always_success_dep, code["code_cell_dep"]],
    }

def output_cell_from_tx(commit, tx, index):
    output = tx["outputs"][index]
    return {
        "tx_hash": commit["tx_hash"],
        "index": index,
        "capacity": parse_hex_u64(output["capacity"]),
        "lock": output["lock"],
        "type": output.get("type"),
        "data_hex": tx["outputs_data"][index],
    }

def assert_not_live(tx_hash, index, label):
    result = rpc("get_live_cell", [out_point(tx_hash, index), True])
    if result and result.get("status") == "live":
        raise RuntimeError(f"{label} is still live after stateful spend: {result}")
    return result

def assert_stateful_step_constraints(label, constraints):
    failures = []
    if constraints.get("consensus_serialized_tx_size_bytes") is None:
        failures.append("consensus tx size was not measured")
    if constraints.get("occupied_capacity_shannons") is None:
        failures.append("occupied capacity was not derived")
    if constraints.get("capacity_is_sufficient") is not True:
        failures.append(
            "outputs are under-capacity"
            if constraints.get("capacity_is_sufficient") is False
            else "capacity sufficiency was not measured"
        )
    if failures:
        detail = {
            "label": label,
            "failures": failures,
            "tx_measure_error": constraints.get("tx_measure_error"),
            "under_capacity_output_indexes": constraints.get("under_capacity_output_indexes"),
        }
        raise RuntimeError("stateful step constraint measurement failed: " + json.dumps(detail, sort_keys=True))

def run_stateful_step(scenario, step, tx, consumed_cells=None, live_output_indexes=None):
    consumed_cells = consumed_cells or []
    live_output_indexes = list(range(len(tx["outputs"]))) if live_output_indexes is None else live_output_indexes
    dry_run = rpc("dry_run_transaction", [tx])
    constraints = measure_release_constraints(tx, dry_run)
    assert_stateful_step_constraints(f"{scenario}.{step}", constraints)
    commit = submit_and_commit(tx, f"stateful {scenario}.{step}")
    consumed = [
        assert_not_live(cell["tx_hash"], cell["index"], f"stateful {scenario}.{step} consumed input {index}")
        for index, cell in enumerate(consumed_cells)
    ]
    outputs_live = {
        str(index): assert_live(commit["tx_hash"], index, f"stateful {scenario}.{step} output {index}").get("status") == "live"
        for index in live_output_indexes
    }
    return {
        "step": step,
        "dry_run": dry_run,
        "commit": commit,
        "measured_constraints": constraints,
        "consumed_inputs": consumed,
        "outputs_live": outputs_live,
        "status": "passed",
    }

def action_example(record):
    example = record.get("example")
    if example:
        return pathlib.Path(example).name
    original_source = record.get("original_source") or record.get("source")
    if original_source:
        return pathlib.Path(original_source).name
    name = record.get("name", "")
    for row in (report.get("ckb_business_coverage") or {}).get("rows", []):
        candidate = row.get("example", "")
        if candidate.removesuffix(".cell") in name:
            return candidate
    raise RuntimeError(f"cannot determine example for action record: {record}")

def action_id(record_or_action):
    record = record_or_action.get("record", record_or_action)
    return f"{action_example(record)}:{record['action']}"

def action_ids(records_or_actions):
    return [action_id(record_or_action) for record_or_action in records_or_actions]

def expected_stateful_action_ids():
    coverage_rows = (report.get("ckb_business_coverage") or {}).get("rows", [])
    if not coverage_rows:
        raise RuntimeError("acceptance report does not contain CKB business coverage rows")
    return sorted(
        f"{example}:{action}"
        for row in coverage_rows
        for example in [row["example"]]
        for action in (row.get("strict_ckb_actions") or row.get("source_actions") or [])
    )

def all_stateful_action_records():
    records = (
        token_action_artifacts
        + nft_action_artifacts
        + timelock_action_artifacts
        + multisig_action_artifacts
        + vesting_action_artifacts
        + amm_action_artifacts
        + launch_action_artifacts
    )
    by_id = {}
    for record in records:
        by_id.setdefault(action_id(record), record)
    return [by_id[action] for action in sorted(by_id)]

def consumed_cells_from_tx(tx):
    consumed = []
    for tx_input in tx.get("inputs", []):
        previous_output = tx_input["previous_output"]
        consumed.append({
            "tx_hash": previous_output["tx_hash"],
            "index": parse_hex_u64(previous_output["index"]),
        })
    return consumed

def build_stateful_action_branch_case(record, always_success_dep):
    deployed = deploy_stateful_action(record, always_success_dep)
    cellscript_lock = deployed["lock"]
    cell_deps = deployed["cell_deps"]
    example = action_example(record)

    if example == "token.cell":
        cellscript_type = always_success_lock()
        destination_lock = always_success_lock()
        case = build_token_action_case(
            record["action"],
            cellscript_lock,
            cellscript_type,
            destination_lock,
            decode_hex(script_hash(destination_lock), 32),
            b"TOKEN001",
            cell_deps,
        )
    elif example == "nft.cell":
        destination_lock = always_success_lock()
        case = build_nft_action_case(
            record,
            cellscript_lock,
            always_success_lock(),
            destination_lock,
            decode_hex(script_hash(cellscript_lock), 32),
            decode_hex(script_hash(destination_lock), 32),
            bytes(range(32)),
            bytes(reversed(range(32))),
            always_success_lock("0x21"),
            always_success_lock("0x22"),
            always_success_lock("0x23"),
            always_success_lock("0x24"),
            cell_deps,
        )
    elif example == "timelock.cell":
        owner = decode_hex(script_hash(cellscript_lock), 32)
        case = build_timelock_action_case(record, cellscript_lock, always_success_lock(), owner, cell_deps)
    elif example == "multisig.cell":
        case = build_multisig_action_case(
            record,
            cellscript_lock,
            always_success_lock("0x51"),
            always_success_lock("0x52"),
            always_success_lock("0x53"),
            always_success_lock("0x54"),
            decode_hex(script_hash(cellscript_lock), 32),
            decode_hex(script_hash(always_success_lock("0x55")), 32),
            decode_hex(script_hash(always_success_lock("0x56")), 32),
            decode_hex(script_hash(always_success_lock("0x57")), 32),
            bytes(32),
            cell_deps,
        )
    elif example == "vesting.cell":
        admin_lock = always_success_lock()
        case = build_vesting_action_case(
            record,
            cellscript_lock,
            admin_lock,
            always_success_lock("0x41"),
            decode_hex(script_hash(admin_lock), 32),
            b"VEST0001",
            10,
            100,
            True,
            cell_deps,
        )
    elif example == "amm_pool.cell":
        case = build_amm_action_case(record, cellscript_lock, always_success_lock(), cell_deps)
    elif example == "launch.cell":
        action = record["action"]
        symbol = b"LAUNCH01"
        max_supply = 10_000
        initial_mint = 1_000
        pool_seed_amount = 500
        paired_amount = 250
        paired_symbol = b"PAIR0001"
        fee_rate_bps = 30
        creator_lock = always_success_lock("0x60")
        recipient_amounts = [10, 20, 30, 40] if action == "launch_token" else [10, 20]
        recipient_locks = [always_success_lock("0x7" + format(index, "x")) for index in range(len(recipient_amounts))]
        recipients = [
            (decode_hex(script_hash(lock), 32), amount)
            for lock, amount in zip(recipient_locks, recipient_amounts)
        ]
        case = build_launch_action_case(
            record,
            cellscript_lock,
            always_success_lock("0x61"),
            always_success_lock("0x62"),
            always_success_lock("0x63"),
            always_success_lock("0x64"),
            always_success_lock("0x65"),
            symbol,
            max_supply,
            initial_mint,
            pool_seed_amount,
            paired_amount,
            paired_symbol,
            fee_rate_bps,
            creator_lock,
            decode_hex(script_hash(creator_lock), 32),
            recipient_locks,
            recipients,
            fixed_recipient_tuple_array4(recipients) if action == "launch_token" else fixed_recipient_tuple_array(recipients),
            sum(amount for _, amount in recipients),
            cell_deps,
        )
    else:
        raise RuntimeError(f"unsupported stateful action branch example: {example}")

    return {
        "record": record,
        "deployed_action": deployed,
        "initial": case["initial"],
        "builder_name": case["builder_name"],
        "valid_tx": case["valid_tx"],
    }

def run_stateful_action_branch(record, always_success_dep):
    case = build_stateful_action_branch_case(record, always_success_dep)
    coverage_id = action_id(record)
    scenario = coverage_id.replace(":", ".") + ".stateful-branch"
    step = run_stateful_step(
        scenario,
        "valid_action_branch",
        case["valid_tx"],
        consumed_cells_from_tx(case["valid_tx"]),
    )
    return {
        "name": scenario,
        "kind": "stateful-action-branch",
        "builder_backed": True,
        "builder_name": case["builder_name"],
        "actions": [record["action"]],
        "action_ids": [coverage_id],
        "initial_cells": case["initial"],
        "steps": [step],
        "status": "passed",
    }

def run_stateful_action_branch_coverage(always_success_dep, required_records, already_covered):
    branch_runs = []
    for record in required_records:
        if action_id(record) in already_covered:
            continue
        branch_runs.append(run_stateful_action_branch(record, always_success_dep))
    return branch_runs

def run_stateful_token_lifecycle(always_success_dep):
    scenario = "token.mint-with-authority-transfer-mint-with-authority-merge-burn"
    actions = {
        name: deploy_stateful_action(action_record_by(token_action_artifacts, name), always_success_dep)
        for name in ("mint_with_authority", "transfer_token", "merge", "burn")
    }
    token_type = always_success_lock("0xa1")
    token_symbol = b"STATE001"
    steps = []

    initial = create_script_locked_cells(
        "stateful.token.auth",
        [{
            "capacity": 700 * 100_000_000,
            "lock": actions["mint_with_authority"]["lock"],
            "type": token_type,
            "data": mint_authority_data(token_symbol, 1000, 0),
        }],
        actions["mint_with_authority"]["cell_deps"],
    )
    auth0 = initial["cells"][0]
    tx1 = transaction(
        auth0,
        [
            {"capacity": hex_u64(600 * 100_000_000), "lock": actions["mint_with_authority"]["lock"], "type": token_type},
            {"capacity": hex_u64(100 * 100_000_000), "lock": actions["transfer_token"]["lock"], "type": token_type},
        ],
        [
            "0x" + mint_authority_data(token_symbol, 1000, 5).hex(),
            "0x" + token_data(5, token_symbol).hex(),
        ],
        actions["mint_with_authority"]["cell_deps"],
        [entry_witness(actions["transfer_token"]["lock_hash"], 5)],
    )
    step = run_stateful_step(scenario, "mint_first_token_to_transfer", tx1, [auth0])
    steps.append(step)
    auth1 = output_cell_from_tx(step["commit"], tx1, 0)
    token_a = output_cell_from_tx(step["commit"], tx1, 1)

    tx2 = transaction(
        token_a,
        [{"capacity": hex_u64(100 * 100_000_000), "lock": actions["merge"]["lock"], "type": token_type}],
        ["0x" + token_data(5, token_symbol).hex()],
        actions["transfer_token"]["cell_deps"],
        [entry_witness(actions["merge"]["lock_hash"])],
    )
    step = run_stateful_step(scenario, "transfer_first_token_to_merge", tx2, [token_a])
    steps.append(step)
    token_a_for_merge = output_cell_from_tx(step["commit"], tx2, 0)

    tx3 = transaction(
        auth1,
        [
            {"capacity": hex_u64(500 * 100_000_000), "lock": actions["mint_with_authority"]["lock"], "type": token_type},
            {"capacity": hex_u64(100 * 100_000_000), "lock": actions["merge"]["lock"], "type": token_type},
        ],
        [
            "0x" + mint_authority_data(token_symbol, 1000, 12).hex(),
            "0x" + token_data(7, token_symbol).hex(),
        ],
        actions["mint_with_authority"]["cell_deps"],
        [entry_witness(actions["merge"]["lock_hash"], 7)],
    )
    step = run_stateful_step(scenario, "mint_second_token_to_merge", tx3, [auth1])
    steps.append(step)
    auth2 = output_cell_from_tx(step["commit"], tx3, 0)
    token_b_for_merge = output_cell_from_tx(step["commit"], tx3, 1)

    tx4 = transaction(
        [token_a_for_merge, token_b_for_merge],
        [{"capacity": hex_u64(200 * 100_000_000), "lock": actions["burn"]["lock"], "type": token_type}],
        ["0x" + token_data(12, token_symbol).hex()],
        actions["merge"]["cell_deps"],
        [entry_witness(actions["burn"]["lock_hash"]), "0x"],
    )
    step = run_stateful_step(scenario, "merge_tokens_to_burn", tx4, [token_a_for_merge, token_b_for_merge])
    steps.append(step)
    merged_token = output_cell_from_tx(step["commit"], tx4, 0)

    tx5 = transaction(
        merged_token,
        [{"capacity": hex_u64(200 * 100_000_000), "lock": always_success_lock(), "type": None}],
        ["0x"],
        actions["burn"]["cell_deps"],
        [entry_witness()],
    )
    step = run_stateful_step(scenario, "burn_merged_token", tx5, [merged_token])
    steps.append(step)

    auth2_live = assert_live(auth2["tx_hash"], auth2["index"], f"stateful {scenario} final mint authority").get("status") == "live"
    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "final_live_cells": {"mint_authority": auth2_live},
        "status": "passed",
    }

def run_stateful_timelock_release(always_success_dep):
    scenario = "timelock.create-lock-lock-asset-request-release-execute"
    actions = {
        name: deploy_stateful_action(action_record_by(timelock_action_artifacts, name), always_success_dep)
        for name in ("create_absolute_lock", "lock_asset", "request_release", "execute_release")
    }
    time_lock_type = always_success_lock("0xb1")
    locked_asset_type = always_success_lock("0xb2")
    request_type = always_success_lock("0xb3")
    record_type = always_success_lock("0xb4")
    owner = actions["execute_release"]["lock_hash"]
    lock_id = decode_hex(script_hash(time_lock_type), 32)
    asset_type_payload = bytes([0])
    current_height = 50
    unlock_height = 100
    release_height = 125
    steps = []

    initial = create_script_locked_cells(
        "stateful.timelock.create",
        [{"capacity": 500 * 100_000_000, "lock": actions["create_absolute_lock"]["lock"], "type": None, "data": b""}],
        actions["create_absolute_lock"]["cell_deps"],
    )
    create_input = initial["cells"][0]
    tx1 = transaction(
        create_input,
        [{"capacity": hex_u64(300 * 100_000_000), "lock": actions["execute_release"]["lock"], "type": time_lock_type}],
        ["0x" + timelock_data(owner, 0, unlock_height, current_height, lock_id=lock_id).hex()],
        actions["create_absolute_lock"]["cell_deps"],
        [entry_witness(lock_id, owner, unlock_height, current_height)],
    )
    step = run_stateful_step(scenario, "create_absolute_lock_for_release", tx1, [create_input])
    steps.append(step)
    time_lock_cell = output_cell_from_tx(step["commit"], tx1, 0)
    time_lock_dep = cell_dep_for(time_lock_cell)

    lock_asset_initial = create_script_locked_cells(
        "stateful.timelock.lock_asset",
        [{"capacity": 1000 * 100_000_000, "lock": actions["lock_asset"]["lock"], "type": None, "data": b""}],
        actions["lock_asset"]["cell_deps"],
    )
    lock_asset_input = lock_asset_initial["cells"][0]
    tx2 = transaction(
        lock_asset_input,
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": actions["execute_release"]["lock"], "type": locked_asset_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ],
        [
            "0x" + locked_asset_molecule_data(asset_type_payload, 42, lock_id).hex(),
            "0x",
        ],
        [time_lock_dep] + actions["lock_asset"]["cell_deps"],
        [entry_witness(molecule_bytes(asset_type_payload), 42)],
    )
    step = run_stateful_step(scenario, "lock_asset_against_live_lock", tx2, [lock_asset_input])
    steps.append(step)
    locked_asset_cell = output_cell_from_tx(step["commit"], tx2, 0)

    request_initial = create_script_locked_cells(
        "stateful.timelock.request_release",
        [{"capacity": 1000 * 100_000_000, "lock": actions["request_release"]["lock"], "type": None, "data": b""}],
        actions["request_release"]["cell_deps"],
    )
    request_input = request_initial["cells"][0]
    tx3 = transaction(
        request_input,
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": actions["execute_release"]["lock"], "type": request_type},
            {"capacity": hex_u64(700 * 100_000_000), "lock": always_success_lock(), "type": None},
        ],
        [
            "0x" + release_request_data(lock_id, owner, release_height, state=0).hex(),
            "0x",
        ],
        [time_lock_dep] + actions["request_release"]["cell_deps"],
        [entry_witness(owner, release_height)],
    )
    step = run_stateful_step(scenario, "request_release_from_live_lock", tx3, [request_input])
    steps.append(step)
    request_cell = output_cell_from_tx(step["commit"], tx3, 0)

    tx4 = transaction(
        [time_lock_cell, locked_asset_cell, request_cell],
        [{"capacity": hex_u64(300 * 100_000_000), "lock": always_success_lock(), "type": record_type}],
        ["0x" + release_record_data(lock_id, release_height, owner).hex()],
        actions["execute_release"]["cell_deps"],
        [entry_witness(owner, release_height), "0x", "0x"],
    )
    step = run_stateful_step(scenario, "execute_release_from_live_cells", tx4, [time_lock_cell, locked_asset_cell, request_cell])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_nft_listing_sale(always_success_dep):
    scenario = "nft.mint-list-transfer-by-listing"
    actions = {
        name: deploy_stateful_action(action_record_by(nft_action_artifacts, name), always_success_dep)
        for name in ("create_collection", "mint", "create_listing", "buy_from_listing")
    }
    collection_type = always_success_lock("0xc1")
    nft_type = always_success_lock("0xc2")
    listing_type = always_success_lock("0xc3")
    royalty_payment_type = always_success_lock("0xc4")
    seller = bytes([0x11]) * 32
    buyer_lock = always_success_lock("0xc5")
    buyer = decode_hex(script_hash(buyer_lock), 32)
    collection_creator = actions["mint"]["lock_hash"]
    royalty_recipient = collection_creator
    collection_name = b"Stateful Collection"
    collection_symbol = b"SNFT"
    collection_base_uri = b"ckb://cellscript/stateful-nft/"
    max_supply = 200
    metadata_hash = bytes([0x33]) * 32
    token_id = 1
    price = 10_000
    royalty_amount = 250
    seller_amount = price - royalty_amount
    created_at = 70
    steps = []

    initial = create_script_locked_cells(
        "stateful.nft.collection_seed",
        [{
            "capacity": 900 * 100_000_000,
            "lock": actions["create_collection"]["lock"],
            "type": None,
            "data": b"",
        }],
        actions["create_collection"]["cell_deps"],
    )
    collection_seed = initial["cells"][0]
    tx1 = transaction(
        collection_seed,
        [{"capacity": hex_u64(800 * 100_000_000), "lock": actions["mint"]["lock"], "type": collection_type}],
        ["0x" + collection_molecule_data(collection_creator, 0, max_supply, collection_name, collection_symbol, collection_base_uri).hex()],
        actions["create_collection"]["cell_deps"],
        [
            entry_witness(
                collection_creator,
                max_supply,
                molecule_string_witness(collection_name),
                molecule_string_witness(collection_symbol),
                molecule_string_witness(collection_base_uri),
            )
        ],
    )
    step = run_stateful_step(scenario, "create_collection_for_live_mint", tx1, [collection_seed])
    steps.append(step)
    collection0 = output_cell_from_tx(step["commit"], tx1, 0)

    tx2 = transaction(
        collection0,
        [
            {"capacity": hex_u64(500 * 100_000_000), "lock": actions["mint"]["lock"], "type": collection_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": actions["buy_from_listing"]["lock"], "type": nft_type},
        ],
        [
            "0x" + collection_molecule_data(collection_creator, token_id, max_supply, collection_name, collection_symbol, collection_base_uri).hex(),
            "0x" + nft_data(token_id, seller, metadata_hash, royalty_recipient, 250).hex(),
        ],
        actions["mint"]["cell_deps"],
        [entry_witness(seller, metadata_hash)],
    )
    step = run_stateful_step(scenario, "mint_nft_for_listing_sale", tx2, [collection0])
    steps.append(step)
    nft_for_sale = output_cell_from_tx(step["commit"], tx2, 1)
    nft_dep = cell_dep_for(nft_for_sale)

    listing_initial = create_script_locked_cells(
        "stateful.nft.create_listing",
        [{"capacity": 500 * 100_000_000, "lock": actions["create_listing"]["lock"], "type": None, "data": b""}],
        actions["create_listing"]["cell_deps"],
    )
    listing_input = listing_initial["cells"][0]
    tx3 = transaction(
        listing_input,
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": actions["buy_from_listing"]["lock"], "type": listing_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": always_success_lock(), "type": None},
        ],
        [
            "0x" + listing_data(token_id, seller, price, created_at, state=0).hex(),
            "0x",
        ],
        [nft_dep] + actions["create_listing"]["cell_deps"],
        [entry_witness(price, created_at)],
    )
    step = run_stateful_step(scenario, "create_listing_from_live_nft_dep", tx3, [listing_input])
    steps.append(step)
    listing = output_cell_from_tx(step["commit"], tx3, 0)

    tx4 = transaction(
        [nft_for_sale, listing],
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": buyer_lock, "type": nft_type},
            {"capacity": hex_u64(150 * 100_000_000), "lock": always_success_lock(), "type": royalty_payment_type},
            {"capacity": hex_u64(150 * 100_000_000), "lock": always_success_lock(), "type": royalty_payment_type},
        ],
        [
            "0x" + nft_data(token_id, buyer, metadata_hash, royalty_recipient, 250).hex(),
            "0x" + royalty_payment_data(token_id, royalty_recipient, royalty_amount).hex(),
            "0x" + royalty_payment_data(token_id, seller, seller_amount).hex(),
        ],
        actions["buy_from_listing"]["cell_deps"],
        [entry_witness(buyer, seller, price), "0x"],
    )
    step = run_stateful_step(scenario, "buy_listing_from_live_nft_and_listing", tx4, [nft_for_sale, listing])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_launch_to_token_mint(always_success_dep):
    scenario = "launch.launch-token-then-mint-with-authority"
    launch = deploy_stateful_action(action_record_by(launch_action_artifacts, "launch_token"), always_success_dep)
    mint = deploy_stateful_action(action_record_by(token_action_artifacts, "mint_with_authority"), always_success_dep)
    actions = {"launch_token": launch, "mint_with_authority": mint}
    auth_type = always_success_lock("0x91")
    token_type = always_success_lock("0x92")
    pool_paired_type = always_success_lock("0x93")
    pool_type = always_success_lock("0x94")
    lp_type = always_success_lock("0x95")
    symbol = b"LAUNCH01"
    paired_symbol = b"PAIR0001"
    max_supply = 10_000
    initial_mint = 1_000
    extra_mint = 25
    pool_seed_amount = 500
    paired_amount = 250
    fee_rate_bps = 30
    creator = mint["lock_hash"]
    recipient_locks = [always_success_lock("0xa" + format(index, "x")) for index in range(4)]
    recipients = [
        (decode_hex(script_hash(lock), 32), amount)
        for lock, amount in zip(recipient_locks, [10, 20, 30, 40])
    ]
    recipient_payload = fixed_recipient_tuple_array4(recipients)
    total_distributed = sum(amount for _, amount in recipients)
    remaining = initial_mint - total_distributed - pool_seed_amount
    pool_id = decode_hex(script_hash(pool_type), 32)
    steps = []

    initial = create_script_locked_cells(
        "stateful.launch.paired_token",
        [{
            "capacity": 4000 * 100_000_000,
            "lock": launch["lock"],
            "type": pool_paired_type,
            "data": token_data(paired_amount, paired_symbol),
        }],
        launch["cell_deps"],
    )
    paired_input = initial["cells"][0]
    outputs = [{"capacity": hex_u64(400 * 100_000_000), "lock": mint["lock"], "type": auth_type}]
    outputs_data = ["0x" + mint_authority_data(symbol, max_supply, initial_mint).hex()]
    for recipient_lock, (_, amount) in zip(recipient_locks, recipients):
        outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": recipient_lock, "type": token_type})
        outputs_data.append("0x" + token_data(amount, symbol).hex())
    outputs.append({"capacity": hex_u64(400 * 100_000_000), "lock": always_success_lock(), "type": pool_type})
    outputs_data.append("0x" + pool_data(symbol, paired_symbol, pool_seed_amount, paired_amount, pool_seed_amount, fee_rate_bps).hex())
    outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": mint["lock"], "type": lp_type})
    outputs_data.append("0x" + lp_receipt_data(pool_id, pool_seed_amount, creator).hex())
    outputs.append({"capacity": hex_u64(200 * 100_000_000), "lock": mint["lock"], "type": token_type})
    outputs_data.append("0x" + token_data(remaining, symbol).hex())

    tx1 = transaction(
        paired_input,
        outputs,
        outputs_data,
        launch["cell_deps"],
        [entry_witness(symbol, max_supply, initial_mint, pool_seed_amount, bytes([fee_rate_bps & 0xff, fee_rate_bps >> 8]), creator, recipient_payload)],
    )
    step = run_stateful_step(scenario, "launch_token_to_live_mint_authority", tx1, [paired_input])
    steps.append(step)
    auth_for_mint = output_cell_from_tx(step["commit"], tx1, 0)

    to_lock = always_success_lock("0xa4")
    to = decode_hex(script_hash(to_lock), 32)
    tx2 = transaction(
        auth_for_mint,
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": mint["lock"], "type": auth_type},
            {"capacity": hex_u64(100 * 100_000_000), "lock": to_lock, "type": token_type},
        ],
        [
            "0x" + mint_authority_data(symbol, max_supply, initial_mint + extra_mint).hex(),
            "0x" + token_data(extra_mint, symbol).hex(),
        ],
        mint["cell_deps"],
        [entry_witness(to, extra_mint)],
    )
    step = run_stateful_step(scenario, "mint_with_authority_again_from_launched_authority", tx2, [auth_for_mint])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_amm_pool_lifecycle(always_success_dep):
    scenario = "amm.seed-add-swap-remove"
    actions = {
        name: deploy_stateful_action(action_record_by(amm_action_artifacts, name), always_success_dep)
        for name in ("seed_pool", "add_liquidity", "swap_a_for_b", "remove_liquidity")
    }
    token_a_symbol = b"AMMA0001"
    token_b_symbol = b"AMMB0001"
    token_a_type = always_success_lock("0xd1")
    token_b_type = always_success_lock("0xd2")
    pool_type = always_success_lock("0xd3")
    lp_type = always_success_lock("0xd4")
    provider_lock = actions["remove_liquidity"]["lock"]
    provider = actions["remove_liquidity"]["lock_hash"]
    pool_id = decode_hex(script_hash(pool_type), 32)
    fee_rate_bps = 30
    steps = []

    seed_initial = create_script_locked_cells(
        "stateful.amm.seed_inputs",
        [
            {"capacity": 200 * 100_000_000, "lock": actions["seed_pool"]["lock"], "type": token_a_type, "data": token_data(4, token_a_symbol)},
            {"capacity": 200 * 100_000_000, "lock": actions["seed_pool"]["lock"], "type": token_b_type, "data": token_data(9, token_b_symbol)},
        ],
        actions["seed_pool"]["cell_deps"],
    )
    tx1 = transaction(
        seed_initial["cells"],
        [
            {"capacity": hex_u64(200 * 100_000_000), "lock": actions["add_liquidity"]["lock"], "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": lp_type},
        ],
        [
            "0x" + pool_data(token_a_symbol, token_b_symbol, 4, 9, 6, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, 6, provider).hex(),
        ],
        actions["seed_pool"]["cell_deps"],
        [entry_witness(fee_rate_bps.to_bytes(2, "little"), provider), "0x"],
    )
    step = run_stateful_step(scenario, "seed_pool_for_add_liquidity", tx1, seed_initial["cells"])
    steps.append(step)
    pool_for_add = output_cell_from_tx(step["commit"], tx1, 0)

    add_tokens = create_script_locked_cells(
        "stateful.amm.add_liquidity_tokens",
        [
            {"capacity": 200 * 100_000_000, "lock": actions["add_liquidity"]["lock"], "type": token_a_type, "data": token_data(4, token_a_symbol)},
            {"capacity": 200 * 100_000_000, "lock": actions["add_liquidity"]["lock"], "type": token_b_type, "data": token_data(9, token_b_symbol)},
        ],
        actions["add_liquidity"]["cell_deps"],
    )
    add_inputs = [pool_for_add, *add_tokens["cells"]]
    tx2 = transaction(
        add_inputs,
        [
            {"capacity": hex_u64(200 * 100_000_000), "lock": actions["swap_a_for_b"]["lock"], "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": actions["remove_liquidity"]["lock"], "type": lp_type},
        ],
        [
            "0x" + pool_data(token_a_symbol, token_b_symbol, 8, 18, 12, fee_rate_bps).hex(),
            "0x" + lp_receipt_data(pool_id, 6, provider).hex(),
        ],
        actions["add_liquidity"]["cell_deps"],
        [entry_witness(provider), "0x", "0x"],
    )
    step = run_stateful_step(scenario, "add_liquidity_to_live_pool", tx2, add_inputs)
    steps.append(step)
    pool_for_swap = output_cell_from_tx(step["commit"], tx2, 0)
    receipt_for_remove = output_cell_from_tx(step["commit"], tx2, 1)

    swap_token = create_script_locked_cells(
        "stateful.amm.swap_token",
        [{"capacity": 200 * 100_000_000, "lock": actions["swap_a_for_b"]["lock"], "type": token_a_type, "data": token_data(2, token_a_symbol)}],
        actions["swap_a_for_b"]["cell_deps"],
    )
    swap_inputs = [pool_for_swap, swap_token["cells"][0]]
    to_lock = always_success_lock("0xd5")
    to = decode_hex(script_hash(to_lock), 32)
    tx3 = transaction(
        swap_inputs,
        [
            {"capacity": hex_u64(200 * 100_000_000), "lock": actions["remove_liquidity"]["lock"], "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": to_lock, "type": token_b_type},
        ],
        [
            "0x" + pool_data(token_a_symbol, token_b_symbol, 10, 15, 12, fee_rate_bps).hex(),
            "0x" + token_data(3, token_b_symbol).hex(),
        ],
        actions["swap_a_for_b"]["cell_deps"],
        [entry_witness(2, to), "0x"],
    )
    step = run_stateful_step(scenario, "swap_against_live_pool", tx3, swap_inputs)
    steps.append(step)
    pool_for_remove = output_cell_from_tx(step["commit"], tx3, 0)

    remove_funding = find_spendable_cellbase()
    remove_change_capacity = remove_funding["capacity"] - 200 * 100_000_000
    tx4 = transaction(
        [pool_for_remove, receipt_for_remove, remove_funding],
        [
            {"capacity": hex_u64(200 * 100_000_000), "lock": always_success_lock(), "type": pool_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_a_type},
            {"capacity": hex_u64(200 * 100_000_000), "lock": provider_lock, "type": token_b_type},
            {"capacity": hex_u64(remove_change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [
            "0x" + pool_data(token_a_symbol, token_b_symbol, 5, 8, 6, fee_rate_bps).hex(),
            "0x" + token_data(5, token_a_symbol).hex(),
            "0x" + token_data(7, token_b_symbol).hex(),
            "0x",
        ],
        actions["remove_liquidity"]["cell_deps"],
        [entry_witness(provider), "0x", "0x"],
    )
    step = run_stateful_step(scenario, "remove_liquidity_from_live_pool", tx4, [pool_for_remove, receipt_for_remove, remove_funding])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_vesting_revoke(always_success_dep):
    scenario = "vesting.create-config-grant-revoke"
    actions = {
        name: deploy_stateful_action(action_record_by(vesting_action_artifacts, name), always_success_dep)
        for name in ("create_vesting_config", "grant_vesting", "revoke_grant")
    }
    symbol = b"VEST0001"
    cliff_period = 10
    total_period = 100
    amount = 77
    config_type = always_success_lock("0x41")
    token_type = always_success_lock("0x44")
    grant_type = always_success_lock("0x43")
    admin_lock = always_success_lock()
    admin = decode_hex(script_hash(admin_lock), 32)
    beneficiary = actions["revoke_grant"]["lock_hash"]
    header_dep = get_block_by_number(0)["header"]["hash"]
    steps = []

    config_initial = create_script_locked_cells(
        "stateful.vesting.config_input",
        [{"capacity": 1000 * 100_000_000, "lock": actions["create_vesting_config"]["lock"], "type": None, "data": b""}],
        actions["create_vesting_config"]["cell_deps"],
    )
    config_input = config_initial["cells"][0]
    tx1 = transaction(
        config_input,
        [{"capacity": hex_u64(300 * 100_000_000), "lock": admin_lock, "type": config_type}],
        ["0x" + vesting_config_data(admin, symbol, cliff_period, total_period, True).hex()],
        actions["create_vesting_config"]["cell_deps"],
        [entry_witness(admin, symbol, cliff_period, total_period, bytes([1]))],
    )
    step = run_stateful_step(scenario, "create_config_for_grant", tx1, [config_input])
    steps.append(step)
    config_cell = output_cell_from_tx(step["commit"], tx1, 0)
    config_dep = cell_dep_for(config_cell)

    grant_initial = create_script_locked_cells(
        "stateful.vesting.grant_tokens",
        [{"capacity": 200 * 100_000_000, "lock": actions["grant_vesting"]["lock"], "type": token_type, "data": token_data(amount, symbol)}],
        actions["grant_vesting"]["cell_deps"],
    )
    grant_input = grant_initial["cells"][0]
    funding_input = find_spendable_cellbase()
    grant_change_capacity = grant_input["capacity"] + funding_input["capacity"] - 300 * 100_000_000
    tx2 = transaction(
        [grant_input, funding_input],
        [
            {"capacity": hex_u64(300 * 100_000_000), "lock": actions["revoke_grant"]["lock"], "type": grant_type},
            {"capacity": hex_u64(grant_change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [
            "0x" + vesting_grant_data(0, beneficiary, amount, 0, 0, cliff_period, total_period, symbol).hex(),
            "0x",
        ],
        [config_dep] + actions["grant_vesting"]["cell_deps"],
        [entry_witness(beneficiary), "0x"],
        [header_dep],
    )
    step = run_stateful_step(scenario, "grant_vesting_from_live_config", tx2, [grant_input, funding_input])
    steps.append(step)
    grant_cell = output_cell_from_tx(step["commit"], tx2, 0)

    tx3 = transaction(
        grant_cell,
        [
            {"capacity": hex_u64(150 * 100_000_000), "lock": actions["revoke_grant"]["lock"], "type": token_type},
            {"capacity": hex_u64(150 * 100_000_000), "lock": admin_lock, "type": token_type},
        ],
        [
            "0x" + token_data(0, symbol).hex(),
            "0x" + token_data(amount, symbol).hex(),
        ],
        [config_dep] + actions["revoke_grant"]["cell_deps"],
        [entry_witness(admin)],
        [header_dep],
    )
    step = run_stateful_step(scenario, "revoke_live_grant", tx3, [grant_cell])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_multisig_execution(always_success_dep):
    scenario = "multisig.create-propose-sign-sign-execute"
    actions = {
        name: deploy_stateful_action(action_record_by(multisig_action_artifacts, name), always_success_dep)
        for name in ("create_wallet", "propose_transfer", "add_signature", "execute_proposal")
    }
    wallet_type = always_success_lock("0xf1")
    proposal_type = always_success_lock("0xf2")
    confirmation_type = always_success_lock("0xf3")
    execution_type = always_success_lock("0xf4")
    signer_a = actions["propose_transfer"]["lock_hash"]
    signer_b = decode_hex(script_hash(always_success_lock("0xf5")), 32)
    target = decode_hex(script_hash(always_success_lock("0xf6")), 32)
    signature_a = bytes([0xa5]) * 64
    signature_b = bytes([0xb6]) * 64
    wallet_id = decode_hex(script_hash(wallet_type), 32)
    signers = [signer_a, signer_b]
    proposal_id = 1
    created_at = 20
    expires_at = created_at + 1440
    steps = []

    wallet_initial = create_script_locked_cells(
        "stateful.multisig.wallet_input",
        [{"capacity": 2000 * 100_000_000, "lock": actions["create_wallet"]["lock"], "type": None, "data": b""}],
        actions["create_wallet"]["cell_deps"],
    )
    wallet_input = wallet_initial["cells"][0]
    tx1 = transaction(
        wallet_input,
        [{"capacity": hex_u64(2000 * 100_000_000), "lock": actions["propose_transfer"]["lock"], "type": wallet_type}],
        ["0x" + multisig_wallet_molecule_data(wallet_id, signers, 2, 0, 10).hex()],
        actions["create_wallet"]["cell_deps"],
        [entry_witness(wallet_id, molecule_bytes(molecule_fixvec(signers)), bytes([2]), 10)],
    )
    step = run_stateful_step(scenario, "create_wallet_for_proposal", tx1, [wallet_input])
    steps.append(step)
    wallet_for_propose = output_cell_from_tx(step["commit"], tx1, 0)

    proposal_payload = multisig_proposal_molecule_data(
        wallet_id, proposal_id, signer_a, 0, target, 500, b"", [], 2, created_at, expires_at
    )
    wallet_after_payload = multisig_wallet_molecule_data(wallet_id, signers, 2, proposal_id, 10)
    tx2 = transaction(
        wallet_for_propose,
        [
            {"capacity": hex_u64(500 * 100_000_000), "lock": actions["propose_transfer"]["lock"], "type": wallet_type},
            {"capacity": hex_u64(1500 * 100_000_000), "lock": actions["add_signature"]["lock"], "type": proposal_type},
        ],
        ["0x" + wallet_after_payload.hex(), "0x" + proposal_payload.hex()],
        actions["propose_transfer"]["cell_deps"],
        [entry_witness(signer_a, target, 500, created_at)],
    )
    step = run_stateful_step(scenario, "propose_transfer_from_live_wallet", tx2, [wallet_for_propose])
    steps.append(step)
    wallet_dep_cell = output_cell_from_tx(step["commit"], tx2, 0)
    wallet_dep = cell_dep_for(wallet_dep_cell)
    proposal0 = output_cell_from_tx(step["commit"], tx2, 1)

    proposal1_payload = multisig_proposal_molecule_data(
        wallet_id, proposal_id, signer_a, 0, target, 500, b"", [(signer_a, signature_a)], 2, created_at, expires_at
    )
    tx3 = transaction(
        proposal0,
        [
            {"capacity": hex_u64(1200 * 100_000_000), "lock": actions["add_signature"]["lock"], "type": proposal_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": always_success_lock(), "type": confirmation_type},
        ],
        [
            "0x" + proposal1_payload.hex(),
            "0x" + signature_confirmation_data(proposal_id, signer_a, 30).hex(),
        ],
        [wallet_dep] + actions["add_signature"]["cell_deps"],
        [entry_witness(signer_a, signature_a, 30)],
    )
    step = run_stateful_step(scenario, "add_first_signature", tx3, [proposal0])
    steps.append(step)
    proposal1 = output_cell_from_tx(step["commit"], tx3, 0)

    proposal2_payload = multisig_proposal_molecule_data(
        wallet_id, proposal_id, signer_a, 0, target, 500, b"", [(signer_a, signature_a), (signer_b, signature_b)], 2, created_at, expires_at
    )
    tx4 = transaction(
        proposal1,
        [
            {"capacity": hex_u64(900 * 100_000_000), "lock": actions["execute_proposal"]["lock"], "type": proposal_type},
            {"capacity": hex_u64(300 * 100_000_000), "lock": always_success_lock(), "type": confirmation_type},
        ],
        [
            "0x" + proposal2_payload.hex(),
            "0x" + signature_confirmation_data(proposal_id, signer_b, 31).hex(),
        ],
        [wallet_dep] + actions["add_signature"]["cell_deps"],
        [entry_witness(signer_b, signature_b, 31)],
    )
    step = run_stateful_step(scenario, "add_second_signature", tx4, [proposal1])
    steps.append(step)
    proposal2 = output_cell_from_tx(step["commit"], tx4, 0)

    tx5 = transaction(
        proposal2,
        [{"capacity": hex_u64(400 * 100_000_000), "lock": always_success_lock(), "type": execution_type}],
        ["0x" + execution_record_data(proposal_id, signer_a, 40, 1).hex()],
        [wallet_dep] + actions["execute_proposal"]["cell_deps"],
        [entry_witness(signer_a, 40)],
    )
    step = run_stateful_step(scenario, "execute_signed_proposal", tx5, [proposal2])
    steps.append(step)

    return {
        "name": scenario,
        "kind": "stateful-scenario",
        "builder_backed": True,
        "builder_name": "cellscript-stateful-scenario-builder-v1",
        "actions": list(actions.keys()),
        "action_ids": action_ids(actions.values()),
        "steps": steps,
        "status": "passed",
    }

def run_stateful_scenario_suite(always_success_dep):
    required_records = all_stateful_action_records()
    required_ids = sorted(action_id(record) for record in required_records)
    expected_ids = expected_stateful_action_ids()
    missing_artifact_ids = sorted(set(expected_ids) - set(required_ids))
    unexpected_artifact_ids = sorted(set(required_ids) - set(expected_ids))
    if missing_artifact_ids:
        raise RuntimeError("stateful action artifacts missing: " + ", ".join(missing_artifact_ids))

    main_runs = [
        run_stateful_token_lifecycle(always_success_dep),
        run_stateful_nft_listing_sale(always_success_dep),
        run_stateful_timelock_release(always_success_dep),
        run_stateful_launch_to_token_mint(always_success_dep),
        run_stateful_amm_pool_lifecycle(always_success_dep),
        run_stateful_vesting_revoke(always_success_dep),
        run_stateful_multisig_execution(always_success_dep),
    ]
    covered_ids = set()
    for run in main_runs:
        covered_ids.update(run.get("action_ids", []))
    branch_runs = run_stateful_action_branch_coverage(always_success_dep, required_records, covered_ids)
    runs = main_runs + branch_runs
    for run in branch_runs:
        covered_ids.update(run.get("action_ids", []))
    missing_stateful_action_ids = sorted(set(required_ids) - covered_ids)
    if missing_stateful_action_ids:
        raise RuntimeError("stateful action coverage missing: " + ", ".join(missing_stateful_action_ids))

    return {
        "status": "passed",
        "scope": (
            "Strict stateful local CKB scenarios. End-to-end flows commit live output handoffs between "
            "related actions; branch scenarios then commit every remaining production acceptance action."
        ),
        "scenario_count": len(runs),
        "step_count": sum(len(run.get("steps", [])) for run in runs),
        "end_to_end_scenario_count": len(main_runs),
        "action_branch_scenario_count": len(branch_runs),
        "stateful_action_coverage": {
            "status": "passed",
            "required_action_count": len(required_ids),
            "covered_action_count": len(covered_ids),
            "required_action_ids": required_ids,
            "covered_action_ids": sorted(covered_ids),
            "missing_action_ids": missing_stateful_action_ids,
            "missing_artifact_ids": missing_artifact_ids,
            "unexpected_artifact_ids": unexpected_artifact_ids,
        },
        "runs": runs,
    }

try:
    tip_before = rpc("get_tip_header")
    genesis = get_block_by_number(0)
    genesis_cellbase_hash = genesis["transactions"][0]["hash"]
    always_success_dep = {
        "out_point": out_point(genesis_cellbase_hash, int(ALWAYS_SUCCESS_INDEX, 16)),
        "dep_type": "code",
    }
    report["onchain"].update({
        "tip_before": tip_before,
        "genesis_cellbase_hash": genesis_cellbase_hash,
    })
    write_report()

    for artifact_record in bundled_example_deployment_artifacts:
        deployment_result = run_bundled_example_deployment(artifact_record, always_success_dep)
        report["onchain"]["bundled_example_deployment_runs"].append(deployment_result)
        report["onchain"]["completed_bundled_example_deployments"] = len(
            report["onchain"]["bundled_example_deployment_runs"]
        )
        write_report()

    for artifact_record in artifacts:
        artifact_result = run_artifact(artifact_record, always_success_dep)
        report["onchain"]["artifact_runs"].append(artifact_result)
        report["onchain"]["completed_artifacts"] = len(report["onchain"]["artifact_runs"])
        write_report()

    for action_record in token_action_artifacts:
        action_result = run_token_action(action_record, always_success_dep)
        report["onchain"]["token_action_runs"].append(action_result)
        report["onchain"]["completed_token_actions"] = len(report["onchain"]["token_action_runs"])
        write_report()

    for action_record in nft_action_artifacts:
        action_result = run_nft_action(action_record, always_success_dep)
        report["onchain"]["nft_action_runs"].append(action_result)
        report["onchain"]["completed_nft_actions"] = len(report["onchain"]["nft_action_runs"])
        write_report()

    for action_record in timelock_action_artifacts:
        action_result = run_timelock_action(action_record, always_success_dep)
        report["onchain"]["timelock_action_runs"].append(action_result)
        report["onchain"]["completed_timelock_actions"] = len(report["onchain"]["timelock_action_runs"])
        write_report()

    for action_record in multisig_action_artifacts:
        action_result = run_multisig_action(action_record, always_success_dep)
        report["onchain"]["multisig_action_runs"].append(action_result)
        report["onchain"]["completed_multisig_actions"] = len(report["onchain"]["multisig_action_runs"])
        write_report()

    for action_record in vesting_action_artifacts:
        action_result = run_vesting_action(action_record, always_success_dep)
        report["onchain"]["vesting_action_runs"].append(action_result)
        report["onchain"]["completed_vesting_actions"] = len(report["onchain"]["vesting_action_runs"])
        write_report()

    for action_record in amm_action_artifacts:
        action_result = run_amm_action(action_record, always_success_dep)
        report["onchain"]["amm_action_runs"].append(action_result)
        report["onchain"]["completed_amm_actions"] = len(report["onchain"]["amm_action_runs"])
        write_report()

    for action_record in launch_action_artifacts:
        action_result = run_launch_action(action_record, always_success_dep)
        report["onchain"]["launch_action_runs"].append(action_result)
        report["onchain"]["completed_launch_actions"] = len(report["onchain"]["launch_action_runs"])
        write_report()

    for lock_record in original_scoped_lock_artifacts:
        lock_result = run_lock_spend_matrix(lock_record, always_success_dep)
        report["onchain"]["lock_spend_matrix_runs"].append(lock_result)
        report["onchain"]["completed_lock_spend_matrix"] = len(report["onchain"]["lock_spend_matrix_runs"])
        write_report()

    if run_stateful_scenarios:
        stateful_result = run_stateful_scenario_suite(always_success_dep)
        report["onchain"]["stateful_scenarios"] = stateful_result
        report["onchain"]["stateful_scenario_runs"] = stateful_result["runs"]
        write_report()

    tip_after = rpc("get_tip_header")
    report["onchain"]["tip_after"] = tip_after
    expected_artifact_count = len(artifacts)
    completed_artifact_names = [
        run["name"]
        for run in report["onchain"]["artifact_runs"]
        if run.get("status") == "passed"
        and run.get("code_cell_live") is True
        and run.get("locked_cell_live") is True
        and run.get("locked_cell_live_after_malformed_spend") is True
        and run.get("spend_recipient_live") is True
    ]
    report["onchain"]["bundled_examples_deployed_and_spent"] = [
        run["name"] for run in report["onchain"]["artifact_runs"] if run["kind"].startswith("bundled-example-")
    ]
    report["onchain"]["bundled_examples_deployed"] = [
        run["name"]
        for run in report["onchain"]["bundled_example_deployment_runs"]
        if run.get("status") == "passed" and run.get("code_cell_live") is True
    ]
    report["onchain"]["all_bundled_examples_deployed"] = (
        report["onchain"]["bundled_examples_deployed"] == report["bundled_examples_exact_order"]
    )
    report["onchain"]["all_artifacts_deployed_and_spent"] = (
        len(completed_artifact_names) == expected_artifact_count
        and len(report["onchain"]["artifact_runs"]) == expected_artifact_count
    )
    report["onchain"]["token_actions_exercised"] = [run["action"] for run in report["onchain"]["token_action_runs"]]
    report["onchain"]["all_token_actions_exercised"] = sorted(report["onchain"]["token_actions_exercised"]) == [
        "burn",
        "merge",
        "mint_with_authority",
        "transfer_token",
    ]
    report["onchain"]["nft_actions_exercised"] = [run["action"] for run in report["onchain"]["nft_action_runs"]]
    report["onchain"]["all_nft_actions_exercised"] = sorted(report["onchain"]["nft_actions_exercised"]) == [
        "accept_offer",
        "batch_mint",
        "burn",
        "buy_from_listing",
        "cancel_listing",
        "create_collection",
        "create_listing",
        "create_offer",
        "mint",
        "transfer",
    ]
    report["onchain"]["timelock_actions_exercised"] = [run["action"] for run in report["onchain"]["timelock_action_runs"]]
    report["onchain"]["all_timelock_actions_exercised"] = report["onchain"]["timelock_actions_exercised"] == [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "request_emergency_release",
        "approve_emergency_release",
        "extend_lock",
        "execute_release",
        "execute_emergency_release",
        "batch_create_locks",
    ]
    report["onchain"]["multisig_actions_exercised"] = [run["action"] for run in report["onchain"]["multisig_action_runs"]]
    report["onchain"]["all_multisig_actions_exercised"] = sorted(report["onchain"]["multisig_actions_exercised"]) == [
        "add_signature",
        "cancel_proposal",
        "create_wallet",
        "execute_proposal",
        "propose_add_signer",
        "propose_change_threshold",
        "propose_remove_signer",
        "propose_transfer",
    ]
    report["onchain"]["vesting_actions_exercised"] = [run["action"] for run in report["onchain"]["vesting_action_runs"]]
    report["onchain"]["all_vesting_actions_exercised"] = report["onchain"]["vesting_actions_exercised"] == [
        "create_vesting_config",
        "grant_vesting",
        "claim_vested",
        "revoke_grant",
    ]
    report["onchain"]["amm_actions_exercised"] = [run["action"] for run in report["onchain"]["amm_action_runs"]]
    report["onchain"]["all_amm_actions_exercised"] = sorted(report["onchain"]["amm_actions_exercised"]) == [
        "add_liquidity",
        "isqrt",
        "min",
        "remove_liquidity",
        "seed_pool",
        "swap_a_for_b",
    ]
    report["onchain"]["launch_actions_exercised"] = [run["action"] for run in report["onchain"]["launch_action_runs"]]
    report["onchain"]["all_launch_actions_exercised"] = report["onchain"]["launch_actions_exercised"] == [
        "launch_token",
        "bootstrap_token",
    ]
    all_action_runs = (
        report["onchain"]["token_action_runs"]
        + report["onchain"]["nft_action_runs"]
        + report["onchain"]["timelock_action_runs"]
        + report["onchain"]["multisig_action_runs"]
        + report["onchain"]["vesting_action_runs"]
        + report["onchain"]["amm_action_runs"]
        + report["onchain"]["launch_action_runs"]
    )
    report["onchain"]["builder_backed_action_count"] = sum(1 for run in all_action_runs if run.get("builder_backed"))
    report["onchain"]["handwritten_harness_action_count"] = sum(1 for run in all_action_runs if not run.get("builder_backed"))
    report["onchain"]["measured_cycles_action_count"] = sum(
        1
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("measured_cycles")) is not None
    )
    report["onchain"]["tx_size_measured_action_count"] = sum(
        1
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("consensus_serialized_tx_size_bytes")) is not None
    )
    report["onchain"]["occupied_capacity_measured_action_count"] = sum(
        1
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("occupied_capacity_shannons")) is not None
    )
    all_lock_runs = report["onchain"]["lock_spend_matrix_runs"]
    expected_lock_spend_count = len(original_scoped_lock_artifacts)
    report["onchain"]["lock_spend_matrix_count"] = len(all_lock_runs)
    report["onchain"]["builder_backed_lock_spend_matrix_count"] = sum(
        1 for run in all_lock_runs if run.get("builder_backed")
    )
    report["onchain"]["lock_valid_spend_count"] = sum(
        1
        for run in all_lock_runs
        if (run.get("valid_spend") or {}).get("status") == "passed"
        and (run.get("valid_spend") or {}).get("output_live") is True
    )
    report["onchain"]["lock_invalid_spend_count"] = sum(
        1
        for run in all_lock_runs
        if ((run.get("invalid_spend") or {}).get("rejection") or {}).get("expected_reason_matched") is True
        and ((run.get("invalid_spend") or {}).get("rejection") or {}).get("policy_or_capacity_reason") is False
    )
    report["onchain"]["measured_cycles_lock_count"] = sum(
        1
        for run in all_lock_runs
        if ((run.get("measured_constraints") or {}).get("measured_cycles")) is not None
    )
    report["onchain"]["tx_size_measured_lock_count"] = sum(
        1
        for run in all_lock_runs
        if ((run.get("measured_constraints") or {}).get("consensus_serialized_tx_size_bytes")) is not None
    )
    report["onchain"]["occupied_capacity_measured_lock_count"] = sum(
        1
        for run in all_lock_runs
        if ((run.get("measured_constraints") or {}).get("occupied_capacity_shannons")) is not None
    )
    report["onchain"]["locks_behavior_exercised"] = [run["name"] for run in all_lock_runs]
    report["onchain"]["all_locks_behavior_exercised"] = (
        report["onchain"]["lock_spend_matrix_count"] == expected_lock_spend_count
        and report["onchain"]["builder_backed_lock_spend_matrix_count"] == expected_lock_spend_count
        and report["onchain"]["lock_valid_spend_count"] == expected_lock_spend_count
        and report["onchain"]["lock_invalid_spend_count"] == expected_lock_spend_count
    )
    final_hardening_failures = []
    handwritten_actions = [f"{run['name']}" for run in all_action_runs if not run.get("builder_backed")]
    if handwritten_actions:
        final_hardening_failures.append(
            "builder-generated transactions are still missing for: " + ", ".join(handwritten_actions)
        )
    missing_tx_size_actions = [
        run["name"]
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("consensus_serialized_tx_size_bytes")) is None
    ]
    if missing_tx_size_actions:
        final_hardening_failures.append(
            "consensus-serialized tx size is not yet measured for: " + ", ".join(missing_tx_size_actions)
        )
    missing_occupied_capacity_actions = [
        run["name"]
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("occupied_capacity_shannons")) is None
    ]
    if missing_occupied_capacity_actions:
        final_hardening_failures.append(
            "exact occupied capacity is not yet derived for: " + ", ".join(missing_occupied_capacity_actions)
        )
    under_capacity_actions = [
        f"{run['name']}@{(run.get('measured_constraints') or {}).get('under_capacity_output_indexes')}"
        for run in all_action_runs
        if ((run.get("measured_constraints") or {}).get("capacity_is_sufficient") is False)
    ]
    if under_capacity_actions:
        final_hardening_failures.append(
            "builder-generated transactions contain under-capacity outputs: " + ", ".join(under_capacity_actions)
        )
    missing_lock_matrix = [
        run["name"]
        for run in all_lock_runs
        if not run.get("builder_backed")
        or (run.get("valid_spend") or {}).get("status") != "passed"
        or ((run.get("invalid_spend") or {}).get("rejection") or {}).get("expected_reason_matched") is not True
        or ((run.get("invalid_spend") or {}).get("rejection") or {}).get("policy_or_capacity_reason") is not False
    ]
    if len(all_lock_runs) != expected_lock_spend_count or missing_lock_matrix:
        final_hardening_failures.append(
            "builder-backed lock valid/invalid spend matrix is incomplete: "
            + ", ".join(missing_lock_matrix or [f"{len(all_lock_runs)}/{expected_lock_spend_count} locks"])
        )
    stateful_scenarios = report["onchain"].get("stateful_scenarios")
    if run_stateful_scenarios:
        stateful_coverage = (stateful_scenarios or {}).get("stateful_action_coverage") or {}
        if (
            not stateful_scenarios
            or stateful_scenarios.get("status") != "passed"
            or stateful_coverage.get("status") != "passed"
            or stateful_coverage.get("missing_action_ids")
            or stateful_coverage.get("missing_artifact_ids")
        ):
            final_hardening_failures.append(
                "stateful scenario coverage is incomplete: "
                + json.dumps(stateful_coverage, sort_keys=True)
            )
    missing_lock_tx_size = [
        run["name"]
        for run in all_lock_runs
        if ((run.get("measured_constraints") or {}).get("consensus_serialized_tx_size_bytes")) is None
    ]
    if missing_lock_tx_size:
        final_hardening_failures.append(
            "consensus-serialized tx size is not yet measured for lock spends: " + ", ".join(missing_lock_tx_size)
        )
    under_capacity_locks = [
        f"{run['name']}@{(run.get('measured_constraints') or {}).get('under_capacity_output_indexes')}"
        for run in all_lock_runs
        if ((run.get("measured_constraints") or {}).get("capacity_is_sufficient") is False)
    ]
    if under_capacity_locks:
        final_hardening_failures.append(
            "builder-generated lock spend transactions contain under-capacity outputs: " + ", ".join(under_capacity_locks)
        )
    build_report_gate = refresh_build_report_deployments()
    if build_report_gate.get("status") != "passed":
        final_hardening_failures.append(
            "build report live artifact linkage failed: "
            + json.dumps(
                {
                    "missing_onchain_deployments": build_report_gate.get("missing_onchain_deployments"),
                    "live_code_cell_data_hash_mismatches": build_report_gate.get("live_code_cell_data_hash_mismatches"),
                    "unexpected_onchain_artifacts": build_report_gate.get("unexpected_onchain_artifacts"),
                },
                sort_keys=True,
            )
        )
    report["final_production_hardening_gate"] = {
        "status": "passed" if not final_hardening_failures else "blocked",
        "ready": not final_hardening_failures,
        "requires_builder_generated_transactions": True,
        "requires_measured_cycles": True,
        "requires_consensus_serialized_tx_size": True,
        "requires_exact_occupied_capacity": True,
        "requires_stateful_action_coverage": run_stateful_scenarios,
        "requires_build_report_live_artifact_linkage": True,
        "failures": final_hardening_failures,
    }
    update_ckb_business_coverage({
        "token.cell": report["onchain"]["token_actions_exercised"],
        "nft.cell": report["onchain"]["nft_actions_exercised"],
        "timelock.cell": report["onchain"]["timelock_actions_exercised"],
        "multisig.cell": report["onchain"]["multisig_actions_exercised"],
        "vesting.cell": report["onchain"]["vesting_actions_exercised"],
        "amm_pool.cell": report["onchain"]["amm_actions_exercised"],
        "launch.cell": report["onchain"]["launch_actions_exercised"],
    })
    missing_strict_original_deployments = sorted(
        set(report["bundled_examples_exact_order"]) - set(report["onchain"]["bundled_examples_deployed"])
    )
    report["onchain"]["strict_original_bundled_deployment_gate"] = {
        "status": "passed" if not missing_strict_original_deployments else "partial",
        "deployed": report["onchain"]["bundled_examples_deployed"],
        "missing": missing_strict_original_deployments,
        "fatal_in_mode": report.get("acceptance_mode") == "production",
    }
    if report.get("acceptance_mode") == "production" and not report["onchain"]["all_bundled_examples_deployed"]:
        raise RuntimeError(
            "not all primitive-strict original bundled examples deployed: "
            f"deployed={report['onchain']['bundled_examples_deployed']}, "
            f"expected={report['bundled_examples_exact_order']}"
        )
    if not report["onchain"]["all_artifacts_deployed_and_spent"]:
        raise RuntimeError(
            "not all CKB artifacts deployed and spent: "
            f"completed={completed_artifact_names}, "
            f"expected_artifact_count={expected_artifact_count}"
        )
    if not report["onchain"]["all_token_actions_exercised"]:
        raise RuntimeError(f"incomplete token action coverage: {report['onchain']['token_actions_exercised']}")
    if not report["onchain"]["all_nft_actions_exercised"]:
        raise RuntimeError(f"incomplete nft action coverage: {report['onchain']['nft_actions_exercised']}")
    if not report["onchain"]["all_timelock_actions_exercised"]:
        raise RuntimeError(f"incomplete timelock action coverage: {report['onchain']['timelock_actions_exercised']}")
    if not report["onchain"]["all_multisig_actions_exercised"]:
        raise RuntimeError(f"incomplete multisig action coverage: {report['onchain']['multisig_actions_exercised']}")
    if not report["onchain"]["all_vesting_actions_exercised"]:
        raise RuntimeError(f"incomplete vesting action coverage: {report['onchain']['vesting_actions_exercised']}")
    if not report["onchain"]["all_amm_actions_exercised"]:
        raise RuntimeError(f"incomplete AMM action coverage: {report['onchain']['amm_actions_exercised']}")
    if not report["onchain"]["all_launch_actions_exercised"]:
        raise RuntimeError(f"incomplete launch action coverage: {report['onchain']['launch_actions_exercised']}")
    if not report["onchain"]["all_locks_behavior_exercised"]:
        raise RuntimeError(f"incomplete lock behavior coverage: {report['onchain']['locks_behavior_exercised']}")
    report["status"] = "passed"
    report["onchain"]["status"] = "passed"
    write_report()
except Exception as error:
    report["status"] = "failed"
    report["onchain"]["status"] = "failed"
    report["onchain"]["error"] = str(error)
    write_report()
    raise
PY

if [[ "$ACCEPTANCE_MODE" == "production" ]]; then
  if [[ "$RUN_ONCHAIN" == "1" ]]; then
    python3 "$REPO_ROOT/scripts/validate_ckb_cellscript_production_evidence.py" "$REPORT_JSON"
  else
    python3 "$REPO_ROOT/scripts/validate_ckb_cellscript_production_evidence.py" "$REPORT_JSON" --compile-only
    echo "CKB compile-only production evidence is not sufficient for external release; run without --compile-only for final hardening." >&2
  fi
fi
echo "CKB CellScript $ACCEPTANCE_MODE acceptance passed: $REPORT_JSON"
