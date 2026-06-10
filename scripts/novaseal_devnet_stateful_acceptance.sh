#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_ONLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pretty)
      shift
      ;;
    --report-only)
      REPORT_ONLY=true
      shift
      ;;
    --repo-root)
      if [[ $# -lt 2 ]]; then
        echo "--repo-root requires a value" >&2
        exit 2
      fi
      ROOT_DIR="$2"
      shift 2
      ;;
    *)
      echo "unsupported argument: $1" >&2
      exit 2
      ;;
  esac
done

REPORT="$ROOT_DIR/target/novaseal-devnet-stateful-acceptance.json"
cert_status=0
if [[ "$REPORT_ONLY" != true ]]; then
  if [[ -z "${CELLC_BIN:-}" ]]; then
    CELLC_BIN="$ROOT_DIR/target/debug/cellc"
    if [[ ! -x "$CELLC_BIN" ]]; then
      cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --bin cellc >/dev/null
    fi
  elif [[ ! -x "$CELLC_BIN" ]]; then
    echo "CELLC_BIN is not executable: $CELLC_BIN" >&2
    exit 2
  fi
  "$CELLC_BIN" certify --plugin novaseal-profile-v0 --repo-root "$ROOT_DIR" --json >/dev/null || cert_status=$?
fi

if [[ ! -f "$REPORT" ]]; then
  echo "missing $REPORT; run target/debug/cellc certify --plugin novaseal-profile-v0 --repo-root $ROOT_DIR --json first" >&2
  exit 1
fi

summary="$(python3 - "$REPORT" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    report = json.load(handle)

def field(name):
    value = report.get(name, "unknown")
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)

print("\t".join([field("status"), field("live_devnet_rpc_executed"), field("blocker_count")]))
PY
)"
IFS=$'\t' read -r status live_devnet_rpc_executed blockers <<< "$summary"
printf 'wrote %s status=%s live_devnet_rpc_executed=%s blockers=%s\n' "$REPORT" "$status" "$live_devnet_rpc_executed" "$blockers"
exit "$cert_status"
