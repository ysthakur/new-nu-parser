use crate::compiler::Span;

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Error,
    Note,
}

#[derive(Debug, Clone)]
pub struct SourceError {
    pub message: String,
    pub span: Span,
    pub severity: Severity,
}
