use std::{
    env,
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

use crate::diagnostics::{Diagnostic, RuleCategory};

#[derive(Debug)]
pub struct RustcPipelineOptions {
    pub manifest_path: PathBuf,
    pub packages: Vec<String>,
    pub driver_path: PathBuf,
    pub toolchain: String,
    pub target_dir: Option<PathBuf>,
    pub rules: Vec<String>,
    pub file_filters: Vec<String>,
    pub lib: bool,
    pub no_default_features: bool,
    pub show_cargo_progress: bool,
}

#[derive(Debug)]
pub enum RustcPipelineError {
    Io(std::io::Error),
    Json {
        path: PathBuf,
        line: String,
        source: serde_json::Error,
    },
    CargoFailed {
        status: Option<i32>,
        stderr: String,
    },
    ToolchainProbe {
        toolchain: String,
        status: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for RustcPipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RustcPipelineError::Io(e) => write!(f, "{e}"),
            RustcPipelineError::Json { path, line, source } => write!(
                f,
                "failed to parse rustc diagnostic JSONL from {}: {source}; line: {line}",
                path.display()
            ),
            RustcPipelineError::CargoFailed { status, stderr } => {
                write!(
                    f,
                    "compiler-backed cargo check failed with status {}",
                    status
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".to_string())
                )?;
                if !stderr.trim().is_empty() {
                    write!(f, "\n{stderr}")?;
                }
                Ok(())
            }
            RustcPipelineError::ToolchainProbe {
                toolchain,
                status,
                stderr,
            } => write!(
                f,
                "failed to locate rustc compiler libraries for toolchain {toolchain} (status {}): {stderr}",
                status
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            ),
        }
    }
}

impl std::error::Error for RustcPipelineError {}

impl From<std::io::Error> for RustcPipelineError {
    fn from(value: std::io::Error) -> Self {
        RustcPipelineError::Io(value)
    }
}

#[derive(Debug, Deserialize)]
struct RustcDiagnostic {
    rule_id: String,
    rule_name: String,
    file: String,
    line: usize,
    column: usize,
    message: String,
}

pub fn run_cargo_check(
    options: &RustcPipelineOptions,
) -> Result<Vec<Diagnostic>, RustcPipelineError> {
    let jsonl_path = temporary_jsonl_path();
    let driver_path = absolutize(&options.driver_path)?;
    let manifest_path = absolutize(&options.manifest_path)?;
    let target_dir = options
        .target_dir
        .as_ref()
        .map(|path| absolutize(path))
        .transpose()?;
    let rustc_library_dir = rustc_library_dir(&options.toolchain)?;

    fs::File::create(&jsonl_path)?;

    let mut command = Command::new("cargo");
    command.arg(format!("+{}", options.toolchain));
    command.arg("check");
    command.arg("--manifest-path").arg(&manifest_path);
    for package in &options.packages {
        command.arg("-p").arg(package);
    }
    if options.lib {
        command.arg("--lib");
    }
    if options.no_default_features {
        command.arg("--no-default-features");
    }

    command
        .env("RUSTC_WORKSPACE_WRAPPER", &driver_path)
        .env("POLKADOT_LINTER_RUSTC_JSONL", &jsonl_path)
        .env(
            "POLKADOT_LINTER_RUSTC_MANIFEST_ROOT",
            manifest_path.parent().unwrap_or_else(|| Path::new(".")),
        )
        .stdout(Stdio::null());
    prepend_dynamic_library_path(&mut command, &rustc_library_dir);
    if !options.rules.is_empty() {
        command.env("POLKADOT_LINTER_RUSTC_RULES", options.rules.join(","));
    }
    if !options.file_filters.is_empty() {
        command.env(
            "POLKADOT_LINTER_RUSTC_FILE_CONTAINS",
            options.file_filters.join(","),
        );
    }
    if let Some(target_dir) = &target_dir {
        command.env("CARGO_TARGET_DIR", target_dir);
    }

    let (status, stderr) = if options.show_cargo_progress {
        // Cargo writes its build progress and compiler diagnostics to stderr. Keep it
        // attached to the caller so compiler-backed scans have normal Cargo feedback.
        command.stderr(Stdio::inherit());
        (command.status()?, String::new())
    } else {
        command.stderr(Stdio::piped());
        let output = command.output()?;
        (
            output.status,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    if !status.success() {
        let _ = fs::remove_file(&jsonl_path);
        return Err(RustcPipelineError::CargoFailed {
            status: status.code(),
            stderr,
        });
    }

    let diagnostics_result = read_jsonl_diagnostics(&jsonl_path);
    let _ = fs::remove_file(&jsonl_path);
    let mut diagnostics = diagnostics_result?;
    diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then(a.rule_id.cmp(&b.rule_id))
            .then(a.message.cmp(&b.message))
    });
    diagnostics.dedup_by(|a, b| {
        a.rule_id == b.rule_id
            && a.file == b.file
            && a.line == b.line
            && a.column == b.column
            && a.message == b.message
    });

    Ok(diagnostics.into_iter().map(Into::into).collect())
}

fn rustc_library_dir(toolchain: &str) -> Result<PathBuf, RustcPipelineError> {
    let output = Command::new("rustc")
        .arg(format!("+{toolchain}"))
        .args(["--print", "sysroot"])
        .output()?;
    if !output.status.success() {
        return Err(RustcPipelineError::ToolchainProbe {
            toolchain: toolchain.to_string(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).join("lib"))
}

fn prepend_dynamic_library_path(command: &mut Command, library_dir: &Path) {
    let variable = if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else if cfg!(target_os = "windows") {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let mut paths = vec![library_dir.to_path_buf()];
    if let Some(existing) = env::var_os(variable) {
        paths.extend(env::split_paths(&existing));
    }
    let joined = env::join_paths(paths).unwrap_or_else(|_| OsString::from(library_dir));
    command.env(variable, joined);
}

fn read_jsonl_diagnostics(path: &Path) -> Result<Vec<RustcDiagnostic>, RustcPipelineError> {
    let content = fs::read_to_string(path)?;
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<RustcDiagnostic>(line).map_err(|source| {
                RustcPipelineError::Json {
                    path: path.to_path_buf(),
                    line: line.to_string(),
                    source,
                }
            })
        })
        .collect()
}

fn temporary_jsonl_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "polkadot-linter-rustc-{}-{nanos}.jsonl",
        std::process::id()
    ))
}

fn absolutize(path: &Path) -> Result<PathBuf, RustcPipelineError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

impl From<RustcDiagnostic> for Diagnostic {
    fn from(value: RustcDiagnostic) -> Self {
        let severity = crate::rules::compiler_backed_rule_default_severity(&value.rule_id);
        Diagnostic {
            rule_id: value.rule_id,
            rule_name: value.rule_name,
            category: RuleCategory::Semantic,
            severity,
            file: PathBuf::from(value.file),
            line: value.line,
            column: Some(value.column),
            end_line: None,
            message: value.message,
            explanation: "Compiler-backed analysis used rustc-resolved types, expanded cfg/macro context, and HIR/typeck evidence for this diagnostic.".to_string(),
            suggestion: Some("Review the resolved compiler-backed finding and bound, meter, or handle the reported runtime path explicitly.".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::diagnostics::{Diagnostic, RuleCategory, Severity};

    use super::{read_jsonl_diagnostics, RustcDiagnostic};

    #[test]
    fn reads_jsonl_diagnostics_and_ignores_blank_lines() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("diagnostics.jsonl");
        fs::write(
            &path,
            r#"
{"rule_id":"SEC009","rule_name":"raw-arithmetic-in-fallible","file":"src/lib.rs","line":42,"column":9,"message":"raw arithmetic"}

"#,
        )
        .expect("fixture should be written");

        let diagnostics = read_jsonl_diagnostics(&path).expect("jsonl should parse");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "SEC009");
        assert_eq!(diagnostics[0].file, "src/lib.rs");
        assert_eq!(diagnostics[0].line, 42);
    }

    #[test]
    fn rejects_malformed_jsonl_with_source_line_context() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("diagnostics.jsonl");
        fs::write(&path, "not json\n").expect("fixture should be written");

        let error = read_jsonl_diagnostics(&path)
            .expect_err("malformed jsonl should report an error")
            .to_string();

        assert!(error.contains("not json"));
        assert!(error.contains("failed to parse rustc diagnostic JSONL"));
    }

    #[test]
    fn maps_rustc_diagnostic_to_public_diagnostic() {
        let diagnostic = Diagnostic::from(RustcDiagnostic {
            rule_id: "SEC003".to_string(),
            rule_name: "missing-decode-depth-limit".to_string(),
            file: "runtime/src/lib.rs".to_string(),
            line: 7,
            column: 3,
            message: "Recursive runtime type is decoded without a depth limit".to_string(),
        });

        assert_eq!(diagnostic.rule_id, "SEC003");
        assert_eq!(diagnostic.rule_name, "missing-decode-depth-limit");
        assert_eq!(diagnostic.category, RuleCategory::Semantic);
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(
            diagnostic.file,
            std::path::PathBuf::from("runtime/src/lib.rs")
        );
        assert_eq!(diagnostic.column, Some(3));
    }

    #[test]
    fn preserves_compiler_rule_default_severity() {
        let diagnostic = Diagnostic::from(RustcDiagnostic {
            rule_id: "SEM010".to_string(),
            rule_name: "xor-as-exponentiation".to_string(),
            file: "runtime/src/lib.rs".to_string(),
            line: 7,
            column: 3,
            message: "integer XOR looks like exponentiation".to_string(),
        });

        assert_eq!(diagnostic.severity, Severity::Error);
    }
}
