# Summary

[Introduction](README.md)

# Use the linter

- [Install](guide/installation.md)
- [Quick start](guide/quick-start.md)
- [Configuration](guide/configuration.md)
  - [Command-line options](guide/cli-reference.md)
  - [Suppress findings and baselines](guide/suppressing-findings.md)
- [CI and AI agents](guide/ci-integration.md)

# Rules

- [All rules](rule-index.md)
- [Runtime configuration (RTC)](rules/runtime-config.md)
  - [RTC001 - Placeholder WeightInfo](rules/RTC001.md)
  - [RTC003 - XCM delivery is free](rules/RTC003.md)
  - [RTC004 - Zero deposit](rules/RTC004.md)
  - [RTC005 - Collective-flip randomness](rules/RTC005.md)
  - [RTC006 - XCM fees burned](rules/RTC006.md)
  - [RTC007 - XCM deny barrier misses nested programs](rules/RTC007.md)
  - [RTC008 - Proxy filter fails open](rules/RTC008.md)
  - [RTC009 - Vulnerable crate version in Cargo.lock](rules/RTC009.md)
  - [RTC010 - Storage-reading aliaser on the unpaid barrier](rules/RTC010.md)
- [Security (SEC)](rules/security.md)
  - [SEC004 - Non-saturating weight arithmetic](rules/SEC004.md)
  - [SEC006 - Unchecked repatriate_reserved remainder](rules/SEC006.md)
  - [SEC010 - Hook without a storage layer](rules/SEC010.md)
  - [SEC011 - Storage iteration in a dispatchable](rules/SEC011.md)
  - [SEC012 - Unbounded clear_prefix](rules/SEC012.md)
  - [SEC015 - dispatch_bypass_filter in production](rules/SEC015.md)
  - [SEC019 - Imbalance escapes a storage layer](rules/SEC019.md)
  - [SEC020 - Discarded verification verdict](rules/SEC020.md)
  - [SEC021 - Inverted verification guard](rules/SEC021.md)
  - [SEC022 - Discarded origin check](rules/SEC022.md)
  - [SEC023 - Option equality in an origin check](rules/SEC023.md)
  - [SEC024 - Discarded funds in an error](rules/SEC024.md)
- [Semantic (SEM)](rules/semantic.md)
  - [SEM009 - Redundant contains_key](rules/SEM009.md)
  - [SEM010 - XOR used as a power](rules/SEM010.md)
  - [SEM011 - Zero weight](rules/SEM011.md)
  - [SEM015 - Missing weight_of_authorize](rules/SEM015.md)
  - [SEM016 - Missing AuthorizeCall](rules/SEM016.md)
  - [SEM019 - Verifier defaults to true](rules/SEM019.md)
- [Tests (TST)](rules/testing.md)
  - [TST002 - assert_ok! on apply_extrinsic](rules/TST002.md)
- [Terminology (TRM)](rules/terminology.md)
  - [TRM001 - Spelling and terminology](rules/TRM001.md)
- [LNT001 - Invalid allow comment](rules/LNT001.md)
- [Best-practice coverage](best-practices.md)

# Background

- [How rules are measured](measurement.md)
- [Retired rules](retired-rules.md)
- [Contributing](contributing.md)
