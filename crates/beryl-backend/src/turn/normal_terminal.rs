use std::fmt;

use beryl_model::{CasThreadId, CasTurnId};

use super::CodexErrorInfo;

/// Maximum aggregate UTF-8 bytes retained from one normal terminal error.
pub const NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES: usize = 4_096;

/// Closed terminal status accepted from pinned CAS `turn/completed`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalTurnTerminalStatus {
    Completed,
    Interrupted,
    Failed,
}

/// Bounded status-only terminal control from one complete pinned `turn/completed` document.
pub struct NormalTurnTerminal {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    status: NormalTurnTerminalStatus,
    codex_error_info: Option<CodexErrorInfo>,
    diagnostic: NormalTurnTerminalDiagnostic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NormalTurnTerminalDiagnosticField {
    Message,
    AdditionalDetails,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct DiagnosticSlice {
    start: u16,
    len: u16,
    truncated: bool,
}

#[derive(Clone, Copy)]
struct ActiveDiagnostic {
    field: NormalTurnTerminalDiagnosticField,
    start: u16,
    truncated: bool,
}

pub(crate) struct NormalTurnTerminalDiagnostic {
    bytes: [u8; NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES],
    len: u16,
    active: Option<ActiveDiagnostic>,
    message: Option<DiagnosticSlice>,
    additional_details: Option<DiagnosticSlice>,
}

impl NormalTurnTerminal {
    pub(crate) fn decoded(
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        status: NormalTurnTerminalStatus,
        codex_error_info: Option<CodexErrorInfo>,
        diagnostic: NormalTurnTerminalDiagnostic,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            status,
            codex_error_info,
            diagnostic,
        }
    }

    /// Returns the exact CAS thread named by the terminal control.
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    /// Returns the exact CAS turn named by the terminal control.
    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    /// Returns the closed provider terminal status.
    #[must_use]
    pub const fn status(&self) -> NormalTurnTerminalStatus {
        self.status
    }

    /// Returns the retained prefix of the required failed-turn error message.
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.diagnostic.projected(self.diagnostic.message)
    }

    /// Reports whether the failed-turn error message exceeded the shared diagnostic space.
    #[must_use]
    pub const fn error_message_was_truncated(&self) -> bool {
        match self.diagnostic.message {
            Some(slice) => slice.truncated,
            None => false,
        }
    }

    /// Returns the optional closed machine-readable CAS error fact.
    #[must_use]
    pub const fn codex_error_info(&self) -> Option<&CodexErrorInfo> {
        self.codex_error_info.as_ref()
    }

    /// Returns the retained prefix of optional additional error detail.
    #[must_use]
    pub fn additional_details(&self) -> Option<&str> {
        self.diagnostic
            .projected(self.diagnostic.additional_details)
    }

    /// Reports whether additional detail exceeded the remaining shared diagnostic space.
    #[must_use]
    pub const fn additional_details_was_truncated(&self) -> bool {
        match self.diagnostic.additional_details {
            Some(slice) => slice.truncated,
            None => false,
        }
    }

    /// Reports whether either diagnostic projection was truncated.
    #[must_use]
    pub const fn diagnostic_was_truncated(&self) -> bool {
        self.error_message_was_truncated() || self.additional_details_was_truncated()
    }
}

impl NormalTurnTerminalDiagnostic {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: [0; NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES],
            len: 0,
            active: None,
            message: None,
            additional_details: None,
        }
    }

    pub(crate) fn begin(&mut self, field: NormalTurnTerminalDiagnosticField) -> bool {
        if self.active.is_some() || self.slice(field).is_some() {
            return false;
        }
        self.active = Some(ActiveDiagnostic {
            field,
            start: self.len,
            truncated: false,
        });
        true
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> bool {
        let Some(active) = &mut self.active else {
            return bytes.is_empty();
        };
        if bytes.is_empty() || active.truncated {
            return true;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        let remaining = self.bytes.len().saturating_sub(usize::from(self.len));
        let proposed = remaining.min(bytes.len());
        let retained = (0..=proposed)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(0);
        let start = usize::from(self.len);
        let end = start + retained;
        self.bytes[start..end].copy_from_slice(&bytes[..retained]);
        self.len = u16::try_from(end).expect("terminal diagnostic capacity fits u16");
        active.truncated = retained < bytes.len();
        true
    }

    pub(crate) fn finish(&mut self, field: NormalTurnTerminalDiagnosticField) -> bool {
        let Some(active) = self.active.take() else {
            return false;
        };
        if active.field != field {
            return false;
        }
        let slice = DiagnosticSlice {
            start: active.start,
            len: self.len - active.start,
            truncated: active.truncated,
        };
        match field {
            NormalTurnTerminalDiagnosticField::Message => self.message = Some(slice),
            NormalTurnTerminalDiagnosticField::AdditionalDetails => {
                self.additional_details = Some(slice);
            }
        }
        true
    }

    pub(crate) const fn is_idle(&self) -> bool {
        self.active.is_none()
    }

    fn slice(&self, field: NormalTurnTerminalDiagnosticField) -> Option<DiagnosticSlice> {
        match field {
            NormalTurnTerminalDiagnosticField::Message => self.message,
            NormalTurnTerminalDiagnosticField::AdditionalDetails => self.additional_details,
        }
    }

    fn projected(&self, slice: Option<DiagnosticSlice>) -> Option<&str> {
        let slice = slice?;
        let start = usize::from(slice.start);
        let end = start + usize::from(slice.len);
        Some(
            std::str::from_utf8(&self.bytes[start..end])
                .expect("terminal diagnostic fragments are retained at UTF-8 boundaries"),
        )
    }
}

impl fmt::Debug for NormalTurnTerminal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NormalTurnTerminal")
            .field("thread_id", &self.thread_id)
            .field("turn_id", &self.turn_id)
            .field("status", &self.status)
            .field("error_message_present", &self.diagnostic.message.is_some())
            .field("codex_error_info", &self.codex_error_info)
            .field(
                "additional_details_present",
                &self.diagnostic.additional_details.is_some(),
            )
            .field("diagnostic_bytes", &self.diagnostic.len)
            .field("diagnostic_was_truncated", &self.diagnostic_was_truncated())
            .finish()
    }
}
