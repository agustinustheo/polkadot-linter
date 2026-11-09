# All rules

29 rules and LNT001. "Off" rules are off by default. To turn one on, see
[Configuration](guide/configuration.md). "Compiler" rules need the
[driver](guide/installation.md#the-compiler-driver).

| Rule | Finds | Severity | Default | Engine |
| --- | --- | --- | --- | --- |
| [RTC001](rules/RTC001.md) | `type WeightInfo = ()` or `TestWeightInfo` in a production runtime | warning | on | syntax |
| [RTC003](rules/RTC003.md) | Free XCM delivery: `WaiveDeliveryFees` or a `()` delivery price | warning | on | syntax |
| [RTC004](rules/RTC004.md) | A zero existential deposit or storage deposit | warning | off | syntax |
| [RTC005](rules/RTC005.md) | The runtime uses collective-flip randomness | warning | on | syntax |
| [RTC006](rules/RTC006.md) | XCM `FeeManager = ()` burns delivery fees | warning | on | syntax |
| [RTC007](rules/RTC007.md) | A `DenyThenTry` deny that nested XCM programs bypass | warning | on | syntax |
| [RTC008](rules/RTC008.md) | A proxy filter that fails open | warning | on | syntax |
| [RTC009](rules/RTC009.md) | `Cargo.lock` has a crate version with a known security bug | error | on | syntax |
| [RTC010](rules/RTC010.md) | `AuthorizedAliasers` on the unpaid-execution barrier | warning | on | syntax |
| [SEC004](rules/SEC004.md) | Non-saturating arithmetic in a weight attribute | warning | off | compiler |
| [SEC006](rules/SEC006.md) | The remainder of `repatriate_reserved` is ignored | warning | on | compiler |
| [SEC010](rules/SEC010.md) | A hook writes storage more than once without a storage layer | warning | on | compiler |
| [SEC011](rules/SEC011.md) | A dispatchable or hook iterates over storage | warning | off | compiler |
| [SEC012](rules/SEC012.md) | `clear_prefix` with no real limit | warning | on | compiler |
| [SEC015](rules/SEC015.md) | `dispatch_bypass_filter` in production code | warning | on | compiler |
| [SEC019](rules/SEC019.md) | A token imbalance escapes a storage layer | error | on | compiler |
| [SEC020](rules/SEC020.md) | A discarded verification verdict | error | on | compiler |
| [SEC021](rules/SEC021.md) | `if verify(..) { reject }`: an inverted guard | error | on | compiler |
| [SEC022](rules/SEC022.md) | A discarded origin check | error | on | compiler |
| [SEC023](rules/SEC023.md) | An origin check that compares two `Option`s | error | on | compiler |
| [SEC024](rules/SEC024.md) | A discarded `Result` whose `Err` holds funds | error | off | compiler |
| [SEM009](rules/SEM009.md) | `contains_key` just before `remove` or `take` | advisory | on | compiler |
| [SEM010](rules/SEM010.md) | `^` used as a power (it is XOR) | error | on | compiler |
| [SEM011](rules/SEM011.md) | A zero weight on a call | warning | on | syntax |
| [SEM015](rules/SEM015.md) | `#[pallet::authorize]` without `#[pallet::weight_of_authorize]` | warning | on | syntax |
| [SEM016](rules/SEM016.md) | `create_extension()` without `AuthorizeCall` | warning | on | compiler |
| [SEM019](rules/SEM019.md) | A verifier trait method that returns `true` by default | warning | on | syntax |
| [TST002](rules/TST002.md) | `assert_ok!` on `apply_extrinsic` | error | on | syntax |
| [TRM001](rules/TRM001.md) | Words that break your spelling list | advisory | on | syntax |
| [LNT001](rules/LNT001.md) | An allow comment that is invalid or unused | warning | on | syntax |

## Families

| Prefix | Family | What it checks |
| --- | --- | --- |
| RTC | [Runtime configuration](rules/runtime-config.md) | `impl Config for Runtime`, XCM config, `Cargo.lock` |
| SEC | [Security](rules/security.md) | Pallet and XCM code that can lose funds or skip checks |
| SEM | [Semantic](rules/semantic.md) | Code that compiles but does the wrong thing |
| TST | [Tests](rules/testing.md) | Tests that hide failures |
| TRM | [Terminology](rules/terminology.md) | Spelling and terms |
| LNT | [LNT001](rules/LNT001.md) | The linter's own allow comments |

Use a prefix or an ID with `--rules`, for example `--rules RTC,SEC020`.

We deleted 35 other rules. [Retired rules](retired-rules.md) lists them.
