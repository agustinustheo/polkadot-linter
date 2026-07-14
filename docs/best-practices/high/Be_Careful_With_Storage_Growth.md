# Be Careful With Storage Growth

**Severity**: <span style="color:orange;">High</span>

> **Linter coverage — Enforced.** Several rules flag unbounded storage shapes by default: SEC013 for unbounded storage collections, SEC001 for unbounded `Vec` inputs in extrinsics, SEC017 for unbounded `Vec` in events, and SEC018 for missing input-length weight accounting. SEC014 (Identity hasher on common keys) is also related. See [SEC013](../../rules/SEC013.md), [SEC001](../../rules/SEC001.md), [SEC017](../../rules/SEC017.md), [SEC018](../../rules/SEC018.md), and [SEC014](../../rules/SEC014.md).

## Description

Allowing unlimited entries in storage structures can lead to uncontrolled storage growth, resulting in overflow, increased costs, and performance issues during operations that manage these storage items. In a blockchain context, this can impact execution weight, hinder scalability, and degrade network performance.

## What should be avoided

The following code allows adding entries without any limit, leading to uncontrolled storage growth:

```rust
#[pallet::storage]
#[pallet::unbounded]
pub type Entries<T: Config> = StorageValue<_, Vec<u32>>;

fn add_entry(entry: u32) {
    // Adds entries without limits
    Entries::<T>::mutate(|entries| {
        entries.push(entry);
    });
}
```

## Best practice

Using `BoundedVec`, we can set a fixed maximum number of entries, enforcing storage limits directly within the data structure. This approach automatically restricts the growth of entries, enhancing efficiency.

```rust
#[pallet::storage]
pub type Entries<T: Config> = StorageValue<_, BoundedVec<u32, T::MaxEntries>>;

#[pallet::error]
pub enum Error<T> {
	/// MaxEntries limit reached.
	TooManyEntries,
}

fn add_entry_limited(entry: u32) -> Result<(), DispatchError> {
    Entries::<T>::try_append(entry).map_err(|_| Error::<T>::TooManyEntries)?;
    Ok(())
}
```

Here, the `BoundedVec` ensures that the number of entries cannot exceed `T::MaxEntries`, which enforces storage limits directly. This approach maintains predictable storage usage and efficient operations by preventing uncontrolled accumulation of data.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/high/Be_Careful_With_Storage_Growth.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
