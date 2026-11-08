# All Rules

Every rule on one page. Each code links to its full reference page. The
**Severity** column is the rule's built-in severity; `config/default.toml` may
override it. Rules marked "Off by default" are disabled in the bundled
configuration because they were not accurate enough on real code (see
[Configuration](guide/configuration.md)); enable them in `[rules.enabled]` or
run with `config/all-rules.toml`.

| Prefix | Family |
| --- | --- |
| `SEM` | Semantic checks |
| `TST` | Test quality checks |
| `TRM` | Terminology and text conventions |
| `SEC` | Security-focused rules |
| `RTC` | Runtime configuration (`impl Config for Runtime`, XCM, deposits, benchmarks) |
| `LNT` | The linter's own allow comments |

| Code | Family | Name | Severity | What it entails |
| --- | --- | --- | --- | --- |
| [`SEM009`](rules/SEM009.md) | Semantic | `redundant-contains-key-before-remove` | `advisory` | A storage map is checked with `contains_key()` immediately before `remove()` or `take()`. |
| [`SEM010`](rules/SEM010.md) | Semantic | `xor-as-exponentiation` | `error` | `^` is used as if it were exponentiation, even though Rust treats it as bitwise XOR. |
| [`SEM011`](rules/SEM011.md) | Semantic | `weight-zero-placeholder` | `warning` | A pallet weight attribute is zero (`Weight::zero()`, `0`, `Weight::from_parts(0, 0)`), making the call effectively free. |
| [`SEM014`](rules/SEM014.md) | Semantic | `submit-transaction-log-target` | `advisory` | Nearby `log::...!` calls around `SubmitTransaction::<...>::submit_transaction(...)` should include `target: LOG_TARGET` for traceable OCW submission failures. |
| [`SEM015`](rules/SEM015.md) | Semantic | `missing-weight-of-authorize` | `warning` | `#[pallet::authorize]` should be paired with `#[pallet::weight_of_authorize(...)]` so authorize-path validation cost is benchmarked separately. |
| [`SEM016`](rules/SEM016.md) | Semantic | `missing-authorizecall-in-create-authorized-transaction` | `warning` | `CreateAuthorizedTransaction::create_extension()` should include `AuthorizeCall::new()` so authorized transactions do not bypass authorize-stage routing. |
| [`SEM019`](rules/SEM019.md) | Semantic | `verifier-defaults-to-true` | `warning` | A verifier-named trait method (`verify`, `valid`, `ownership_proof`, ...) takes input but its default body is `true`, so implementers that do not override it accept everything (polkadot-sdk #1739). |
| [`TST002`](rules/TST002.md) | Test smell | `apply-extrinsic-assert-ok` | `error` | `assert_ok!` is used on `apply_extrinsic`, which can hide the inner `DispatchError`. |
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
| [`SEC004`](rules/SEC004.md) | Security | `unsafe-weight-arithmetic` | `warning` | A weight attribute uses non-saturating arithmetic that can overflow or undercount. Off by default. |
| [`SEC006`](rules/SEC006.md) | Security | `unchecked-repatriate-reserved` | `warning` | The remaining balance from `repatriate_reserved` is ignored instead of being checked. |
| [`SEC010`](rules/SEC010.md) | Security | `missing-transactional-in-hook` | `warning` | A hook performs multiple storage writes without a transactional storage layer. |
| [`SEC011`](rules/SEC011.md) | Security | `storage-iteration-in-dispatchables` | `warning` | Dispatchable or hook code iterates storage in a state-size-dependent way. |
| [`SEC012`](rules/SEC012.md) | Security | `unbounded-clear-prefix` | `warning` | Trie cleanup uses an effectively unbounded `clear_prefix` or related call. |
| [`SEC015`](rules/SEC015.md) | Security | `dispatch-bypass-filter-in-production` | `warning` | Production code calls `.dispatch_bypass_filter()`, bypassing the runtime filter. |
| [`SEC019`](rules/SEC019.md) | Security | `imbalance-escapes-transaction` | `error` | A rollback-capable `with_storage_layer`/`with_transaction`/`ProcessTransaction::process` closure moves a token imbalance into an outer local, so its `Drop` writes issuance after a rollback. |
| [`SEC020`](rules/SEC020.md) | Security | `discarded-verification-result` | `error` | A verifier's `bool`/`Result<bool, _>` verdict is discarded, so an invalid proof is accepted. |
| [`SEC021`](rules/SEC021.md) | Security | `inverted-verification-guard` | `error` | `if verify(..) { reject }`: the rejecting branch runs when the verifier returns `true`. |
| [`SEC022`](rules/SEC022.md) | Security | `discarded-origin-check` | `error` | The `Result` of `ensure_signed`/`ensure_root`/`EnsureOrigin::ensure_origin` is discarded, so any origin passes. |
| [`SEC023`](rules/SEC023.md) | Security | `option-equality-authorization` | `error` | An `EnsureOrigin` impl compares two `Option`s, so `None == None` authorizes. |
| [`SEC024`](rules/SEC024.md) | Security | `discarded-funds-in-error` | `error` | A `Result` whose `Err` holds a `Credit`/`Debt`/imbalance (e.g. `resolve`) is discarded, so the funds are burned or minted. Off by default. |

**Retired:** BEN001-BEN003, MOK001, SEC001-SEC003, SEC005, SEC007-SEC009,
SEC013, SEC014, SEC016-SEC018, SEM002-SEM008, SEM012, SEM013, TST001,
TST003-TST006 and VAL001-VAL003 were deleted because they were mostly wrong or
not worth fixing on real projects. [What we tried and removed](retired-rules.md)
records what each one did and why it failed.
