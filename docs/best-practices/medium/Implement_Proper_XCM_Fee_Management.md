# Implement Proper XCM Fee Management

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** Correct fee economics depend on the SDK version and runtime configuration, so no shipping rule can enforce them.

## Description

The `FeeManager` trait is used to manage fees for executing XCM messages. When properly configured, it allows for fees to be distributed to specified accounts or components. However, if `FeeManager` is set to the empty tuple type `()`, all fees will be burned.

## What should be avoided

Setting FeeManager to the unit type `()` should be done with caution. This setting will automatically burn all fees collected.

```rust
// Fees will be burned.
type FeeManager = ();
```

In this example:

- `FeeManager` is set to `()`, meaning that there is no mechanism to process or allocate the collected fees, causing them to be automatically burned.

## Best practice

Configure `FeeManager` to allow fees to be either deposited or distributed.

```rust
// Fees will be deposited into an account.
type FeeManager = XcmFeeManagerFromComponents<
    WaivedLocations,
    XcmFeeToAccountId20<Self::AssetTransactor, AccountId, StakingPot>,
>;
```

In this example, the `FeeManager` accepts `WaivedLocations` that are exempt from fees and transfers any charged fees to a `StakingPot` account.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Implement_Proper_XCM_Fee_Management.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
