# Security rules (SEC)

SEC rules find pallet and XCM code that can lose funds, skip a check or do
unbounded work. Most copy the shape of a real incident. All SEC rules use the
compiler engine, so they need the [driver](../guide/installation.md#the-compiler-driver).

| Rule | Finds | Severity |
| --- | --- | --- |
| [SEC004](SEC004.md) | Non-saturating weight arithmetic (off by default) | warning |
| [SEC006](SEC006.md) | Unchecked `repatriate_reserved` remainder | warning |
| [SEC010](SEC010.md) | Hook writes storage without a storage layer | warning |
| [SEC011](SEC011.md) | Storage iteration in a dispatchable or hook | warning |
| [SEC012](SEC012.md) | Unbounded `clear_prefix` | warning |
| [SEC015](SEC015.md) | `dispatch_bypass_filter` in production code | warning |
| [SEC019](SEC019.md) | Token imbalance escapes a storage layer | error |
| [SEC020](SEC020.md) | Discarded verification verdict | error |
| [SEC021](SEC021.md) | Inverted verification guard | error |
| [SEC022](SEC022.md) | Discarded origin check | error |
| [SEC023](SEC023.md) | `Option` equality in an origin check | error |
| [SEC024](SEC024.md) | Discarded funds in an error (off by default) | error |
