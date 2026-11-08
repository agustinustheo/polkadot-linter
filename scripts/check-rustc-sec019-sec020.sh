#!/usr/bin/env bash
# SEC019 (imbalance escapes a transactional closure) and SEC020 (discarded
# verification verdict). Lines marked `// BAD` must be reported, lines marked
# `// OK` must not.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sec019-sec020.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"
mkdir -p "$WORK_DIR/frame-support/src" "$WORK_DIR/xcm-executor/src" "$WORK_DIR/pallet/src"

cat > "$WORK_DIR/Cargo.toml" <<'TOML'
[workspace]
members = ["frame-support", "xcm-executor", "pallet"]
resolver = "2"
TOML

cat > "$WORK_DIR/frame-support/Cargo.toml" <<'TOML'
[package]
name = "frame-support"
version = "0.1.0"
edition = "2021"
TOML

cat > "$WORK_DIR/frame-support/src/lib.rs" <<'RS'
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $err:expr) => {
        if !$cond {
            return Err($err);
        }
    };
}
pub enum TransactionOutcome<R> {
    Commit(R),
    Rollback(R),
}
pub mod storage {
    use super::TransactionOutcome;
    pub fn with_storage_layer<R, E>(f: impl FnOnce() -> Result<R, E>) -> Result<R, E> {
        f()
    }
    pub fn with_transaction<R, E>(f: impl FnOnce() -> TransactionOutcome<Result<R, E>>) -> Result<R, E> {
        match f() {
            TransactionOutcome::Commit(r) | TransactionOutcome::Rollback(r) => r,
        }
    }
}
pub mod traits {
    pub mod tokens {
        pub mod fungible {
            pub struct Imbalance<B>(pub B);
            impl<B> Drop for Imbalance<B> {
                fn drop(&mut self) {}
            }
            pub type Credit<B> = Imbalance<B>;
        }
    }
}
RS

cat > "$WORK_DIR/xcm-executor/Cargo.toml" <<'TOML'
[package]
name = "xcm-executor"
version = "0.1.0"
edition = "2021"
TOML

cat > "$WORK_DIR/xcm-executor/src/lib.rs" <<'RS'
#[derive(Default)]
pub struct AssetsInHolding(Vec<u8>);
impl AssetsInHolding {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn subsume_assets(&mut self, other: AssetsInHolding) {
        self.0.extend(other.0);
    }
}
pub trait ProcessTransaction {
    fn process<F: FnOnce() -> Result<(), ()>>(f: F) -> Result<(), ()>;
}
pub struct FrameTransactionalProcessor;
impl ProcessTransaction for () {
    fn process<F: FnOnce() -> Result<(), ()>>(f: F) -> Result<(), ()> {
        f()
    }
}
impl ProcessTransaction for FrameTransactionalProcessor {
    fn process<F: FnOnce() -> Result<(), ()>>(f: F) -> Result<(), ()> {
        f()
    }
}
pub struct Pipeline;
impl Pipeline {
    pub fn process<F: FnOnce() -> Result<(), ()>>(f: F) -> Result<(), ()> {
        f()
    }
}
RS

cat > "$WORK_DIR/pallet/Cargo.toml" <<'TOML'
[package]
name = "pallet"
version = "0.1.0"
edition = "2021"

[dependencies]
frame-support = { path = "../frame-support" }
xcm-executor = { path = "../xcm-executor" }
TOML

cat > "$WORK_DIR/pallet/src/lib.rs" <<'RS'
use frame_support::{
    storage::{with_storage_layer, with_transaction},
    traits::tokens::fungible::{Credit, Imbalance},
    TransactionOutcome::{Commit, Rollback},
};
use xcm_executor::{AssetsInHolding, ProcessTransaction};

fn withdraw() -> Result<AssetsInHolding, ()> {
    Ok(AssetsInHolding::new())
}

pub fn withdraw_asset<P: ProcessTransaction>() -> Result<(), ()> {
    let mut withdrawn = AssetsInHolding::new(); // BAD SEC019
    let mut total_surplus = 0u64; // OK SEC019
    P::process(|| {
        for _ in 0..2 {
            let credit = withdraw()?;
            withdrawn.subsume_assets(credit);
            total_surplus += 1;
        }
        Ok(())
    })?;
    drop(withdrawn);
    Ok(())
}

pub fn withdraw_credits() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // BAD SEC019
    with_storage_layer(|| {
        credits.push(Imbalance(1));
        Err::<(), ()>(())
    })
}

fn withdraw_credit() -> Result<Credit<u64>, ()> {
    Ok(Imbalance(1))
}

pub fn exit_before_move() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        credits.push(withdraw_credit()?);
        Ok(())
    })
}

pub fn ensure_before_move(ok: bool) -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        frame_support::ensure!(ok, ());
        credits.push(Imbalance(1));
        Ok(())
    })
}

pub fn uncalled_nested_closure() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        let _f = || {
            credits.push(Imbalance(1));
        };
        Err::<(), ()>(())
    })
}

pub fn loop_breaks_after_move() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        loop {
            credits.push(withdraw_credit()?);
            break;
        }
        Ok(())
    })
}

pub fn labeled_break_after_move() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        'outer: loop {
            loop {
                credits.push(withdraw_credit()?);
                break 'outer
            }
        }
        Ok(())
    })
}

pub fn inner_break_outer_loop_iterates() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // BAD SEC019
    with_storage_layer(|| {
        for _ in 0..2 {
            loop {
                credits.push(withdraw_credit()?);
                break;
            }
        }
        Ok(())
    })
}

pub fn break_in_enclosing_block() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        loop {
            {
                credits.push(withdraw_credit()?);
            }
            break;
        }
        Ok(())
    })
}

pub fn return_in_enclosing_block() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        loop {
            {
                credits.push(withdraw_credit()?);
            }
            return Ok(());
        }
    })
}

pub fn labeled_break_in_enclosing_block() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        'outer: loop {
            loop {
                {
                    credits.push(withdraw_credit()?);
                }
                break;
            }
            break 'outer;
        }
        Ok(())
    })
}

pub fn move_in_if_then_break(more: bool) -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        loop {
            if more {
                credits.push(withdraw_credit()?);
            }
            break;
        }
        Ok(())
    })
}

pub fn inner_loop_iterates_before_outer_break(more: bool) -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // BAD SEC019
    with_storage_layer(|| {
        loop {
            loop {
                credits.push(withdraw_credit()?);
                if !more {
                    break;
                }
            }
            break;
        }
        Ok(())
    })
}

pub fn discarded_err() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        credits.push(Imbalance(1));
        let _ = Err::<(), ()>(());
        Ok::<(), ()>(())
    })
}

pub fn exit_after_move(fail: bool) -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // BAD SEC019
    with_storage_layer(|| {
        credits.push(Imbalance(1));
        if fail {
            return Err(());
        }
        Ok(())
    })
}

pub fn borrowed_credits(credit: &Credit<u64>) -> Result<(), ()> {
    let mut credits: Vec<&Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        credits.extend(std::iter::once(credit));
        Err::<(), ()>(())
    })
}

pub fn clear_credits() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        credits.clear();
        Err::<(), ()>(())
    })
}

pub fn never_rolls_back() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_storage_layer(|| {
        credits.push(Imbalance(1));
        let _ = [1].iter().map(|_| -> Result<(), ()> { Err(())? }).count();
        Ok::<(), ()>(())
    })
}

pub fn transaction_commit_only() -> Result<(), ()> {
    let mut credits: Vec<Credit<u64>> = Vec::new(); // OK SEC019
    with_transaction(|| {
        credits.push(Imbalance(1));
        Commit(Err::<(), ()>(()))
    })
}

pub fn transaction_rollback(fail: bool) -> Result<(), ()> {
    let mut credit = None; // BAD SEC019
    with_transaction(|| {
        credit = Some(Imbalance(1u64));
        if fail {
            return Rollback(Err(()));
        }
        Commit(Ok(()))
    })
}

pub fn unit_processor() -> Result<(), ()> {
    let mut withdrawn = AssetsInHolding::new(); // OK SEC019
    <() as ProcessTransaction>::process(|| {
        for _ in 0..2 {
            withdrawn.subsume_assets(withdraw()?);
        }
        Ok(())
    })
}

pub fn fixed_withdraw_asset<P: ProcessTransaction>() -> Result<(), ()> {
    let outer = AssetsInHolding::new(); // OK SEC019
    P::process(|| {
        let mut withdrawn = AssetsInHolding::new(); // OK SEC019
        withdrawn.subsume_assets(withdraw()?);
        let _read = &outer;
        Ok(())
    })
}

pub fn concrete_processor() -> Result<(), ()> {
    let mut withdrawn = AssetsInHolding::new(); // BAD SEC019
    xcm_executor::FrameTransactionalProcessor::process(|| {
        for _ in 0..2 {
            withdrawn.subsume_assets(withdraw()?);
        }
        Ok(())
    })
}

pub fn concrete_processor_always_ok() -> Result<(), ()> {
    let mut withdrawn = AssetsInHolding::new(); // OK SEC019
    xcm_executor::FrameTransactionalProcessor::process(|| {
        withdrawn.subsume_assets(AssetsInHolding::new());
        Ok(())
    })
}

pub fn non_transactional_process() -> Result<(), ()> {
    let mut withdrawn = AssetsInHolding::new(); // OK SEC019
    xcm_executor::Pipeline::process(|| {
        for _ in 0..2 {
            withdrawn.subsume_assets(withdraw()?);
        }
        Ok(())
    })
}

pub fn unrelated_closure() {
    let mut holding = AssetsInHolding::new(); // OK SEC019
    let mut f = || holding.subsume_assets(AssetsInHolding::new());
    f();
}

pub struct Proof;
impl Proof {
    pub fn verify_ancestor(&self) -> Result<bool, ()> {
        Ok(false)
    }
    pub fn verify(&self) -> bool {
        false
    }
    pub fn verify_opt(&self) -> Option<bool> {
        None
    }
    pub fn verify_unit(&self) -> Result<(), ()> {
        Ok(())
    }
}

pub fn ancestry(p: &Proof) -> Result<u32, ()> {
    p.verify_ancestor().map_err(|_| ())?; // BAD SEC020
    Ok(1)
}

pub fn statement(p: &Proof) {
    p.verify(); // BAD SEC020
    let _ = p.verify(); // BAD SEC020
    let _ = p.verify_ancestor().is_ok(); // BAD SEC020
    p.verify_opt().ok_or(()).ok(); // BAD SEC020
}

pub fn mapped(p: &Proof) -> Result<(), ()> {
    p.verify_ancestor().map(|_| ()) // BAD SEC020
}

pub fn checked(p: &Proof) -> Result<bool, ()> {
    let is_valid = p.verify_ancestor().map_err(|_| ())?; // OK SEC020
    if !is_valid {
        return Err(());
    }
    if p.verify() { // OK SEC020
        return Ok(true);
    }
    if !p.verify_opt().unwrap_or(false) { // OK SEC020
        return Err(());
    }
    if p.verify_ancestor()? == true { // OK SEC020
        return Ok(true);
    }
    assert!(p.verify()); // OK SEC020
    p.verify_unit()?; // OK SEC020
    p.verify_ancestor().inspect(|valid| assert!(*valid))?; // OK SEC020
    p.verify_ancestor() // OK SEC020
}

pub fn returned(p: &Proof) -> bool {
    p.verify() // OK SEC020
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/all-rules.toml" \
  --format json \
  --rules SEC019,SEC020 \
  --driver-path "$DRIVER" \
  "$WORK_DIR/pallet" > "$OUTPUT" || [ $? -eq 1 ]  # exits 1 when it reports errors

status=0
for rule in SEC019 SEC020; do
  expected="$(grep -n "// BAD $rule" "$WORK_DIR/pallet/src/lib.rs" | cut -d: -f1 | sort -n | tr '\n' ' ')"
  actual="$(jq -r --arg rule "$rule" '[.[] | select(.rule_id == $rule) | .line] | unique | .[]' "$OUTPUT" | sort -n | tr '\n' ' ')"
  echo "$rule expected lines: $expected"
  echo "$rule reported lines: $actual"
  if [ "$expected" != "$actual" ]; then
    status=1
  fi
done
exit "$status"
