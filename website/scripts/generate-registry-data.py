#!/usr/bin/env python3
"""Generate website registry data from real CellScript package metadata."""

from __future__ import annotations

import json
import subprocess
import tomllib
from pathlib import Path
from typing import Any


WEBSITE_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = WEBSITE_ROOT.parent
OUT = WEBSITE_ROOT / "src" / "data" / "registry-packages.json"

SKIP_DIRS = {
    ".git",
    ".astro",
    ".cell",
    "dist",
    "node_modules",
    "target",
}


def is_skipped(path: Path) -> bool:
    parts = set(path.relative_to(REPO_ROOT).parts)
    return bool(parts & SKIP_DIRS)


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def read_toml(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    return tomllib.loads(path.read_text(encoding="utf-8"))


def git_revision(path: Path) -> str | None:
    try:
        result = subprocess.run(
            ["git", "log", "-1", "--format=%H", "--", str(path.relative_to(REPO_ROOT))],
            cwd=REPO_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip() or None


def latest_version(versions: list[dict[str, Any]]) -> dict[str, Any] | None:
    active = [version for version in versions if not version.get("yanked")]
    candidates = active or versions
    if not candidates:
        return None
    return sorted(candidates, key=lambda item: str(item.get("released_at") or item.get("version") or ""))[-1]


def deployment_summary(deployed: dict[str, Any]) -> dict[str, Any]:
    deployments = deployed.get("deployments")
    if not isinstance(deployments, list):
        deployments = []
    active = [item for item in deployments if item.get("status") == "active"]
    networks = sorted({str(item.get("network")) for item in deployments if item.get("network")})
    return {
        "count": len(deployments),
        "active_count": len(active),
        "networks": networks,
        "active": active,
    }


def package_record(registry_path: Path) -> dict[str, Any] | None:
    registry = read_json(registry_path)
    versions = registry.get("versions")
    if not isinstance(versions, list):
        return None

    package_dir = registry_path.parent
    manifest = read_toml(package_dir / "Cell.toml")
    deployed = read_toml(package_dir / "Deployed.toml")
    package_manifest = manifest.get("package", {}) if isinstance(manifest.get("package"), dict) else {}
    policy = manifest.get("policy", {}) if isinstance(manifest.get("policy"), dict) else {}
    metadata = manifest.get("metadata", {}) if isinstance(manifest.get("metadata"), dict) else {}
    latest = latest_version(versions)

    namespace = str(registry.get("namespace") or package_manifest.get("namespace") or "")
    name = str(registry.get("name") or package_manifest.get("name") or "")
    if not namespace or not name:
        return None

    deployments = deployment_summary(deployed)
    latest_version_value = str((latest or {}).get("version") or package_manifest.get("version") or "")
    status = "active" if deployments["active_count"] else str(metadata.get("status") or "source-only")

    rel_path = registry_path.relative_to(REPO_ROOT).as_posix()
    package_rel_path = package_dir.relative_to(REPO_ROOT).as_posix()

    return {
        "coordinate": f"{namespace}/{name}",
        "namespace": namespace,
        "name": name,
        "path": package_rel_path,
        "registry_path": rel_path,
        "source_revision": git_revision(registry_path),
        "description": package_manifest.get("description") or "",
        "license": (latest or {}).get("license") or package_manifest.get("license") or "",
        "repository": package_manifest.get("repository") or "",
        "homepage": package_manifest.get("homepage") or "",
        "documentation": package_manifest.get("documentation") or "",
        "keywords": package_manifest.get("keywords") or [],
        "categories": package_manifest.get("categories") or [],
        "production": bool(policy.get("production", False)),
        "policy": policy,
        "metadata": metadata,
        "latest_version": latest_version_value,
        "latest": latest,
        "versions": versions,
        "deployment": deployments,
        "status": status,
        "install_command": f"cellc install {namespace}/{name}@{latest_version_value}" if latest_version_value else f"cellc install {namespace}/{name}",
        "package_command_prefix": f"cd {package_rel_path}",
        "verify_command": f"cd {package_rel_path} && cellc registry verify --live --json",
        "publish_command": f"cd {package_rel_path} && cellc publish",
        "publish_dry_run_command": f"cd {package_rel_path} && cellc publish --dry-run",
        "edit_command": f"cd {package_rel_path} && cellc registry edit",
    }


def main() -> None:
    records: list[dict[str, Any]] = []
    for registry_path in sorted(REPO_ROOT.rglob("registry.json")):
        if is_skipped(registry_path):
            continue
        record = package_record(registry_path)
        if record:
            records.append(record)

    records.sort(key=lambda item: item["coordinate"])
    payload = {
        "schema_version": 1,
        "source": "repo registry.json + Cell.toml + Deployed.toml scan",
        "packages": records,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"generated {OUT.relative_to(REPO_ROOT)} with {len(records)} package(s)")


if __name__ == "__main__":
    main()
