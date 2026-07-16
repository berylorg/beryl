use std::{any::TypeId, error::Error};

use fjall::PersistMode;
use thiserror::Error;

use crate::{
    DomainHandle, HealthGateError, HomeStore, RecordCodec, StorageDomain, health::FailureSeverity,
    writer::ActiveWriter,
};

const MAX_CORRUPTION_FIXTURE_BYTES: usize = 1_048_576;
const MAX_CORRUPTION_KEY_BYTES: usize = u16::MAX as usize;

struct FailClosedOnPanic<'a> {
    store: &'a HomeStore,
}

impl Drop for FailClosedOnPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.store
                .health
                .signal_failure(FailureSeverity::Structural);
        }
    }
}

/// Storage stage used by the feature-gated persisted-corruption seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedCorruptionStage {
    /// Insert the deliberately malformed physical record.
    Insert,
    /// Complete the durability barrier for the malformed record.
    Persist,
}

/// Why a bounded post-registration corruption fixture could not be installed.
#[derive(Debug, Error)]
pub enum PersistedCorruptionError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] HealthGateError),

    /// A panic poisoned the serialized writer.
    #[error("the Beryl-home writer lock is poisoned")]
    WriterPoisoned,

    /// The same thread is already inside this store's serialized writer.
    #[error("reentrant use of the same Beryl-home writer is forbidden")]
    ReentrantWriter,

    /// A panic poisoned or removed the current store generation.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,

    /// The supplied typed handle does not belong to the current generation.
    #[error("domain handle `{domain}` does not belong to this home generation")]
    ForeignDomain {
        /// Stable typed domain name.
        domain: &'static str,
    },

    /// The codec names no family in its exact registered domain.
    #[error("record codec names unknown family `{family}` in domain `{domain}`")]
    UnknownFamily {
        /// Stable typed domain name.
        domain: &'static str,
        /// Unknown logical family.
        family: &'static str,
    },

    /// The registered family belongs to another exact Rust codec type.
    #[error("record codec does not own family `{family}` in domain `{domain}`")]
    CodecTypeMismatch {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family registered to another codec type.
        family: &'static str,
    },

    /// Fjall does not support a physical empty key.
    #[error("the persisted-corruption fixture key must be nonempty")]
    EmptyKey,

    /// The fixture exceeds Fjall's supported physical key ceiling.
    #[error("corruption fixture key has {actual} bytes, exceeding engine limit {maximum}")]
    FixtureKeyBoundExceeded {
        /// Fjall's physical key ceiling.
        maximum: usize,
        /// Requested stored-key bytes.
        actual: usize,
    },

    /// The registered exact codec accepts the requested physical envelope.
    #[error("record codec accepts the fixture envelope for `{domain}`/`{family}`")]
    CodecAcceptedEnvelope {
        /// Stable typed domain name.
        domain: &'static str,
        /// Exact logical family.
        family: &'static str,
    },

    /// The deliberately malformed fixture exceeds the hard test-only bound.
    #[error("corruption fixture has {actual} bytes, exceeding test limit {maximum}")]
    FixtureBoundExceeded {
        /// Hard maximum combined key/value bytes.
        maximum: usize,
        /// Requested combined key/value bytes.
        actual: usize,
    },

    /// Fjall failed while installing or persisting the corruption fixture.
    #[error("persisted-corruption fixture failed during {stage:?}: {source}")]
    Storage {
        /// Exact fixture stage.
        stage: PersistedCorruptionStage,
        /// Engine source hidden behind the package boundary.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

impl HomeStore {
    /// Durably inserts one bounded codec-rejected envelope into an exact live family.
    ///
    /// This feature-gated seam exists only for post-registration read-health,
    /// verification, and same-home recovery proofs. It never exposes the
    /// database or keyspace and rejects every envelope the registered exact
    /// codec accepts.
    pub fn inject_persisted_corrupt_record<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: DomainHandle<D>,
        encoded_key: &[u8],
        encoded_value: &[u8],
    ) -> Result<(), PersistedCorruptionError> {
        if ActiveWriter::already_active(self.writer_id) {
            return Err(PersistedCorruptionError::ReentrantWriter);
        }
        let _writer = self.writer.lock().map_err(|_| {
            self.health.signal_failure(FailureSeverity::Structural);
            PersistedCorruptionError::WriterPoisoned
        })?;
        let _active = ActiveWriter::enter(self.writer_id);
        let admission = self.health.admit()?;
        let _fail_closed_on_panic = FailClosedOnPanic { store: self };
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(PersistedCorruptionError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(PersistedCorruptionError::GenerationPoisoned);
            }
        };
        let domain = generation
            .resolve_domain(handle)
            .ok_or(PersistedCorruptionError::ForeignDomain { domain: D::NAME })?;
        let family = domain
            .family(R::FAMILY)
            .ok_or(PersistedCorruptionError::UnknownFamily {
                domain: D::NAME,
                family: R::FAMILY,
            })?;
        if family.codec_type != TypeId::of::<R>() {
            return Err(PersistedCorruptionError::CodecTypeMismatch {
                domain: D::NAME,
                family: R::FAMILY,
            });
        }
        validate_fixture(encoded_key, encoded_value)?;
        if (family.validate_envelope)(encoded_key, encoded_value).is_ok() {
            return Err(PersistedCorruptionError::CodecAcceptedEnvelope {
                domain: D::NAME,
                family: R::FAMILY,
            });
        }

        if let Err(source) = family.keyspace.insert(encoded_key, encoded_value) {
            admission.fail(FailureSeverity::Verify);
            return Err(storage(PersistedCorruptionStage::Insert, source));
        }
        if let Err(source) = generation.database.persist(PersistMode::SyncAll) {
            admission.fail(FailureSeverity::Verify);
            return Err(storage(PersistedCorruptionStage::Persist, source));
        }
        admission.confirm()?;
        Ok(())
    }
}

fn validate_fixture(
    encoded_key: &[u8],
    encoded_value: &[u8],
) -> Result<(), PersistedCorruptionError> {
    if encoded_key.is_empty() {
        return Err(PersistedCorruptionError::EmptyKey);
    }
    if encoded_key.len() > MAX_CORRUPTION_KEY_BYTES {
        return Err(PersistedCorruptionError::FixtureKeyBoundExceeded {
            maximum: MAX_CORRUPTION_KEY_BYTES,
            actual: encoded_key.len(),
        });
    }
    let actual = encoded_key.len().saturating_add(encoded_value.len());
    if actual > MAX_CORRUPTION_FIXTURE_BYTES {
        return Err(PersistedCorruptionError::FixtureBoundExceeded {
            maximum: MAX_CORRUPTION_FIXTURE_BYTES,
            actual,
        });
    }
    Ok(())
}

fn storage(
    stage: PersistedCorruptionStage,
    source: impl Error + Send + Sync + 'static,
) -> PersistedCorruptionError {
    PersistedCorruptionError::Storage {
        stage,
        source: Box::new(source),
    }
}
