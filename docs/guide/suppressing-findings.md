# Suppressing Findings

Some findings are correct but not worth acting on: a zero deposit on a dev
chain, a check the caller already performs. You can record "we reviewed this,
it is intentional" so the finding stops showing up. Prefer fixing the code;
suppress only after review, and always say why.

There are two mechanisms:

- **Inline allow comments**, for individual findings you have reviewed.
- **A baseline file**, for adopting the linter on an existing codebase: record
  today's findings once, then only new findings are reported.

Both apply to every rule, from the syntax engine and the compiler-backed
driver alike, after their results are merged.

## Inline allow comments

```rust
impl pallet_balances::Config for Runtime {
    /// The minimum balance.
    // polkadot-linter: allow(RTC004, reason = "dev chain: accounts are free by design")
    type ExistentialDeposit =
        ConstU128<0>;
}
```

The syntax is `// polkadot-linter: allow(RULE, ..., reason = "...")`. Several
rule IDs can be listed: `allow(RTC004, SEC020, reason = "...")`.

What a comment covers:

- **Trailing comment**: the line it is on.

  ```rust
  pub const EXISTENTIAL_DEPOSIT: Balance = 0; // polkadot-linter: allow(RTC004, reason = "testnet")
  ```

- **Comment on its own line**: the next statement or declaration. Blank,
  comment, doc-comment and `#[...]` attribute lines are skipped; coverage then
  runs from that line to the first line that ends the statement (with `;` or
  `,`) or opens or closes a block (`{`, `}`). So a `type X =`
  whose value is on the next line is covered, but an allow above an `impl`,
  `mod` or `parameter_types! {` covers only the header line, never the block
  body: put the allow next to the finding.

  ```rust
  fn submit(origin: OriginFor<T>, sig: Signature) -> DispatchResult {
      // polkadot-linter: allow(SEC020, reason = "verified in validate_unsigned")
      let _ = verify(&sig);
      Ok(())
  }
  ```

Rules for allow comments, enforced by rule `LNT001`:

- `reason` is required and must be a non-empty string. An allow without one
  suppresses nothing and is reported as a warning.
- Every rule ID must exist. An allow naming an unknown rule (a typo, a rule
  that was removed) suppresses nothing and is reported as a warning.
- An allow that suppresses nothing is reported as an advisory "unused allow",
  so stale comments get cleaned up once the code changes. Only rules that ran
  are checked: with `--syntax-only` or `--rules`, an allow for a rule that did
  not run is not reported as unused.

Markers inside doc comments (`/// // polkadot-linter: ...`, `//! ...`) are
examples, not allows. Allow comments only work in `.rs` files. Findings in other files, such as
`RTC009` (vulnerable crate versions in `Cargo.lock`), can only be hidden with
a baseline. Disable `LNT001` with `LNT001 = false` under `[rules.enabled]` if
you do not want these checks.

## Baseline file

```bash
# Record every current finding (all severities) and exit 0.
polkadot-linter --write-baseline polkadot-linter-baseline.json .

# Later runs: hide recorded findings; only new ones are printed and affect the exit code.
polkadot-linter --baseline polkadot-linter-baseline.json .
```

The second command prints one line on stderr such as
`polkadot-linter: baseline polkadot-linter-baseline.json hid 42 known finding(s)`.
The JSON and SARIF outputs contain only the findings that remain.

The format is plain JSON:

```json
{
  "version": 1,
  "findings": [
    {
      "rule_id": "RTC004",
      "file": "runtime/src/lib.rs",
      "line": "type ExistentialDeposit = ConstU128<0>;",
      "occurrence": 0
    }
  ]
}
```

A finding is identified by its rule ID, its file path relative to the current
directory, the trimmed text of the flagged source line, and an occurrence index
that tells apart identical lines in the same file. A finding whose source line
cannot be read is never recorded or hidden (a warning is printed on stderr). Line
numbers are not stored, so edits elsewhere in the file do not invalidate the
baseline. Editing the flagged line itself makes the finding new again, which is
usually what you want: the code changed, so review it again.

Run the linter from the same directory when writing and using a baseline, so
the relative paths match. Commit the baseline file and
shrink it over time: fix findings, then rewrite it.
