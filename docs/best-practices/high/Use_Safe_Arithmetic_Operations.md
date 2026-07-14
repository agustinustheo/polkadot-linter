# Use Safe Arithmetic Operations

**Severity**: <span style="color:orange;">High</span>

> **Linter coverage — Enforced.** Multiple rules flag unsafe arithmetic by default: SEC009 for raw arithmetic in fallible code, SEC004 for unsafe weight arithmetic, VAL002 for unguarded division, and SEM010 for `^` mistaken as exponentiation. See [SEC009](../../rules/SEC009.md), [SEC004](../../rules/SEC004.md), [VAL002](../../rules/VAL002.md), and [SEM010](../../rules/SEM010.md).

## Description

In Substrate runtime development, uncontrolled overflow or underflow is a critical issue because it can cause the chain to stall. Substrate’s deterministic execution model requires every node in the network to process transactions in the exact same way. If an unchecked arithmetic operation causes an overflow or underflow, the runtime will panic, leading to an unrecoverable state for the affected block.

## What should be avoided

The following code performs addition without checking for overflow, which may cause the program to wrap around to an unintended value:

```rust
// Potential for overflow if a + b exceeds the maximum value of the type.
let total: u16 = a + b;
```

In this example:

- If `a` and `b` are large, the result may exceed the data type’s maximum value, causing an overflow and leading to incorrect results.

## Best practice

### Option 1: Use Checked Arithmetic

Use `checked_add` to return an error if the operation exceeds the maximum value:

```rust
let total: u16 = a.checked_add(b).ok_or(Error::<T>::Overflow)?;
```

In this example:

- `checked_add` returns `None` if `a + b` would overflow, allowing us to handle the error with `ok_or`.
- This ensures that the operation will only proceed if the result is within bounds, preventing overflow-related issues.

### Option 2: Use Saturating Arithmetic

Alternatively, `saturating_add` ensures that the result will cap at the maximum value of the type rather than overflowing:

```rust
let total: u16 = a.saturating_add(b);
```

In this example:

- `saturating_add` will set `total` to the maximum possible value if `a + b` exceeds the type’s limit.
- This approach avoids panics or errors by safely capping the result, which is useful when an upper bound is acceptable in the application logic.

Both methods improve the reliability of arithmetic operations, ensuring predictable behavior without overflow.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/high/Use_Safe_Arithmetic_Operations.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
