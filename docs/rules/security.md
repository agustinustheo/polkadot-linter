# Security Rules (SEC)

Security rules target patterns that put a live chain at risk: weight-exhaustion and storage-bloat denial-of-service vectors, panics that can stall block production, origin-check bypasses, and unsafe migrations. All SEC rules are compiler-backed — the rustc driver is the authority, giving them resolved-type evidence rather than text matching.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`SEC001`](SEC001.md) | Unbounded Vec in Extrinsic Parameters | Warning | compiler-backed |
| [`SEC002`](SEC002.md) | Debug Assertions Reachable from Runtime Entry Points | Warning | compiler-backed |
| [`SEC003`](SEC003.md) | Decoding Recursive Runtime Types Without a Depth Limit | Warning | compiler-backed |
| [`SEC004`](SEC004.md) | Non-Saturating Arithmetic in Weight Attributes | Warning | compiler-backed |
| [`SEC005`](SEC005.md) | Expensive Operations in Weight Calculation | Warning | compiler-backed |
| [`SEC006`](SEC006.md) | Unchecked repatriate_reserved Remaining Balance | Warning | compiler-backed |
| [`SEC007`](SEC007.md) | Discarded Result via Let-Underscore | Warning | compiler-backed |
| [`SEC008`](SEC008.md) | Panics in Production Runtime Code | Warning | compiler-backed |
| [`SEC009`](SEC009.md) | Raw Integer Arithmetic in Fallible Functions | Advisory | compiler-backed |
| [`SEC010`](SEC010.md) | Missing Transactional Layer in FRAME Hooks | Warning | compiler-backed |
| [`SEC011`](SEC011.md) | Unbounded Storage Iteration in Callable Runtime Paths | Warning | compiler-backed |
| [`SEC012`](SEC012.md) | Unbounded clear_prefix Deletion Limit | Warning | compiler-backed |
| [`SEC013`](SEC013.md) | Unbounded Collections in Storage Items | Warning | compiler-backed |
| [`SEC014`](SEC014.md) | Identity Hasher on Common Key Types | Warning | compiler-backed |
| [`SEC015`](SEC015.md) | Unguarded dispatch_bypass_filter in Production Code | Warning | compiler-backed |
| [`SEC016`](SEC016.md) | Runtime Upgrade Writes Storage Without a StorageVersion Check | Warning | compiler-backed |
| [`SEC017`](SEC017.md) | Unbounded Vec Fields in Pallet Events | Warning | compiler-backed |
| [`SEC018`](SEC018.md) | Weight Does Not Account for Unbounded Extrinsic Input | Warning | compiler-backed |
