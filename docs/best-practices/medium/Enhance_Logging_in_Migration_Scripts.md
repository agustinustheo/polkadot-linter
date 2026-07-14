# Enhance Logging in Migration Scripts

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** No shipping rule checks migration logging; SEM014 (log target on off-chain-worker `SubmitTransaction`) is a related logging-target convention, not a substitute, and SEC016 (unguarded migration) addresses a different concern. See [SEM014](../../rules/SEM014.md), [SEC016](../../rules/SEC016.md).

## Description

Logging messages in migration scripts should provide clear and specific information about the migration process. In Substrate-based blockchains, where migrations often involve updating on-chain state, detailed logs are critical for tracking progress, diagnosing issues, and ensuring transparency during the process. Insufficient logging can make it difficult to pinpoint errors or understand the steps taken during a migration.

## What should be avoided

Using general log messages in migration scripts provides minimal information:

```rust
log::info!("Migration started");
```

In this example:

- The log message gives no indication of what migration is running, what data is being processed, or whether specific conditions were met, making debugging and tracking nearly impossible.

## Best practice

Use more detailed logging, including migration-specific information and conditions:

```rust
log::info!("Migration started for version: {}", current_version);
if let Some(bucket) = translate(process) {
    log::info!("Translated bucket data for migration: {:?}", bucket);
} else {
    log::warn!("Translation process returned None for bucket data");
}
```

In this example:

- Each log message includes specific information, making it easier to trace migration steps and identify issues if they occur.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Enhance_Logging_in_Migration_Scripts.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
