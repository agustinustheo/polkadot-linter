# Validation Order Rules (VAL)

Validation rules enforce the FRAME principle of *verify first, write last*: cheap checks must run before heavy storage reads, state must not be written before the call is fully validated, and divisors must be proven nonzero before use.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`VAL001`](VAL001.md) | Heavy Storage Reads Before Cheap Validation | Warning | source (syn) |
| [`VAL002`](VAL002.md) | Division by Unproven Config, Storage, or Collection Divisors | Warning | compiler-backed |
| [`VAL003`](VAL003.md) | Storage Writes Before Validation | Warning | compiler-backed |
