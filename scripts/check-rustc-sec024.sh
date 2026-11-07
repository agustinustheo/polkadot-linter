#!/usr/bin/env bash
# SEC024 (discarded `Result` whose `Err` holds funds). Lines marked `// BAD SEC024` must be
# reported, lines marked `// OK SEC024` must not.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sec024.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"
mkdir -p "$WORK_DIR/frame-support/src" "$WORK_DIR/other/src" "$WORK_DIR/pallet/src"

cat > "$WORK_DIR/Cargo.toml" <<'TOML'
[workspace]
members = ["frame-support", "other", "pallet"]
resolver = "2"
TOML

for crate in frame-support other; do
  cat > "$WORK_DIR/$crate/Cargo.toml" <<TOML
[package]
name = "$crate"
version = "0.1.0"
edition = "2021"
TOML
done

cat > "$WORK_DIR/frame-support/src/lib.rs" <<'RS'
pub mod traits {
    pub mod fungible {
        pub struct Imbalance<B>(pub B);
        pub type Credit<B> = Imbalance<B>;
        pub type Debt<B> = Imbalance<B>;
        pub trait Balanced<A> {
            fn resolve(who: &A, credit: Credit<u64>) -> Result<(), Credit<u64>>;
            fn settle(who: &A, debt: Debt<u64>) -> Result<Credit<u64>, Debt<u64>>;
            fn deposit(who: &A, value: u64) -> Result<Debt<u64>, DispatchError>;
        }
        pub struct DispatchError;
    }
    pub trait Currency<A> {
        type NegativeImbalance;
        fn resolve_into_existing(
            who: &A,
            value: Self::NegativeImbalance,
        ) -> Result<(), Self::NegativeImbalance>;
    }
    pub trait Defensive<T> {
        fn defensive(self) -> Option<T>;
    }
    impl<T, E> Defensive<T> for Result<T, E> {
        fn defensive(self) -> Option<T> {
            self.ok()
        }
    }
}
RS

cat > "$WORK_DIR/other/src/lib.rs" <<'RS'
pub struct Imbalance;
pub fn give(_who: &u64, credit: Imbalance) -> Result<(), Imbalance> {
    Err(credit)
}
RS

cat > "$WORK_DIR/pallet/Cargo.toml" <<'TOML'
[package]
name = "pallet"
version = "0.1.0"
edition = "2021"

[dependencies]
frame-support = { path = "../frame-support" }
other = { path = "../other" }
TOML

cat > "$WORK_DIR/pallet/src/lib.rs" <<'RS'
use frame_support::traits::{
    fungible::{Balanced, Credit, Debt, DispatchError, Imbalance},
    Currency, Defensive,
};

pub struct Balances;
impl Balanced<u64> for Balances {
    fn resolve(_who: &u64, credit: Credit<u64>) -> Result<(), Credit<u64>> {
        Err(credit)
    }
    fn settle(_who: &u64, debt: Debt<u64>) -> Result<Credit<u64>, Debt<u64>> {
        Err(debt)
    }
    fn deposit(_who: &u64, value: u64) -> Result<Debt<u64>, DispatchError> {
        Ok(Imbalance(value))
    }
}

pub struct NegativeImbalance;
pub struct Legacy;
impl Currency<u64> for Legacy {
    type NegativeImbalance = NegativeImbalance;
    fn resolve_into_existing(_who: &u64, value: NegativeImbalance) -> Result<(), NegativeImbalance> {
        Err(value)
    }
}

pub fn handle(_credit: Credit<u64>) {}
fn c() -> Credit<u64> {
    Imbalance(1)
}

pub fn discarded(who: u64) {
    let _ = Balances::resolve(&who, c()); // BAD SEC024
    Balances::resolve(&who, c()).ok(); // BAD SEC024
    Balances::resolve(&who, c()); // BAD SEC024
    Balances::resolve(&who, c()).is_ok(); // BAD SEC024
    let _unused = Balances::resolve(&who, c()); // BAD SEC024
    let _ = Balances::settle(&who, c()); // BAD SEC024
    let _ = Legacy::resolve_into_existing(&who, NegativeImbalance); // BAD SEC024
    let _ = <Balances as Balanced<u64>>::resolve(&who, c()).map(|()| 1); // BAD SEC024
    let run = || {
        let _ = Balances::resolve(&who, c()); // BAD SEC024
    };
    run();
}

pub fn generic<B: Balanced<u64>, C: Currency<u64>>(who: u64, neg: C::NegativeImbalance) {
    let _ = B::resolve(&who, c()); // BAD SEC024
    let _ = C::resolve_into_existing(&who, neg); // BAD SEC024
}

pub fn handled(who: u64) -> Result<(), Credit<u64>> {
    Balances::resolve(&who, c())?; // OK SEC024
    match Balances::resolve(&who, c()) { // OK SEC024
        Ok(()) => {}
        Err(credit) => handle(credit),
    }
    if let Err(credit) = Balances::resolve(&who, c()) { // OK SEC024
        handle(credit);
    }
    let _ = Balances::resolve(&who, c()).map_err(|credit| drop(credit)); // OK SEC024
    Balances::resolve(&who, c()).unwrap_or_else(|credit| handle(credit)); // OK SEC024
    let _ = Balances::resolve(&who, c()).defensive(); // OK SEC024
    if Balances::resolve(&who, c()).is_err() { // OK SEC024
        return Ok(());
    }
    let result = Balances::resolve(&who, c()); // OK SEC024
    if let Err(credit) = result {
        handle(credit);
    }
    let _kept = Balances::resolve(&who, c()); // OK SEC024
    _kept?;
    let _ = Balances::deposit(&who, 1); // OK SEC024
    let _ = other::give(&who, other::Imbalance); // OK SEC024
    Balances::resolve(&who, c()) // OK SEC024
}

#[cfg(test)]
mod tests {
    #[test]
    fn burns_in_test() {
        let _ = super::Balances::resolve(&1, super::c()); // OK SEC024
    }
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/all-rules.toml" \
  --format json \
  --rules SEC024 \
  --driver-path "$DRIVER" \
  "$WORK_DIR/pallet" > "$OUTPUT" || true  # exits 1 when it reports errors

expected="$(cd "$WORK_DIR/pallet/src" && grep -Hn "// BAD SEC024" *.rs | cut -d: -f1,2 | sort | tr '\n' ' ')"
actual="$(jq -r '[.[] | select(.rule_id == "SEC024" and (.file | contains("pallet/src/"))) | "\(.file | split("/") | last):\(.line)"] | unique | .[]' "$OUTPUT" | sort | tr '\n' ' ')"
echo "SEC024 expected lines: $expected"
echo "SEC024 reported lines: $actual"
[ "$expected" = "$actual" ]
