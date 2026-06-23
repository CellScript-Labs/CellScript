#!/usr/bin/env python3
"""Fetch GitHub releases and commits for the website nav rail.

Fetches at build time to avoid runtime API rate limits (unauthenticated
GitHub API allows 60 req/hour; a popular page would exhaust that in
minutes). The data is embedded as static JSON and refreshed when the
website is rebuilt.

Usage:
    python3 website/scripts/fetch-github-data.py

Requires: Python 3.8+, requests (or urllib fallback).
"""
from __future__ import annotations

import json
import sys
import urllib.request
from pathlib import Path

REPO = "a19q3/CellScript"
OUT = Path(__file__).resolve().parents[1] / "src" / "data" / "github-activity.json"


def fetch_json(url: str) -> dict | list:
    req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "User-Agent": "cellscript-website"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read())


def truncate(text: str, max_len: int = 140) -> str:
    text = text.strip()
    if len(text) <= max_len:
        return text
    return text[: max_len - 1].rstrip() + "…"


def main() -> int:
    try:
        releases_raw = fetch_json(f"https://api.github.com/repos/{REPO}/releases?per_page=3")
        commits_raw = fetch_json(f"https://api.github.com/repos/{REPO}/commits?per_page=5")
    except Exception as e:
        print(f"error: failed to fetch GitHub data: {e}", file=sys.stderr)
        return 1

    releases = []
    for r in releases_raw:
        releases.append({
            "tag": r.get("tag_name", ""),
            "name": r.get("name", ""),
            "date": (r.get("published_at") or "")[:10],
            "body": truncate(r.get("body") or "", 140),
            "url": r.get("html_url", ""),
        })

    commits = []
    for c in commits_raw:
        msg = (c.get("commit", {}).get("message") or "").split("\n")[0]
        author = c.get("commit", {}).get("author", {}).get("name", "")
        commits.append({
            "sha": (c.get("sha") or "")[:7],
            "message": truncate(msg, 80),
            "author": author,
            "date": (c.get("commit", {}).get("author", {}).get("date") or "")[:10],
            "url": c.get("html_url", ""),
        })

    data = {"releases": releases, "commits": commits}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(data, indent=2) + "\n")
    print(f"wrote {OUT.relative_to(Path(__file__).resolve().parents[2])} ({OUT.stat().st_size} bytes)")
    print(f"  releases: {len(releases)}, commits: {len(commits)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
