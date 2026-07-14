# Implement Tests For All Error Cases

**Severity**: <span style="color:gold;">Medium</span>

> **Linter coverage — Partial.** The linter checks several test-quality signals — TST001 (prefer `assert_noop`), TST002 (`assert_ok` on `apply_extrinsic`), TST004 (fee error-path companion), and TST006 (observable events) — but it cannot prove that every error case has a test. See [TST001](../../rules/TST001.md), [TST002](../../rules/TST002.md), [TST004](../../rules/TST004.md), [TST006](../../rules/TST006.md).

## Description

Omitting tests for error cases in extrinsics can leave critical failure paths unverified, leading to unhandled scenarios and unexpected behavior in runtime execution. By neglecting to test that specific errors are emitted under invalid conditions, the robustness and predictability of the runtime can be compromised, especially under edge cases.

## What should be avoided

Avoid leaving out tests that validate the expected errors when certain invalid inputs or conditions are encountered.

```rust
// lib.rs
#[pallet::storage]
pub type ManagerAccount<T: Config> = StorageValue<_, T::AccountId>;

#[pallet::call_index(0)]
#[pallet::weight(T::WeightInfo::do_something())]
pub fn do_something(origin: OriginFor<T>, input: u32) -> DispatchResult {
    let who = ensure_signed(origin)?;

    ensure!(who == ManagerAccount::<T>::get(), Error::<T>::UnexpectedAccount);
    ensure!(input > 0, Error::<T>::IncorrectInput);

    // Rest of the logic

    Ok(())
}

// tests.rs
#[test]
fn do_something_works() {
    let right_account = ManagerAccount::<Test>::get();
    let valid_input = 5;

    assert_ok!(SomethingPallet::<Test>::do_something(
        RuntimeOrigin::signed(right_account),
        valid_input
    ));
}
```

In this example:

- The test only confirms that the function succeeds with valid inputs. It does not check if proper errors are triggered when invalid inputs or accounts are provided.

## Best practice

Include tests that verify all error cases the extrinsic is expected to handle. This ensures predictable behavior even under incorrect inputs, improving the reliability and resilience of the runtime.

```rust
#[test]
fn do_something_works() {
    let right_account = ManagerAccount::<Test>::get();
    let valid_input = 5;

    assert_ok!(SomethingPallet::<Test>::do_something(
        RuntimeOrigin::signed(right_account),
        valid_input
    ));
}

#[test]
fn do_something_fails_if_wrong_account() {
    let wrong_account = get_wrong_account();
    let valid_input = 5;

    assert_noop!(SomethingPallet::<Test>::do_something(
        RuntimeOrigin::signed(wrong_account),
        valid_input
    ), Error::<Test>::UnexpectedAccount);
}

#[test]
fn do_something_fails_if_invalid_input() {
    let right_account = ManagerAccount::<Test>::get();
    let wrong_input = 0;

    assert_noop!(SomethingPallet::<Test>::do_something(
        RuntimeOrigin::signed(right_account),
        wrong_input
    ), Error::<Test>::IncorrectInput);
}
```

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/medium/Implement_Tests_For_All_Error_Cases.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
