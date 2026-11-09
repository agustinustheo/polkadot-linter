#!/usr/bin/env bash
# SEC021 (inverted verification guard), SEC022 (discarded origin check) and SEC023
# (`Option == Option` authorization). Lines marked `// BAD <rule>` must be reported,
# lines marked `// OK <rule>` must not.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sec021-sec023.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"
mkdir -p "$WORK_DIR/frame-support/src" "$WORK_DIR/frame-system/src" "$WORK_DIR/pallet/src"

cat > "$WORK_DIR/Cargo.toml" <<'TOML'
[workspace]
members = ["frame-support", "frame-system", "pallet"]
resolver = "2"
TOML

for crate in frame-support frame-system; do
  cat > "$WORK_DIR/$crate/Cargo.toml" <<TOML
[package]
name = "$crate"
version = "0.1.0"
edition = "2021"
TOML
done
echo 'frame-support = { path = "../frame-support" }' >> "$WORK_DIR/frame-system/Cargo.toml"
sed -i.bak 's/^edition = "2021"$/edition = "2021"\n[dependencies]/' "$WORK_DIR/frame-system/Cargo.toml"

cat > "$WORK_DIR/frame-support/src/lib.rs" <<'RS'
pub mod traits {
    pub trait EnsureOrigin<O> {
        type Success;
        fn try_origin(o: O) -> Result<Self::Success, O>;
        fn ensure_origin(o: O) -> Result<Self::Success, BadOrigin> {
            Self::try_origin(o).map_err(|_| BadOrigin)
        }
    }
    pub struct BadOrigin;
}
#[macro_export]
macro_rules! ensure {
    ($x:expr, $y:expr $(,)?) => {
        if !$x {
            return Err($y.into());
        }
    };
}
RS

cat > "$WORK_DIR/frame-system/src/lib.rs" <<'RS'
pub use frame_support::traits::BadOrigin;
pub struct Origin(pub Option<u64>);
pub fn ensure_signed(o: Origin) -> Result<u64, BadOrigin> {
    o.0.ok_or(BadOrigin)
}
pub fn ensure_root(o: Origin) -> Result<(), BadOrigin> {
    o.0.map_or(Ok(()), |_| Err(BadOrigin))
}
RS

cat > "$WORK_DIR/pallet/Cargo.toml" <<'TOML'
[package]
name = "pallet"
version = "0.1.0"
edition = "2021"

[dependencies]
frame-support = { path = "../frame-support" }
frame-system = { path = "../frame-system" }

[features]
default = ["runtime-benchmarks"]
runtime-benchmarks = []
TOML

cat > "$WORK_DIR/pallet/src/benchmarking.rs" <<'RS'
use frame_system::{ensure_root, Origin};

pub fn setup(o: Origin) {
    let _ = ensure_root(o); // OK SEC022
}
RS

cat > "$WORK_DIR/pallet/src/lib.rs" <<'RS'
use frame_support::{ensure, traits::EnsureOrigin};
use frame_system::{ensure_root, ensure_signed, BadOrigin, Origin};

pub struct Error;
impl From<BadOrigin> for Error {
    fn from(_: BadOrigin) -> Self {
        Error
    }
}

// SEC021: inverted verification guard.
pub struct Sig;
pub fn ed25519_verify(_sig: &Sig, _msg: &[u8]) -> bool {
    true
}
pub fn verify_proof(_msg: &[u8]) -> Result<bool, Error> {
    Ok(true)
}
pub fn is_member(_msg: &[u8]) -> bool {
    true
}

pub fn inverted(sig: &Sig, msg: &[u8]) -> Result<(), Error> {
    if ed25519_verify(sig, msg) { // BAD SEC021
        Err(Error)?
    }
    if ed25519_verify(sig, msg) { // BAD SEC021
        return Err(Error);
    }
    ensure!(!ed25519_verify(sig, msg), Error); // BAD SEC021
    Ok(())
}

pub fn inverted_bool(sig: &Sig, msg: &[u8]) -> bool {
    if ed25519_verify(sig, msg) { // BAD SEC021
        return false;
    }
    true
}

pub fn correct(sig: &Sig, msg: &[u8]) -> Result<(), Error> {
    if !ed25519_verify(sig, msg) { // OK SEC021
        Err(Error)?
    }
    ensure!(ed25519_verify(sig, msg), Error); // OK SEC021
    if ed25519_verify(sig, msg) { // OK SEC021
        return Ok(());
    }
    if is_member(msg) { // OK SEC021
        return Err(Error);
    }
    if verify_proof(msg)? { // OK SEC021
        return Ok(());
    }
    let run = || if ed25519_verify(sig, msg) { 1 } else { 0 }; // OK SEC021
    let _ = run();
    Err(Error)
}

// Only a rejection the verified branch itself performs counts.
pub fn nested_reject(sig: &Sig, msg: &[u8], seen: bool) -> Result<(), Error> {
    if ed25519_verify(sig, msg) { // OK SEC021
        if seen {
            return Err(Error);
        }
        Ok(())
    } else {
        Err(Error)
    }
}

pub fn match_reject(sig: &Sig, msg: &[u8]) -> Result<u32, Error> {
    if ed25519_verify(sig, msg) { // OK SEC021
        match verify_proof(msg) {
            Ok(true) => Ok(1),
            _ => Err(Error),
        }
    } else {
        Ok(0)
    }
}

pub fn unused_err(sig: &Sig, msg: &[u8]) -> Result<(), Error> {
    if ed25519_verify(sig, msg) { // OK SEC021
        let _unused = Err::<(), Error>(Error);
        Ok(())
    } else {
        Err(Error)
    }
}

// SEC022: discarded origin check.
pub struct AdminOrigin;
impl EnsureOrigin<Origin> for AdminOrigin {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        if o.0.is_none() { Ok(()) } else { Err(o) }
    }
}

pub fn swallowed(o: Origin, o2: Origin, o3: Origin, o4: Origin, o5: Origin) -> Result<(), Error> {
    let _ = AdminOrigin::ensure_origin(o); // BAD SEC022
    ensure_signed(o2).ok(); // BAD SEC022
    let _who = ensure_signed(o3); // BAD SEC022
    let _ = ensure_root(o4).map_err(|_| Error); // BAD SEC022
    AdminOrigin::try_origin(o5).is_ok(); // BAD SEC022
    Ok(())
}

pub fn swallowed_in_closure(o: Origin) -> Result<(), Error> {
    let body = move || {
        let _ = ensure_root(o); // BAD SEC022
        Ok(())
    };
    body()
}

pub fn propagated(o: Origin, o2: Origin, o3: Origin, o4: Origin, o5: Origin) -> Result<u64, Error> {
    AdminOrigin::ensure_origin(o)?; // OK SEC022
    let _ = ensure_root(o2)?; // OK SEC022
    if AdminOrigin::try_origin(o3).is_ok() { // OK SEC022
        return Ok(0);
    }
    let checked = ensure_root(o4); // OK SEC022
    checked?;
    Ok(ensure_signed(o5)?) // OK SEC022
}

pub fn underscore_used(o: Origin) -> Result<(), Error> {
    let _checked = ensure_root(o); // OK SEC022
    _checked?;
    Ok(())
}

mod benchmarking;

#[cfg(any(test, feature = "runtime-benchmarks"))]
pub fn bench_only(o: Origin) {
    let _ = ensure_root(o); // OK SEC022
}

#[cfg(all(feature = "runtime-benchmarks", not(test)))]
pub fn bench_only_compound(o: Origin) {
    let _ = ensure_root(o); // OK SEC022
}

#[cfg(all(not(feature = "try-runtime"), any(test, feature = "runtime-benchmarks")))]
pub fn bench_only_nested(o: Origin) {
    let _ = ensure_root(o); // OK SEC022
}

#[cfg(not(test))]
pub fn not_test(o: Origin) {
    let _ = ensure_root(o); // BAD SEC022
}

#[cfg(any(feature = "runtime-benchmarks", not(test)))]
pub fn bench_or_other(o: Origin) {
    let _ = ensure_root(o); // BAD SEC022
}

// SEC023: `Option == Option` authorization inside an `EnsureOrigin` impl.
pub fn owner_of(_asset: u32) -> Option<u64> {
    None
}
pub fn account_of(_origin: &Origin) -> Option<u64> {
    None
}

pub struct ForeignAssetOwner;
impl EnsureOrigin<Origin> for ForeignAssetOwner {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let owner = owner_of(1);
        let from = account_of(&o);
        if from != owner { // BAD SEC023
            return Err(o);
        }
        let check = || -> Result<(), Error> {
            // Suppressed: a closure may run after a guard written below it.
            ensure!(account_of(&Origin(None)) == owner_of(1), Error); // OK SEC023
            Ok(())
        };
        if !(&from == &owner) { // BAD SEC023
            return Err(o);
        }
        check().map_err(|_| o)
    }
}

pub struct LocalAssetOwner;
impl EnsureOrigin<Origin> for LocalAssetOwner {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let Some(owner) = owner_of(1) else { return Err(o) };
        let Some(from) = account_of(&o) else { return Err(o) };
        if from != owner { // OK SEC023
            return Err(o);
        }
        if owner_of(1) != Some(from) { // OK SEC023
            return Err(o);
        }
        if owner_of(1) != None { // OK SEC023
            return Err(o);
        }
        if owner_of(1) == account_of(&o) { // OK SEC023
            return Err(o);
        }
        if owner_of(1) != account_of(&o) { // OK SEC023
            return Ok(());
        }
        Ok(())
    }
}

pub struct CheckedOwner;
impl EnsureOrigin<Origin> for CheckedOwner {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let from = account_of(&o);
        if from.is_none() {
            return Err(o);
        }
        if from != owner_of(1) { // OK SEC023
            return Err(o);
        }
        let owner = Some(123);
        if account_of(&o) != owner { // OK SEC023
            return Err(o);
        }
        if !(account_of(&o) < owner_of(1)) { // OK SEC023
            return Err(o);
        }
        Ok(())
    }
}

pub struct ParenCheckedOwner;
impl EnsureOrigin<Origin> for ParenCheckedOwner {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let from = account_of(&o);
        let to = account_of(&o);
        if (from).is_none() || (&to).is_none() {
            return Err(o);
        }
        if from != owner_of(1) { // OK SEC023
            return Err(o);
        }
        if &to != &owner_of(2) { // OK SEC023
            return Err(o);
        }
        Ok(())
    }
}

pub struct ClosureBeforeGuard;
impl EnsureOrigin<Origin> for ClosureBeforeGuard {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let from = account_of(&o);
        let check = || -> Result<(), Error> {
            if from != owner_of(1) { // OK SEC023
                return Err(Error);
            }
            Ok(())
        };
        if from.is_none() {
            return Err(o);
        }
        check().map_err(|_| o)
    }
}

pub struct ShadowedOwner;
impl EnsureOrigin<Origin> for ShadowedOwner {
    type Success = ();
    fn try_origin(o: Origin) -> Result<(), Origin> {
        let from = owner_of(1);
        if from.is_none() {
            return Err(o);
        }
        let from = account_of(&o);
        if from != owner_of(1) { // BAD SEC023
            return Err(o);
        }
        Ok(())
    }
}

// Outside an origin check `None == None` can be the intended semantics (XCM `ExpectOrigin`).
pub fn expect_origin(context: Option<u64>, expected: Option<u64>) -> Result<(), Error> {
    ensure!(context == expected, Error); // OK SEC023
    Ok(())
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/all-rules.toml" \
  --format json \
  --rules SEC021,SEC022,SEC023 \
  --driver-path "$DRIVER" \
  "$WORK_DIR/pallet" > "$OUTPUT" || true  # exits 1 when it reports errors

status=0
for rule in SEC021 SEC022 SEC023; do
  expected="$(cd "$WORK_DIR/pallet/src" && grep -n "// BAD $rule" *.rs | cut -d: -f1,2 | sort | tr '\n' ' ')"
  actual="$(jq -r --arg rule "$rule" '[.[] | select(.rule_id == $rule and (.file | contains("pallet/src/"))) | "\(.file | split("/") | last):\(.line)"] | unique | .[]' "$OUTPUT" | sort | tr '\n' ' ')"
  echo "$rule expected lines: $expected"
  echo "$rule reported lines: $actual"
  if [ "$expected" != "$actual" ]; then
    status=1
  fi
done
exit "$status"
