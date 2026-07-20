#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIBER_REPO="${FIBER_REPO:-$REPO_ROOT/../fiber}"
FIBER_REVISION="${FIBER_REVISION:-04e091b08953368aa5ee977f562ad628c3000ff4}"
MODE="static"
ACCEPTANCE_REPORT=""
COMPATIBILITY_REPORT=""
REGISTRATION_REPORT=""
TOPOLOGY_REPORT=""

usage() {
  cat <<'USAGE'
Usage: scripts/cellscript_fiber_acceptance.sh [--static] [--full <report options>] [options]

Runs the non-gating CellScript 0.22 Fiber acceptance boundary.

  --static                       Run compiler CKB-VM scenarios plus adapter tests.
  --full                         Also validate a concrete, complete lifecycle matrix.
  --acceptance-report <path>     Generated acceptance.json containing every required row.
  --compatibility-report <path>  Generated compatibility.json bound to the same environment.
  --registration-report <path>   LocalNodeAdvertised registration.json for the same binding.
  --topology-report <path>       TopologyCertified topology.json for the same binding.
  --fiber-repo <path>            Fiber checkout used by the topology (default: ../fiber).
  --fiber-revision <commit>      Exact accepted Fiber commit.
  -h, --help                     Show this help.

The full mode validates externally produced lifecycle evidence. It does not
start, configure, sign for, or stop user-owned Fiber/CKB nodes.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --static)
      MODE="static"
      shift
      ;;
    --full)
      MODE="full"
      shift
      ;;
    --acceptance-report)
      ACCEPTANCE_REPORT="${2:?missing value for --acceptance-report}"
      shift 2
      ;;
    --compatibility-report)
      COMPATIBILITY_REPORT="${2:?missing value for --compatibility-report}"
      shift 2
      ;;
    --registration-report)
      REGISTRATION_REPORT="${2:?missing value for --registration-report}"
      shift 2
      ;;
    --topology-report)
      TOPOLOGY_REPORT="${2:?missing value for --topology-report}"
      shift 2
      ;;
    --fiber-repo)
      FIBER_REPO="${2:?missing value for --fiber-repo}"
      shift 2
      ;;
    --fiber-revision)
      FIBER_REVISION="${2:?missing value for --fiber-revision}"
      shift 2
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

cd "$REPO_ROOT"

cargo test --locked -p cellscript --test fiber_compatibility -- --test-threads=1
cargo test --locked -p cellscript-fiber-adapter -- --test-threads=1
cargo clippy --locked -p cellscript-fiber-adapter --all-targets -- -D warnings

if [[ "$MODE" == "static" ]]; then
  echo "CellScript Fiber static and CKB-VM acceptance passed; no Fiber topology claim was made."
  exit 0
fi

if [[ -z "$ACCEPTANCE_REPORT" || -z "$COMPATIBILITY_REPORT" || -z "$REGISTRATION_REPORT" || -z "$TOPOLOGY_REPORT" ]]; then
  echo "--full requires acceptance, compatibility, registration, and topology reports" >&2
  exit 2
fi
if [[ ! -d "$FIBER_REPO/.git" && ! -f "$FIBER_REPO/.git" ]]; then
  echo "Fiber checkout not found at $FIBER_REPO" >&2
  exit 1
fi

actual_revision="$(git -C "$FIBER_REPO" rev-parse HEAD)"
if [[ "$actual_revision" != "$FIBER_REVISION" ]]; then
  echo "Fiber revision mismatch: expected $FIBER_REVISION, got $actual_revision" >&2
  exit 1
fi

python3 - "$COMPATIBILITY_REPORT" "$ACCEPTANCE_REPORT" "$FIBER_REVISION" <<'PY'
import json
import pathlib
import sys

compatibility_path = pathlib.Path(sys.argv[1])
acceptance_path = pathlib.Path(sys.argv[2])
expected_fiber_revision = sys.argv[3]

compatibility = json.loads(compatibility_path.read_text(encoding="utf-8"))
acceptance = json.loads(acceptance_path.read_text(encoding="utf-8"))

if compatibility.get("binding", {}).get("fiber_revision") != expected_fiber_revision:
    raise SystemExit("compatibility report Fiber revision does not match the pinned checkout")
if compatibility.get("binding_fingerprint") != acceptance.get("binding_fingerprint"):
    raise SystemExit("acceptance report is not bound to compatibility.json")
if compatibility.get("status") not in {"LocalNodeAdvertised", "ChannelReady", "TopologyCertified"}:
    raise SystemExit("full acceptance requires at least LocalNodeAdvertised compatibility evidence")
PY

cargo run --locked -p cellscript-fiber-adapter --bin cellscript-fiber -- accept "$ACCEPTANCE_REPORT" \
  --compatibility-report "$COMPATIBILITY_REPORT" \
  --registration-report "$REGISTRATION_REPORT" \
  --topology-report "$TOPOLOGY_REPORT"
echo "CellScript Fiber full lifecycle evidence passed for $FIBER_REVISION."
