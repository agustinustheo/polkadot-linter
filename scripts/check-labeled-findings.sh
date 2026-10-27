#!/usr/bin/env bash

# Checks benchmarks/labels/*.tsv ground-truth labels against a syntax-only scan
# of the pinned Polkadot SDK. TP rows must be reported at path:line, FP rows
# must not be. See benchmarks/labels/README.md for the label format.
#
#   scripts/check-labeled-findings.sh [SDK_DIR] [LABELS_DIR]
#   scripts/check-labeled-findings.sh --self-test
#
# LABELS_LINTER overrides the linter command (used by --self-test). Rule IDs
# are checked against the rule registry by tests/labels_test.rs.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-labeled-findings.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SELF="$ROOT_DIR/scripts/$(basename "${BASH_SOURCE[0]}")"
HEADER=$'rule_id\tpath\tline\tlabel\tevidence\tnote'

# Runs the checker against a fake SDK and a stub linter, asserting that each
# malformed or mismatched label set fails and each matching one passes.
self_test() {
  local failures=0
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' EXIT
  mkdir -p "$dir/sdk/substrate/frame/x/src"
  printf 'fn a() {}\nfn b() {}\nfn c() {}\n' >"$dir/sdk/substrate/frame/x/src/lib.rs"
  cat >"$dir/linter" <<'EOF'
#!/usr/bin/env bash
cat "$STUB_FINDINGS"
EOF
  chmod +x "$dir/linter"
  local file="$dir/sdk/substrate/frame/x/src/lib.rs"
  local hit="[{\"rule_id\":\"VAL001\",\"file\":\"$file\",\"line\":2}]"
  local other="[{\"rule_id\":\"SEM004\",\"file\":\"$file\",\"line\":1}]"

  # expect <pass|error pattern> <description> <findings json> <label rows...>
  # A failing case must exit non-zero and print the error pattern.
  expect() {
    local want="$1" description="$2" findings="$3" status=0
    shift 3
    rm -rf "$dir/labels" && mkdir "$dir/labels"
    printf '%s\n' "$findings" >"$dir/findings.json"
    printf '%s\n' "$HEADER" >"$dir/labels/A.tsv"
    local row
    for row in "$@"; do
      case "$row" in
        B:*) printf '%s\n' "$HEADER" "${row#B:}" >"$dir/labels/B.tsv" ;;
        *) printf '%s\n' "$row" >>"$dir/labels/A.tsv" ;;
      esac
    done
    STUB_FINDINGS="$dir/findings.json" LABELS_LINTER="$dir/linter" \
      "$SELF" "$dir/sdk" "$dir/labels" >"$dir/out" 2>&1 || status=$?
    if [[ "$want" == pass && "$status" -ne 0 ]] ||
      [[ "$want" != pass && ("$status" -eq 0 || -z "$(grep -E -- "$want" "$dir/out")") ]]; then
      echo "self-test FAILED: $description (expected $want, exit $status)" >&2
      sed 's/^/    /' "$dir/out" >&2
      failures=$((failures + 1))
    else
      echo "self-test ok: $description"
    fi
  }

  local p=substrate/frame/x/src/lib.rs t=$'\t'
  expect pass "matching TP and FP" "$hit" \
    "VAL001${t}$p${t}2${t}TP${t}manual${t}n" "VAL001${t}$p${t}3${t}FP${t}manual${t}n"
  expect pass "scope: evidence is a must-not-report label" "$hit" \
    "VAL001${t}$p${t}3${t}FP${t}scope:dev-only${t}n"
  expect "produced no findings" "empty scan with a TP label" "[]" "VAL001${t}$p${t}2${t}TP${t}manual${t}n"
  expect "produced no findings" "scan without findings with only FP labels" "[]" "VAL001${t}$p${t}2${t}FP${t}manual${t}n"
  expect "missing TP" "missing TP" "$other" "VAL001${t}$p${t}2${t}TP${t}manual${t}n"
  expect "reported FP" "reported FP" "$hit" "VAL001${t}$p${t}2${t}FP${t}manual${t}n"
  expect "no label rows" "label file without rows" "$hit"
  expect "canonical relative" "non-canonical path" "$hit" "VAL001${t}substrate/frame/./x/src/lib.rs${t}2${t}FP${t}manual${t}n"
  expect "canonical relative" "parent path segment" "$hit" "VAL001${t}substrate/frame/x/../x/src/lib.rs${t}2${t}FP${t}manual${t}n"
  expect "past the end" "line beyond end of file" "$hit" "VAL001${t}$p${t}4${t}FP${t}manual${t}n"
  expect "needs label FP" "scope: evidence on a TP" "$hit" "VAL001${t}$p${t}2${t}TP${t}scope:x${t}n"
  expect "duplicate of A.tsv" "duplicate across files" "$hit" \
    "VAL001${t}$p${t}2${t}TP${t}manual${t}n" "B:VAL001${t}$p${t}2${t}TP${t}manual${t}n"

  if [[ "$failures" -ne 0 ]]; then
    echo "check-labeled-findings self-test: $failures case(s) failed" >&2
    exit 1
  fi
  echo "check-labeled-findings self-test passed"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit 0
fi

SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
LABELS_DIR="${2:-$ROOT_DIR/benchmarks/labels}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$LABELS_DIR" = /* ]] || LABELS_DIR="$ROOT_DIR/$LABELS_DIR"
SDK_DIR="$(cd "$SDK_DIR" && pwd)"

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

# Validate every file before scanning. Rows are keyed across all files so a
# label duplicated in two files is reported too.
SEEN="$WORK_DIR/seen.tsv"
: >"$SEEN"
format_errors=0
for labels in "${label_files[@]}"; do
  name="$(basename "$labels")"
  if [[ "$(head -n 1 "$labels")" != "$HEADER" ]]; then
    echo "error: $name: first line must be the header: ${HEADER//$'\t'/<TAB>}" >&2
    format_errors=$((format_errors + 1))
    continue
  fi
  if ! awk -F'\t' -v name="$name" -v seen_file="$SEEN" '
    BEGIN {
      while ((getline line < seen_file) > 0) { split(line, s, "\t"); seen[s[1] FS s[2] FS s[3]] = s[4] }
    }
    NR == 1 { next }
    /^[[:space:]]*$/ { printf "error: %s:%d: blank line\n", name, NR; bad++; next }
    NF != 6 { printf "error: %s:%d: expected 6 tab-separated fields, got %d\n", name, NR, NF; bad++; next }
    {
      rows++
      if ($2 !~ /^[A-Za-z0-9_-][A-Za-z0-9_.-]*(\/[A-Za-z0-9_.-]+)*\.rs$/ || $2 ~ /(^|\/)\.\.?\//) {
        printf "error: %s:%d: path must be a canonical relative .rs path: %s\n", name, NR, $2; bad++
      }
      if ($3 !~ /^[1-9][0-9]*$/) { printf "error: %s:%d: invalid line %s\n", name, NR, $3; bad++ }
      if ($4 != "TP" && $4 != "FP") { printf "error: %s:%d: label must be TP or FP, got %s\n", name, NR, $4; bad++ }
      if ($5 != "manual" && $5 !~ /^(oracle|scope):[^[:space:]]+$/) {
        printf "error: %s:%d: evidence must be manual, oracle:<name> or scope:<reason>, got %s\n", name, NR, $5; bad++
      }
      if ($5 ~ /^scope:/ && $4 != "FP") { printf "error: %s:%d: scope: evidence means must-not-report and needs label FP\n", name, NR; bad++ }
      if ($6 ~ /^[[:space:]]*$/) { printf "error: %s:%d: note must justify the label\n", name, NR; bad++ }
      key = $1 FS $2 FS $3
      if (key in seen) { printf "error: %s:%d: duplicate of %s\n", name, NR, seen[key]; bad++ }
      seen[key] = name ":" NR
      printf "%s\t%s\t%s\t%s:%d\n", $1, $2, $3, name, NR >> seen_file
    }
    END {
      if (rows == 0) { printf "error: %s: no label rows\n", name; bad++ }
      exit bad > 0
    }
  ' "$labels" >&2; then
    format_errors=$((format_errors + 1))
  fi
  while IFS=$'\t' read -r _ path line _; do
    if [[ ! -f "$SDK_DIR/$path" ]]; then
      echo "error: $name: $path does not exist in $SDK_DIR" >&2
      format_errors=$((format_errors + 1))
    elif [[ "$line" =~ ^[0-9]+$ ]] && (( line > $(awk 'END { print NR }' "$SDK_DIR/$path") )); then
      echo "error: $name: $path:$line is past the end of the file" >&2
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
    substrate/bin/node/runtime/*) echo "substrate/bin/node/runtime" ;;
    cumulus/*) echo "cumulus" ;;
    polkadot/runtime/*) echo "polkadot/runtime" ;;
    polkadot/xcm/*) echo "polkadot/xcm" ;;
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

if [[ -n "${LABELS_LINTER:-}" ]]; then
  linter=("$LABELS_LINTER")
else
  linter=(cargo +1.93.0 run --quiet --bin polkadot-linter --)
fi

while IFS= read -r scan_root; do
  results_json="$WORK_DIR/scan.json"
  set +e
  "${linter[@]}" \
    --config "$ROOT_DIR/config/all-rules.toml" \
    --syntax-only \
    --format json \
    "$SDK_DIR/$scan_root" >"$results_json"
  scan_status=$?
  set -e
  if [[ "$scan_status" -ne 0 && "$scan_status" -ne 1 ]]; then
    echo "labeled-findings scan of $scan_root failed with status $scan_status" >&2
    exit "$scan_status"
  fi
  # Every scanned root has findings; an empty scan means the linter broke,
  # and would otherwise make every FP label pass.
  if [[ "$(jq 'length' "$results_json")" -eq 0 ]]; then
    echo "error: the scan of $scan_root produced no findings" >&2
    exit 1
  fi
  jq -r --arg prefix "$SDK_DIR/" \
    '.[] | [.rule_id, (.file | ltrimstr($prefix)), (.line | tostring)] | @tsv' \
    "$results_json" >>"$FINDINGS"
done <<<"$scan_roots"

failed_files=0
for labels in "${label_files[@]}"; do
  name="$(basename "$labels")"
  if awk -F'\t' -v name="$name" -v findings="$FINDINGS" '
    BEGIN {
      while ((getline line < findings) > 0) {
        split(line, f, "\t")
        reported[f[1] FS f[2] FS f[3]] = 1
      }
    }
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
      if (tp + fp == 0) { bad++ }
      printf "%s %s: %d TP, %d FP labels", (bad ? "FAIL" : "PASS"), name, tp, fp
      if (bad) printf ", %d mismatched", bad
      printf "\n"
      exit bad > 0
    }
  ' "$labels"; then
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
