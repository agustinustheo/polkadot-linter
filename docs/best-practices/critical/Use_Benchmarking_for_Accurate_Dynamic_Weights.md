# Use Benchmarking for Accurate Dynamic Weights

**Severity**: <span style="color:red;">Critical</span>

> **Linter coverage — Partial.** [SEM011](../../rules/SEM011.md) flags zero weights in `#[pallet::weight]`, [RTC001](../../rules/RTC001.md) flags placeholder `WeightInfo` bindings in a runtime, [RTC002](../../rules/RTC002.md) flags runtime-local weights that `define_benchmarks!` never regenerates, and [SEC004](../../rules/SEC004.md) (off by default) flags non-saturating weight arithmetic. The linter does not check that every weight function or extrinsic has a benchmark: BEN001, BEN003, SEM005, SEM006, SEC005 and SEC018 were deleted; see [What we tried and removed](../../retired-rules.md).

## Description

Hardcoding weights for extrinsics in a blockchain can lead to significant inaccuracies in execution resource estimation. When weights are fixed, they may not reflect the actual execution costs or resource usage, resulting in either overestimation or underestimation.

A hardcoded weight might underestimate the cost of processing transactions with complex logic, resulting in unexpected execution costs and causing issues when building a block. Conversely, overestimated weights could prevent some transactions from executing, wasting network resources and limiting scalability.

## What should be avoided

Avoid using hardcoded weights directly in the function definition of extrinsics.

```rust
#[pallet::call_index(0)]
#[pallet::weight(
    // Hardcoded weights
    Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1)
)]
pub fn do_something(
    origin: OriginFor<T>,
    some_data: u32
) -> DispatchResult {
    // Extrinsic logic
}
```

In this example:

- The weight is fixed, leading to potential inaccuracies in resource estimation, which can result in suboptimal performance and affect transaction processing on the network.

## Best practice

Implement proper benchmarking to dynamically assess the weights of your functions. This process involves measuring the actual execution costs during test runs and then applying the generated weights in your extrinsic definitions.

```rust
// benchmarking.rs file
#[benchmark]
fn do_something() {
    // Setup
    // ...

    // Execution
	#[extrinsic_call]
	_(RawOrigin::Signed(account), 5u32);
}

// Execute the benchmarks and then add the corresponding WeightInfo
// to the extrinsic in the lib.rs file
#[pallet::call_index(0)]
#[pallet::weight(T::WeightInfo::do_something())]
pub fn do_something(
    origin: OriginFor<T>,
    some_data: u32
) -> DispatchResult {
   // Extrinsic logic
}
```

---

*Adapted from [Libro — Polkadot SDK Best Practices](https://libro.blockdeep.dev/critical/Use_Benchmarking_for_Accurate_Dynamic_Weights.html) by [BlockDeep](https://github.com/blockdeep/libro), used under the Apache-2.0 license. The **Linter coverage** note above is added by the polkadot-linter project.*
