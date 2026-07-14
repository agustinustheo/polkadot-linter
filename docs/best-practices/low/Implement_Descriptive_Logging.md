# Implement Descriptive Logging

**Severity**: <span style="color:green;">Low</span>

> **Linter coverage — Review gate.** Whether log messages carry enough context is a project logging convention that needs human review; no shipping rule assesses log-message quality broadly. [SEM014](../../rules/SEM014.md) enforces one narrow convention — requiring a `target: LOG_TARGET` on logging near an off-chain-worker `SubmitTransaction` — and is related, not a substitute.

## Description

In Substrate runtime development, logging is an essential tool for monitoring and debugging. Without descriptive and contextual logging, it becomes difficult to trace the flow of operations or identify issues effectively. Log messages that lack sufficient detail can make troubleshooting slow and inefficient, especially when dealing with complex or production-grade systems. By implementing descriptive logging, developers provide valuable insights that can help quickly diagnose problems, track system behavior, and improve the overall maintainability and observability of the blockchain runtime.

## What should be avoided

Logging messages without context or specific details can make troubleshooting challenging and time-consuming:

```rust
log::info!("Process started");
```

## Best practice

Add context and relevant details to log messages to improve clarity and facilitate debugging:

```rust
const LOG_TARGET: &str = "pallet-logging";
log::info!(LOG_TARGET, "Process started for user: {:?}", user_id);
```

This approach enhances traceability, readability, and the overall effectiveness of the logging system by providing meaningful information in each log entry.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/low/Implement_Descriptive_Logging.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
