use syndic_storage::{RecoveryBudgetKind, RecoveryProjectionError};

use crate::cas_projection::ProjectionExecutionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageHistoryRecovery {
    Available,
    Denied(NativeLineageRecoveryDenial),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageRecoveryDenial {
    ModelContextUnavailable,
    SelectedPathChanged,
    PendingTurnRequired,
    MissingHistory,
    CompleteHistoryRequired,
    UnsupportedHistory,
    MediaHistory,
    EmptyHistoryItem,
    ItemCountLimit,
    Utf8BytesLimit,
    HistoryReadFailed,
    InvalidHistory,
    ExactSourceUnavailable,
    PublicationUnavailable,
    Cancelled,
}

impl NativeLineageHistoryRecovery {
    pub fn from_preflight(result: Result<(), ProjectionExecutionError>) -> Self {
        match result {
            Ok(()) => Self::Available,
            Err(error) => Self::Denied(NativeLineageRecoveryDenial::from_execution_error(&error)),
        }
    }

    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }

    pub const fn disabled_explanation(self) -> &'static str {
        match self {
            Self::Available => "",
            Self::Denied(denial) => denial.explanation(),
        }
    }
}

impl NativeLineageRecoveryDenial {
    fn from_execution_error(error: &ProjectionExecutionError) -> Self {
        match error {
            ProjectionExecutionError::RecoveryProjection(error) => match error {
                RecoveryProjectionError::MissingModelContextWindow
                | RecoveryProjectionError::ZeroModelContextWindow => Self::ModelContextUnavailable,
                RecoveryProjectionError::StaleSelectedPath
                | RecoveryProjectionError::ConcurrentChange => Self::SelectedPathChanged,
                RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser => {
                    Self::PendingTurnRequired
                }
                RecoveryProjectionError::MissingHistory { .. } => Self::MissingHistory,
                RecoveryProjectionError::IncompleteHistory { .. } => Self::CompleteHistoryRequired,
                RecoveryProjectionError::UnsupportedHistory { .. } => Self::UnsupportedHistory,
                RecoveryProjectionError::MediaHistory { .. } => Self::MediaHistory,
                RecoveryProjectionError::EmptyHistoryItem => Self::EmptyHistoryItem,
                RecoveryProjectionError::BudgetOverflow { kind, .. } => match kind {
                    RecoveryBudgetKind::ItemCount => Self::ItemCountLimit,
                    RecoveryBudgetKind::Utf8Bytes => Self::Utf8BytesLimit,
                },
                RecoveryProjectionError::Read(_) => Self::HistoryReadFailed,
                RecoveryProjectionError::CursorTerminal
                | RecoveryProjectionError::InvalidCursorPageLimit { .. }
                | RecoveryProjectionError::CursorPageLimitTooSmall { .. }
                | RecoveryProjectionError::CursorMismatch { .. }
                | RecoveryProjectionError::Invariant(_) => Self::InvalidHistory,
            },
            ProjectionExecutionError::NativeLineageRecoveryDecisionStale { .. }
            | ProjectionExecutionError::ProjectionBasisChanged { .. } => Self::SelectedPathChanged,
            ProjectionExecutionError::SyndicRead(_) => Self::HistoryReadFailed,
            ProjectionExecutionError::Publication(_) => Self::PublicationUnavailable,
            ProjectionExecutionError::Cancelled => Self::Cancelled,
            _ => Self::ExactSourceUnavailable,
        }
    }

    pub const fn explanation(self) -> &'static str {
        match self {
            Self::ModelContextUnavailable => {
                "Recover from Syndic history is unavailable because exact nonzero model context-window metadata is unavailable."
            }
            Self::SelectedPathChanged => {
                "Recover from Syndic history is unavailable because the exact selected history or source binding changed. Wait for a current recovery decision."
            }
            Self::PendingTurnRequired => {
                "Recover from Syndic history is unavailable because the selected tail is not an eligible pending ordinary-user turn."
            }
            Self::MissingHistory => {
                "Recover from Syndic history is unavailable because required durable history records are missing."
            }
            Self::CompleteHistoryRequired => {
                "Recover from Syndic history is unavailable because the required exact history is incomplete, unfinalized, or blocked by unresolved turn or item state."
            }
            Self::UnsupportedHistory => {
                "Recover from Syndic history is unavailable because required history has no supported lossless representation."
            }
            Self::MediaHistory => {
                "Recover from Syndic history is unavailable because required history contains unsupported media."
            }
            Self::EmptyHistoryItem => {
                "Recover from Syndic history is unavailable because required history contains an empty canonical item."
            }
            Self::ItemCountLimit => {
                "Recover from Syndic history is unavailable because the complete history exceeds the recovery item-count limit."
            }
            Self::Utf8BytesLimit => {
                "Recover from Syndic history is unavailable because the complete history exceeds the recovery UTF-8 byte limit."
            }
            Self::HistoryReadFailed => {
                "Recover from Syndic history is unavailable because required durable history could not be read."
            }
            Self::InvalidHistory => {
                "Recover from Syndic history is unavailable because the exact history proof or representation is invalid."
            }
            Self::ExactSourceUnavailable => {
                "Recover from Syndic history is unavailable because the exact source or execution authority could not be validated."
            }
            Self::PublicationUnavailable => {
                "Recover from Syndic history is unavailable because exact recovery publication could not be established."
            }
            Self::Cancelled => {
                "Recover from Syndic history is unavailable because validation of this recovery decision was cancelled."
            }
        }
    }
}
