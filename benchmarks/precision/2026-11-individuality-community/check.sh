#!/usr/bin/env bash
# Re-run the individuality-community acceptance test with the bundled default
# config. Usage: check.sh /path/to/individuality-community (checked out at 58da05d)
set -euo pipefail
IC="${1:?usage: check.sh /path/to/individuality-community}"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cargo build --release --quiet --manifest-path "$ROOT/Cargo.toml" --bin polkadot-linter
OUT="$(cd "$IC" && "$ROOT/target/release/polkadot-linter" --syntax-only -f json . 2>/dev/null || true)"
python3 -c '
import json, sys
found = ["%s %s:%d" % (d["rule_id"], d["file"].removeprefix("./"), d["line"]) for d in json.loads(sys.argv[1])]
print("\n".join(found)); print(f"{len(found)} findings")
must = ["RTC003 runtimes/next-people-paseo/src/xcm_config.rs:424",
        "RTC007 runtimes/next-asset-hub-paseo/src/xcm_config.rs:340",
        "RTC008 runtimes/next-asset-hub-paseo/src/lib.rs:832"]
missing = [m for m in must if m not in found]
if missing or len(found) > 6:
    sys.exit(f"FAIL: missing {missing} or more than 6 findings")
print("OK")' "$OUT"
