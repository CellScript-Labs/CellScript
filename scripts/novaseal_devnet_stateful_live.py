#!/usr/bin/env python3
"""Run a minimal live CKB devnet NovaSeal stateful lifecycle.

This is intentionally narrow: it proves that the core NovaSeal lifecycle type
can be deployed as a live CellDep, create a bootstrap state cell, then consume
that exact outpoint in a signed transition that materializes the next state and
receipt outputs.
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
import time
import urllib.error
import urllib.request
from typing import Any


CKB_BLAKE2B_PERSONAL = b"ckb-default-hash"
PACKED_HASH_DOMAIN = b"CellScriptPackedHashV0\0"
ALWAYS_SUCCESS_CODE_HASH = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5"
ALWAYS_SUCCESS_INDEX = "0x5"
SHANNONS = 100_000_000
STATE_CAPACITY = 1_000 * SHANNONS
RECEIPT_CAPACITY = 1_000 * SHANNONS
VERSION = 0
OP_BOOTSTRAP = 0
OP_KEY_AUTH_TRANSITION = 1
TEST_SECRET_KEY = bytes.fromhex("3e7490680639a2f7bbe8361dd3f34eb6429a9c924d8b342c015e555e628f94e5")
TEST_AUX_RAND = bytes([0x42]) * 32
ZERO_HASH = bytes(32)
_UNSET = object()

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
G = (
    0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
    0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8,
)


class LiveAcceptanceError(RuntimeError):
    def __init__(self, message: str, *, rpc_error: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.rpc_error = rpc_error


SCRIPT_ERROR_CODE_KEYS = {
    "error_code",
    "errorCode",
    "exit_code",
    "exitCode",
    "script_error_code",
    "scriptErrorCode",
}


def _script_error_code_from_rpc_error(value: Any) -> int | None:
    if isinstance(value, dict):
        for key, nested in value.items():
            if key in SCRIPT_ERROR_CODE_KEYS:
                try:
                    return int(nested)
                except (TypeError, ValueError):
                    continue
            found = _script_error_code_from_rpc_error(nested)
            if found is not None:
                return found
    if isinstance(value, list):
        for nested in value:
            found = _script_error_code_from_rpc_error(nested)
            if found is not None:
                return found
    return None


def script_error_code_matches(reason: str, expected: int, rpc_error: dict[str, Any] | None = None) -> bool:
    if _script_error_code_from_rpc_error(rpc_error) == expected:
        return True
    patterns = [
        rf"\berror code\s*[:#]?\s*{expected}\b",
        rf"\berror_code\s*[:=]\s*{expected}\b",
        rf"\bexit[_ ]?code\s*[:=]\s*{expected}\b",
        rf"\bExitCode\(\s*{expected}\s*\)",
        rf"#{expected}\b",
    ]
    return any(re.search(pattern, reason, re.IGNORECASE) for pattern in patterns)


def sha256_hex(data: bytes) -> str:
    return "0x" + hashlib.sha256(data).hexdigest()


def file_sha256_hex(path: pathlib.Path) -> str:
    return sha256_hex(path.read_bytes())


def display_path(path: pathlib.Path, repo_root: pathlib.Path) -> str:
    try:
        return path.relative_to(repo_root).as_posix()
    except ValueError:
        return str(path)


def git_commit(repo_root: pathlib.Path) -> str | None:
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo_root, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def source_tree_hash(repo_root: pathlib.Path, paths: list[pathlib.Path]) -> dict[str, Any]:
    files: list[pathlib.Path] = []
    invalid_paths: list[str] = []
    for raw_path in paths:
        path = raw_path if raw_path.is_absolute() else repo_root / raw_path
        if path.is_symlink():
            invalid_paths.append(display_path(path, repo_root))
            continue
        if path.is_file():
            files.append(path)
            continue
        if path.is_dir():
            for child in path.rglob("*"):
                if any(part in {"target", "build", ".git", "__pycache__"} for part in child.relative_to(path).parts):
                    continue
                if child.is_symlink():
                    invalid_paths.append(display_path(child, repo_root))
                    continue
                if not child.is_file():
                    continue
                if child.suffix in {".cell", ".schema", ".toml", ".py", ".json", ".rs"} or child.name == "Cargo.lock":
                    files.append(child)
    h = hashlib.sha256()
    rows = []
    for path in sorted(set(files)):
        rel = display_path(path, repo_root)
        digest = hashlib.sha256(path.read_bytes()).digest()
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(digest)
        rows.append(rel)
    return {
        "sha256": None if invalid_paths else "0x" + h.hexdigest(),
        "files": rows,
        "file_count": len(rows),
        "valid": not invalid_paths,
        "invalid_paths": sorted(invalid_paths),
    }


def stateful_provenance(repo_root: pathlib.Path, source_paths: list[pathlib.Path], artifacts: dict[str, pathlib.Path]) -> dict[str, Any]:
    return {
        "repo_commit": git_commit(repo_root),
        "source_tree": source_tree_hash(repo_root, source_paths),
        "artifacts": {
            name: {
                "path": display_path(path, repo_root),
                "sha256": file_sha256_hex(path),
                "ckb_data_hash": ckb_hash_hex(path.read_bytes()),
                "size_bytes": path.stat().st_size,
            }
            for name, path in artifacts.items()
        },
    }


def parse_args() -> argparse.Namespace:
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    default_ckb_repo = repo_root.parent / "ckb"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=pathlib.Path, default=repo_root)
    parser.add_argument("--ckb-repo", type=pathlib.Path, default=default_ckb_repo)
    parser.add_argument("--ckb-bin", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=repo_root / "target/novaseal-devnet-stateful-live.json")
    parser.add_argument("--run-dir", type=pathlib.Path)
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--keep-node", action="store_true")
    return parser.parse_args()


def ckb_hash(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32, person=CKB_BLAKE2B_PERSONAL).digest()


def ckb_hash_hex(data: bytes) -> str:
    return "0x" + ckb_hash(data).hex()


def tagged_hash(tag: str, data: bytes) -> bytes:
    tag_hash = hashlib.sha256(tag.encode("ascii")).digest()
    return hashlib.sha256(tag_hash + tag_hash + data).digest()


def has_even_y(point: tuple[int, int]) -> bool:
    return point[1] % 2 == 0


def point_add(a: tuple[int, int] | None, b: tuple[int, int] | None) -> tuple[int, int] | None:
    if a is None:
        return b
    if b is None:
        return a
    x1, y1 = a
    x2, y2 = b
    if x1 == x2 and (y1 + y2) % P == 0:
        return None
    if a == b:
        lam = (3 * x1 * x1 * pow(2 * y1, -1, P)) % P
    else:
        lam = ((y2 - y1) * pow(x2 - x1, -1, P)) % P
    x3 = (lam * lam - x1 - x2) % P
    y3 = (lam * (x1 - x3) - y1) % P
    return (x3, y3)


def point_mul(k: int, point: tuple[int, int] = G) -> tuple[int, int] | None:
    result: tuple[int, int] | None = None
    addend: tuple[int, int] | None = point
    while k:
        if k & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result


def lift_x(x: int) -> tuple[int, int] | None:
    if x >= P:
        return None
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if (y * y) % P != y_sq:
        return None
    return (x, y if y % 2 == 0 else P - y)


def xonly_pubkey(secret_key: bytes) -> bytes:
    d = int.from_bytes(secret_key, "big")
    if not 1 <= d < N:
        raise LiveAcceptanceError("test secret key is out of range")
    point = point_mul(d)
    if point is None:
        raise LiveAcceptanceError("failed to derive test pubkey")
    return point[0].to_bytes(32, "big")


def schnorr_sign(message32: bytes, secret_key: bytes, aux_rand32: bytes) -> tuple[bytes, bytes]:
    if len(message32) != 32 or len(secret_key) != 32 or len(aux_rand32) != 32:
        raise LiveAcceptanceError("BIP340 signer expects 32-byte message, secret, and aux rand")
    d0 = int.from_bytes(secret_key, "big")
    if not 1 <= d0 < N:
        raise LiveAcceptanceError("secret key is out of range")
    p0 = point_mul(d0)
    if p0 is None:
        raise LiveAcceptanceError("secret key produced infinity")
    d = d0 if has_even_y(p0) else N - d0
    pubkey = p0[0].to_bytes(32, "big")
    t = bytes(a ^ b for a, b in zip(d.to_bytes(32, "big"), tagged_hash("BIP0340/aux", aux_rand32)))
    k0 = int.from_bytes(tagged_hash("BIP0340/nonce", t + pubkey + message32), "big") % N
    if k0 == 0:
        raise LiveAcceptanceError("BIP340 nonce was zero")
    r0 = point_mul(k0)
    if r0 is None:
        raise LiveAcceptanceError("BIP340 nonce produced infinity")
    k = k0 if has_even_y(r0) else N - k0
    rx = r0[0].to_bytes(32, "big")
    e = int.from_bytes(tagged_hash("BIP0340/challenge", rx + pubkey + message32), "big") % N
    sig = rx + ((k + e * d) % N).to_bytes(32, "big")
    if not schnorr_verify(message32, pubkey, sig):
        raise LiveAcceptanceError("self-generated BIP340 signature failed verification")
    return pubkey, sig


def schnorr_verify(message32: bytes, pubkey32: bytes, signature64: bytes) -> bool:
    if len(message32) != 32 or len(pubkey32) != 32 or len(signature64) != 64:
        return False
    px = int.from_bytes(pubkey32, "big")
    r = int.from_bytes(signature64[:32], "big")
    s = int.from_bytes(signature64[32:], "big")
    if px >= P or r >= P or s >= N:
        return False
    point = lift_x(px)
    if point is None:
        return False
    e = int.from_bytes(tagged_hash("BIP0340/challenge", signature64[:32] + pubkey32 + message32), "big") % N
    r_point = point_add(point_mul(s), point_mul(N - e, point))
    return r_point is not None and has_even_y(r_point) and r_point[0] == r


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def decode_hex(value: str) -> bytes:
    return bytes.fromhex(value[2:] if value.startswith("0x") else value)


def u8(value: int) -> bytes:
    return int(value).to_bytes(1, "little")


def u16(value: int) -> bytes:
    return int(value).to_bytes(2, "little")


def u32(value: int) -> bytes:
    return int(value).to_bytes(4, "little")


def u64(value: int) -> bytes:
    return int(value).to_bytes(8, "little")


def packed_hash(type_name: str, packed: bytes) -> bytes:
    preimage = PACKED_HASH_DOMAIN + type_name.encode("ascii") + b"\0" + u32(len(packed)) + packed
    return ckb_hash(preimage)


def cell_data_hash(packed: bytes) -> bytes:
    return ckb_hash(packed)


def pack_out_point(tx_hash: str, index: int) -> bytes:
    tx_hash_bytes = decode_hex(tx_hash)
    if len(tx_hash_bytes) != 32:
        raise LiveAcceptanceError(f"tx hash must be 32 bytes: {tx_hash}")
    return tx_hash_bytes + u32(index)


def pack_novaseal_cell(
    *,
    authority_hash: bytes,
    state_hash: bytes,
    policy_hash: bytes,
    latest_receipt_hash: bytes,
    nonce: int,
    expiry: int,
) -> bytes:
    return (
        u16(VERSION)
        + authority_hash
        + state_hash
        + policy_hash
        + latest_receipt_hash
        + u64(nonce)
        + u64(expiry)
    )


def pack_cell_commitment(*, authority_hash: bytes, state_hash: bytes, policy_hash: bytes, nonce: int, expiry: int) -> bytes:
    return u16(VERSION) + authority_hash + state_hash + policy_hash + u64(nonce) + u64(expiry)


def pack_intent_core(
    *,
    protocol_id: bytes,
    package_hash: bytes,
    policy_hash: bytes,
    action: int,
    terminal_path: int,
    old_tx_hash: str,
    old_index: int,
    old_state_hash: bytes,
    new_state_hash: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
) -> bytes:
    return (
        protocol_id
        + package_hash
        + policy_hash
        + u8(action)
        + u8(terminal_path)
        + pack_out_point(old_tx_hash, old_index)
        + old_state_hash
        + new_state_hash
        + u64(old_nonce)
        + u64(new_nonce)
        + u64(expiry)
    )


def pack_receipt_commitment(
    *,
    protocol_id: bytes,
    package_hash: bytes,
    policy_hash: bytes,
    action: int,
    terminal_path: int,
    old_tx_hash: str,
    old_index: int,
    new_cell_commitment: bytes,
    old_state_hash: bytes,
    new_state_hash: bytes,
    old_nonce: int,
    new_nonce: int,
    intent_core_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return (
        protocol_id
        + package_hash
        + policy_hash
        + u8(action)
        + u8(terminal_path)
        + pack_out_point(old_tx_hash, old_index)
        + new_cell_commitment
        + old_state_hash
        + new_state_hash
        + u64(old_nonce)
        + u64(new_nonce)
        + intent_core_hash
        + payout_commitment_hash
    )


def pack_receipt(
    *,
    protocol_id: bytes,
    package_hash: bytes,
    policy_hash: bytes,
    action: int,
    terminal_path: int,
    old_tx_hash: str,
    old_index: int,
    new_cell_commitment: bytes,
    old_state_hash: bytes,
    new_state_hash: bytes,
    old_nonce: int,
    new_nonce: int,
    intent_core_hash: bytes,
    signed_intent_hash: bytes,
    payout_commitment_hash: bytes,
    signer_authority_hash: bytes,
    expiry: int,
) -> bytes:
    return (
        protocol_id
        + package_hash
        + policy_hash
        + u8(action)
        + u8(terminal_path)
        + pack_out_point(old_tx_hash, old_index)
        + new_cell_commitment
        + old_state_hash
        + new_state_hash
        + u64(old_nonce)
        + u64(new_nonce)
        + intent_core_hash
        + signed_intent_hash
        + payout_commitment_hash
        + signer_authority_hash
        + u64(expiry)
    )


def build_transition_material(old_tx_hash: str, old_index: int, old_cell: dict[str, Any], new_state_hash: bytes) -> dict[str, bytes]:
    protocol_id = ckb_hash(b"NovaSeal/core/v0")
    package_hash = ckb_hash(b"NovaSeal/devnet/stateful/live")
    policy_hash = old_cell["policy_hash"]
    authority_hash = old_cell["authority_hash"]
    old_state_hash = old_cell["state_hash"]
    old_nonce = old_cell["nonce"]
    new_nonce = old_nonce + 1
    expiry = old_cell["expiry"]
    new_cell_commitment = packed_hash(
        "NovaSealCellCommitmentV0",
        pack_cell_commitment(
            authority_hash=authority_hash,
            state_hash=new_state_hash,
            policy_hash=policy_hash,
            nonce=new_nonce,
            expiry=expiry,
        ),
    )
    core = pack_intent_core(
        protocol_id=protocol_id,
        package_hash=package_hash,
        policy_hash=policy_hash,
        action=OP_KEY_AUTH_TRANSITION,
        terminal_path=OP_KEY_AUTH_TRANSITION,
        old_tx_hash=old_tx_hash,
        old_index=old_index,
        old_state_hash=old_state_hash,
        new_state_hash=new_state_hash,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
    )
    intent_core_hash = packed_hash("NovaSealIntentCoreV0", core)
    receipt_commitment = pack_receipt_commitment(
        protocol_id=protocol_id,
        package_hash=package_hash,
        policy_hash=policy_hash,
        action=OP_KEY_AUTH_TRANSITION,
        terminal_path=OP_KEY_AUTH_TRANSITION,
        old_tx_hash=old_tx_hash,
        old_index=old_index,
        new_cell_commitment=new_cell_commitment,
        old_state_hash=old_state_hash,
        new_state_hash=new_state_hash,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        intent_core_hash=intent_core_hash,
        payout_commitment_hash=ZERO_HASH,
    )
    materialized_receipt_hash = packed_hash("ProofReceiptCommitmentV0", receipt_commitment)
    signed_intent = core + materialized_receipt_hash
    signed_intent_hash = packed_hash("NovaSealSignedIntentV0", signed_intent)
    pubkey, signature = schnorr_sign(signed_intent_hash, TEST_SECRET_KEY, TEST_AUX_RAND)
    if pubkey != authority_hash:
        raise LiveAcceptanceError("derived pubkey does not match old cell authority hash")
    new_cell_data = pack_novaseal_cell(
        authority_hash=authority_hash,
        state_hash=new_state_hash,
        policy_hash=policy_hash,
        latest_receipt_hash=materialized_receipt_hash,
        nonce=new_nonce,
        expiry=expiry,
    )
    receipt_data = pack_receipt(
        protocol_id=protocol_id,
        package_hash=package_hash,
        policy_hash=policy_hash,
        action=OP_KEY_AUTH_TRANSITION,
        terminal_path=OP_KEY_AUTH_TRANSITION,
        old_tx_hash=old_tx_hash,
        old_index=old_index,
        new_cell_commitment=new_cell_commitment,
        old_state_hash=old_state_hash,
        new_state_hash=new_state_hash,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        intent_core_hash=intent_core_hash,
        signed_intent_hash=signed_intent_hash,
        payout_commitment_hash=ZERO_HASH,
        signer_authority_hash=authority_hash,
        expiry=expiry,
    )
    return {
        "core": core,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "state_hash_commitment": ckb_hash(new_state_hash),
        "signature_payload": pubkey + signature,
        "new_cell_data": new_cell_data,
        "receipt_data": receipt_data,
        "materialized_receipt_hash": materialized_receipt_hash,
        "new_state_hash": new_state_hash,
    }


def entry_witness(op: int, old_cell_data: bytes, signed_intent: bytes, state_hash_commitment: bytes, sig_payload: bytes) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + state_hash_commitment
        + u32(len(sig_payload))
        + sig_payload
    )
    return hex0x(payload)


def pick_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def resolve_ckb_bin(ckb_repo: pathlib.Path, ckb_bin: pathlib.Path | None) -> pathlib.Path:
    if ckb_bin is not None:
        if not ckb_bin.exists() or not os.access(ckb_bin, os.X_OK):
            raise LiveAcceptanceError(f"CKB binary is not executable: {ckb_bin}")
        return ckb_bin.resolve()
    for candidate in (ckb_repo / "target/debug/ckb", ckb_repo / "target/release/ckb"):
        if candidate.exists() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    raise LiveAcceptanceError(f"no CKB binary found under {ckb_repo}; pass --ckb-bin")


def patch_ckb_toml(path: pathlib.Path, rpc_port: int, p2p_port: int) -> None:
    text = path.read_text(encoding="utf-8")
    text = re.sub(r'listen_address = "127\.0\.0\.1:\d+"', f'listen_address = "127.0.0.1:{rpc_port}"', text, count=1)
    text = re.sub(
        r'listen_addresses = \["/ip4/0\.0\.0\.0/tcp/\d+"\]',
        f'listen_addresses = ["/ip4/127.0.0.1/tcp/{p2p_port}"]',
        text,
        count=1,
    )
    path.write_text(text, encoding="utf-8")


class CkbDevnet:
    def __init__(self, ckb_repo: pathlib.Path, ckb_bin: pathlib.Path, run_dir: pathlib.Path):
        self.ckb_repo = ckb_repo
        self.ckb_bin = ckb_bin
        self.run_dir = run_dir
        self.ckb_dir = run_dir / "ckb-node"
        self.log_path = run_dir / "ckb.log"
        self.rpc_port = pick_port()
        self.p2p_port = pick_port()
        self.rpc_url = f"http://127.0.0.1:{self.rpc_port}"
        self.proc: subprocess.Popen[bytes] | None = None
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.reserved: set[tuple[str, int]] = set()

    def start(self) -> None:
        template = self.ckb_repo / "test/template"
        if not template.is_dir():
            raise LiveAcceptanceError(f"CKB test template not found: {template}")
        self.ckb_dir.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(template, self.ckb_dir)
        patch_ckb_toml(self.ckb_dir / "ckb.toml", self.rpc_port, self.p2p_port)
        log = self.log_path.open("wb")
        self.proc = subprocess.Popen(
            [str(self.ckb_bin), "-C", str(self.ckb_dir), "run", "--ba-advanced"],
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        for _ in range(80):
            try:
                self.rpc("get_tip_header")
                return
            except Exception:
                if self.proc.poll() is not None:
                    raise LiveAcceptanceError(f"CKB process exited early; see {self.log_path}")
                time.sleep(0.25)
        raise LiveAcceptanceError(f"CKB RPC did not become ready at {self.rpc_url}; see {self.log_path}")

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=5)

    def rpc(self, method: str, params: list[Any] | None = None) -> Any:
        body = json.dumps({"id": 42, "jsonrpc": "2.0", "method": method, "params": params or []}).encode()
        request = urllib.request.Request(self.rpc_url, data=body, headers={"Content-Type": "application/json"})
        last_error: Exception | None = None
        for attempt in range(6):
            try:
                with self.opener.open(request, timeout=20) as response:
                    payload = json.loads(response.read().decode("utf-8"))
                    break
            except (urllib.error.HTTPError, urllib.error.URLError) as error:
                last_error = error
                if attempt == 5:
                    raise LiveAcceptanceError(f"RPC {method} failed after retries: {last_error}") from error
                time.sleep(0.25 * (attempt + 1))
        else:
            raise LiveAcceptanceError(f"RPC {method} failed: {last_error}")
        if payload.get("error"):
            raise LiveAcceptanceError(f"RPC {method} returned error: {payload['error']}", rpc_error=payload["error"])
        return payload.get("result")

    def get_block(self, block_hash: str) -> dict[str, Any]:
        for _ in range(20):
            block = self.rpc("get_block", [block_hash])
            if block is not None:
                return block
            time.sleep(0.05)
        raise LiveAcceptanceError(f"block not found: {block_hash}")

    def get_block_by_number(self, number: int) -> dict[str, Any]:
        block = self.rpc("get_block_by_number", [hex(number)])
        if block is None:
            raise LiveAcceptanceError(f"block number not found: {number}")
        return block

    def wait_live_cell(self, tx_hash: str, index: int) -> dict[str, Any]:
        last = None
        for _ in range(40):
            last = self.rpc("get_live_cell", [{"tx_hash": tx_hash, "index": hex(index)}, True])
            if last and last.get("status") == "live":
                return last
            time.sleep(0.05)
        raise LiveAcceptanceError(f"cell is not live: {tx_hash}:{index}; last={last}")

    def assert_live_cell(
        self,
        tx_hash: str,
        index: int,
        *,
        label: str,
        expected_capacity: int | None = None,
        expected_lock: dict[str, Any] | None = None,
        expected_type: Any = _UNSET,
        expected_data: bytes | None = None,
    ) -> dict[str, Any]:
        live = self.wait_live_cell(tx_hash, index)
        cell = live.get("cell") or {}
        output = cell.get("output") or {}
        data = cell.get("data") or {}
        if expected_capacity is not None and int(output.get("capacity", "0x0"), 16) != expected_capacity:
            raise LiveAcceptanceError(f"{label} capacity mismatch: {output.get('capacity')} != {hex(expected_capacity)}")
        if expected_lock is not None and output.get("lock") != expected_lock:
            raise LiveAcceptanceError(f"{label} lock mismatch: {output.get('lock')} != {expected_lock}")
        if expected_type is not _UNSET and output.get("type") != expected_type:
            raise LiveAcceptanceError(f"{label} type mismatch: {output.get('type')} != {expected_type}")
        if expected_data is not None:
            expected_content = hex0x(expected_data)
            expected_hash = ckb_hash_hex(expected_data)
            if data.get("content") != expected_content:
                raise LiveAcceptanceError(f"{label} data content mismatch")
            if data.get("hash") != expected_hash:
                raise LiveAcceptanceError(f"{label} data hash mismatch: {data.get('hash')} != {expected_hash}")
        return live

    def wait_dead_cell(self, tx_hash: str, index: int) -> dict[str, Any]:
        last = None
        for _ in range(40):
            last = self.rpc("get_live_cell", [{"tx_hash": tx_hash, "index": hex(index)}, False])
            if last and last.get("status") != "live":
                return last
            time.sleep(0.05)
        raise LiveAcceptanceError(f"cell is still live: {tx_hash}:{index}; last={last}")

    def find_spendable_cellbase(self, max_blocks: int = 80) -> dict[str, Any]:
        for _ in range(max_blocks):
            block_hash = self.rpc("generate_block")
            block = self.get_block(block_hash)
            cellbase = block["transactions"][0]
            for index, output in enumerate(cellbase.get("outputs", [])):
                capacity = int(output["capacity"], 16)
                key = (cellbase["hash"], index)
                if capacity > 0 and key not in self.reserved:
                    self.wait_live_cell(cellbase["hash"], index)
                    self.reserved.add(key)
                    return {"tx_hash": cellbase["hash"], "index": index, "capacity": capacity}
        raise LiveAcceptanceError("no spendable cellbase found")

    def collect_spendable(self, min_capacity: int) -> dict[str, Any]:
        cells = []
        total = 0
        while total < min_capacity:
            cell = self.find_spendable_cellbase()
            cells.append(cell)
            total += int(cell["capacity"])
        return {"cells": cells, "total_capacity": total}

    def submit_and_commit(self, tx: dict[str, Any], label: str) -> dict[str, Any]:
        tx_hash = self.rpc("send_test_transaction", [tx, "passthrough"])
        last_status = None
        for generated in range(80):
            status = self.rpc("get_transaction", [tx_hash])
            tx_status = (status or {}).get("tx_status", {})
            last_status = tx_status
            if tx_status.get("status") == "committed":
                return {"tx_hash": tx_hash, "generated_blocks_after_submit": generated}
            if tx_status.get("status") == "rejected":
                raise LiveAcceptanceError(f"{label} rejected: {tx_hash}; status={tx_status}")
            self.rpc("generate_block")
            time.sleep(0.05)
        raise LiveAcceptanceError(f"{label} not committed: {tx_hash}; last_status={last_status}")

    def dry_run_rejects(
        self,
        tx: dict[str, Any],
        label: str,
        *,
        expected_source: str | None = None,
        expected_data_hash: str | None = None,
        expected_error_code: int | None = None,
    ) -> dict[str, Any]:
        try:
            result = self.rpc("dry_run_transaction", [tx])
        except LiveAcceptanceError as error:
            reason = str(error)
            checks: dict[str, bool] = {}
            if expected_source is not None:
                checks["source"] = expected_source in reason
            if expected_data_hash is not None:
                checks["data_hash"] = expected_data_hash.lower().removeprefix("0x") in reason.lower()
            if expected_error_code is not None:
                checks["error_code"] = script_error_code_matches(reason, expected_error_code, error.rpc_error)
            matched = all(checks.values()) if checks else True
            if not matched:
                raise LiveAcceptanceError(f"{label} rejected for unexpected reason: checks={checks} reason={reason}") from error
            return {
                "status": "rejected",
                "label": label,
                "reason": reason,
                "expected": {
                    "source": expected_source,
                    "data_hash": expected_data_hash,
                    "error_code": expected_error_code,
                },
                "matched_expected": matched,
            }
        raise LiveAcceptanceError(f"{label} unexpectedly passed dry-run: {result}")


def out_point(tx_hash: str, index: int) -> dict[str, str]:
    return {"tx_hash": tx_hash, "index": hex(index)}


def always_success_dep(genesis_cellbase_hash: str) -> dict[str, Any]:
    return {"out_point": out_point(genesis_cellbase_hash, int(ALWAYS_SUCCESS_INDEX, 16)), "dep_type": "code"}


def always_success_lock(args: str = "0x") -> dict[str, str]:
    return {"code_hash": ALWAYS_SUCCESS_CODE_HASH, "hash_type": "data", "args": args}


def transaction(
    input_cells: list[dict[str, Any]] | dict[str, Any],
    outputs: list[dict[str, Any]],
    outputs_data: list[str],
    cell_deps: list[dict[str, Any]],
    witnesses: list[str],
    header_deps: list[str],
) -> dict[str, Any]:
    if isinstance(input_cells, dict) and "cells" in input_cells:
        input_cells = input_cells["cells"]
    elif isinstance(input_cells, dict):
        input_cells = [input_cells]
    return {
        "version": "0x0",
        "cell_deps": cell_deps,
        "header_deps": header_deps,
        "inputs": [{"previous_output": out_point(cell["tx_hash"], cell["index"]), "since": "0x0"} for cell in input_cells],
        "outputs": outputs,
        "outputs_data": outputs_data,
        "witnesses": witnesses,
    }


def deploy_code_cell(devnet: CkbDevnet, name: str, artifact: bytes, always_dep: dict[str, Any]) -> dict[str, Any]:
    min_capacity = (len(artifact) + 1_000) * SHANNONS
    funding = devnet.collect_spendable(min_capacity)
    tx = transaction(
        funding,
        [{"capacity": hex(funding["total_capacity"]), "lock": always_success_lock(), "type": None}],
        [hex0x(artifact)],
        [always_dep],
        ["0x" for _ in funding["cells"]],
        [],
    )
    commit = devnet.submit_and_commit(tx, f"deploy {name}")
    devnet.assert_live_cell(
        commit["tx_hash"],
        0,
        label=f"deploy {name}",
        expected_capacity=funding["total_capacity"],
        expected_lock=always_success_lock(),
        expected_type=None,
        expected_data=artifact,
    )
    return {
        "name": name,
        "artifact_size_bytes": len(artifact),
        "data_hash": ckb_hash_hex(artifact),
        "cell_dep": {"out_point": out_point(commit["tx_hash"], 0), "dep_type": "code"},
        "commit": commit,
    }


def compile_lifecycle(repo_root: pathlib.Path, output: pathlib.Path) -> None:
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        "proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell",
        "--target-profile",
        "ckb",
        "--target",
        "riscv64-elf",
        "--entry-action",
        "novaseal_lifecycle",
        "-o",
        str(output),
    ]
    subprocess.run(cmd, cwd=repo_root, check=True)


def build_bootstrap_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    initial_cell_data: bytes,
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("bootstrap funding capacity is too small")
    lifecycle_type = {"code_hash": lifecycle_data_hash, "hash_type": "data2", "args": "0x"}
    witness = entry_witness(OP_BOOTSTRAP, initial_cell_data, bytes(254), ZERO_HASH, bytes(96))
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(initial_cell_data), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_transition_tx(
    *,
    old_cell_ref: dict[str, Any],
    old_cell_state: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    funding: dict[str, Any],
    new_state_hash: bytes,
    mutate_signature: bool = False,
) -> tuple[dict[str, Any], dict[str, Any]]:
    old_cell_data = pack_novaseal_cell(
        authority_hash=old_cell_state["authority_hash"],
        state_hash=old_cell_state["state_hash"],
        policy_hash=old_cell_state["policy_hash"],
        latest_receipt_hash=old_cell_state["latest_receipt_hash"],
        nonce=old_cell_state["nonce"],
        expiry=old_cell_state["expiry"],
    )
    material = build_transition_material(old_cell_ref["tx_hash"], old_cell_ref["index"], old_cell_state, new_state_hash)
    sig_payload = bytearray(material["signature_payload"])
    if mutate_signature:
        sig_payload[-1] ^= 1
    witness = entry_witness(
        OP_KEY_AUTH_TRANSITION,
        old_cell_data,
        material["signed_intent"],
        material["state_hash_commitment"],
        bytes(sig_payload),
    )
    change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("transition funding capacity is too small")
    lifecycle_type = {"code_hash": lifecycle_data_hash, "hash_type": "data2", "args": "0x"}
    tx = transaction(
        [old_cell_ref] + funding["cells"],
        [
            {"capacity": hex(old_cell_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )
    new_state = {
        "authority_hash": old_cell_state["authority_hash"],
        "state_hash": material["new_state_hash"],
        "policy_hash": old_cell_state["policy_hash"],
        "latest_receipt_hash": material["materialized_receipt_hash"],
        "nonce": old_cell_state["nonce"] + 1,
        "expiry": old_cell_state["expiry"],
    }
    return tx, {"new_state": new_state, "material": material}


def run_live(args: argparse.Namespace) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "novaseal-lifecycle-type.elf"
    compile_lifecycle(repo_root, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-devnet-stateful-live-v0.1",
        "status": "running",
        "scenario": "core_bootstrap_then_key_auth_transition",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
    }
    try:
        devnet.start()
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier_artifact = verifier_elf.read_bytes()
        lifecycle_artifact = lifecycle_elf.read_bytes()
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_artifact, always_dep)
        lifecycle = deploy_code_cell(devnet, "novaseal_lifecycle_type", lifecycle_artifact, always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/Cell.toml"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/src"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )

        header_hash = devnet.rpc("get_tip_header")["hash"]
        authority_hash = xonly_pubkey(TEST_SECRET_KEY)
        initial_state = {
            "authority_hash": authority_hash,
            "state_hash": ckb_hash(b"novaseal devnet initial state"),
            "policy_hash": ckb_hash(b"novaseal devnet policy"),
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": (1 << 63) - 1,
        }
        initial_cell_data = pack_novaseal_cell(**initial_state)
        bootstrap_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)
        bootstrap_tx = build_bootstrap_tx(bootstrap_funding, lifecycle["data_hash"], cell_deps, header_hash, initial_cell_data)
        bootstrap_dry_run = devnet.rpc("dry_run_transaction", [bootstrap_tx])
        bootstrap_commit = devnet.submit_and_commit(bootstrap_tx, "novaseal bootstrap")
        bootstrap_live = devnet.assert_live_cell(
            bootstrap_commit["tx_hash"],
            0,
            label="bootstrap state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type={"code_hash": lifecycle["data_hash"], "hash_type": "data2", "args": "0x"},
            expected_data=initial_cell_data,
        )

        state_ref = {"tx_hash": bootstrap_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}
        transition_header = devnet.rpc("get_tip_header")["hash"]
        transition_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        transition_tx, transition_material = build_transition_tx(
            old_cell_ref=state_ref,
            old_cell_state=initial_state,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=transition_header,
            funding=transition_funding,
            new_state_hash=ckb_hash(b"novaseal devnet state after transition"),
        )
        transition_dry_run = devnet.rpc("dry_run_transaction", [transition_tx])
        transition_commit = devnet.submit_and_commit(transition_tx, "novaseal key-auth transition")
        bootstrap_dead = devnet.wait_dead_cell(bootstrap_commit["tx_hash"], 0)
        new_state_live = devnet.assert_live_cell(
            transition_commit["tx_hash"],
            0,
            label="transition new state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type={"code_hash": lifecycle["data_hash"], "hash_type": "data2", "args": "0x"},
            expected_data=transition_material["material"]["new_cell_data"],
        )
        receipt_live = devnet.assert_live_cell(
            transition_commit["tx_hash"],
            1,
            label="transition receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=transition_material["material"]["receipt_data"],
        )

        negative_header = devnet.rpc("get_tip_header")["hash"]
        negative_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        negative_ref = {"tx_hash": transition_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}
        negative_tx, _ = build_transition_tx(
            old_cell_ref=negative_ref,
            old_cell_state=transition_material["new_state"],
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header,
            funding=negative_funding,
            new_state_hash=ckb_hash(b"novaseal devnet rejected state"),
            mutate_signature=True,
        )
        wrong_signature_reject = devnet.dry_run_rejects(
            negative_tx,
            "wrong signature transition",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        still_live = devnet.assert_live_cell(
            transition_commit["tx_hash"],
            0,
            label="post-negative state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type={"code_hash": lifecycle["data_hash"], "hash_type": "data2", "args": "0x"},
            expected_data=transition_material["material"]["new_cell_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {
                    "verifier": verifier,
                    "lifecycle": lifecycle,
                },
                "provenance": provenance,
                "bootstrap": {
                    "dry_run_cycles": bootstrap_dry_run.get("cycles"),
                    "commit": bootstrap_commit,
                    "state_cell_live": bootstrap_live.get("status") == "live",
                    "state_data_hash": hex0x(cell_data_hash(initial_cell_data)),
                },
                "transition": {
                    "dry_run_cycles": transition_dry_run.get("cycles"),
                    "commit": transition_commit,
                    "old_state_not_live": bootstrap_dead.get("status") != "live",
                    "new_state_live": new_state_live.get("status") == "live",
                    "receipt_live": receipt_live.get("status") == "live",
                    "signed_intent_hash": hex0x(transition_material["material"]["signed_intent_hash"]),
                    "latest_receipt_hash": hex0x(transition_material["new_state"]["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_signature_dry_run": wrong_signature_reject,
                    "post_negative_state_still_live": still_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update({"status": "failed", "error": str(error), "ckb_log": str(devnet.log_path), "rpc_url": devnet.rpc_url})
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def main() -> int:
    args = parse_args()
    report = run_live(args)
    output = args.output if args.output.is_absolute() else args.repo_root.resolve() / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"wrote {output} status={report['status']} "
        f"live_devnet_rpc_executed={report.get('live_devnet_rpc_executed', False)}"
    )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
