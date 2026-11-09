# Suppress findings and baselines

Fix the code first. Suppress a finding only after you review it, and always
give a reason. There are two methods:

- An **allow comment** hides one finding that you reviewed.
- A **baseline file** hides all findings that exist today. Use it when you add
  the linter to an existing codebase.

Both methods work for all rules and both engines.

## Allow comments

```rust
impl pallet_balances::Config for Runtime {
    // polkadot-linter: allow(RTC004, reason = "dev chain: accounts are free by design")
    type ExistentialDeposit = ConstU128<0>;
}
```

The syntax is `// polkadot-linter: allow(RULE, ..., reason = "...")`. You can
name more than one rule: `allow(RTC004, SEC020, reason = "...")`.

What a comment covers:

- **At the end of a line:** that line.
- **On its own line:** the next statement. The linter skips blank lines,
  comments, doc comments and `#[...]` attributes. The statement ends at the
  first line that ends with `;` or `,`, or that opens or closes a block.

An allow above `impl`, `mod` or `parameter_types! {` covers only that first
line, not the block. Put the allow next to the finding.

[LNT001](../rules/LNT001.md) checks allow comments:

- An allow with no reason suppresses nothing. LNT001 reports it as a warning.
- An allow for an unknown rule suppresses nothing. LNT001 reports it as a
  warning.
- An allow that hides no finding is "unused". LNT001 reports it as an advisory.
  It checks only the rules that ran.

Allow comments work only in `.rs` files. Text inside doc comments is not an
allow. To hide a finding in another file, such as RTC009 in `Cargo.lock`, use a
baseline.

## Baseline file

```bash
# 1. Record all current findings. The command exits with code 0.
polkadot-linter --write-baseline polkadot-linter-baseline.json .

# 2. Later runs: hide the recorded findings. Only new findings are reported.
polkadot-linter --baseline polkadot-linter-baseline.json .
```

The second command prints the number of hidden findings on stderr. Only the
findings that remain go into the output and the exit code.

The baseline stores the rule ID, the file path, the text of the flagged line
and an occurrence number. It does not store line numbers. So:

- Edits in other parts of the file do not change the baseline.
- An edit to the flagged line makes the finding new again. Review it again.

Run the linter from the same directory each time, because the paths are
relative. Commit the baseline file. Fix findings over time, then write the
baseline again so it gets smaller.
