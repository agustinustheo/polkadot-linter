# Remove Unnecessary Return Values

**Severity**: <span style="color:green;">Low</span>

> **Linter coverage — Review gate.** No shipping rule detects return values that add no information; deciding a return is redundant depends on function intent and is review work. [SEC007](../../rules/SEC007.md) (a `Result` discarded via `let _ =`) is a related but distinct concern, not a substitute.

## Description

Returning unnecessary values in functions can clutter the code, making it harder to read and maintain. Unused return values do not add value and can lead to confusion about the function's purpose. By removing redundant return values, developers can simplify function signatures, reduce cognitive load, and make the code more efficient. This practice also enhances the clarity of the function’s intent, helping future developers understand the logic more easily and ensuring a cleaner codebase.

## What should be avoided

Returning an input parameter without modification is redundant and can be confusing:

```rust
fn project_validation(project_metadata: Metadata) -> Result<Metadata, Error> {
    validate_metadata(&metadata)?;

    // Returns the same value without changes
    project_metadata
}
```

## Best practice

If the return value is not needed, avoid returning it, focusing the function solely on its primary operation:

```rust
fn project_validation(project_metadata: &Metadata) -> Result<(), Error> {
    // Perform validation without returning project_metadata
    validate_metadata(metadata)?;

    Ok(())
}
```

In this approach:

- The function is more straightforward, focusing only on validation without an unnecessary return value, improving clarity and maintainability.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/low/Remove_Unnecessary_Return_Values.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
