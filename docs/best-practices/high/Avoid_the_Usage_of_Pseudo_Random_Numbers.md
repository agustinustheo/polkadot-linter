# Avoid the Usage of Pseudo Random Numbers

**Severity**: <span style="color:orange;">High</span>

> **Linter coverage — Review gate.** No shipping rule detects pseudo-random number usage in the runtime. Entropy quality, determinism, and selection fairness require human and design review.

## Description

In Substrate runtime development, using non-deterministic methods for critical processes, such as selecting or assigning tasks, can create opportunities for manipulation. This may allow certain participants to be chosen more frequently or unfairly, undermining the trust and fairness essential to decentralized systems.

## What should be avoided

The following code relies on a random selection, which can lead to inconsistent results and potential bias:

```rust
#[pallet::storage]
pub type Verifiers<T: Config> = StorageValue<_, Vec<T::AccountId>;

fn assign_verifier() -> Verifier {
    let verifiers = Verifiers::<T>::get();

    // Random selection from the list
    let index = rand::random::<usize>() % verifiers.len();
    let verifier = verifiers[index];
    // Task assigned to verifier unpredictably
    verifier.clone()
}
```

In this example:

- `rand::random::<usize>() % verifiers.len()` selects a verifier at random, which could result in unfair frequency of selection for certain verifiers.

## Best practice

Implement a deterministic selection method, such as using an ID as a basis to ensure a fair, repeatable selection:

```rust
#[pallet::storage]
pub type Verifiers<T: Config> = StorageValue<_, Vec<T::AccountId>;

fn assign_verifier_deterministic(task_id: u32) -> Verifier {
    let verifiers = Verifiers::<T>::get();

    // Deterministic selection based on task ID
    let index = task_id as usize % verifiers.len();
    verifiers[index].clone()
}
```

In this improved example:

- The verifier is selected based on `task_id`, ensuring that each task consistently maps to a specific verifier in a fair, predictable way.
- This approach prevents any single verifier from being chosen disproportionately, ensuring fair distribution and reducing the risk of manipulation.

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/high/Avoid_the_Usage_of_Pseudo_Random_Numbers.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
