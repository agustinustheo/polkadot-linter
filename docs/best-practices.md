# Polkadot SDK Best Practices

This guide adapts the structure of [Libro's Polkadot SDK best-practices
book](https://libro.blockdeep.dev/) for `polkadot-linter`. It is a navigation
and enforcement guide, not a replacement for Libro's explanations and
examples. The upstream material is maintained in
[blockdeep/libro](https://github.com/blockdeep/libro) and is Apache-2.0
licensed.

Each practice is deliberately classified as one of:

- **Enforced**: a current linter rule has evidence strong enough to report by
  default.
- **Partial**: a rule checks a concrete, high-signal subset; reviewers must
  still assess intent and system context.
- **Review gate**: the recommendation requires chain policy, runtime
  configuration, economic assumptions, or whole-program evidence that static
  analysis cannot establish reliably.

## Critical

| Practice | Linter coverage | What the check proves |
| --- | --- | --- |
| Appropriate origin checks | Enforced subset: `SEC020` | Finds a narrow authorization bug: not using or validating `ensure_signed_or_root` before writing storage keyed by a caller-supplied account. Privilege policy still requires review. |
| Avoid unbounded iteration | Enforced: `SEC011`, `SEC012` | Resolved storage iteration and unbounded `clear_prefix` calls reachable from runtime entry points. |
| Validate input | Partial: `VAL001`, `VAL002`, `VAL003`, `SEC001`, `SEC003`, `SEC018` | Validation order, zero divisors, premature writes, unbounded vectors, depth-limited decode, and weight accounting. |
| Avoid runtime unwraps | Enforced: `SEC002`, `SEC008` | Reachable debug assertions and panic paths such as `unwrap`, `expect`, `panic!`, and `todo!`. |
| Benchmark dynamic weights | Enforced: `BEN001`, `BEN003`, `SEC004`, `SEC005`, `SEC018`, `SEM006` | Benchmark identity, resolved weight calls, unsafe arithmetic, expensive weight expressions, input accounting, and PoV omissions. |
| Prefer reserve transfers to teleports | Review gate | XCM asset-trust policy cannot be inferred safely from a call site. |

## High Severity

| Practice | Linter coverage | What remains for review |
| --- | --- | --- |
| Benchmark worst-case extrinsics | Partial: `BEN001`, `BEN002`, `BEN003`, `SEC018` | The linter proves benchmark presence and some input accounting, not that benchmark inputs cover every worst-case state. |
| Keep dependencies current | Review gate | Run dependency and advisory tooling against the project's declared update policy. |
| Avoid pseudo-random selection | Enforced subset: `SEC019` | Detects the resolved external `rand::random` function in reachable runtime and pallet code; entropy quality and fairness still require design review. |
| Use safe arithmetic | Enforced: `SEC004`, `SEC009`, `VAL002` | Resolved raw arithmetic in fallible code, unsafe weight arithmetic, and unguarded division. |
| Bound storage growth | Enforced: `SEC001`, `SEC013`, `SEC017`, `SEC018` | Unbounded public inputs, storage aliases, event payloads, and missing input-weight accounting. |
| Distribute finalization cost | Partial: `SEC011`, `SEC012` | Finds unbounded storage work but cannot prove economic scheduling is fair. |
| Make multi-write hooks transactional | Enforced subset: `SEC010` | Lifecycle hooks with multiple storage writes need a transaction layer; non-hook atomicity is contextual. |
| Avoid redundant storage work | Enforced subset: `SEM009` | Finds `contains_key` immediately before resolved `remove`/`take`; mutation-specific redundancy remains a review item. |
| Add try-state checks | Review gate | A useful `try_state` hook depends on pallet invariants and migration design. |
| Configure XCM barriers and junctions safely | Review gate | This needs runtime composition, trusted-origin policy, and protocol-specific intent. |
| Follow host-chain asset schemas | Review gate | Correctness is relative to the host chain's published schema. |

## Medium Severity

| Practice | Linter coverage | What remains for review |
| --- | --- | --- |
| Prefer `try_append` for a simple append | Review gate | A mutator may perform validation or other updates; a syntactic conversion would be unsafe. |
| Avoid deprecated storage getters | Enforced: `SEM017` | Exact authored `#[pallet::getter]` attributes are reported. |
| Avoid hardcoded parameters and configurable bounds | Partial: `SEC001`, `SEC013`, `SEC018` | Bounded data and input costs are checked; whether a constant belongs in `Config` is an API decision. |
| Test edge and error paths | Partial: `TST001`, `TST004`, `TST006`, `TST007` | Rollback assertion style, fee error-path companions, observable events, and success-only dispatch tests. |
| Document extrinsics, errors, events, and Config | Enforced presence: `DOC001` | Public FRAME API items and variants must have rustdoc; documentation accuracy and usefulness remain review work. |
| Avoid large files and complex functions | Review gate | Complexity thresholds need project-specific policy. |
| Use efficient data structures and avoid duplication | Review gate | Performance and data ownership depend on workload and storage access patterns. |
| Replace magic numbers with constants | Enforced subset: `SEM018` | Flags dispatchable primitive inputs immediately converted to runtime-associated scalar types; broader protocol-constant design remains contextual. |
| Keep hooks cheap | Enforced subset: `SEC010`, `SEC011`, `SEC012` | Transactional layers and unbounded storage work are checked; bounded work still needs benchmark review. |
| Move away from deprecated `Currency` and use correct XCM fees | Review gate | Trait migration and fee economics depend on the SDK version and runtime configuration. |

## Low and Informational

| Practice | Linter coverage | What the check proves |
| --- | --- | --- |
| Naming, terminology, and spelling | Enforced subset: `TRM001` | Configured spelling and terminology conventions in authored text. |
| Avoid unnecessary clones and generic repetition | Review gate | Requires ownership, allocation, and readability context. |
| Avoid hardcoded error strings; use descriptive logging | Review gate | Error and logging conventions are project policy. |
| Prefer enums for optional input and remove unnecessary return values | Review gate | API semantics cannot be classified from syntax alone. |
| Keep benchmarks current, expose runtime APIs, remove unused code | Partial: `SEM012` | Production `#[allow(dead_code)]` suppressions are reported; the remaining practices need release and API review. |
| Keep documentation consistent and backend frontend-agnostic | Review gate | These are architecture and documentation standards rather than local code facts. |

## Using This Guide

Run the linter alongside formatting, Clippy, dependency checks, benchmark
review, and runtime integration tests. A clean lint run means only that the
enforced evidence is absent; it does not satisfy the review gates above.

When adding a new Libro-derived rule, require a precise rule statement, a
focused positive and negative fixture, and a pinned Polkadot SDK benchmark if
the pattern exists upstream. Use rustc for resolved calls, types, trait
implementations, macro-expanded code, and control/data-flow facts. Keep a
source rule only when its evidence is inherently authored text or attributes,
and document why a rustc implementation would be weaker.
