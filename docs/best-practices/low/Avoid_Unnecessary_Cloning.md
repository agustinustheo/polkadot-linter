# Avoid Unnecessary Cloning

**Severity**: <span style="color:green;">Low</span>

> **Linter coverage — Review gate.** No shipping rule flags unnecessary `.clone()` calls; judging whether a clone is redundant needs ownership and allocation context that only a reviewer can supply. [SEM003](../../rules/SEM003.md) (prefer reference iteration over `.iter()`) is a related idiom, not a substitute.

## Description

Unnecessary cloning and unused code increase memory usage and processing overhead, leading to inefficiencies in Substrate runtime development. Each clone operation results in additional memory allocations, which can quickly add up, especially in resource-constrained environments. By avoiding unnecessary cloning and ensuring that data is accessed directly through references, developers can improve both memory efficiency and performance, maintaining a cleaner and more optimized codebase.

## What should be avoided

Cloning data unnecessarily creates additional memory allocations, as shown here:

```rust
fn process_data(data: &Vec<u32>) {
    // Cloning the entire vector unnecessarily
    let cloned_data = data.clone();

    for elem in cloned_data {
        // processing the element
    }
}
```

In this example:

- The entire `data` vector is cloned, doubling the memory usage even if the original data can be processed directly or accessed via reference.

## Best practice

Use references to avoid unnecessary cloning, and review code for unused or redundant sections regularly to keep the codebase lean:

```rust
fn process_data(data: &Vec<u32>) {
    // Process data directly via reference without cloning
    for elem in data {
        // elem is a reference here
    }
}
```

This approach eliminates the need for additional memory allocation, making the code more efficient and easier to maintain. By using references, you reduce both memory usage and potential performance overhead.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/low/Avoid_Unnecessary_Cloning.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
