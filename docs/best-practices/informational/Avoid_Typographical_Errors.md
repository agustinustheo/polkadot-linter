# Avoid Typographical Errors

**Severity**: <span style="color:cyan;">Informational</span>

> **Linter coverage — Enforced.** Once a dictionary is configured, [TRM001](../../rules/TRM001.md) flags spelling and terminology violations across comments, docs, strings, and configured identifiers, catching a strong subset of the typos described here automatically. See [TRM001](../../rules/TRM001.md).

## Description

Typographical errors, while often overlooked, can significantly affect the clarity and reliability of a codebase. In a decentralized environment where precision is critical, such errors can lead to confusion, incorrect assumptions, and even subtle bugs. Whether in variable names, comments, or documentation, ensuring accuracy in spelling helps maintain clear communication within the team and with external contributors, reducing the likelihood of misunderstandings and improving the overall quality of the project.

## What should be avoided

Typographical errors can make code less readable and may even lead to bugs if used inconsistently:

```rust
// Typo in variable name.
let amout_valu = 100;
```

In this example:

- The misspelled variable `amout_valu` is unclear and could lead to confusion, especially if referenced in multiple parts of the code.

## Best practice

Perform thorough proofreading to catch typos and enhance clarity, ensuring variable names and comments are accurate and descriptive.

```rust
// Correctly spelled variable name.
let amount_value = 100;
```

Using clear and correctly spelled names improves readability, maintains professionalism, and helps prevent
misunderstandings within the codebase.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/informational/Avoid_Typographical_Errors.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
