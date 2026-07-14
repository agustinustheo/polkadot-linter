# Keep Dependencies Up to Date

**Severity**: <span style="color:orange;">High</span>

> **Linter coverage — Review gate.** No shipping rule covers this. Use dedicated dependency and security-advisory tooling (for example `cargo update` and `cargo audit`) together with human review to keep dependencies current.

## Description

Using outdated libraries can introduce security vulnerabilities, compatibility issues, and reduced performance, as older dependencies may lack recent bug fixes and security patches. In blockchain development, using outdated dependencies is particularly risky because the decentralized nature of the ecosystem amplifies the consequences of vulnerabilities or compatibility issues. Unlike traditional applications, blockchain systems operate in trustless environments where security, reliability, and performance are paramount.

## What should be avoided

### Example 1

The following code relies on an outdated library that may contain deprecated or insecure functions:

```rust
use outdated_library::deprecated_fn;
```

### Example 2

The following configuration uses an outdated branch of the Polkadot-SDK repository, which may include deprecated features, unresolved issues, or security vulnerabilities:

```toml
# In Cargo.toml
sp-runtime = { git = "https://github.com/paritytech/substrate.git", branch = "polkadot-v1.0.0" }
```

## Best practice

Regularly update dependencies to their latest stable versions, ensuring the use of secure and well-maintained functions:

### Example 1

Replace deprecated functions by migrating to the latest version of the library:

```rust
use latest_library::safe_fn;
```

### Example 2

Ensure you are using the most recent stable branch of the Polkadot SDK repository:

```toml
# In Cargo.toml
sp-runtime = { git = "https://github.com/paritytech/polkadot-sdk.git", branch = "polkadot-stable2412-1" }
```

By updating to the latest versions, you benefit from security improvements, performance enhancements, and better compatibility, ensuring a more robust and secure application.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/high/Keep_Dependencies_Up_To_Date.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
