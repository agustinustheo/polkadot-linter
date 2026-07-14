# Provide Event Documentation

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** No documentation-presence rule ships; TST006 checks that mutating extrinsics actually emit events (a different concern), not that events are documented. See [TST006](../../rules/TST006.md).

## Description

Events in Substrate serve as a critical communication layer between the runtime and external clients, providing insights into state changes and operations. Proper documentation of events is essential because it is embedded into the runtime's metadata, which can be accessed by frontend developers and users. This documentation helps users understand the purpose and context of emitted events, ensuring that interactions with the blockchain are clear and predictable. Without proper documentation, frontend developers may struggle to display meaningful messages or interpret blockchain actions, leading to a suboptimal user experience. By documenting each event variant, developers create a richer, more user-friendly ecosystem that bridges the gap between the runtime and external interfaces.

## What should be avoided

Emitting events without descriptions or explanations:

```rust
#[pallet::event]
#[pallet::generate_deposit(pub(super) fn deposit_event)]
pub type Event<T: Config> {
    AccountCreated,
    AccountUpdated,
    AccountDeleted,
    AccountLocked,
}
```

In this example:

- No documentation is provided for the event variants inside the `Event` enum.

## Best practice

Provide detailed comments for each event to explain its purpose and usage:

```rust
#[pallet::event]
#[pallet::generate_deposit(pub(super) fn deposit_event)]
pub type Event<T: Config> {
    /// An account was just created. This event gets triggered when the account
    /// has a balance greater than the existential deposit for the first time.
    AccountCreated,
    /// The account got its balance updated, but over the existential deposit.
    AccountUpdated,
    /// The account got its balance under the existential deposit and its
    /// storage was completely wiped out.
    AccountDeleted,
    /// The account got blocked due to suspicious activities or misbehabiour.
    AccountLocked,
}
```

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Provide_Event_Documentation.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
