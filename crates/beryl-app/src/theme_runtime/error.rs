use std::{error::Error, fmt};

use super::WindowAdapterId;

macro_rules! exhausted_error {
    ($name:ident, $message:literal) => {
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $name;

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str($message)
            }
        }

        impl Error for $name {}
    };
}

exhausted_error!(GenerationExhausted, "appearance generation exhausted");
exhausted_error!(WindowEpochExhausted, "window-set epoch exhausted");
exhausted_error!(PreviewSequenceExhausted, "preview sequence exhausted");

/// Preview-source construction rejects the reserved zero identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewSourceError {
    ZeroIdentity,
}

impl fmt::Display for PreviewSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("preview source identity must be nonzero")
    }
}

impl Error for PreviewSourceError {}

/// Bounded failure class returned by a pure window adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterFailureClass {
    Unavailable,
    Rejected,
    ApplicationFailed,
}

/// Content-free last-publication failure class retained in diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationFailureClass {
    Stale,
    CandidateMismatch,
    AdapterUnavailable,
    AdapterRejected,
    AdapterApplicationFailed,
    GenerationExhausted,
    PreviewSequenceExhausted,
    WindowEpochExhausted,
}

impl From<AdapterFailureClass> for PublicationFailureClass {
    fn from(value: AdapterFailureClass) -> Self {
        match value {
            AdapterFailureClass::Unavailable => Self::AdapterUnavailable,
            AdapterFailureClass::Rejected => Self::AdapterRejected,
            AdapterFailureClass::ApplicationFailed => Self::AdapterApplicationFailed,
        }
    }
}

/// Exact freshness fence that rejected a completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StalePublicationReason {
    ForeignService,
    DurableGeneration,
    CurrentGeneration,
    WindowSetEpoch,
    PreviewSequence,
    DurableAttempt,
}

/// Registration fails before the adapter becomes publication-eligible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterRegistrationError {
    CapacityReached,
    DuplicateIdentity(WindowAdapterId),
    Preparation {
        adapter: WindowAdapterId,
        class: AdapterFailureClass,
    },
    WindowEpochExhausted,
}

impl fmt::Display for AdapterRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityReached => formatter.write_str("window adapter capacity reached"),
            Self::DuplicateIdentity(_) => formatter.write_str("duplicate window adapter identity"),
            Self::Preparation { .. } => {
                formatter.write_str("window adapter rejected current appearance")
            }
            Self::WindowEpochExhausted => formatter.write_str("window-set epoch exhausted"),
        }
    }
}

impl Error for AdapterRegistrationError {}

/// Durable-base preparation or all-window publication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurablePublicationError {
    AttemptExhausted,
    GenerationExhausted,
    PreviewSequenceExhausted,
    Stale(StalePublicationReason),
    CandidateMismatch,
    Adapter {
        adapter: WindowAdapterId,
        class: AdapterFailureClass,
    },
}

impl fmt::Display for DurablePublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttemptExhausted => formatter.write_str("durable publication attempt exhausted"),
            Self::GenerationExhausted => formatter.write_str("appearance generation exhausted"),
            Self::PreviewSequenceExhausted => formatter.write_str("preview sequence exhausted"),
            Self::Stale(_) => formatter.write_str("stale durable appearance completion"),
            Self::CandidateMismatch => formatter.write_str("durable appearance identity mismatch"),
            Self::Adapter { .. } => {
                formatter.write_str("window adapter rejected durable appearance")
            }
        }
    }
}

impl Error for DurablePublicationError {}

/// Preview arbitration or all-window publication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewPublicationError {
    SequenceExhausted,
    GenerationExhausted,
    Stale(StalePublicationReason),
    CandidateMismatch,
    Adapter {
        adapter: WindowAdapterId,
        class: AdapterFailureClass,
    },
}

impl fmt::Display for PreviewPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SequenceExhausted => formatter.write_str("preview sequence exhausted"),
            Self::GenerationExhausted => formatter.write_str("appearance generation exhausted"),
            Self::Stale(_) => formatter.write_str("stale preview completion"),
            Self::CandidateMismatch => formatter.write_str("preview candidate identity mismatch"),
            Self::Adapter { .. } => {
                formatter.write_str("window adapter rejected preview appearance")
            }
        }
    }
}

impl Error for PreviewPublicationError {}
