#!/usr/bin/env bash

# Checks benchmarks/labels/*.tsv ground-truth labels against a syntax-only scan
# of the pinned Polkadot SDK. TP rows must be reported at path:line, FP rows
# must not be. See benchmarks/labels/README.md for the label format.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-labeled-findings.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
LABELS_DIR="${2:-$ROOT_DIR/benchmarks/labels}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$LABELS_DIR" = /* ]] || LABELS_DIR="$ROOT_DIR/$LABELS_DIR"
SDK_DIR="$(cd "$SDK_DIR" && pwd)"

HEADER=$'rule_id\tpath\tline\tlabel\tevidence\tnote'

shopt -s nullglob
label_files=("$LABELS_DIR"/*.tsv)
shopt -u nullglob
if [[ "${#label_files[@]}" -eq 0 ]]; then
  echo "error: no label files found in $LABELS_DIR" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
FINDINGS="$WORK_DIR/findings.tsv"
: >"$FINDINGS"

# Validate the format of every file before scanning.
format_errors=0
for labels in "${label_files[@]}"; do
  name="$(basename "$labels")"
  if [[ "$(head -n 1 "$labels")" != "$HEADER" ]]; then
    echo "error: $name: first line must be the header: ${HEADER//$'\t'/<TAB>}" >&2
    format_errors=$((format_errors + 1))
    continue
  fi
  if ! awk -F'\t' -v name="$name" '
    NR == 1 { next }
    /^[[:space:]]*$/ { printf "error: %s:%d: blank line\n", name, NR; bad++; next }
    NF != 6 { printf "error: %s:%d: expected 6 tab-separated fields, got %d\n", name, NR, NF; bad++; next }
    $1 !~ /^[A-Z]+[0-9]+$/ { printf "error: %s:%d: invalid rule_id %s\n", name, NR, $1; bad++ }
    $2 !~ /^[^\/].*\.rs$/ { printf "error: %s:%d: path must be a relative .rs path: %s\n", name, NR, $2; bad++ }
    $3 !~ /^[1-9][0-9]*$/ { printf "error: %s:%d: invalid line %s\n", name, NR, $3; bad++ }
    $4 != "TP" && $4 != "FP" { printf "error: %s:%d: label must be TP or FP, got %s\n", name, NR, $4; bad++ }
    $5 != "manual" && $5 !~ /^oracle:[^[:space:]]+$/ { printf "error: %s:%d: evidence must be manual or oracle:<name>, got %s\n", name, NR, $5; bad++ }
    $6 ~ /^[[:space:]]*$/ { printf "error: %s:%d: note must justify the label\n", name, NR; bad++ }
    {
      key = $1 FS $2 FS $3
      if (key in seen) { printf "error: %s:%d: duplicate of line %d\n", name, NR, seen[key]; bad++ }
      seen[key] = NR
    }
    END { exit bad > 0 }
  ' "$labels" >&2; then
    format_errors=$((format_errors + 1))
  fi
  while IFS=$'\t' read -r _ path _; do
    if [[ ! -f "$SDK_DIR/$path" ]]; then
      echo "error: $name: $path does not exist in $SDK_DIR" >&2
      format_errors=$((format_errors + 1))
    fi
  done < <(tail -n +2 "$labels")
done
if [[ "$format_errors" -ne 0 ]]; then
  echo "labeled findings are malformed" >&2
  exit 1
fi

# Scan only the roots the labels reference.
scan_root_for() {
  case "$1" in
    substrate/frame/*) echo "substrate/frame" ;;
    cumulus/*) echo "cumulus" ;;
    polkadot/runtime/*) echo "polkadot/runtime" ;;
    templates/*) echo "templates" ;;
    *) return 1 ;;
  esac
}

ROOTS_FILE="$WORK_DIR/roots.txt"
: >"$ROOTS_FILE"
for labels in "${label_files[@]}"; do
  while IFS=$'\t' read -r _ path _; do
    if ! scan_root_for "$path" >>"$ROOTS_FILE"; then
      echo "error: $(basename "$labels"): no scan root covers $path; extend scripts/check-labeled-findings.sh" >&2
      exit 1
    fi
  done < <(tail -n +2 "$labels")
done
scan_roots="$(sort -u "$ROOTS_FILE")"

while IFS= read -r scan_root; do
  results_json="$WORK_DIR/scan.json"
  set +e
  cargo +1.93.0 run --quiet --bin polkadot-linter -- \
    --config "$ROOT_DIR/config/default.toml" \
    --syntax-only \
    --format json \
    "$SDK_DIR/$scan_root" >"$results_json"
  scan_status=$?
  set -e
  if [[ "$scan_status" -ne 0 && "$scan_status" -ne 1 ]]; then
    echo "labeled-findings scan of $scan_root failed with status $scan_status" >&2
    exit "$scan_status"
  fi
  jq -r --arg prefix "$SDK_DIR/" \
    '.[] | [.rule_id, (.file | ltrimstr($prefix)), (.line | tostring)] | @tsv' \
    "$results_json" >>"$FINDINGS"
done <<<"$scan_roots"

failed_files=0
for labels in "${label_files[@]}"; do
  name="$(basename "$labels")"
  if awk -F'\t' -v name="$name" '
    NR == FNR { reported[$1 FS $2 FS $3] = 1; next }
    FNR == 1 { next }
    {
      key = $1 FS $2 FS $3
      if ($4 == "TP") {
        tp++
        if (!(key in reported)) { printf "  missing TP: %s %s:%s (%s)\n", $1, $2, $3, $6; bad++ }
      } else {
        fp++
        if (key in reported) { printf "  reported FP: %s %s:%s (%s)\n", $1, $2, $3, $6; bad++ }
      }
    }
    END {
      printf "%s %s: %d TP, %d FP labels", (bad ? "FAIL" : "PASS"), name, tp, fp
      if (bad) printf ", %d mismatched", bad
      printf "\n"
      exit bad > 0
    }
  ' "$FINDINGS" "$labels"; then
    :
  else
    failed_files=$((failed_files + 1))
  fi
done

if [[ "$failed_files" -ne 0 ]]; then
  echo "labeled findings differ from the linter output in $failed_files file(s)" >&2
  exit 1
fi

echo "all labeled findings match"
