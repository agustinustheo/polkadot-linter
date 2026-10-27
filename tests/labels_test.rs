//! Benchmark labels are compared against a `--syntax-only` scan, so every
//! labelled rule must be one the syntax engine registers.

use std::fs;
use std::path::Path;

#[test]
fn benchmark_labels_name_syntax_engine_rules() {
    // The scan uses config/all-rules.toml, which also enables the default-off rules.
    let config = polkadot_linter::config::Config::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("config/all-rules.toml"),
    )
    .expect("config/all-rules.toml is valid");
    let registered = polkadot_linter::rules::all_rules(&config)
        .into_iter()
        .map(|rule| rule.id().to_string())
        .collect::<Vec<_>>();
    let labels_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/labels");

    let mut errors = Vec::new();
    let mut files = 0;
    for entry in fs::read_dir(&labels_dir).expect("benchmarks/labels exists") {
        let path = entry.expect("readable directory entry").path();
        if path.extension().is_none_or(|ext| ext != "tsv") {
            continue;
        }
        files += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let content = fs::read_to_string(&path).expect("readable label file");
        for (index, row) in content.lines().enumerate().skip(1) {
            let Some(rule_id) = row.split('\t').next().filter(|id| !id.trim().is_empty()) else {
                continue;
            };
            let line = index + 1;
            if polkadot_linter::rules::COMPILER_BACKED_RULE_IDS.contains(&rule_id) {
                errors.push(format!(
                    "{name}:{line}: {rule_id} is compiler-backed and never runs under --syntax-only"
                ));
            } else if !registered.iter().any(|id| id == rule_id) {
                errors.push(format!("{name}:{line}: unknown rule_id {rule_id}"));
            }
        }
    }
    assert!(files > 0, "no label files in {}", labels_dir.display());
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn all_rules_config_enables_every_rule() {
    let config = polkadot_linter::config::Config::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("config/all-rules.toml"),
    )
    .expect("config/all-rules.toml is valid");
    // With no [rules.enabled] entries, all_rules() registers every source rule.
    let mut unfiltered = config.clone();
    unfiltered.rules.enabled.clear();
    let source_ids = polkadot_linter::rules::all_rules(&unfiltered)
        .into_iter()
        .map(|rule| rule.id().to_string());
    let compiler_ids = polkadot_linter::rules::COMPILER_BACKED_RULE_IDS
        .iter()
        .map(|id| id.to_string());
    let disabled = source_ids
        .chain(compiler_ids)
        .filter(|id| !config.rule_enabled(id))
        .collect::<Vec<_>>();
    assert!(
        disabled.is_empty(),
        "config/all-rules.toml leaves {disabled:?} disabled"
    );
}
