use std::num::{NonZeroU32, NonZeroU64};

use beryl_model::{
    CasLoadedSessionGeneration, RecoveryItemSequenceDigest, SyndicPathDigest, SyndicTurnId,
    ThreadRevision,
};

use super::{SyndicTimestamp, SyndicValueError, TranscriptGeneration, TranscriptPosition};

const RECOVERY_MAX_UTF8_BYTES: u64 = 262_144;
const RECOVERY_MAX_ITEMS: u64 = RECOVERY_MAX_UTF8_BYTES;

/// Exact canonical format used to project Syndic history for one recovery injection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryProjectionVersion {
    /// Closed user/input-text and assistant/output-text sequence defined by the target system.
    V1,
}

/// Number of ordered items in one non-empty recovery projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecoveryItemCount(NonZeroU32);

impl RecoveryItemCount {
    /// Maximum number of nonempty canonical text items in one recovery projection.
    pub const MAX: u64 = RECOVERY_MAX_ITEMS;

    pub fn new(value: u64) -> Result<Self, SyndicValueError> {
        if value > Self::MAX {
            return Err(SyndicValueError::CountTooLarge {
                kind: "recovery item count",
                maximum: Self::MAX,
                actual: value,
            });
        }
        let value = u32::try_from(value).map_err(|_| SyndicValueError::CountTooLarge {
            kind: "recovery item count",
            maximum: Self::MAX,
            actual: value,
        })?;
        NonZeroU32::new(value)
            .map(Self)
            .ok_or(SyndicValueError::ZeroCount {
                kind: "recovery item count",
            })
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Canonical UTF-8 payload bytes in one accepted recovery projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecoveryUtf8ByteCount(NonZeroU64);

impl RecoveryUtf8ByteCount {
    /// Maximum independent byte ceiling before the model-specific half-window check.
    pub const MAX: u64 = RECOVERY_MAX_UTF8_BYTES;

    pub fn new(value: u64) -> Result<Self, SyndicValueError> {
        let value = NonZeroU64::new(value).ok_or(SyndicValueError::ZeroCount {
            kind: "recovery UTF-8 byte count",
        })?;
        if value.get() > Self::MAX {
            return Err(SyndicValueError::CountTooLarge {
                kind: "recovery UTF-8 byte count",
                maximum: Self::MAX,
                actual: value.get(),
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Exact current selected path of one Syndic thread.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SelectedPathProof {
    tail: Option<SyndicTurnId>,
    thread_revision: ThreadRevision,
    digest: SyndicPathDigest,
}

/// Exact committed Syndic prefix already represented by one CAS thread.
///
/// This type is deliberately distinct from [`SelectedPathProof`]. A pending
/// submitted turn may be the selected tail while CAS still represents only its
/// parent prefix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CasRepresentedPrefixProof {
    tail: Option<SyndicTurnId>,
    source_thread_revision: ThreadRevision,
    digest: SyndicPathDigest,
}

impl CasRepresentedPrefixProof {
    #[must_use]
    pub const fn new(
        tail: Option<SyndicTurnId>,
        source_thread_revision: ThreadRevision,
        digest: SyndicPathDigest,
    ) -> Self {
        Self {
            tail,
            source_thread_revision,
            digest,
        }
    }

    #[must_use]
    pub const fn tail(self) -> Option<SyndicTurnId> {
        self.tail
    }

    #[must_use]
    pub const fn source_thread_revision(self) -> ThreadRevision {
        self.source_thread_revision
    }

    #[must_use]
    pub const fn digest(self) -> SyndicPathDigest {
        self.digest
    }
}

/// Exact bounded location of one item in the currently selected transcript generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CurrentTranscriptEntryProof {
    generation: TranscriptGeneration,
    position: TranscriptPosition,
}

impl CurrentTranscriptEntryProof {
    #[must_use]
    pub const fn new(generation: TranscriptGeneration, position: TranscriptPosition) -> Self {
        Self {
            generation,
            position,
        }
    }

    #[must_use]
    pub const fn generation(self) -> TranscriptGeneration {
        self.generation
    }

    #[must_use]
    pub const fn position(self) -> TranscriptPosition {
        self.position
    }
}

impl SelectedPathProof {
    #[must_use]
    pub const fn new(
        tail: Option<SyndicTurnId>,
        thread_revision: ThreadRevision,
        digest: SyndicPathDigest,
    ) -> Self {
        Self {
            tail,
            thread_revision,
            digest,
        }
    }

    #[must_use]
    pub const fn tail(self) -> Option<SyndicTurnId> {
        self.tail
    }

    #[must_use]
    pub const fn thread_revision(self) -> ThreadRevision {
        self.thread_revision
    }

    #[must_use]
    pub const fn digest(self) -> SyndicPathDigest {
        self.digest
    }

    /// Returns whether this proof is the same selected path at an equal or newer thread revision.
    ///
    /// Draft and accepted-input admission can advance the enclosing thread revision without
    /// changing its committed tail or selected-path digest. Consumers that intentionally tolerate
    /// that drift use this relation instead of weakening exact proof equality globally.
    #[must_use]
    pub fn is_compatible_descendant_of(self, prior: Self) -> bool {
        self.tail == prior.tail
            && self.digest == prior.digest
            && self.thread_revision.get() >= prior.thread_revision.get()
    }
}

/// CAS-native mechanism whose own history already represents the selected path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeCasLineage {
    Fresh,
    Continuation,
    Resume,
    Fork,
}

/// Coarse lineage classification stored with an exclusive CAS binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CasLineageMode {
    Native,
    RecoveredInjection,
}

/// Exact session-scoped proof produced by one completed fresh recovery injection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecoveredInjectionProof {
    version: RecoveryProjectionVersion,
    established_prefix: CasRepresentedPrefixProof,
    sequence_digest: RecoveryItemSequenceDigest,
    item_count: RecoveryItemCount,
    utf8_bytes: RecoveryUtf8ByteCount,
    completed_at: SyndicTimestamp,
    loaded_generation: CasLoadedSessionGeneration,
}

impl RecoveredInjectionProof {
    pub fn new(
        version: RecoveryProjectionVersion,
        established_prefix: CasRepresentedPrefixProof,
        sequence_digest: RecoveryItemSequenceDigest,
        item_count: RecoveryItemCount,
        utf8_bytes: RecoveryUtf8ByteCount,
        completed_at: SyndicTimestamp,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> Result<Self, SyndicValueError> {
        if established_prefix.tail().is_none() {
            return Err(SyndicValueError::InvalidLineageProof {
                reason: "recovered injection requires a non-empty committed path",
            });
        }
        if u64::from(item_count.get()) > utf8_bytes.get() {
            return Err(SyndicValueError::InvalidLineageProof {
                reason: "recovered nonempty item count exceeds its UTF-8 byte count",
            });
        }
        Ok(Self {
            version,
            established_prefix,
            sequence_digest,
            item_count,
            utf8_bytes,
            completed_at,
            loaded_generation,
        })
    }

    #[must_use]
    pub const fn version(self) -> RecoveryProjectionVersion {
        self.version
    }

    #[must_use]
    pub const fn established_prefix(self) -> CasRepresentedPrefixProof {
        self.established_prefix
    }

    #[must_use]
    pub const fn sequence_digest(self) -> RecoveryItemSequenceDigest {
        self.sequence_digest
    }

    #[must_use]
    pub const fn item_count(self) -> RecoveryItemCount {
        self.item_count
    }

    #[must_use]
    pub const fn utf8_bytes(self) -> RecoveryUtf8ByteCount {
        self.utf8_bytes
    }

    #[must_use]
    pub const fn completed_at(self) -> SyndicTimestamp {
        self.completed_at
    }

    #[must_use]
    pub const fn loaded_generation(self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
}

/// Exact proof of how one exclusive CAS lineage was established.
///
/// The binding's later represented prefix is a separate fact. The recovered
/// variant retains its original injection sequence and loaded-session proof even
/// after ordinary CAS turns advance that represented prefix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CasLineageProof {
    Native {
        mechanism: NativeCasLineage,
        established_prefix: CasRepresentedPrefixProof,
    },
    RecoveredInjection(RecoveredInjectionProof),
}

impl CasLineageProof {
    pub fn native(
        mechanism: NativeCasLineage,
        established_prefix: CasRepresentedPrefixProof,
    ) -> Result<Self, SyndicValueError> {
        match (mechanism, established_prefix.tail()) {
            (NativeCasLineage::Fresh, Some(_)) => {
                return Err(SyndicValueError::InvalidLineageProof {
                    reason: "fresh native lineage cannot represent committed history",
                });
            }
            (NativeCasLineage::Fresh, None) => {}
            (
                NativeCasLineage::Continuation | NativeCasLineage::Resume | NativeCasLineage::Fork,
                None,
            ) => {
                return Err(SyndicValueError::InvalidLineageProof {
                    reason: "continuation, resume, and fork require a committed tail",
                });
            }
            (_, Some(_)) => {}
        }
        Ok(Self::Native {
            mechanism,
            established_prefix,
        })
    }

    #[must_use]
    pub const fn recovered(proof: RecoveredInjectionProof) -> Self {
        Self::RecoveredInjection(proof)
    }

    #[must_use]
    pub const fn mode(self) -> CasLineageMode {
        match self {
            Self::Native { .. } => CasLineageMode::Native,
            Self::RecoveredInjection(_) => CasLineageMode::RecoveredInjection,
        }
    }

    #[must_use]
    pub const fn established_prefix(self) -> CasRepresentedPrefixProof {
        match self {
            Self::Native {
                established_prefix, ..
            } => established_prefix,
            Self::RecoveredInjection(proof) => proof.established_prefix(),
        }
    }

    /// Returns the exact loaded-session generation that established recovered lineage.
    #[must_use]
    pub const fn recovered_injection_generation(self) -> Option<CasLoadedSessionGeneration> {
        match self {
            Self::Native { .. } => None,
            Self::RecoveredInjection(proof) => Some(proof.loaded_generation()),
        }
    }

    /// Returns when a recovered lineage's one-time injection completed.
    #[must_use]
    pub const fn recovered_completed_at(self) -> Option<SyndicTimestamp> {
        match self {
            Self::Native { .. } => None,
            Self::RecoveredInjection(proof) => Some(proof.completed_at()),
        }
    }
}
