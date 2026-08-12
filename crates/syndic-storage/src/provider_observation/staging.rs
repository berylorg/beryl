use beryl_home_store::{CommandError, CommandOutcome, CommitReceipt, ReconciliationCustody};
use beryl_model::ProviderObservationId;
use std::fmt;
#[cfg(feature = "test-faults")]
use std::sync::{Arc, Weak};

use super::{
    CanonicalObservationError, CanonicalObservationState, ProviderObservationBegin,
    ProviderObservationBuildLifecycle, ProviderObservationBuildRecord,
    ProviderObservationChunkRecord, ProviderObservationControl, ProviderObservationValidatorError,
    ProviderObservationValidatorState, ProviderValueContext, SealedProviderObservationHandle,
};

/// Maximum bytes in one caller-owned provider fragment and one durable fragment record.
pub const PROVIDER_OBSERVATION_CHUNK_MAX_BYTES: usize = 65_536;

/// One bounded borrowed typed fragment offered to the staging sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderObservationStagingBytes<'a> {
    context: ProviderValueContext,
    bytes: &'a [u8],
}

impl<'a> ProviderObservationStagingBytes<'a> {
    pub fn new(
        context: ProviderValueContext,
        bytes: &'a [u8],
    ) -> Result<Self, ProviderObservationStageBatchError> {
        if bytes.is_empty() {
            return Err(ProviderObservationStageBatchError::EmptyFragment);
        }
        if bytes.len() > PROVIDER_OBSERVATION_CHUNK_MAX_BYTES {
            return Err(ProviderObservationStageBatchError::FragmentTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(Self { context, bytes })
    }

    #[must_use]
    pub const fn context(self) -> ProviderValueContext {
        self.context
    }

    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }
}

/// Exact durable position of one begin, append, or seal batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationStageBatchState {
    Expected,
    Next,
    Conflict,
}

/// One bounded atomic unpublished-observation frontier transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationStageBatch {
    expected: Option<ProviderObservationBuildRecord>,
    next: ProviderObservationBuildRecord,
    chunk: Option<ProviderObservationChunkRecord>,
}

impl ProviderObservationStageBatch {
    pub(crate) fn begin(next: ProviderObservationBuildRecord) -> Self {
        Self {
            expected: None,
            next,
            chunk: None,
        }
    }

    pub(crate) fn advance(
        expected: ProviderObservationBuildRecord,
        next: ProviderObservationBuildRecord,
        chunk: Option<ProviderObservationChunkRecord>,
    ) -> Self {
        Self {
            expected: Some(expected),
            next,
            chunk,
        }
    }

    #[must_use]
    pub const fn expected_build(&self) -> Option<&ProviderObservationBuildRecord> {
        self.expected.as_ref()
    }

    #[must_use]
    pub const fn next_build(&self) -> &ProviderObservationBuildRecord {
        &self.next
    }

    #[must_use]
    pub const fn chunk(&self) -> Option<&ProviderObservationChunkRecord> {
        self.chunk.as_ref()
    }

    /// Classifies a point-read current build without inferring durability from a receipt.
    #[must_use]
    pub fn classify_current(
        &self,
        current: Option<&ProviderObservationBuildRecord>,
    ) -> ProviderObservationStageBatchState {
        if current == self.expected.as_ref() {
            ProviderObservationStageBatchState::Expected
        } else if current == Some(&self.next) {
            ProviderObservationStageBatchState::Next
        } else {
            ProviderObservationStageBatchState::Conflict
        }
    }

    pub(crate) fn validate_shape(&self) -> Result<(), ProviderObservationStageBatchError> {
        match (&self.expected, &self.chunk) {
            (None, None)
                if self.next.revision() == 1
                    && self.next.chunk_count() == 0
                    && self.next.lifecycle() == ProviderObservationBuildLifecycle::Building =>
            {
                Ok(())
            }
            (Some(expected), Some(chunk))
                if expected.lifecycle() == ProviderObservationBuildLifecycle::Building
                    && self.next.lifecycle() == ProviderObservationBuildLifecycle::Building
                    && self.next.identity() == expected.identity()
                    && self.next.begin() == expected.begin()
                    && self.next.revision() == expected.revision().checked_add(1).unwrap_or(0)
                    && self.next.chunk_count()
                        == expected.chunk_count().checked_add(1).unwrap_or(0)
                    && chunk.identity() == expected.identity()
                    && chunk.ordinal() == self.next.chunk_count() =>
            {
                Ok(())
            }
            (Some(expected), None)
                if expected.lifecycle() == ProviderObservationBuildLifecycle::Building
                    && self.next.lifecycle() == ProviderObservationBuildLifecycle::Sealed
                    && self.next.identity() == expected.identity()
                    && self.next.begin() == expected.begin()
                    && self.next.revision() == expected.revision().checked_add(1).unwrap_or(0)
                    && self.next.chunk_count() == expected.chunk_count() =>
            {
                Ok(())
            }
            _ => Err(ProviderObservationStageBatchError::InvalidTransition),
        }
    }
}

/// Synchronous exact durable-outcome boundary.
pub trait ProviderObservationStageCallback {
    /// Classifies the offered batch without hiding its durable result.
    fn stage_batch(&mut self, batch: &ProviderObservationStageBatch) -> CommandOutcome;
}

impl<F> ProviderObservationStageCallback for F
where
    F: FnMut(&ProviderObservationStageBatch) -> CommandOutcome,
{
    fn stage_batch(&mut self, batch: &ProviderObservationStageBatch) -> CommandOutcome {
        self(batch)
    }
}

/// Exact durable outcome of one provider-observation staging step.
#[derive(Debug)]
pub enum ProviderObservationStageOutcome<T> {
    /// The offered batch definitely did not commit and state did not advance.
    NotCommitted { evidence: CommandError },
    /// The batch committed and the returned state is its exact durable successor.
    Committed {
        value: T,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    /// The offered batch may have committed and state did not advance locally.
    Indeterminate {
        failure: CommandError,
        reconciliation: ReconciliationCustody,
    },
}

/// Exact durable outcome of consuming one provider-observation seal attempt.
pub enum ProviderObservationSealOutcome {
    /// The seal definitely did not commit and exposes no sealed authority.
    NotCommitted { evidence: CommandError },
    /// The seal committed and returned its exact durable sealed handle.
    Committed {
        value: SealedProviderObservationHandle,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    /// The seal may have committed and retains its consumed stager until custody installation.
    Indeterminate {
        failure: CommandError,
        custody: ProviderObservationSealCustodyGuard,
    },
}

impl fmt::Debug for ProviderObservationSealOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCommitted { evidence } => formatter
                .debug_struct("NotCommitted")
                .field("evidence", evidence)
                .finish(),
            Self::Committed {
                receipt,
                later_failure,
                ..
            } => formatter
                .debug_struct("Committed")
                .field("receipt", receipt)
                .field("later_failure", later_failure)
                .finish_non_exhaustive(),
            Self::Indeterminate { failure, .. } => formatter
                .debug_struct("Indeterminate")
                .field("failure", failure)
                .finish_non_exhaustive(),
        }
    }
}

/// Move-only terminal custody for an indeterminate consuming seal.
#[must_use = "indeterminate seal custody must be installed synchronously"]
pub struct ProviderObservationSealCustodyGuard {
    reconciliation: ReconciliationCustody,
    stager: ProviderObservationStager,
}

impl ProviderObservationSealCustodyGuard {
    /// Installs home custody before releasing the inert consumed stager.
    pub fn install(self) {
        let Self {
            reconciliation,
            stager,
        } = self;
        reconciliation.install();
        drop(stager);
    }
}

/// Consuming, non-cloneable unpublished observation stager.
pub struct ProviderObservationStager {
    current: ProviderObservationBuildRecord,
    validator: ProviderObservationValidatorState,
    canonical: CanonicalObservationState,
    #[cfg(feature = "test-faults")]
    lifetime: Arc<()>,
}

impl ProviderObservationStager {
    /// Begins one caller-identified durable unpublished observation.
    pub fn begin<C: ProviderObservationStageCallback>(
        identity: ProviderObservationId,
        begin: ProviderObservationBegin,
        callback: &mut C,
    ) -> Result<ProviderObservationStageOutcome<Self>, ProviderObservationStagingError> {
        let canonical = CanonicalObservationState::initial(begin);
        let current = ProviderObservationBuildRecord::initial(
            identity,
            begin,
            canonical.canonical_bytes(),
            canonical.digest(),
        );
        let batch = ProviderObservationStageBatch::begin(current.clone());
        match callback.stage_batch(&batch) {
            CommandOutcome::NotCommitted { evidence } => {
                Ok(ProviderObservationStageOutcome::NotCommitted { evidence })
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
            } => Ok(ProviderObservationStageOutcome::Committed {
                value: Self {
                    current,
                    validator: ProviderObservationValidatorState::initial(),
                    canonical,
                    #[cfg(feature = "test-faults")]
                    lifetime: Arc::new(()),
                },
                receipt,
                later_failure,
            }),
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Ok(ProviderObservationStageOutcome::Indeterminate {
                failure,
                reconciliation,
            }),
        }
    }

    pub(crate) fn from_replayed(
        current: ProviderObservationBuildRecord,
        validator: ProviderObservationValidatorState,
        canonical: CanonicalObservationState,
    ) -> Result<Self, ProviderObservationStageBatchError> {
        if current.lifecycle() != ProviderObservationBuildLifecycle::Building
            || current.validator() != &validator
            || current.canonical_bytes() != canonical.canonical_bytes()
            || current.digest() != canonical.digest()
        {
            return Err(ProviderObservationStageBatchError::ReplayMismatch);
        }
        Ok(Self {
            current,
            validator,
            canonical,
            #[cfg(feature = "test-faults")]
            lifetime: Arc::new(()),
        })
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn lifetime_probe(&self) -> Weak<()> {
        Arc::downgrade(&self.lifetime)
    }

    /// Stages one bounded typed structural control.
    pub fn control<C: ProviderObservationStageCallback>(
        &mut self,
        control: ProviderObservationControl,
        callback: &mut C,
    ) -> Result<ProviderObservationStageOutcome<()>, ProviderObservationStagingError> {
        let mut validator = self.validator.clone();
        validator.control(self.current.begin(), control)?;
        let mut canonical = self.canonical.clone();
        canonical.control(control).map_err(map_canonical)?;
        let ordinal = self
            .current
            .chunk_count()
            .checked_add(1)
            .ok_or(ProviderObservationStageBatchError::FrontierOverflow)?;
        let chunk =
            ProviderObservationChunkRecord::control(self.current.identity(), ordinal, control)?;
        self.commit_append(validator, canonical, chunk, callback)
    }

    /// Stages one nonempty bounded UTF-8 fragment for the currently open typed field.
    pub fn fragment<C: ProviderObservationStageCallback>(
        &mut self,
        fragment: ProviderObservationStagingBytes<'_>,
        callback: &mut C,
    ) -> Result<ProviderObservationStageOutcome<()>, ProviderObservationStagingError> {
        let mut validator = self.validator.clone();
        for byte in fragment.bytes() {
            validator.fragment_byte(fragment.context(), *byte)?;
        }
        let mut canonical = self.canonical.clone();
        canonical
            .fragment(fragment.bytes())
            .map_err(map_canonical)?;
        let ordinal = self
            .current
            .chunk_count()
            .checked_add(1)
            .ok_or(ProviderObservationStageBatchError::FrontierOverflow)?;
        let chunk = ProviderObservationChunkRecord::fragment(
            self.current.identity(),
            ordinal,
            fragment.context(),
            fragment.bytes(),
        )?;
        self.commit_append(validator, canonical, chunk, callback)
    }

    fn commit_append<C: ProviderObservationStageCallback>(
        &mut self,
        validator: ProviderObservationValidatorState,
        canonical: CanonicalObservationState,
        chunk: ProviderObservationChunkRecord,
        callback: &mut C,
    ) -> Result<ProviderObservationStageOutcome<()>, ProviderObservationStagingError> {
        let next = self.current.advance(
            canonical.canonical_bytes(),
            canonical.digest(),
            validator.clone(),
            ProviderObservationBuildLifecycle::Building,
            true,
        )?;
        let batch =
            ProviderObservationStageBatch::advance(self.current.clone(), next.clone(), Some(chunk));
        match callback.stage_batch(&batch) {
            CommandOutcome::NotCommitted { evidence } => {
                Ok(ProviderObservationStageOutcome::NotCommitted { evidence })
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
            } => {
                self.current = next;
                self.validator = validator;
                self.canonical = canonical;
                Ok(ProviderObservationStageOutcome::Committed {
                    value: (),
                    receipt,
                    later_failure,
                })
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Ok(ProviderObservationStageOutcome::Indeterminate {
                failure,
                reconciliation,
            }),
        }
    }

    /// Seals the exact structurally complete observation and consumes the stager.
    pub fn seal<C: ProviderObservationStageCallback>(
        self,
        callback: &mut C,
    ) -> Result<ProviderObservationSealOutcome, ProviderObservationStagingError> {
        self.validator.finish(self.current.begin())?;
        let next = self.current.advance(
            self.canonical.canonical_bytes(),
            self.canonical.digest(),
            self.validator.clone(),
            ProviderObservationBuildLifecycle::Sealed,
            false,
        )?;
        let batch =
            ProviderObservationStageBatch::advance(self.current.clone(), next.clone(), None);
        match callback.stage_batch(&batch) {
            CommandOutcome::NotCommitted { evidence } => {
                Ok(ProviderObservationSealOutcome::NotCommitted { evidence })
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
            } => Ok(ProviderObservationSealOutcome::Committed {
                value: SealedProviderObservationHandle::from_build(&next),
                receipt,
                later_failure,
            }),
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Ok(ProviderObservationSealOutcome::Indeterminate {
                failure,
                custody: ProviderObservationSealCustodyGuard {
                    reconciliation,
                    stager: self,
                },
            }),
        }
    }

    /// Explicitly abandons this unpublished generation without a durable mutation.
    pub fn abandon(self) {}
}

fn map_canonical(_: CanonicalObservationError) -> ProviderObservationStageBatchError {
    ProviderObservationStageBatchError::FrontierOverflow
}

/// Why one bounded batch shape or replay state was invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderObservationStageBatchError {
    #[error("provider-observation fragment must be nonempty")]
    EmptyFragment,
    #[error("provider-observation fragment has {actual} bytes; maximum is 65536")]
    FragmentTooLarge { actual: usize },
    #[error("provider-observation batch transition is not canonical")]
    InvalidTransition,
    #[error("provider-observation frontier overflowed")]
    FrontierOverflow,
    #[error("replayed provider-observation state disagrees with its durable build")]
    ReplayMismatch,
}

/// Why unpublished staging could not advance one exact durable frontier.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationStagingError {
    #[error(transparent)]
    Validation(#[from] ProviderObservationValidatorError),
    #[error(transparent)]
    Batch(#[from] ProviderObservationStageBatchError),
    #[error(transparent)]
    Record(#[from] crate::SyndicRecordError),
}

pub(crate) fn replay_chunk(
    begin: ProviderObservationBegin,
    validator: &mut ProviderObservationValidatorState,
    canonical: &mut CanonicalObservationState,
    chunk: &ProviderObservationChunkRecord,
) -> Result<(), ProviderObservationStageBatchError> {
    match chunk.payload() {
        super::ProviderObservationChunkPayload::Control(control) => validator
            .control(begin, *control)
            .map_err(|_| ProviderObservationStageBatchError::ReplayMismatch)?,
        super::ProviderObservationChunkPayload::Fragment { context, bytes } => {
            for byte in bytes.iter().copied() {
                validator
                    .fragment_byte(*context, byte)
                    .map_err(|_| ProviderObservationStageBatchError::ReplayMismatch)?;
            }
        }
    }
    canonical
        .apply_chunk(chunk.payload())
        .map_err(|_| ProviderObservationStageBatchError::FrontierOverflow)
}
