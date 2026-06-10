#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pretty|--report-only)
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

CELLC_BIN="${CELLC_BIN:-$ROOT_DIR/target/debug/cellc}"
if [[ ! -x "$CELLC_BIN" ]]; then
  cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --bin cellc >/dev/null
fi

"$CELLC_BIN" certify --plugin novaseal-profile-v0 --repo-root "$ROOT_DIR" --json >/dev/null

REPORT="$ROOT_DIR/target/novaseal-devnet-stateful-acceptance.json"
status="$(grep -m1 '"status"' "$REPORT" | sed 's/.*: *"//; s/".*//')"
live_devnet_rpc_executed="$(grep -m1 '"live_devnet_rpc_executed"' "$REPORT" | sed 's/.*: *//; s/,.*//')"
blockers="$(grep -m1 '"blocker_count"' "$REPORT" | sed 's/.*: *//; s/,.*//')"
printf 'wrote %s status=%s live_devnet_rpc_executed=%s blockers=%s\n' "$REPORT" "$status" "$live_devnet_rpc_executed" "$blockers"
