#!/usr/bin/env python3
"""Local package/registry pressure gate for DOB-EVO/1.

The script is intentionally offline. It checks the package artefacts that can be
verified without pretending a live CKB deployment exists.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
REPO_ROOT = ROOT.parents[2]


def cellc() -> list[str]:
    configured = os.environ.get("CELLC")
    if configured:
        return [configured]
    binary = REPO_ROOT / "target" / "debug" / "cellc"
    if binary.exists():
        return [str(binary)]
    return ["cargo", "run", "--locked", "-p", "cellscript", "--manifest-path", str(REPO_ROOT / "Cargo.toml"), "--"]


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)


def require(condition: bool, message: str) -> None:
    if not condition:
        print(f"registry-pressure: {message}", file=sys.stderr)
        sys.exit(1)


def locked_source_hash(lock_text: str) -> str | None:
    for line in lock_text.splitlines():
        if line.startswith("source_hash = "):
            return line.split("\"", 2)[1]
    return None


def main() -> int:
    manifest = ROOT / "Cell.toml"
    require(manifest.exists(), "Cell.toml is missing")
    manifest_text = manifest.read_text()
    require("namespace = \"dob\"" in manifest_text, "package namespace must remain dob")
    require("production = true" in manifest_text, "compiler production policy must remain enabled")
    require("deny_fail_closed = true" in manifest_text, "fail-closed denial must remain enabled")
    require("deny_runtime_obligations = true" in manifest_text, "runtime-obligation denial must remain enabled")
    require("legacy_version_support = false" in manifest_text, "legacy support must remain disabled")

    build = run(*cellc(), "build", "--release", "--target", "riscv64-elf", "--target-profile", "ckb", "--primitive-strict", "0.16")
    require(build.returncode == 0, f"cellc build failed\nSTDOUT:\n{build.stdout}\nSTDERR:\n{build.stderr}")

    production_check = run(*cellc(), "check", "--target-profile", "ckb", "--primitive-strict", "0.16", "--json")
    require(production_check.returncode == 0, f"production check failed\nSTDOUT:\n{production_check.stdout}\nSTDERR:\n{production_check.stderr}")
    check_json = json.loads(production_check.stdout)
    require(check_json.get("status") == "ok", f"production check returned {check_json.get('status')}")
    require(check_json.get("policy", {}).get("production") is True, "production check did not use production policy")
    for target in check_json.get("checked_targets", []):
        require(not target.get("fail_closed_runtime_features"), "production check exposed fail-closed runtime features")
        require(target.get("runtime_required_verifier_obligations") == 0, "production check exposed runtime-required verifier obligations")
        require(
            target.get("runtime_required_transaction_runtime_input_requirements") == 0,
            "production check exposed runtime-required transaction runtime inputs",
        )

    package_verify = run(*cellc(), "package", "verify", "--json")
    require(package_verify.returncode == 0, f"package verify failed\nSTDOUT:\n{package_verify.stdout}\nSTDERR:\n{package_verify.stderr}")
    package_json = json.loads(package_verify.stdout)
    require(package_json.get("status") == "ok", f"package verify returned {package_json.get('status')}")

    publish = run(*cellc(), "publish", "--dry-run")
    require(publish.returncode == 0, f"publish dry-run failed\nSTDOUT:\n{publish.stdout}\nSTDERR:\n{publish.stderr}")

    lock = ROOT / "Cell.lock"
    require(lock.exists(), "Cell.lock was not written by build")
    lock_text = lock.read_text()
    require("name = \"evolving-dob-profile-v1\"" in lock_text, "Cell.lock package name mismatch")
    require("namespace = \"dob\"" in lock_text, "Cell.lock namespace mismatch")
    require("[package_build]" in lock_text, "Cell.lock has no package_build identity")

    registry = ROOT / "registry.json"
    if registry.exists():
        data = json.loads(registry.read_text())
        require(data.get("name") == "evolving-dob-profile-v1", "registry.json package name mismatch")
        require(data.get("namespace") == "dob", "registry.json namespace mismatch")
        version = next((v for v in data.get("versions", []) if v.get("version") == "1.0.0"), None)
        require(version is not None, "registry.json missing v1.0.0")
        require(version.get("source_hash") == locked_source_hash(lock_text), "registry.json source hash does not match Cell.lock")

    deployed = ROOT / "Deployed.toml"
    if deployed.exists():
        registry_verify = run(*cellc(), "registry", "verify", "--json", "--require-publisher-signature", "--require-audit-report")
        require(
            registry_verify.returncode == 0,
            f"registry verify failed with Deployed.toml present\nSTDOUT:\n{registry_verify.stdout}\nSTDERR:\n{registry_verify.stderr}",
        )

    print("registry-pressure: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
