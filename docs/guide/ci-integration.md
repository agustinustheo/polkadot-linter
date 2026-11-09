# CI and AI agents

## CI

Run the linter after the standard Rust tools:

```text
cargo fmt -> cargo clippy -> polkadot-linter
```

This step fails the build on errors and warnings.:

```bash
polkadot-linter --manifest-path Cargo.toml -s warning --fail-on-warning ./runtime ./pallets
```

For GitHub code scanning, write SARIF and upload the file:

```bash
polkadot-linter --manifest-path Cargo.toml -f sarif ./runtime ./pallets > polkadot-linter.sarif
```

To add the linter to an existing codebase, start with a
[baseline](suppressing-findings.md#baseline-file). Then CI fails only on new
findings.

## AI coding agents

[`skill/SKILL.md`](https://github.com/agustinustheo/polkadot-linter/blob/main/skill/SKILL.md)
tells an agent when to run the linter, which commands to use, how to read the
JSON, and how to triage each finding.

- **Claude Code:** copy the skill into your skills directory:

  ```bash
  mkdir -p ~/.claude/skills/polkadot-linter
  curl -fsSL https://raw.githubusercontent.com/agustinustheo/polkadot-linter/main/skill/SKILL.md -o ~/.claude/skills/polkadot-linter/SKILL.md
  ```

- **Codex and other agents:** add this line to `AGENTS.md` or to the agent's
  instructions: *"For Polkadot SDK, FRAME or runtime code, run
  `polkadot-linter` and triage each finding as described in
  https://github.com/agustinustheo/polkadot-linter/blob/main/skill/SKILL.md."*

The linter does not call an LLM.
