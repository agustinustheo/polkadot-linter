# Security Rules (SEC)

Security rules target patterns that put a live chain at risk: unbounded storage work in callable paths, unchecked balance moves, filter and origin-check bypasses, and verification or funds-handling mistakes copied from real incidents. All SEC rules are compiler-backed — the rustc driver is the authority, giving them resolved-type evidence rather than text matching.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`SEC004`](SEC004.md) | Non-Saturating Arithmetic in Weight Attributes | Advisory, off by default | compiler-backed |
| [`SEC006`](SEC006.md) | Unchecked repatriate_reserved Remaining Balance | Warning | compiler-backed |
| [`SEC010`](SEC010.md) | Missing Transactional Layer in FRAME Hooks | Warning | compiler-backed |
| [`SEC011`](SEC011.md) | Unbounded Storage Iteration in Callable Runtime Paths | Warning | compiler-backed |
| [`SEC012`](SEC012.md) | Unbounded clear_prefix Deletion Limit | Warning | compiler-backed |
| [`SEC015`](SEC015.md) | Unguarded dispatch_bypass_filter in Production Code | Warning | compiler-backed |
| [`SEC019`](SEC019.md) | Token Imbalance Escapes a Transactional Storage Layer | Error | compiler-backed |
| [`SEC020`](SEC020.md) | Discarded Verification Verdict | Error | compiler-backed |
| [`SEC021`](SEC021.md) | Inverted Verification Guard | Error | compiler-backed |
| [`SEC022`](SEC022.md) | Discarded Origin Check | Error | compiler-backed |
| [`SEC023`](SEC023.md) | Option Equality in an Origin Check | Error | compiler-backed |
| [`SEC024`](SEC024.md) | Discarded Funds in an Error | Error, off by default | compiler-backed |

Retired rules of this family are listed in [What we tried and removed](../retired-rules.md).
