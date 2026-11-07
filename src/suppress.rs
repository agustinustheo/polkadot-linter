//! Inline `// polkadot-linter: allow(...)` comments and the findings baseline.
//!
//! Both run centrally on the merged diagnostics of the syntax engine and the
//! compiler-backed driver, so they apply to every rule alike.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::diagnostics::{Diagnostic, RuleCategory, Severity};

/// Rule reporting malformed, unknown-rule and unused allow comments.
pub const ALLOW_RULE_ID: &str = "LNT001";
const MARKER: &str = "// polkadot-linter:";
pub const BASELINE_VERSION: u32 = 1;

/// Locates diagnostic files: syntax paths are cwd-relative or absolute, the
/// driver reports paths relative to the Cargo workspace root.
pub struct FileResolver {
    pub workspace_dir: Option<PathBuf>,
}

impl FileResolver {
    /// Canonicalises first: the parent of a bare `Cargo.toml` is `""`, which
    /// has no ancestors to search.
    pub fn new(manifest_path: Option<&Path>) -> Self {
        let workspace_dir = manifest_path
            .and_then(|path| path.canonicalize().ok())
            .and_then(|path| path.parent().map(Path::to_path_buf));
        FileResolver { workspace_dir }
    }

    pub fn resolve(&self, file: &Path) -> Option<PathBuf> {
        let workspace = self.workspace_dir.iter().flat_map(|dir| dir.ancestors());
        std::iter::once(Path::new(""))
            .chain(workspace)
            .map(|dir| dir.join(file))
            .find(|path| path.is_file())
            .and_then(|path| path.canonicalize().ok())
    }
}

/// Drops diagnostics covered by a valid allow comment and returns `LNT001`
/// diagnostics for malformed and unused allows. `extra_files` are searched for
/// allows even without findings; only `active_rules` can be reported unused.
pub fn apply_allow_comments(
    diagnostics: &mut Vec<Diagnostic>,
    extra_files: &[PathBuf],
    resolver: &FileResolver,
    known_rules: &BTreeSet<String>,
    active_rules: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut by_file: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if let Some(path) = resolver.resolve(&diagnostic.file) {
            by_file.entry(path).or_default().push(index);
        }
    }
    for path in extra_files
        .iter()
        .filter_map(|file| file.canonicalize().ok())
    {
        by_file.entry(path).or_default();
    }

    let mut suppressed = BTreeSet::new();
    let mut lnt = Vec::new();
    for (path, indices) in by_file {
        let source = match path.extension() {
            Some(ext) if ext == "rs" => std::fs::read_to_string(&path).unwrap_or_default(),
            _ => continue,
        };
        if !source.contains(MARKER) {
            continue;
        }
        for (line, allow) in parse_allows(&source, known_rules) {
            let (rules, covers) = match allow {
                Ok(allow) => allow,
                Err(message) => {
                    lnt.push(lnt_diagnostic(&path, line, Severity::Warning, message));
                    continue;
                }
            };
            let mut used = BTreeSet::new();
            for &index in &indices {
                let diagnostic = &diagnostics[index];
                if rules.contains(&diagnostic.rule_id)
                    && (diagnostic.line == line || covers.contains(&diagnostic.line))
                {
                    used.insert(&diagnostic.rule_id);
                    suppressed.insert(index);
                }
            }
            let unused = rules
                .iter()
                .filter(|rule| active_rules.contains(*rule) && !used.contains(rule))
                .cloned()
                .collect::<Vec<_>>();
            if !unused.is_empty() {
                let message = format!(
                    "unused allow for {}: no finding on the covered lines; remove the comment",
                    unused.join(", ")
                );
                lnt.push(lnt_diagnostic(&path, line, Severity::Advisory, message));
            }
        }
    }
    let mut index = 0..;
    diagnostics.retain(|_| !suppressed.contains(&index.next().unwrap_or_default()));
    lnt
}

type ParsedAllow = Result<(Vec<String>, std::ops::RangeInclusive<usize>), String>;

/// Every allow comment as `(line, Ok((rules, covered lines)) | Err(message))`.
fn parse_allows(source: &str, known_rules: &BTreeSet<String>) -> Vec<(usize, ParsedAllow)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut allows = Vec::new();
    for (index, text) in lines.iter().enumerate() {
        // Skip the marker in doc comments (examples), code spans and strings.
        let quoted = |at: usize| text[..at].ends_with(['/', '!', '`', '"']);
        let doc = ["///", "//!"]
            .iter()
            .any(|p| text.trim_start().starts_with(p));
        let Some(at) = text.find(MARKER).filter(|&at| !doc && !quoted(at)) else {
            continue;
        };
        let line = index + 1;
        let allow = parse_allow_body(&text[at + MARKER.len()..], known_rules).map(|rules| {
            if !text.trim_start().starts_with("//") {
                return (rules, line..=line);
            }
            // On its own line: the next statement or declaration, past blank,
            // comment and attribute lines, up to the line ending it with `;`
            // or `,` or opening/closing a block; never into a block body.
            let code = |l: &str| l.split("//").next().unwrap_or_default().trim().to_string();
            let skip = |l: &str| code(l).is_empty() || l.trim_start().starts_with("#[");
            let Some(first) = (index + 1..lines.len()).find(|&i| !skip(lines[i])) else {
                return (rules, line..=line);
            };
            let ends = |l: &str| l.ends_with([';', ',']) || l.contains(['{', '}']);
            let last = (first..lines.len())
                .find(|&i| ends(&code(lines[i])))
                .unwrap_or(first);
            (rules, first + 1..=last + 1)
        });
        allows.push((line, allow));
    }
    allows
}

/// Parses ` allow(ID, ID, reason = "...")` into the rule IDs.
fn parse_allow_body(body: &str, known_rules: &BTreeSet<String>) -> Result<Vec<String>, String> {
    let inner = body
        .trim()
        .strip_prefix("allow(")
        .and_then(|rest| rest.rfind(')').map(|end| &rest[..end]))
        .ok_or("malformed allow comment; expected `allow(RULE, reason = \"...\")`")?;
    let (ids, reason) = inner.split_once("reason").unwrap_or((inner, ""));
    let reason = reason.trim().strip_prefix('=').unwrap_or_default().trim();
    let reason = reason.strip_prefix('"').and_then(|r| r.strip_suffix('"'));
    if reason.is_none_or(|reason| reason.trim().is_empty()) {
        return Err(
            "allow comment without a `reason = \"...\"`; it suppresses nothing until one is given"
                .to_string(),
        );
    }
    let rules = ids
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    match rules.iter().find(|id| !known_rules.contains(*id)) {
        _ if rules.is_empty() => Err("allow comment names no rule ID".to_string()),
        Some(unknown) => Err(format!(
            "allow comment names unknown rule `{unknown}`; it suppresses nothing"
        )),
        None => Ok(rules),
    }
}

/// `path` relative to the current directory when it lies below it.
fn relative_to_cwd(path: &Path) -> PathBuf {
    let cwd = std::env::current_dir().and_then(|cwd| cwd.canonicalize());
    cwd.ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
}

fn lnt_diagnostic(path: &Path, line: usize, severity: Severity, message: String) -> Diagnostic {
    Diagnostic {
        rule_id: ALLOW_RULE_ID.to_string(),
        rule_name: "allow-comment".to_string(),
        category: RuleCategory::Semantic,
        severity,
        file: relative_to_cwd(path),
        line,
        column: None,
        end_line: None,
        message,
        explanation: "`// polkadot-linter: allow(RULE, reason = \"...\")` must name known rules, give a reason, and cover a finding.".to_string(),
        suggestion: None,
    }
}

/// A finding identified by rule, cwd-relative file, the trimmed source line,
/// and its index among identical keys; not by line number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub rule_id: String,
    pub file: String,
    pub line: String,
    pub occurrence: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub version: u32,
    pub findings: Vec<BaselineEntry>,
}

/// Baseline keys for `diagnostics` (sorted by file and line), in order; `None`
/// (with a warning) when the flagged source line cannot be read or is blank,
/// so such a finding is never recorded or hidden.
fn baseline_entries(
    diagnostics: &[Diagnostic],
    resolver: &FileResolver,
) -> Vec<Option<BaselineEntry>> {
    let mut sources = HashMap::new();
    let mut seen = HashMap::new();
    diagnostics
        .iter()
        .map(|diagnostic| {
            let path = resolver.resolve(&diagnostic.file);
            let text = path.as_ref().and_then(|path| {
                let source = sources
                    .entry(path.clone())
                    .or_insert_with(|| std::fs::read_to_string(path).unwrap_or_default());
                let text = source.lines().nth(diagnostic.line.checked_sub(1)?)?.trim();
                (!text.is_empty()).then(|| text.to_string())
            });
            let (Some(path), Some(text)) = (path, text) else {
                eprintln!(
                    "polkadot-linter: baseline: cannot read line {} of {}; {} is never baselined",
                    diagnostic.line,
                    diagnostic.file.display(),
                    diagnostic.rule_id
                );
                return None;
            };
            let mut entry = BaselineEntry {
                rule_id: diagnostic.rule_id.clone(),
                file: relative_to_cwd(&path).to_string_lossy().replace('\\', "/"),
                line: text,
                occurrence: 0,
            };
            let occurrence = seen.entry(entry.clone()).or_insert(0);
            entry.occurrence = *occurrence;
            *occurrence += 1;
            Some(entry)
        })
        .collect()
}

/// Removes diagnostics recorded in `baseline` and returns how many it hid.
pub fn apply_baseline(
    diagnostics: &mut Vec<Diagnostic>,
    baseline: &Baseline,
    resolver: &FileResolver,
) -> usize {
    let known = baseline.findings.iter().collect::<BTreeSet<_>>();
    let entries = baseline_entries(diagnostics, resolver);
    let mut keep = entries
        .iter()
        .map(|entry| entry.as_ref().is_none_or(|entry| !known.contains(entry)));
    let before = diagnostics.len();
    diagnostics.retain(|_| keep.next().unwrap_or(true));
    before - diagnostics.len()
}

pub fn write_baseline(
    path: &Path,
    diagnostics: &[Diagnostic],
    resolver: &FileResolver,
) -> Result<usize, String> {
    let mut findings = baseline_entries(diagnostics, resolver)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    findings.sort();
    let count = findings.len();
    let baseline = Baseline {
        version: BASELINE_VERSION,
        findings,
    };
    let json = serde_json::to_string_pretty(&baseline).map_err(|error| error.to_string())?;
    std::fs::write(path, json + "\n").map_err(|error| error.to_string())?;
    Ok(count)
}

pub fn read_baseline(path: &Path) -> Result<Baseline, String> {
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let baseline: Baseline = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    match baseline.version {
        BASELINE_VERSION => Ok(baseline),
        version => Err(format!("unsupported baseline version {version}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(file: &Path, rule_id: &str, line: usize) -> Diagnostic {
        let mut d = lnt_diagnostic(file, line, Severity::Warning, String::new());
        d.rule_id = rule_id.to_string();
        d.file = file.to_path_buf();
        d
    }

    fn rules(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    const RESOLVER: FileResolver = FileResolver {
        workspace_dir: None,
    };

    /// Runs the allow filter on `source` with findings at `findings`, returning
    /// the surviving `(rule, line)` pairs and the LNT001 messages.
    fn run(source: &str, findings: &[(&str, usize)]) -> (Vec<(String, usize)>, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(&file, source).unwrap();
        let mut diagnostics = findings
            .iter()
            .map(|&(rule, line)| diag(&file, rule, line))
            .collect::<Vec<_>>();
        let known = rules(&["RTC004", "SEC020", "SEC008"]);
        let lnt = apply_allow_comments(
            &mut diagnostics,
            std::slice::from_ref(&file),
            &RESOLVER,
            &known,
            &known,
        );
        let kept = diagnostics
            .into_iter()
            .map(|d| (d.rule_id, d.line))
            .collect();
        (
            kept,
            lnt.into_iter()
                .map(|d| format!("{}:{}", d.line, d.message))
                .collect(),
        )
    }

    #[test]
    fn trailing_allow_covers_its_own_line_only() {
        let source = "const A: u32 = 0; // polkadot-linter: allow(RTC004, reason = \"dev\")\nconst B: u32 = 0;\n";
        let (kept, lnt) = run(source, &[("RTC004", 1), ("RTC004", 2)]);
        assert_eq!(kept, vec![("RTC004".to_string(), 2)]);
        assert!(lnt.is_empty(), "{lnt:?}");
    }

    #[test]
    fn standalone_allow_covers_next_code_line() {
        let source = "// polkadot-linter: allow(RTC004, reason = \"dev\")\n// other comment\n\nconst A: u32 = 0;\nconst B: u32 = 0;\n";
        let (kept, lnt) = run(source, &[("RTC004", 4), ("RTC004", 5)]);
        assert_eq!(kept, vec![("RTC004".to_string(), 5)]);
        assert!(lnt.is_empty(), "{lnt:?}");
    }

    #[test]
    fn allow_above_impl_covers_its_header_not_its_body() {
        let source = "// polkadot-linter: allow(RTC004, SEC020, reason = \"free by design\")\n/// Doc.\n#[cfg(test)]\nimpl Config for Runtime {\n    type A = ConstU128<0>;\n}\n";
        let (kept, lnt) = run(source, &[("RTC004", 4), ("RTC004", 5)]);
        assert_eq!(kept, vec![("RTC004".to_string(), 5)]);
        assert_eq!(
            lnt,
            vec!["1:unused allow for SEC020: no finding on the covered lines; remove the comment"]
        );
    }

    #[test]
    fn allow_after_doc_comment_covers_multi_line_associated_type() {
        let source = "impl pallet_balances::Config for Runtime {\n    /// The minimum balance.\n    // polkadot-linter: allow(RTC004, reason = \"dev chain\")\n    type ExistentialDeposit =\n        ConstU128<0>;\n    type Other = ConstU128<0>;\n}\n";
        let (kept, lnt) = run(source, &[("RTC004", 5), ("RTC004", 6)]);
        assert_eq!(kept, vec![("RTC004".to_string(), 6)]);
        assert!(lnt.is_empty(), "{lnt:?}");
    }

    #[test]
    fn allow_in_doc_comment_example_is_not_live() {
        let source = "/// // polkadot-linter: allow(RTC004, reason = \"example\")\n//! // polkadot-linter: allow(RTC004, reason = \"example\")\nconst A: u32 = 0;\n";
        let (kept, lnt) = run(source, &[("RTC004", 3)]);
        assert_eq!(kept, vec![("RTC004".to_string(), 3)]);
        assert!(lnt.is_empty(), "{lnt:?}");
    }

    #[test]
    fn allow_without_reason_or_with_unknown_rule_suppresses_nothing() {
        let source = "/// `// polkadot-linter: allow(RTC004)` in docs is ignored.\n// polkadot-linter: allow(RTC004)\nconst A: u32 = 0;\n// polkadot-linter: allow(RTC999, reason = \"x\")\nconst B: u32 = 0;\n// polkadot-linter: allow(RTC004, reason = \"\")\nconst C: u32 = 0;\n";
        let (kept, lnt) = run(source, &[("RTC004", 3), ("RTC004", 5), ("RTC004", 7)]);
        assert_eq!(kept.len(), 3);
        assert_eq!(lnt.len(), 3, "{lnt:?}");
        assert!(lnt[0].starts_with("2:allow comment without a `reason"));
        assert!(lnt[1].contains("unknown rule `RTC999`"));
    }

    #[test]
    fn unused_allow_is_reported_per_rule() {
        let source =
            "// polkadot-linter: allow(RTC004, SEC008, reason = \"dev\")\nconst A: u32 = 0;\n";
        let (kept, lnt) = run(source, &[("RTC004", 2)]);
        assert!(kept.is_empty());
        assert_eq!(
            lnt,
            vec!["1:unused allow for SEC008: no finding on the covered lines; remove the comment"]
        );
    }

    #[test]
    fn allow_suppresses_compiler_backed_finding_reported_relative_to_workspace() {
        // The driver reports paths relative to the Cargo workspace root.
        let workspace = tempfile::tempdir().unwrap();
        let file = workspace.path().join("pallets/x/src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "fn f() {\n    // polkadot-linter: allow(SEC020, reason = \"checked by caller\")\n    let _ = verify(sig);\n}\n").unwrap();
        let resolver = FileResolver {
            workspace_dir: Some(workspace.path().join("pallets/x")),
        };
        let mut diagnostics = vec![diag(Path::new("pallets/x/src/lib.rs"), "SEC020", 3)];
        let known = rules(&["SEC020"]);
        let lnt = apply_allow_comments(&mut diagnostics, &[], &resolver, &known, &known);
        assert!(
            diagnostics.is_empty() && lnt.is_empty(),
            "{diagnostics:?} {lnt:?}"
        );
    }

    #[test]
    fn baseline_round_trips_survives_line_shift_and_reports_new_findings() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(&file, "const A: u32 = 0;\nconst A: u32 = 0;\n").unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let findings = vec![diag(&file, "RTC004", 1), diag(&file, "RTC004", 2)];
        assert_eq!(
            write_baseline(&baseline_path, &findings, &RESOLVER).unwrap(),
            2
        );
        let baseline = read_baseline(&baseline_path).unwrap();
        assert_eq!(baseline.findings[1].line, "const A: u32 = 0;");
        assert_eq!(baseline.findings[1].occurrence, 1);

        // Lines shift down by two, one recorded line changes, and a finding
        // on an unreadable line is never hidden.
        std::fs::write(
            &file,
            "// new\n\nconst A: u32 = 0;\nconst A: u32 = 0;\nconst A: u32 = 1;\n",
        )
        .unwrap();
        let mut current = vec![
            diag(&file, "RTC004", 3),
            diag(&file, "RTC004", 4),
            diag(&file, "RTC004", 5),
            diag(&file, "RTC004", 2),
            diag(&dir.path().join("missing.rs"), "RTC004", 1),
        ];
        assert_eq!(apply_baseline(&mut current, &baseline, &RESOLVER), 2);
        let lines = current.iter().map(|d| d.line).collect::<Vec<_>>();
        assert_eq!(lines, vec![5, 2, 1]);
    }
}
