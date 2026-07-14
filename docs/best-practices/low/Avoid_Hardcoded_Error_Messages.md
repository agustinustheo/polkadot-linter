# Avoid Hardcoded Error Messages

**Severity**: <span style="color:green;">Low</span>

> **Linter coverage — Review gate.** No shipping rule enforces this; whether error messages should be centralized in a `#[pallet::error]` enum is a project error-handling convention that needs human review. There is no related rule to point to here.

## Description

Hardcoding error messages directly in Substrate code can make it difficult to manage and update error handling across the runtime. When error messages are embedded within function logic, localization becomes cumbersome, and updating messages in the future may lead to inconsistencies. By using enums for error handling, developers can centralize and standardize error messages, making the code more flexible, easier to maintain, and adaptable to future changes, including localization for different languages or regions.

## What should be avoided

Embedding error messages directly in function logic can be inflexible:

```rust
fn something_fails() -> Result<(), Error> {
    // ...
    Err("Insufficient balance")
}
```

## What can be done instead

Store error messages in a centralized location or use an enum for error handling:

```rust
#[pallet::error]
pub enum Error<T> {
	/// The account does not have enough balance.
	InsufficientBalance,
}

fn something_fails() -> DispatchError<()> {
    // ...
    Err(Error::<T>::InsufficientBalance)
}
```

This approach makes error handling more flexible, allowing for easier updates and localization.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/low/Avoid_Hardcoded_Error_Messages.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
