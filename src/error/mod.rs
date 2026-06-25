use camino::Utf8PathBuf;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self { start, end, line, column }
    }

    pub fn combine(&self, other: &Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end), line: self.line, column: self.column }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}-{}:{}:{}", self.line, self.column, self.end, self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticSeverity {
    #[default]
    Error,
    Warning,
}

impl DiagnosticSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    fn colour(self) -> &'static str {
        match self {
            Self::Error => "\x1b[31m",
            Self::Warning => "\x1b[33m",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
    pub file: Option<Utf8PathBuf>,
    pub code: Option<String>,
    pub severity: DiagnosticSeverity,
    pub related: Vec<CompileError>,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self { message: message.into(), span, file: None, code: None, severity: DiagnosticSeverity::Error, related: Vec::new() }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self::new(message, span).with_severity(DiagnosticSeverity::Warning)
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn without_span(message: impl Into<String>) -> Self {
        Self::new(message, Span::default())
    }

    pub fn with_file(mut self, file: Utf8PathBuf) -> Self {
        self.file = Some(file);
        self
    }

    pub fn with_related(mut self, related: Vec<CompileError>) -> Self {
        self.related = related;
        self
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref code) = self.code {
            if let Some(ref file) = self.file {
                write!(f, "{}:{}: [{}] {}", file, self.span.line, code, self.message)
            } else {
                write!(f, "line {}: [{}] {}", self.span.line, code, self.message)
            }
        } else if let Some(ref file) = self.file {
            write!(f, "{}:{}: {}", file, self.span.line, self.message)
        } else {
            write!(f, "line {}: {}", self.span.line, self.message)
        }
    }
}

impl std::error::Error for CompileError {}

impl From<std::io::Error> for CompileError {
    fn from(value: std::io::Error) -> Self {
        Self::without_span(value.to_string())
    }
}

impl From<toml::de::Error> for CompileError {
    fn from(value: toml::de::Error) -> Self {
        Self::without_span(value.to_string())
    }
}

impl From<toml::ser::Error> for CompileError {
    fn from(value: toml::ser::Error) -> Self {
        Self::without_span(value.to_string())
    }
}

impl From<serde_json::Error> for CompileError {
    fn from(value: serde_json::Error) -> Self {
        Self::without_span(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CompileError>;

/// v0.15 migration diagnostic codes.
///
/// These codes are emitted in `--primitive-strict=0.15` mode when the compiler
/// encounters v0.14-era syntax that must be migrated. In `--primitive-compat=0.14`
/// mode they appear as warnings with migration hints instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationDiagnostic {
    /// CS0151: legacy `has destroy` capability must be expressed as kernel effects
    Cs0151,
    /// CS0152: `Address` cannot be used as `LockHash` without a resolver
    Cs0152,
    /// CS0153: CKB entry role must be explicit
    Cs0153,
    /// CS0154: claim proof bindings must be explicit
    Cs0154,
    /// CS0155: type_id lifecycle must be explicit
    Cs0155,
    /// CS0156: protocol capabilities are not allowed in strict mode
    Cs0156,
    /// CS0157: schema-backed replacement requires a layout policy
    Cs0157,
    /// CS0158: invariant trigger and scope must be explicit
    Cs0158,
    /// CS0159: lock_group + transaction scope requires explicit coverage acknowledgement
    Cs0159,
    /// CS0160: builder assumption is not on-chain checked
    Cs0160,
}

impl MigrationDiagnostic {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cs0151 => "CS0151",
            Self::Cs0152 => "CS0152",
            Self::Cs0153 => "CS0153",
            Self::Cs0154 => "CS0154",
            Self::Cs0155 => "CS0155",
            Self::Cs0156 => "CS0156",
            Self::Cs0157 => "CS0157",
            Self::Cs0158 => "CS0158",
            Self::Cs0159 => "CS0159",
            Self::Cs0160 => "CS0160",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Cs0151 => "legacy destroy capability must use consume + burn kernel effects",
            Self::Cs0152 => "Address cannot be used as LockHash without a resolver",
            Self::Cs0153 => "CKB entry role must be explicit",
            Self::Cs0154 => "claim proof bindings must be explicit",
            Self::Cs0155 => "type_id lifecycle must be explicit",
            Self::Cs0156 => "protocol capabilities are not allowed in strict mode",
            Self::Cs0157 => "schema-backed replacement requires a layout policy",
            Self::Cs0158 => "invariant trigger and scope must be explicit",
            Self::Cs0159 => "lock_group + transaction scope requires explicit coverage acknowledgement",
            Self::Cs0160 => "builder assumption is not on-chain checked",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Cs0151 => {
                "replace `has destroy` with `has consume, burn`; use a policy-specific destruction form when the proof needs one"
            }
            Self::Cs0152 => "use LockHash, LockScript, or transfer_to_lock_hash/transfer_to_lock_script explicitly",
            Self::Cs0153 => "add #[entry(lock)] or #[entry(type)] to the entry declaration",
            Self::Cs0154 => "use claim_proof(receipt, signer=..., recipient=..., amount=..., nonce=...) with explicit bindings",
            Self::Cs0155 => {
                "add `identity = ckb_type_id` to the resource declaration and use create_unique/replace_unique/destroy_unique"
            }
            Self::Cs0156 => "replace `has destroy` with `has consume, burn`",
            Self::Cs0157 => "add `preserve_layout<T>()` or `migrate_layout<T>(from=..., to=...)` to the replacement",
            Self::Cs0158 => "add `trigger:` and `scope:` to the invariant declaration",
            Self::Cs0159 => "add `acknowledge_coverage` or restructure to `scope: group`",
            Self::Cs0160 => "promote the builder assumption to an on-chain check or document it explicitly",
        }
    }

    /// Build a full diagnostic message with code, description, and migration hint.
    pub fn full_message(self) -> String {
        format!("{}: {}\n  hint: {}", self.code(), self.message(), self.hint())
    }

    pub fn warning(self, span: Span) -> CompileError {
        CompileError::warning(self.full_message(), span).with_code(self.code())
    }

    pub fn error(self, span: Span) -> CompileError {
        CompileError::new(self.full_message(), span).with_code(self.code())
    }
}

pub struct ErrorReporter {
    diagnostics: Vec<CompileError>,
    source: String,
    filename: Option<Utf8PathBuf>,
}

impl ErrorReporter {
    pub fn new(source: String, filename: Option<Utf8PathBuf>) -> Self {
        Self { diagnostics: Vec::new(), source, filename }
    }

    pub fn report(&mut self, message: impl Into<String>, span: Span) {
        self.push(CompileError::new(message, span));
    }

    pub fn report_warning(&mut self, message: impl Into<String>, span: Span) {
        self.push(CompileError::warning(message, span));
    }

    fn push(&mut self, diagnostic: CompileError) {
        let mut diagnostic = diagnostic;
        if let Some(ref file) = self.filename {
            diagnostic = diagnostic.with_file(file.clone());
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    pub fn errors(&self) -> &[CompileError] {
        &self.diagnostics
    }

    pub fn print_errors(&self) {
        for diagnostic in &self.diagnostics {
            eprintln!("{}{}\x1b[0m: {}", diagnostic.severity.colour(), diagnostic.severity.label(), diagnostic);
            if let Some(line) = self.source.lines().nth(diagnostic.span.line.saturating_sub(1)) {
                eprintln!("  \x1b[34m{}\x1b[0m | {}", diagnostic.span.line, line);
                let spaces = " ".repeat(diagnostic.span.line.to_string().len() + 3);
                let carets = "^".repeat(diagnostic.span.end.saturating_sub(diagnostic.span.start).max(1));
                eprintln!("{}  \x1b[32m{}\x1b[0m", spaces, carets);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_error_defaults_to_error_severity() {
        let error = CompileError::new("boom", Span::default());
        assert_eq!(error.severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn error_reporter_distinguishes_warnings_from_errors() {
        let mut reporter = ErrorReporter::new("let x = 1".to_string(), None);
        reporter.report_warning("compatibility note", Span::new(0, 3, 1, 1));
        assert!(!reporter.has_errors());
        assert_eq!(reporter.errors()[0].severity, DiagnosticSeverity::Warning);

        reporter.report("hard failure", Span::new(4, 5, 1, 5));
        assert!(reporter.has_errors());
        assert_eq!(reporter.errors()[1].severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn migration_diagnostic_can_build_typed_warning() {
        let warning = MigrationDiagnostic::Cs0151.warning(Span::new(0, 3, 1, 1));
        assert_eq!(warning.severity, DiagnosticSeverity::Warning);
        assert_eq!(warning.code.as_deref(), Some("CS0151"));
        assert!(warning.message.contains("legacy destroy capability"));
    }
}
