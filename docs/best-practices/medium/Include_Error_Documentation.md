# Include Error Documentation

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** No documentation-presence rule ships, so error documentation is verified only by human review.

## Description

Errors in runtime code are crucial for communicating issues to developers and users. Without proper documentation, understanding the purpose and cause of specific errors can be challenging, leading to longer debugging cycles and potential misinterpretation of error messages. Documenting error variants ensures clarity and provides essential context directly in the codebase, making it easier for developers to identify and address issues efficiently.

## What should be avoided

Leaving error variants undocumented can lead to confusion and slow down debugging:

```rust
#[pallet::error]
pub enum Error {
    InsufficientBalance,
    InvalidAsset,
}
```

In this example:

- No documentation is provided for the error variants inside the `Error` enum.

## Best practice

Add documentation for each error variant to clarify its cause and expected behavior:

```rust
#[pallet::error]
pub enum Error {
    /// Occurs when the user's balance is too low for the operation.
    InsufficientBalance,
    /// Indicates that the specified asset is not recognized.
    InvalidAsset,
}
```

In this example:

- Documenting each error variant provides more context, helping developers understand the cause and appropriate response for each error.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Include_Error_Documentation.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
