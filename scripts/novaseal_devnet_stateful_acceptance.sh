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
certifier_status=not_run
cert_stderr=""
if [[ "$REPORT_ONLY" != true ]]; then
  rm -f "$REPORT"
  cert_stderr="$(mktemp)"
  if [[ -z "${CELLC_BIN:-}" ]]; then
    CELLC_TARGET_DIR="${CELLSCRIPT_CELLC_TARGET_DIR:-$ROOT_DIR/target/cellscript-cellc}"
    cargo build \
      --locked \
      --manifest-path "$ROOT_DIR/Cargo.toml" \
      --bin cellc \
      --target-dir "$CELLC_TARGET_DIR" \
      >/dev/null
    CELLC_BIN="$CELLC_TARGET_DIR/debug/cellc"
  elif [[ ! -x "$CELLC_BIN" ]]; then
    if [[ -n "$cert_stderr" ]]; then
      rm -f "$cert_stderr"
    fi
    echo "CELLC_BIN is not executable: $CELLC_BIN" >&2
    exit 2
  fi
  "$CELLC_BIN" certify --plugin novaseal-profile-v0 --repo-root "$ROOT_DIR" --json >/dev/null 2>"$cert_stderr" || cert_status=$?
  certifier_status="$cert_status"
fi

if [[ ! -f "$REPORT" ]]; then
  if [[ -n "$cert_stderr" && -s "$cert_stderr" ]]; then
    cat "$cert_stderr" >&2
  fi
  if [[ -n "$cert_stderr" ]]; then
    rm -f "$cert_stderr"
  fi
  echo "missing $REPORT; run target/debug/cellc certify --plugin novaseal-profile-v0 --repo-root $ROOT_DIR --json first" >&2
  exit 1
fi

summary="$(cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
  --root "$ROOT_DIR" novaseal-acceptance-summary "$REPORT")"
IFS=$'\t' read -r status live_devnet_rpc_executed local_blockers acceptance_blockers blockers external_endpoint_status <<< "$summary"
printf 'wrote %s status=%s live_devnet_rpc_executed=%s local_blockers=%s acceptance_blockers=%s blockers=%s external_endpoint_status=%s certifier_status=%s\n' \
  "$REPORT" "$status" "$live_devnet_rpc_executed" "$local_blockers" "$acceptance_blockers" "$blockers" "$external_endpoint_status" "$certifier_status"

report_status=1
case "$status" in
  passed)
    if [[ "$blockers" == "0" ]]; then
      report_status=0
    fi
    ;;
  local_devnet_passed_external_endpoint_required)
    if [[ "$live_devnet_rpc_executed" == "true" && "$local_blockers" == "0" && "$acceptance_blockers" == "1" && "$blockers" == "1" && "$external_endpoint_status" == "external_required" ]]; then
      report_status=0
    fi
    ;;
esac

if [[ "$report_status" -eq 0 ]]; then
  if [[ -n "$cert_stderr" ]]; then
    rm -f "$cert_stderr"
  fi
  exit 0
fi
if [[ -n "$cert_stderr" && -s "$cert_stderr" ]]; then
  cat "$cert_stderr" >&2
fi
if [[ -n "$cert_stderr" ]]; then
  rm -f "$cert_stderr"
fi
if [[ "$cert_status" -ne 0 ]]; then
  exit "$cert_status"
fi
exit "$report_status"
