# Use Appropriate Origin Checks

**Severity**: <span style="color:red;">Critical</span>

> **Linter coverage — Review gate.** No shipping rule can verify that the correct origin check guards a given extrinsic; this requires human review of each privileged call. [SEC015](../../rules/SEC015.md) catches one related privilege-bypass pattern (unguarded `dispatch_bypass_filter`), but it is related, not a substitute.

## Description

Leaving critical or privileged extrinsics without proper origin checks can allow unauthorized actions, potentially compromising security and functionality. Critical operations must enforce strict access control to ensure that only authorized users or roles can execute them.

## What should be avoided

In the following code, the `execute_critical_operation` function can be called by any user, which may lead to unauthorized or malicious actions:

```rust
#[pallet::call_index(0)]
#[pallet::weight(T::WeightInfo::execute_critical_operation())]
pub fn execute_critical_operation(origin: OriginFor<T>) -> DispatchResult {
    // Function with unrestricted access
    execute_critical_operation();
}
```

In this example:

- The extrinsic can be executed by anyone because there are no access control checks in place, which can be particularly problematic for critical chain operations.

## Best practice

Implement appropriate origin checks to restrict function access to specific users or roles, such as elevated origins, to protect critical functions.

### Example 1

```rust
#[pallet::call_index(0)]
#[pallet::weight(T::WeightInfo::execute_critical_operation())]
pub fn execute_critical_operation(origin: OriginFor<T>) -> DispatchResult {
    // Restrict access to the root (admin) user
    ensure_root(origin)?;

    // Secure function logic here
    execute_critical_operation();
}
```

In this example:

- Using `ensure_root` enforces that only users or groups with elevated permissions can execute this function.

### Example 2

```rust
// ---- In pallet/lib.rs ----
#[pallet::config]
	pub trait Config: frame_system::Config {
        //....
        /// Origin allowed to execute critical Operations.
		type AuthorizedOrigin: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;
    }

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::execute_critical_operation())]
    pub fn execute_critical_operation(origin: OriginFor<T>) -> DispatchResult {
        // Use custom AuthorizedOrigin check.
        T::AuthorizedOrigin::ensure_origin(origin)?;

        // Secure function logic here.
        execute_critical_operation();
    }
}
```

In this example:

- The pallet uses a configurable custom origin `AuthorizedOrigin` to specify which entities are allowed to execute the extrinsic.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/critical/Use_Appropriate_Origin_Checks.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
