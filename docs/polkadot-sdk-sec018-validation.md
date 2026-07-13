# Polkadot SDK SEC018 Validation

Target: `.repos/polkadot-sdk` at `b18fb34a8ae348df5866e4b718d82871d744e60d`.

This is an archived validation record for the 13 findings produced by the
retired syntax benchmark. The public `SEC018` implementation now runs through
the rustc driver and CI validates its pinned package baselines directly. The
findings below remain useful audit context: each dispatchable accepts a
variable-length input while its declared weight omits that input length.

Suppressed candidates are not silently ignored: they are encoded as regression
tests for accepted-path validators, bounded conversions, max-block
post-dispatch weight, deprecated calls, mock/test-support paths, or documented
SDK test-support pallets.

Validation notes for the current baseline:

| Path | Extrinsic | Parameter | Validation |
| --- | --- | --- | --- |
| `cumulus/pallets/solo-to-para/src/lib.rs:70` | `schedule_migration` | `code` | Weight is `{0}` while `code` is passed into `schedule_code_upgrade`. |
| `cumulus/pallets/solo-to-para/src/lib.rs:70` | `schedule_migration` | `head_data` | Weight is `{0}` while `head_data` is stored for later validation-head application. |
| `polkadot/runtime/rococo/src/validator_manager.rs:72` | `register_validators` | `validators` | Fixed weight while the vector is cloned, appended item-by-item, and emitted in an event. |
| `polkadot/runtime/rococo/src/validator_manager.rs:89` | `deregister_validators` | `validators` | Fixed weight while the vector is cloned, appended item-by-item, and emitted in an event. |
| `substrate/frame/contracts/src/lib.rs:938` | `call` | `data` | Audit-validated: contract call data length is absent from the weight formula. |
| `substrate/frame/multisig/src/lib.rs:322` | `as_multi_threshold_1` | `other_signatories` | Audit-validated: signatory count is checked after decoding and is absent from the weight formula. |
| `substrate/frame/node-authorization/src/lib.rs:388` | `add_connections` | `connections` | Weight ignores vector length while the body iterates over all connections and emits them. |
| `substrate/frame/node-authorization/src/lib.rs:423` | `remove_connections` | `connections` | Weight ignores vector length while the body iterates over all connections. |
| `substrate/frame/people/src/lib.rs:981` | `force_recognize_personhood` | `people` | No proportional weight while the body loops over every person and reserves IDs. |
| `substrate/frame/revive/src/lib.rs:1163` | `call` | `data` | Weight accounts for execution limit but not input data length passed into `bare_call`. |
| `substrate/frame/revive/src/lib.rs:1413` | `eth_call` | `data` | Weight accounts for transaction encoding length but not the separate call data passed into `bare_call`. |
| `substrate/frame/society/src/lib.rs:1114` | `found_society` | `rules` | Audit-validated: the vector is hashed with fixed weight. |
| `substrate/frame/state-trie-migration/src/lib.rs:785` | `migrate_custom_child` | `root` | Weight accounts for child key count and total size, but not root length before root transformation. |
