# Break Down Complex Functions

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** Complexity thresholds are project policy, so no shipping rule enforces function decomposition.

## Description

Complex functions with multiple responsibilities are harder to test, understand, and maintain, increasing the risk of errors and making debugging more difficult. In Substrate runtime development, where precise logic is critical for the correct functioning of the blockchain, overly complex functions can lead to bugs that are challenging to identify and resolve, potentially impacting the entire network.

## What should be avoided

Combining multiple responsibilities in a single function increases its complexity:

```rust
fn process_transaction() {
    // Transaction validation code
    // ...

    // Fee calculation code
    // ...

    // Balance update code
    // ...

    // Update the storage
    // ...
}
```

In this example:

- The function mixes validation, fee calculation, balance updates, and storage modifications, making it difficult to pinpoint the source of issues or extend the logic without introducing errors.

## Best practice

Apply the single responsibility principle by breaking down the function into smaller, focused functions:

```rust
fn process_transaction() {
    validate_transaction();
    apply_fees();
    update_balances();
    record_transaction();
}
```

This approach simplifies each function, making it easier to test and understand.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Break_Down_Complex_Functions.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
