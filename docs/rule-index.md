# All Rules

Every rule on one page. Each code links to its full reference page. The
**Severity** column is the rule's built-in severity; `config/default.toml` may
override it. Rules marked "Off by default" are disabled in the bundled
configuration because they were not accurate enough on real code (see
[Configuration](guide/configuration.md)); enable them in `[rules.enabled]` or
run with `config/all-rules.toml`.

| Prefix | Family |
| --- | --- |
| `VAL` | Validation order and guard rails |
| `SEM` | Semantic and style checks |
| `TST` | Test quality checks |
| `MOK` | Mock usage heuristics |
| `BEN` | Benchmark coverage and verification |
| `TRM` | Terminology and text conventions |
| `SEC` | Security-focused rules |
| `RTC` | Runtime configuration (`impl Config for Runtime`, XCM, deposits, benchmarks) |
| `LNT` | The linter's own allow comments |

| Code | Family | Name | Severity | What it entails |
| --- | --- | --- | --- | --- |
| [`VAL001`](rules/VAL001.md) | Validation | `validation-before-heavy-read` | `warning` | Expensive storage reads happen before cheap precondition checks such as `ensure!` or bounds checks. Off by default. |
| [`VAL002`](rules/VAL002.md) | Validation | `division-without-zero-guard` | `advisory` | Division uses a config or runtime-derived value without first proving the divisor cannot be zero. Off by default. |
| [`VAL003`](rules/VAL003.md) | Validation | `storage-write-before-validation` | `warning` | Storage is mutated before all validations finish, which can leave unnecessary writes or rollback risk. Off by default. |
| [`SEM002`](rules/SEM002.md) | Semantic | `prefer-collect-turbofish` | `advisory` | `iter.collect()` uses a separate type annotation instead of `.collect::<Vec<_>>()`. Off by default. |
| [`SEM003`](rules/SEM003.md) | Semantic | `prefer-ref-iteration` | `advisory` | A `for` loop uses `.iter()` where borrowing the collection directly would be more idiomatic. Off by default. |
| [`SEM004`](rules/SEM004.md) | Semantic | `no-wildcard-imports` | `warning` | Production code uses wildcard imports such as `use foo::*`. Off by default. |
| [`SEM005`](rules/SEM005.md) | Semantic | `parameterise-weight-functions` | `warning` | Weight functions multiply a constant weight instead of calling a parameterised `WeightInfo` function. Off by default. |
| [`SEM006`](rules/SEM006.md) | Semantic | `dbweight-missing-pov` | `warning` | `DbWeight::get().reads()` is used where proof size accounting should come from a benchmarked weight. Off by default. |
| [`SEM007`](rules/SEM007.md) | Semantic | `runtime-debug-deprecated` | `warning` | Deprecated `RuntimeDebug` or `RuntimeDebugNoBound` derives are used. Off by default. |
| [`SEM008`](rules/SEM008.md) | Semantic | `sp-std-deprecated` | `advisory` | Code still depends on deprecated `sp_std` instead of `alloc`-based `no_std` imports. Off by default. |
| [`SEM009`](rules/SEM009.md) | Semantic | `redundant-contains-key-before-remove` | `advisory` | A storage map is checked with `contains_key()` immediately before `remove()` or `take()`. |
| [`SEM010`](rules/SEM010.md) | Semantic | `xor-as-exponentiation` | `error` | `^` is used as if it were exponentiation, even though Rust treats it as bitwise XOR. |
| [`SEM011`](rules/SEM011.md) | Semantic | `weight-zero-placeholder` | `warning` | A pallet weight attribute is zero (`Weight::zero()`, `0`, `Weight::from_parts(0, 0)`), making the call effectively free. |
| [`SEM012`](rules/SEM012.md) | Semantic | `allow-dead-code-in-pallet` | `warning` | Production pallet code suppresses dead-code warnings instead of removing unused items. Off by default. |
| [`SEM013`](rules/SEM013.md) | Semantic | `custom-invalidity-repr-u8` | `warning` | Custom invalidity enums that feed `InvalidTransaction::Custom(u8)` should declare `#[repr(u8)]` so discriminants stay explicit. |
| [`SEM014`](rules/SEM014.md) | Semantic | `submit-transaction-log-target` | `advisory` | Nearby `log::...!` calls around `SubmitTransaction::<...>::submit_transaction(...)` should include `target: LOG_TARGET` for traceable OCW submission failures. |
| [`SEM015`](rules/SEM015.md) | Semantic | `missing-weight-of-authorize` | `warning` | `#[pallet::authorize]` should be paired with `#[pallet::weight_of_authorize(...)]` so authorize-path validation cost is benchmarked separately. |
| [`SEM016`](rules/SEM016.md) | Semantic | `missing-authorizecall-in-create-authorized-transaction` | `warning` | `CreateAuthorizedTransaction::create_extension()` should include `AuthorizeCall::new()` so authorized transactions do not bypass authorize-stage routing. |
| [`SEM019`](rules/SEM019.md) | Semantic | `verifier-defaults-to-true` | `warning` | A verifier-named trait method (`verify`, `valid`, `ownership_proof`, ...) takes input but its default body is `true`, so implementers that do not override it accept everything (polkadot-sdk #1739). |
| [`TST001`](rules/TST001.md) | Test smell | `prefer-assert-noop` | `warning` | Tests manually check for failure instead of using `assert_noop!`, which also verifies storage is unchanged. Off by default. |
| [`TST002`](rules/TST002.md) | Test smell | `apply-extrinsic-assert-ok` | `error` | `assert_ok!` is used on `apply_extrinsic`, which can hide the inner `DispatchError`. |
| [`TST003`](rules/TST003.md) | Test smell | `imports-inside-closures` | `warning` | `use` imports appear inside functions or closures instead of at module scope. Off by default. |
| [`TST004`](rules/TST004.md) | Test smell | `pays-yes-error-path` | `warning` | A `Pays::No` success path lacks a companion test proving the failing path still pays fees. Off by default. |
| [`TST005`](rules/TST005.md) | Test smell | `implementation-detail-assertions` | `warning` | Tests assert on likely internal fields such as `.inner` or `.state` instead of observable behaviour. Off by default. |
| [`TST006`](rules/TST006.md) | Test smell | `extrinsic-without-event` | `warning` | A storage-mutating extrinsic does not emit an event for external observers. |
| [`MOK001`](rules/MOK001.md) | Mock usage | `excessive-mock-setup` | `warning` | Test setup is dominated by mocks and scaffolding relative to actual assertions. Off by default. |
| [`BEN001`](rules/BEN001.md) | Benchmark | `benchmark-for-weight-function` | `warning` | A weight function exists without a matching benchmark. Off by default. |
| [`BEN002`](rules/BEN002.md) | Benchmark | `benchmark-verification` | `warning` | A benchmark lacks a verification step that proves the benchmarked operation really happened. |
| [`BEN003`](rules/BEN003.md) | Benchmark | `extrinsic-without-benchmark` | `warning` | A dispatchable call has no matching benchmark in the sibling benchmarking file. |
| [`RTC001`](rules/RTC001.md) | Runtime config | `placeholder-weight-info-in-runtime` | `warning` | A production runtime binds `type WeightInfo = ();` or a `TestWeightInfo` in `impl <pallet>::Config for Runtime`. Upstream `SubstrateWeight` is reported as `advisory` only with `flag_substrate_weight = true`. |
| [`RTC002`](rules/RTC002.md) | Runtime config | `pallet-missing-from-define-benchmarks` | `warning` | A pallet binds runtime-local `weights::<pallet>` but is absent from the runtime's `define_benchmarks!`, so its weights are never regenerated. |
| [`RTC003`](rules/RTC003.md) | Runtime config | `xcm-fees-waived` | `error` | The XCM executor `FeeManager` is `WaiveDeliveryFees` or `XcmFeeManagerFromComponents<Everything, _>`, or a delivery price is `()`/`NoPriceForMessageDelivery`, so XCM delivery is free. |
| [`RTC004`](rules/RTC004.md) | Runtime config | `zero-deposit-config` | `warning` | `ExistentialDeposit` or a permissionless storage deposit (approvals, asset accounts, metadata, session keys, ...) is configured as zero, including `type XDeposit = ();`. Off by default. |
| [`RTC005`](rules/RTC005.md) | Runtime config | `insecure-randomness-in-runtime` | `warning` | The runtime declares or consumes `pallet_insecure_randomness_collective_flip` (or its pre-rename `pallet_randomness_collective_flip`). |
| [`RTC006`](rules/RTC006.md) | Runtime config | `xcm-fees-burned` | `warning` | The XCM executor `FeeManager` is `()`, which charges delivery and export fees and then burns them instead of routing them (older SDKs waived them). |
| [`RTC007`](rules/RTC007.md) | Runtime config | `xcm-deny-barrier-not-recursive` | `warning` | `DenyThenTry` uses an instruction-inspecting deny (`DenyReserveTransferToRelayChain`, ...) not wrapped in `DenyRecursively`, so nested `SetAppendix`/`SetErrorHandler`/`ExecuteWithOrigin` programs bypass it. |
| [`RTC008`](rules/RTC008.md) | Runtime config | `proxy-filter-fails-open` | `warning` | A proxy `filter` deny-list (`!matches!`) misses a value-moving pallet of the runtime, or `is_superset` has `(X, _) => true` while X can add proxies and another proxy type admits calls X does not. |
| [`RTC009`](rules/RTC009.md) | Runtime config | `known-vulnerable-crate-version` | `error` | `Cargo.lock` pins a Polkadot crate release with a known security bug fixed in a quiet patch release (no RustSec/GHSA entry), e.g. the dotUSD `staging-xcm-executor` 28.0.1. |
| [`RTC010`](rules/RTC010.md) | Runtime config | `storage-reading-unpaid-aliaser` | `warning` | `AllowExplicitUnpaidExecutionFrom` gets `pallet_xcm::AuthorizedAliasers` as its aliaser filter, so any message triggers unmetered storage reads before fees are taken. |
| [`LNT001`](guide/suppressing-findings.md) | Semantic | `allow-comment` | `warning` | A `// polkadot-linter: allow(...)` comment has no reason or names an unknown rule (warning), or suppresses nothing (advisory). |
| [`TRM001`](rules/TRM001.md) | Terminology | `spelling-conventions` | `advisory` | Comments, docs, strings, or configured identifiers violate the project’s spelling dictionary. |
| [`SEC001`](rules/SEC001.md) | Security | `unbounded-vec-in-extrinsic` | `warning` | An extrinsic accepts an unbounded `Vec<T>` parameter that can grow without limit. Off by default. |
| [`SEC002`](rules/SEC002.md) | Security | `debug-assert-in-production` | `warning` | Production runtime code relies on `debug_assert!`-style checks. Off by default. |
| [`SEC003`](rules/SEC003.md) | Security | `missing-decode-depth-limit` | `warning` | User-controlled decoding happens without `decode_with_depth_limit`. Off by default. |
| [`SEC004`](rules/SEC004.md) | Security | `unsafe-weight-arithmetic` | `warning` | A weight attribute uses non-saturating arithmetic that can overflow or undercount. Off by default. |
| [`SEC005`](rules/SEC005.md) | Security | `expensive-weight-calculation` | `advisory` | Weight calculation performs storage reads, encoding, or other expensive work before dispatch. Off by default. |
| [`SEC006`](rules/SEC006.md) | Security | `unchecked-repatriate-reserved` | `warning` | The remaining balance from `repatriate_reserved` is ignored instead of being checked. |
| [`SEC007`](rules/SEC007.md) | Security | `let-underscore-result` | `warning` | `let _ =` discards a likely `Result`, hiding potential failures. Off by default. |
| [`SEC008`](rules/SEC008.md) | Security | `panic-in-production` | `warning` | Production code uses panic paths such as `unwrap`, `expect`, `panic!`, or `todo!`. Off by default. |
| [`SEC009`](rules/SEC009.md) | Security | `raw-arithmetic-in-fallible` | `advisory` | Fallible functions use raw arithmetic operators where overflow handling should be explicit. Off by default. |
| [`SEC010`](rules/SEC010.md) | Security | `missing-transactional-in-hook` | `warning` | A hook performs multiple storage writes without a transactional storage layer. |
| [`SEC011`](rules/SEC011.md) | Security | `storage-iteration-in-dispatchables` | `warning` | Dispatchable or hook code iterates storage in a state-size-dependent way. |
| [`SEC012`](rules/SEC012.md) | Security | `unbounded-clear-prefix` | `warning` | Trie cleanup uses an effectively unbounded `clear_prefix` or related call. |
| [`SEC013`](rules/SEC013.md) | Security | `unbounded-storage-collections` | `warning` | Storage types use unbounded collections without explicitly marking them as unbounded. Off by default. |
| [`SEC014`](rules/SEC014.md) | Security | `identity-hasher-on-common-keys` | `warning` | `Identity` hashing is used with common key types that should not be stored unhashed. Off by default. |
| [`SEC015`](rules/SEC015.md) | Security | `dispatch-bypass-filter-in-production` | `warning` | Production code calls `.dispatch_bypass_filter()`, bypassing the runtime filter. |
| [`SEC016`](rules/SEC016.md) | Security | `missing-storage-version-check-in-runtime-upgrade` | `warning` | `on_runtime_upgrade` writes storage without guarding on storage version checks. Off by default. |
| [`SEC017`](rules/SEC017.md) | Security | `vec-in-events` | `warning` | A pallet event carries an unbounded `Vec<>` payload. Off by default. |
| [`SEC018`](rules/SEC018.md) | Security | `missing-weight-for-unbounded-input` | `warning` | A dispatchable accepts an unbounded `Vec<>` input but its `#[pallet::weight]` does not account for that input length. Off by default. |
| [`SEC019`](rules/SEC019.md) | Security | `imbalance-escapes-transaction` | `error` | A rollback-capable `with_storage_layer`/`with_transaction`/`ProcessTransaction::process` closure moves a token imbalance into an outer local, so its `Drop` writes issuance after a rollback. Off by default. |
| [`SEC020`](rules/SEC020.md) | Security | `discarded-verification-result` | `error` | A verifier's `bool`/`Result<bool, _>` verdict is discarded, so an invalid proof is accepted. |
| [`SEC021`](rules/SEC021.md) | Security | `inverted-verification-guard` | `error` | `if verify(..) { reject }`: the rejecting branch runs when the verifier returns `true`. |
| [`SEC022`](rules/SEC022.md) | Security | `discarded-origin-check` | `error` | The `Result` of `ensure_signed`/`ensure_root`/`EnsureOrigin::ensure_origin` is discarded, so any origin passes. Off by default. |
| [`SEC023`](rules/SEC023.md) | Security | `option-equality-authorization` | `error` | An `EnsureOrigin` impl compares two `Option`s, so `None == None` authorizes. Off by default. |
