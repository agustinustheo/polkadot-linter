# Remove Deprecated Storage Getters

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Review gate.** No shipping rule targets deprecated `#[pallet::getter]` storage getters; SEM007 (deprecated `RuntimeDebug`) and SEM008 (deprecated `sp_std`) enforce other deprecations and are not substitutes. See [SEM007](../../rules/SEM007.md), [SEM008](../../rules/SEM008.md).

## Description

The `#[pallet::getter]` attribute in Substrate is deprecated, and its continued use may lead to compatibility issues with future framework updates. Adopting modern approaches for storage access ensures compatibility with evolving standards. By transitioning away from deprecated getters, developers can maintain cleaner, more forward-compatible code while leveraging direct storage access or custom getter methods tailored to their needs.

## What should be avoided

Using `#[pallet::getter]` to define storage getters can lead to issues with future updates, as shown below:

```rust
#[pallet::storage]
#[pallet::getter(fn deprecated_getter)]
pub type MyValue<T> = StorageValue<_, u32, OptionQuery>;
```

In this example:

- The `#[pallet::getter]` attribute defines a deprecated getter function (`deprecated_getter`), which may no longer be supported in future Substrate versions.

## Best practice

Access the storage value directly or use custom functions to handle storage access without relying on deprecated getters:

```rust
#[pallet::storage]
pub type MyValue<T> = StorageValue<_, u32, OptionQuery>;

// Create a custom getter
fn get_my_value() -> Option<u32> {
    MyValue::<T>::get()
}

// Or simply access the storage item directly
fn process_stuff() {
    if let Some(my_value) = MyValue::<T>::get() {
        // Use my_value here
    }
}
```

In this example:

- The `get_my_value` function provides controlled access to the storage item without using `#[pallet::getter]`,
  maintaining compatibility with future updates.
- The same results can be yielded by simply accessing the storage item straightforwardly.
- Both approaches ensure that storage access remains up-to-date and adaptable to evolving framework standards.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Remove_Deprecated_Storage_Getters.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
