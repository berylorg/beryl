use std::{error::Error, fmt};

use super::model::{StylePropertyId, StyleRoleId};

pub const MAX_THEME_VALIDATION_DIAGNOSTICS: usize = 64;
pub const MAX_THEME_DIAGNOSTIC_MESSAGE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeDiagnosticKind {
    DuplicateRole,
    UnknownRole,
    UnknownProperty,
    InvalidPropertyType,
    InvalidPropertyValue,
    MissingStaticParent,
    StaticParentCycle,
    InvalidFallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeDiagnostic {
    kind: ThemeDiagnosticKind,
    role_id: Option<StyleRoleId>,
    property_id: Option<StylePropertyId>,
    message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeValidationDiagnostics {
    diagnostics: Vec<ThemeDiagnostic>,
    truncated_count: usize,
}

impl ThemeDiagnostic {
    pub(super) fn new(
        kind: ThemeDiagnosticKind,
        role_id: Option<StyleRoleId>,
        property_id: Option<StylePropertyId>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            role_id,
            property_id,
            message: bounded_message(message.into()),
        }
    }

    pub fn kind(&self) -> ThemeDiagnosticKind {
        self.kind
    }

    pub fn role_id(&self) -> Option<&StyleRoleId> {
        self.role_id.as_ref()
    }

    pub fn property_id(&self) -> Option<&StylePropertyId> {
        self.property_id.as_ref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl ThemeValidationDiagnostics {
    pub(super) fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            truncated_count: 0,
        }
    }

    pub(super) fn push(&mut self, diagnostic: ThemeDiagnostic) {
        if self.diagnostics.len() < MAX_THEME_VALIDATION_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        } else {
            self.truncated_count = self.truncated_count.saturating_add(1);
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.diagnostics.is_empty() && self.truncated_count == 0
    }

    pub fn diagnostics(&self) -> &[ThemeDiagnostic] {
        &self.diagnostics
    }

    pub fn truncated_count(&self) -> usize {
        self.truncated_count
    }

    pub fn total_count(&self) -> usize {
        self.diagnostics.len().saturating_add(self.truncated_count)
    }
}

impl fmt::Display for ThemeValidationDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "theme validation failed with {} diagnostic(s)",
            self.total_count()
        )
    }
}

impl Error for ThemeValidationDiagnostics {}

fn bounded_message(value: String) -> String {
    if value.len() <= MAX_THEME_DIAGNOSTIC_MESSAGE_BYTES {
        return value;
    }

    let mut end = MAX_THEME_DIAGNOSTIC_MESSAGE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}
