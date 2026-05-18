use regex::Regex;

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

// ---------------------------------------------------------------------------
// TRM001: Spelling conventions (Google English)
// ---------------------------------------------------------------------------

pub struct SpellingConventions;

impl LintRule for SpellingConventions {
    fn id(&self) -> &str {
        "TRM001"
    }
    fn name(&self) -> &str {
        "spelling-conventions"
    }
    fn family(&self) -> &str {
        "terminology"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust && !ctx.is_text {
            return None;
        }

        if config.terminology.british_english.is_empty()
            && config.terminology.forbidden_terms.is_empty()
        {
            return None;
        }

        let mut diagnostics = Vec::new();
        let string_literal_re = Regex::new(r#""([^"\\]*(\\.[^"\\]*)*)""#).unwrap();

        for (i, line) in ctx.content.lines().enumerate() {
            let trimmed = line.trim();

            // Determine what kind of content this line is
            let is_comment = trimmed.starts_with("//")
                || trimmed.starts_with("///")
                || trimmed.starts_with("//!");
            let is_doc_attr = trimmed.starts_with("#[doc");
            let is_text_file = ctx.is_text;

            // Skip SPDX license headers
            if trimmed.contains("SPDX")
                || trimmed.contains("Copyright")
                || trimmed.contains("Licensed under")
            {
                continue;
            }

            // FIX #8: Extract only the checkable text portions, not the whole line.
            // For code lines, we need to isolate comments and string literals separately.
            let texts_to_check: Vec<&str> = if is_text_file {
                vec![trimmed]
            } else if is_comment {
                let content = trimmed
                    .trim_start_matches("///")
                    .trim_start_matches("//!")
                    .trim_start_matches("//")
                    .trim();
                vec![content]
            } else if is_doc_attr {
                vec![trimmed]
            } else {
                // For code lines, extract only string literal contents and inline comments
                let mut parts = Vec::new();

                // Extract inline trailing comment if present
                if let Some(comment_start) = find_line_comment(trimmed) {
                    parts.push(&trimmed[comment_start + 2..]);
                }

                // Extract string literal contents if configured
                if config.terminology.check_strings {
                    for cap in string_literal_re.captures_iter(trimmed) {
                        if let Some(m) = cap.get(1) {
                            parts.push(m.as_str());
                        }
                    }
                }

                if parts.is_empty() && config.terminology.check_identifiers {
                    vec![trimmed]
                } else {
                    parts
                }
            };

            if texts_to_check.is_empty() {
                continue;
            }

            // Check spelling conventions
            for (forbidden, preferred) in &config.terminology.british_english {
                let pattern = format!(r"\b{}\b", regex::escape(forbidden));
                if let Ok(re) = Regex::new(&pattern) {
                    let found = texts_to_check
                        .iter()
                        .any(|t| re.is_match(&t.to_lowercase()));
                    if found {
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            rule_name: self.name().to_string(),
                            category: RuleCategory::Terminology,
                            severity: config.rule_severity(self.id(), Severity::Advisory),
                            file: ctx.path.clone(),
                            line: i + 1,
                            column: None,
                            end_line: None,
                            message: format!(
                                "Non-standard spelling `{}` — prefer `{}`",
                                forbidden, preferred
                            ),
                            explanation: "Project convention: use Google English spelling \
                                in all code, comments, documentation, and string literals."
                                .to_string(),
                            suggestion: Some(format!(
                                "Replace `{}` with `{}`",
                                forbidden, preferred
                            )),
                        });
                        break;
                    }
                }
            }

            // Check project-specific forbidden terms
            for (forbidden, preferred) in &config.terminology.forbidden_terms {
                let pattern = format!(r"\b{}\b", regex::escape(forbidden));
                if let Ok(re) = Regex::new(&pattern) {
                    let found = texts_to_check.iter().any(|t| re.is_match(t));
                    if found {
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            rule_name: self.name().to_string(),
                            category: RuleCategory::Terminology,
                            severity: config.rule_severity(self.id(), Severity::Advisory),
                            file: ctx.path.clone(),
                            line: i + 1,
                            column: None,
                            end_line: None,
                            message: format!(
                                "Forbidden term `{}` — use `{}` instead",
                                forbidden, preferred
                            ),
                            explanation: "Project-specific terminology convention.".to_string(),
                            suggestion: Some(format!(
                                "Replace `{}` with `{}`",
                                forbidden, preferred
                            )),
                        });
                    }
                }
            }
        }

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

/// Find the position of a line comment (`//`) that is NOT inside a string literal.
fn find_line_comment(line: &str) -> Option<usize> {
    let mut in_string = false;
    let mut prev = ' ';
    let chars: Vec<char> = line.chars().collect();

    for i in 0..chars.len() {
        if chars[i] == '"' && prev != '\\' {
            in_string = !in_string;
        }
        if !in_string && chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            return Some(i);
        }
        prev = chars[i];
    }
    None
}
