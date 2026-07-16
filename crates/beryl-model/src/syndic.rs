use std::{error::Error, fmt, num::NonZeroU64};

use crate::{SyndicDraftId, SyndicTurnId};

/// Version of one canonical Beryl conversation-tool profile identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum CasConversationToolProfileVersion {
    /// Version 1 identifies the exact canonical tool definitions by SHA-256.
    V1 = 1,
}

/// Exact versioned identity of one canonical Beryl conversation-tool profile.
///
/// The digest is calculated over the canonical tool-definition representation
/// by the app boundary. Storage treats this value as immutable proof carried by
/// every usable CAS projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CasConversationToolProfile {
    version: CasConversationToolProfileVersion,
    digest: [u8; 32],
}

impl CasConversationToolProfile {
    /// Constructs the first supported profile identity from an exact SHA-256 digest.
    #[must_use]
    pub const fn v1(digest: [u8; 32]) -> Self {
        Self {
            version: CasConversationToolProfileVersion::V1,
            digest,
        }
    }

    /// Returns the exact canonical-profile version.
    #[must_use]
    pub const fn version(self) -> CasConversationToolProfileVersion {
        self.version
    }

    /// Returns the exact SHA-256 digest bytes.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }
}

/// Immutable identity of the draft or submitted turn that owns discussion context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscussionContextOwnerId {
    /// Context is owned by the current draft before its first submission.
    Draft(SyndicDraftId),
    /// Context is owned by the submitted turn created from that draft.
    SubmittedTurn(SyndicTurnId),
}

impl DiscussionContextOwnerId {
    /// Returns the submitted owner produced by transitioning one context-bearing draft.
    #[must_use]
    pub const fn submitted_from_draft(draft_id: SyndicDraftId) -> Self {
        Self::SubmittedTurn(draft_id.submitted_turn_id())
    }
}

macro_rules! fixed_digest {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 32]);

        impl $name {
            /// Constructs a digest from bytes calculated by its owning boundary.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// Returns the exact digest bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

fixed_digest!(
    /// SHA-256 digest of the exact selected UTF-8 discussion-context bytes.
    DiscussionContextDigest
);
fixed_digest!(
    /// Exact digest of one selected committed Syndic path.
    SyndicPathDigest
);
fixed_digest!(
    /// Exact chain digest of one ordered chunked Syndic content object.
    SyndicContentDigest
);
fixed_digest!(
    /// Exact digest of one ordered recovery item sequence.
    RecoveryItemSequenceDigest
);

/// Why an exact CAS generation could not be represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CasGenerationError {
    /// Zero is reserved for the absence of an observed generation.
    Zero,
}

impl fmt::Display for CasGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CAS generation must be non-zero")
    }
}

impl Error for CasGenerationError {}

/// Number of actual CAS model turns represented by one loaded thread prefix.
///
/// This is an exact CAS-native position. It is deliberately independent of
/// Syndic conversation-DAG depth because provider-operation turns need not
/// correspond to CAS model turns.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CasNativeTurnCount(u64);

impl CasNativeTurnCount {
    /// The empty loaded CAS prefix before any model turn has completed.
    pub const ZERO: Self = Self(0);

    /// Constructs an exact native CAS turn count.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the exact number of represented CAS model turns.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances the exact count after one correlated CAS model turn.
    pub const fn checked_next(self) -> Result<Self, CasNativeTurnCountError> {
        match self.0.checked_add(1) {
            Some(next) => Ok(Self(next)),
            None => Err(CasNativeTurnCountError::Exhausted),
        }
    }
}

/// Why an exact native CAS turn-count operation could not be represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CasNativeTurnCountError {
    /// One more actual CAS model turn would exceed the exact count domain.
    Exhausted,
}

impl fmt::Display for CasNativeTurnCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => formatter.write_str("native CAS turn count is exhausted"),
        }
    }
}

impl Error for CasNativeTurnCountError {}

macro_rules! cas_generation {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Constructs an exact generation observed by its owning runtime boundary.
            pub fn new(value: u64) -> Result<Self, CasGenerationError> {
                NonZeroU64::new(value).map(Self).ok_or(CasGenerationError::Zero)
            }

            /// Returns the exact observed generation.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

cas_generation!(
    /// Exact generation of one Beryl-managed CAS process.
    CasProcessGeneration
);
cas_generation!(
    /// Exact loaded-session generation of one CAS thread.
    CasLoadedThreadGeneration
);

/// Exact composite generation of one loaded CAS thread inside one managed process.
///
/// The pair is one value so consumers cannot accidentally compare only one
/// generation component when proving loaded-session identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CasLoadedSessionGeneration {
    process: CasProcessGeneration,
    thread: CasLoadedThreadGeneration,
}

impl CasLoadedSessionGeneration {
    /// Combines the exact generation components observed by the runtime boundary.
    #[must_use]
    pub const fn new(process: CasProcessGeneration, thread: CasLoadedThreadGeneration) -> Self {
        Self { process, thread }
    }

    /// Returns the exact managed-process generation.
    #[must_use]
    pub const fn process(self) -> CasProcessGeneration {
        self.process
    }

    /// Returns the exact loaded-thread generation within that process.
    #[must_use]
    pub const fn thread(self) -> CasLoadedThreadGeneration {
        self.thread
    }
}
