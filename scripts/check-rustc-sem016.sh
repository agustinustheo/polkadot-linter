#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sem016.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FIXTURE_DIR="$WORK_DIR/sem016-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"

mkdir -p "$FIXTURE_DIR/src"

cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "sem016-fixture"
version = "0.1.0"
edition = "2021"
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
pub mod frame_system {
    pub mod offchain {
        pub trait CreateAuthorizedTransaction<LocalCall> {
            type Extension;

            fn create_extension() -> Self::Extension;
        }
    }

    pub struct AuthorizeCall<T>(core::marker::PhantomData<T>);

    impl<T> AuthorizeCall<T> {
        pub fn new() -> Self {
            Self(core::marker::PhantomData)
        }
    }
}

pub struct Runtime;
pub struct LocalCall;

impl frame_system::offchain::CreateAuthorizedTransaction<LocalCall> for Runtime {
    type Extension = ();

    fn create_extension() -> Self::Extension {
        ()
    }
}

pub struct GoodRuntime;

impl frame_system::offchain::CreateAuthorizedTransaction<LocalCall> for GoodRuntime {
    type Extension = frame_system::AuthorizeCall<GoodRuntime>;

    fn create_extension() -> Self::Extension {
        frame_system::AuthorizeCall::<GoodRuntime>::new()
    }
}

pub mod unrelated {
    pub trait CreateAuthorizedTransaction<LocalCall> {
        type Extension;

        fn create_extension() -> Self::Extension;
    }
}

pub struct UnrelatedRuntime;

impl unrelated::CreateAuthorizedTransaction<LocalCall> for UnrelatedRuntime {
    type Extension = ();

    fn create_extension() -> Self::Extension {
        ()
    }
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEM016 \
  --driver-path "$DRIVER" \
  "$FIXTURE_DIR" > "$OUTPUT"

finding_count="$(jq '[.[] | select(.rule_id == "SEM016")] | length' "$OUTPUT")"
bad_line_count="$(jq '[.[] | select(.rule_id == "SEM016" and .line == 25)] | length' "$OUTPUT")"
good_line_count="$(jq '[.[] | select(.rule_id == "SEM016" and .line == 35)] | length' "$OUTPUT")"
unrelated_line_count="$(jq '[.[] | select(.rule_id == "SEM016" and .line == 53)] | length' "$OUTPUT")"

echo "SEM016 missing AuthorizeCall findings: $finding_count"
echo "SEM016 valid/unrelated findings: $((good_line_count + unrelated_line_count))"

test "$finding_count" -eq 1
test "$bad_line_count" -eq 1
test "$good_line_count" -eq 0
test "$unrelated_line_count" -eq 0

echo "rustc SEM016 resolves the FRAME trait and AuthorizeCall constructor before reporting"
