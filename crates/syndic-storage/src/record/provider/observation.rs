use beryl_model::ProviderObservationId;

use crate::{
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationValidatorState,
    ProviderValueContext, SyndicRecordError,
};

/// Exact digest of one canonical typed provider observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderObservationDigest([u8; 32]);

impl ProviderObservationDigest {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Durable unpublished-build lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationBuildLifecycle {
    Building,
    Sealed,
}

/// Compact durable frontier for one unpublished provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationBuildRecord {
    identity: ProviderObservationId,
    begin: ProviderObservationBegin,
    revision: u64,
    chunk_count: u64,
    canonical_bytes: u64,
    digest: ProviderObservationDigest,
    validator: ProviderObservationValidatorState,
    lifecycle: ProviderObservationBuildLifecycle,
}

impl ProviderObservationBuildRecord {
    pub(crate) fn initial(
        identity: ProviderObservationId,
        begin: ProviderObservationBegin,
        canonical_bytes: u64,
        digest: ProviderObservationDigest,
    ) -> Self {
        Self {
            identity,
            begin,
            revision: 1,
            chunk_count: 0,
            canonical_bytes,
            digest,
            validator: ProviderObservationValidatorState::initial(),
            lifecycle: ProviderObservationBuildLifecycle::Building,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_stored_parts(
        identity: ProviderObservationId,
        begin: ProviderObservationBegin,
        revision: u64,
        chunk_count: u64,
        canonical_bytes: u64,
        digest: ProviderObservationDigest,
        validator: ProviderObservationValidatorState,
        lifecycle: ProviderObservationBuildLifecycle,
    ) -> Result<Self, SyndicRecordError> {
        if revision == 0 {
            return Err(SyndicRecordError::ZeroValue {
                kind: "provider-observation build revision",
            });
        }
        let expected_revision = chunk_count
            .checked_add(1)
            .and_then(|value| {
                value.checked_add(u64::from(
                    lifecycle == ProviderObservationBuildLifecycle::Sealed,
                ))
            })
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "provider-observation build revision",
            })?;
        if revision != expected_revision {
            return Err(SyndicRecordError::InvalidProviderObservationFrontier);
        }
        Ok(Self {
            identity,
            begin,
            revision,
            chunk_count,
            canonical_bytes,
            digest,
            validator,
            lifecycle,
        })
    }

    pub(crate) fn advance(
        &self,
        canonical_bytes: u64,
        digest: ProviderObservationDigest,
        validator: ProviderObservationValidatorState,
        lifecycle: ProviderObservationBuildLifecycle,
        adds_chunk: bool,
    ) -> Result<Self, SyndicRecordError> {
        if self.lifecycle != ProviderObservationBuildLifecycle::Building
            || canonical_bytes < self.canonical_bytes
            || (lifecycle == ProviderObservationBuildLifecycle::Sealed && adds_chunk)
        {
            return Err(SyndicRecordError::InvalidProviderObservationFrontier);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "provider-observation build revision",
            })?;
        let chunk_count = self.chunk_count.checked_add(u64::from(adds_chunk)).ok_or(
            SyndicRecordError::LengthOverflow {
                kind: "provider-observation chunk frontier",
            },
        )?;
        Self::from_stored_parts(
            self.identity,
            self.begin,
            revision,
            chunk_count,
            canonical_bytes,
            digest,
            validator,
            lifecycle,
        )
    }

    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.identity
    }

    #[must_use]
    pub const fn begin(&self) -> ProviderObservationBegin {
        self.begin
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn chunk_count(&self) -> u64 {
        self.chunk_count
    }

    #[must_use]
    pub const fn canonical_bytes(&self) -> u64 {
        self.canonical_bytes
    }

    #[must_use]
    pub const fn digest(&self) -> ProviderObservationDigest {
        self.digest
    }

    pub(crate) const fn validator(&self) -> &ProviderObservationValidatorState {
        &self.validator
    }

    #[must_use]
    pub const fn lifecycle(&self) -> ProviderObservationBuildLifecycle {
        self.lifecycle
    }

    /// Returns the monotonic complete-history support retained by this observation.
    #[must_use]
    pub const fn history_support(&self) -> crate::ProviderFrameHistorySupportV1 {
        self.validator.history_support()
    }
}

/// One bounded self-delimiting typed fragment of an unpublished observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderObservationChunkPayload {
    Control(ProviderObservationControl),
    Fragment {
        context: ProviderValueContext,
        bytes: Box<[u8]>,
    },
}

/// One immutable bounded staged observation chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationChunkRecord {
    identity: ProviderObservationId,
    ordinal: u64,
    payload: ProviderObservationChunkPayload,
}

impl ProviderObservationChunkRecord {
    pub(crate) fn control(
        identity: ProviderObservationId,
        ordinal: u64,
        control: ProviderObservationControl,
    ) -> Result<Self, SyndicRecordError> {
        Self::new(
            identity,
            ordinal,
            ProviderObservationChunkPayload::Control(control),
        )
    }

    pub(crate) fn fragment(
        identity: ProviderObservationId,
        ordinal: u64,
        context: ProviderValueContext,
        bytes: &[u8],
    ) -> Result<Self, SyndicRecordError> {
        if bytes.is_empty() {
            return Err(SyndicRecordError::Empty {
                kind: "provider-observation fragment",
            });
        }
        if bytes.len() > crate::PROVIDER_OBSERVATION_CHUNK_MAX_BYTES {
            return Err(SyndicRecordError::BytesTooLong {
                kind: "provider-observation fragment",
                maximum: crate::PROVIDER_OBSERVATION_CHUNK_MAX_BYTES,
                actual: bytes.len(),
            });
        }
        Self::new(
            identity,
            ordinal,
            ProviderObservationChunkPayload::Fragment {
                context,
                bytes: bytes.into(),
            },
        )
    }

    fn new(
        identity: ProviderObservationId,
        ordinal: u64,
        payload: ProviderObservationChunkPayload,
    ) -> Result<Self, SyndicRecordError> {
        if ordinal == 0 {
            return Err(SyndicRecordError::ZeroValue {
                kind: "provider-observation chunk ordinal",
            });
        }
        Ok(Self {
            identity,
            ordinal,
            payload,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.identity
    }

    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    #[must_use]
    pub const fn payload(&self) -> &ProviderObservationChunkPayload {
        &self.payload
    }
}
