#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for SDK benchmarks" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
CASES_FILE="${3:-$ROOT_DIR/benchmarks/rustc-sdk-cases.tsv}"
BASELINE_FILE="${4:-$ROOT_DIR/benchmarks/rustc-sdk-baseline.tsv}"
TOOLCHAIN="nightly-2025-09-01"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
LINTER="$ROOT_DIR/target/debug/polkadot-linter"

[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$OUTPUT_DIR" = /* ]] || OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

if [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]]; then
  SDK_TARGET_DIR="$POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR"
else
  SDK_TARGET_DIR="$(mktemp -d)"
fi

SUMMARY="$(mktemp)"
cleanup() {
  rm -f "$SUMMARY"
  [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]] || rm -rf "$SDK_TARGET_DIR"
}
trap cleanup EXIT
: > "$SUMMARY"

cargo +"$TOOLCHAIN" build --manifest-path "$ROOT_DIR/Cargo.toml" --features rustc-driver --bin polkadot-linter-driver
cargo +1.93.0 build --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter

run_cli_case() {
  local rule_id="$1" package="$2" package_dir="$3" source_filter="$4"
  local expected_syntax="$5" expected_rustc="$6" case_dir syntax_json rustc_json case_summary

  case_dir="$(mktemp -d "$OUTPUT_DIR/sdk-case.XXXXXX")"
  syntax_json="$case_dir/syntax.json"
  rustc_json="$case_dir/rustc.json"
  case_summary="$case_dir/summary.tsv"

  "$LINTER" --config "$ROOT_DIR/config/default.toml" --format json --rules "$rule_id" \
    --syntax-only "$SDK_DIR/$package_dir" > "$syntax_json"

  # The driver rule selection is passed through Cargo's rustc wrapper
  # environment, which Cargo does not include in its fingerprint. Rebuild only
  # the selected package so each manifest row invokes the intended rule.
  cargo +"$TOOLCHAIN" clean --quiet --manifest-path "$SDK_DIR/$package_dir/Cargo.toml" \
    --package "$package" --target-dir "$SDK_TARGET_DIR"
  "$LINTER" --config "$ROOT_DIR/config/default.toml" --format json --rules "$rule_id" \
    --package "$package" --lib --no-default-features \
    --driver-path "$DRIVER" --toolchain "$TOOLCHAIN" --target-dir "$SDK_TARGET_DIR" \
    --source-filter "$source_filter" "$SDK_DIR/$package_dir" > "$rustc_json"

  [[ -s "$syntax_json" ]] || printf '[]\n' > "$syntax_json"
  [[ -s "$rustc_json" ]] || printf '[]\n' > "$rustc_json"

  local syntax_count rustc_count
  syntax_count="$(jq --arg rule_id "$rule_id" '[.[] | select(.rule_id == $rule_id)] | length' "$syntax_json")"
  jq -r --arg rule_id "$rule_id" --arg source_filter "$source_filter" '
    [.[] | select(.rule_id == $rule_id and (.file | contains($source_filter)))]
    | unique_by([.rule_id, .file, .line, .message])
    | sort_by(.rule_id, .file, .line, .message)
    | .[]
    | [.rule_id, .file, (.line | tostring), .message]
    | @tsv
  ' "$rustc_json" > "$case_summary"
  rustc_count="$(wc -l < "$case_summary" | tr -d '[:space:]')"

  printf '%s syntax findings: %s; rustc findings: %s\n' "$rule_id/$package" "$syntax_count" "$rustc_count"
  test "$syntax_count" -eq "$expected_syntax"
  test "$rustc_count" -eq "$expected_rustc"
  cat "$case_summary" >> "$SUMMARY"
}

run_wrapper_case() {
  local rule_id="$1" package="$2" package_dir="$3" source_filter="$4" expected_rustc="$5"
  local case_dir raw_json case_summary manifest rustc_count

  case_dir="$(mktemp -d "$OUTPUT_DIR/sdk-case.XXXXXX")"
  raw_json="$case_dir/rustc.jsonl"
  case_summary="$case_dir/summary.tsv"
  manifest="$SDK_DIR/$package_dir/Cargo.toml"

  cargo +"$TOOLCHAIN" clean --quiet --manifest-path "$manifest" --package "$package" --target-dir "$SDK_TARGET_DIR"
  RUSTFLAGS='--cap-lints warn' \
    POLKADOT_LINTER_DRIVER_RULES="$rule_id" \
    POLKADOT_LINTER_DRIVER_JSONL="$raw_json" \
    POLKADOT_LINTER_DRIVER_MANIFEST_ROOT="$SDK_DIR/$package_dir" \
    RUSTC_WORKSPACE_WRAPPER="$DRIVER" \
    DYLD_FALLBACK_LIBRARY_PATH="$(rustup run "$TOOLCHAIN" rustc --print sysroot)/lib" \
    CARGO_TARGET_DIR="$SDK_TARGET_DIR" \
    cargo +"$TOOLCHAIN" check --quiet --locked --manifest-path "$manifest" --package "$package" --lib --no-default-features

  jq -r --arg rule_id "$rule_id" --arg source_filter "$source_filter" '
    select(.rule_id == $rule_id and (.file | contains($source_filter)))
    | [.rule_id, .file, (.line | tostring), .message]
    | @tsv
  ' "$raw_json" | sort -u > "$case_summary"
  rustc_count="$(wc -l < "$case_summary" | tr -d '[:space:]')"

  printf '%s syntax findings: n/a; rustc findings: %s\n' "$rule_id/$package" "$rustc_count"
  test "$rustc_count" -eq "$expected_rustc"
  cat "$case_summary" >> "$SUMMARY"
}

while IFS=$'\t' read -r mode rule_id package package_dir source_filter expected_syntax expected_rustc; do
  [[ -z "$mode" || "$mode" == \#* ]] && continue
  case "$mode" in
    cli)
      run_cli_case "$rule_id" "$package" "$package_dir" "$source_filter" "$expected_syntax" "$expected_rustc"
      ;;
    wrapper)
      run_wrapper_case "$rule_id" "$package" "$package_dir" "$source_filter" "$expected_rustc"
      ;;
    *)
      echo "error: unknown SDK benchmark mode: $mode" >&2
      exit 1
      ;;
  esac
done < "$CASES_FILE"

if ! diff -u <(sort "$BASELINE_FILE") <(sort "$SUMMARY"); then
  echo "error: rustc SDK findings differ from the pinned baseline" >&2
  exit 1
fi

echo "rustc SDK benchmark cases match the pinned baseline"
