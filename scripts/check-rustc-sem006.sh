#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
FIXTURE_DIR="$WORK_DIR/sem006-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-rustc"

mkdir -p "$FIXTURE_DIR/src"
cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "sem006-fixture"
version = "0.1.0"
edition = "2021"
TOML
cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
pub mod frame_support {
    pub mod weights {
        #[derive(Clone, Copy)]
        pub struct RuntimeDbWeight;
        impl RuntimeDbWeight {
            pub fn reads(self, _count: u64) -> u64 { 0 }
            pub fn writes(self, _count: u64) -> u64 { 0 }
        }
    }
}

pub fn production_path() -> u64 {
    frame_support::weights::RuntimeDbWeight.reads(1)
}
RS
cat > "$FIXTURE_DIR/src/weights.rs" <<'RS'
pub fn generated_weight() -> u64 {
    crate::frame_support::weights::RuntimeDbWeight.writes(1)
}
RS
printf '\npub mod weights;\n' >> "$FIXTURE_DIR/src/lib.rs"

cargo +nightly-2025-06-10 build --manifest-path "$ROOT_DIR/Cargo.toml" --features rustc-driver --bin polkadot-linter-rustc
OUTPUT="$(cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- --config "$ROOT_DIR/config/default.toml" --format json --rules SEM006 --rustc-driver "$DRIVER" "$FIXTURE_DIR")"
printf '%s\n' "$OUTPUT"
test "$(jq '[.[] | select(.rule_id == "SEM006" and .file == "src/lib.rs" and .line == 13)] | length' <<<"$OUTPUT")" -eq 1
test "$(jq '[.[] | select(.rule_id == "SEM006" and .file == "src/weights.rs")] | length' <<<"$OUTPUT")" -eq 0
