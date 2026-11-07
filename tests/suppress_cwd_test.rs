//! Changes the process working directory, so it lives in its own test binary.

use std::path::Path;

use polkadot_linter::suppress::{read_baseline, write_baseline, FileResolver};

#[test]
fn relative_manifest_path_resolves_workspace_paths_and_baseline_is_cwd_relative() {
    let workspace = tempfile::tempdir().unwrap();
    let member = workspace.path().join("member");
    std::fs::create_dir_all(member.join("src")).unwrap();
    std::fs::write(member.join("Cargo.toml"), "[package]\n").unwrap();
    std::fs::write(member.join("src/lib.rs"), "const A: u32 = 0;\n").unwrap();
    std::env::set_current_dir(&member).unwrap();

    // The driver reports `member/src/lib.rs`, relative to the workspace root.
    let resolver = FileResolver::new(Some(Path::new("Cargo.toml")));
    let resolved = resolver.resolve(Path::new("member/src/lib.rs"));
    assert_eq!(
        resolved,
        Some(member.join("src/lib.rs").canonicalize().unwrap())
    );

    let mut diagnostic = polkadot_linter::diagnostics::Diagnostic {
        rule_id: "RTC004".to_string(),
        rule_name: String::new(),
        category: polkadot_linter::diagnostics::RuleCategory::RuntimeConfig,
        severity: polkadot_linter::diagnostics::Severity::Warning,
        file: "member/src/lib.rs".into(),
        line: 1,
        column: None,
        end_line: None,
        message: String::new(),
        explanation: String::new(),
        suggestion: None,
    };
    let baseline_path = workspace.path().join("baseline.json");
    write_baseline(&baseline_path, std::slice::from_ref(&diagnostic), &resolver).unwrap();
    let entry = &read_baseline(&baseline_path).unwrap().findings[0];
    assert_eq!(
        (entry.file.as_str(), entry.line.as_str()),
        ("src/lib.rs", "const A: u32 = 0;")
    );
    diagnostic.line = 2;
    write_baseline(&baseline_path, &[diagnostic], &resolver).unwrap();
    assert!(read_baseline(&baseline_path).unwrap().findings.is_empty());
}
