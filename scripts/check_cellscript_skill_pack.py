#!/usr/bin/env python3
"""Validate the CellScript programming skill pack freshness contract."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


EXPECTED_SKILLS = {
    "cellscript-language-basics",
    "cellscript-ckb-model",
    "cellscript-package-cli",
    "cellscript-metadata-audit",
    "cellscript-builder-deployment",
    "cellscript-diagnostics",
}


def parse_front_matter(path: Path) -> dict[str, list[str] | str]:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        raise ValueError(f"{path} is missing YAML-style front matter")
    try:
        header = text.split("---\n", 2)[1]
    except IndexError as error:
        raise ValueError(f"{path} has unterminated front matter") from error
    result: dict[str, list[str] | str] = {}
    current_list: str | None = None
    for raw_line in header.splitlines():
        line = raw_line.rstrip()
        if not line:
            continue
        if line.startswith("  - "):
            if current_list is None:
                raise ValueError(f"{path} has a list item outside a list: {line}")
            value = line[4:].strip()
            result.setdefault(current_list, [])
            assert isinstance(result[current_list], list)
            result[current_list].append(value)
            continue
        current_list = None
        if ":" not in line:
            raise ValueError(f"{path} has malformed front matter line: {line}")
        key, value = line.split(":", 1)
        key = key.strip()
        value = value.strip()
        if value:
            result[key] = value
        else:
            result[key] = []
            current_list = key
    return result


def visible_command_names(repo_root: Path) -> set[str]:
    source = (repo_root / "src/cli/commands.rs").read_text(encoding="utf-8")
    names = set(re.findall(r'ClapCommand::new\("([^"]+)"\)', source))
    names.update({"cellc"})
    return names


def validate_skill(repo_root: Path, path: Path, command_names: set[str]) -> list[str]:
    failures: list[str] = []
    front_matter = parse_front_matter(path)
    name = str(front_matter.get("name", "")).strip()
    if not name:
        failures.append(f"{path}: missing name")
    references = front_matter.get("references")
    if not isinstance(references, list) or not references:
        failures.append(f"{path}: missing references list")
        references = []
    commands = front_matter.get("commands")
    if not isinstance(commands, list) or not commands:
        failures.append(f"{path}: missing commands list")
        commands = []

    has_current_doc_or_example = False
    for reference in references:
        ref_path = reference.split("#", 1)[0]
        if ref_path.startswith("../") or "/../" in ref_path:
            failures.append(f"{path}: reference escapes repo root: {reference}")
            continue
        full = repo_root / ref_path
        if not full.exists():
            failures.append(f"{path}: referenced file does not exist: {reference}")
            continue
        if ref_path.startswith(("docs/wiki/", "docs/CELLSCRIPT_", "examples/")):
            has_current_doc_or_example = True
    if not has_current_doc_or_example:
        failures.append(f"{path}: references must include current docs/wiki, docs/CELLSCRIPT_*, or examples files")

    for command in commands:
        parts = command.split()
        if not parts or parts[0] != "cellc":
            failures.append(f"{path}: command must start with 'cellc': {command}")
            continue
        for part in parts[1:]:
            if part.startswith("-") or part.startswith("<"):
                continue
            if part not in command_names:
                failures.append(f"{path}: command token is not present in CLI registry: {command} ({part})")
    return failures


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    skill_files = sorted((repo_root / "docs/skills").glob("cellscript-*/SKILL.md"))
    found = {path.parent.name for path in skill_files}
    failures: list[str] = []
    missing = sorted(EXPECTED_SKILLS - found)
    extra = sorted(found - EXPECTED_SKILLS)
    if missing:
        failures.append(f"missing skill directories: {', '.join(missing)}")
    if extra:
        failures.append(f"unexpected CellScript skill directories: {', '.join(extra)}")
    command_names = visible_command_names(repo_root)
    for path in skill_files:
        failures.extend(validate_skill(repo_root, path, command_names))

    report = {
        "schema": "cellscript-skill-pack-freshness-v0.21",
        "status": "failed" if failures else "passed",
        "skills": sorted(found),
        "skill_count": len(skill_files),
        "failures": failures,
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    if failures:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
