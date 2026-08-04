use beryl_backend::{StreamedInputSourceError, StreamedInputSourceIdentity};
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::{BerylHomeId, RuntimeMode, SyndicItemId, SyndicThreadId};
use beryl_state::AssetOwner;
use sha2::{Digest, Sha256};
use syndic_storage::{AcceptedInputRecord, ContentReference, SyndicStorage};

use super::{InputReplayPrepareError, point_limit};
use crate::cas_projection::LoadedCasProjection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct InputReplayContext {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    runtime_mode: RuntimeMode,
}

impl InputReplayContext {
    pub(in crate::cas_projection) fn from_projection(projection: &LoadedCasProjection) -> Self {
        Self {
            home_id: projection.home_id(),
            home_generation: projection.home_generation(),
            runtime_mode: projection.execution_binding().root_path().mode().clone(),
        }
    }

    pub(in crate::cas_projection) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        runtime_mode: RuntimeMode,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            runtime_mode,
        }
    }

    pub(super) const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    pub(super) const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(super) const fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub(super) fn check_home(&self, store: &HomeStore) -> Result<(), InputReplayPrepareError> {
        let health = store.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(InputReplayPrepareError::HomeNotHealthy {
                state: health.state(),
                expected_home_id: self.home_id,
                actual_home_id: store.home_id(),
                expected_generation: self.home_generation,
                actual_generation: health.generation(),
            });
        }
        let Some(actual_generation) = health.generation() else {
            return Err(InputReplayPrepareError::HealthyHomeGenerationMissing);
        };
        if store.home_id() != self.home_id {
            return Err(InputReplayPrepareError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: store.home_id(),
            });
        }
        if actual_generation != self.home_generation {
            return Err(InputReplayPrepareError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: Some(actual_generation),
                state: health.state(),
            });
        }
        Ok(())
    }

    pub(super) fn check_home_source(
        &self,
        store: &HomeStore,
        expected: StreamedInputSourceIdentity,
        actual: impl FnOnce(BerylHomeId, HomeGeneration) -> StreamedInputSourceIdentity,
    ) -> Result<(), StreamedInputSourceError> {
        let health = store.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(StreamedInputSourceError::ReadFailed);
        }
        let Some(generation) = health.generation() else {
            return Err(StreamedInputSourceError::ReadFailed);
        };
        if store.home_id() != self.home_id || generation != self.home_generation {
            return Err(StreamedInputSourceError::SourceIdentityMismatch {
                expected,
                actual: actual(store.home_id(), generation),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum InputReplayRecord {
    Submitted {
        thread_id: SyndicThreadId,
        item_id: SyndicItemId,
    },
    Accepted(AcceptedInputRecord),
}

impl InputReplayRecord {
    pub(in crate::cas_projection) const fn submitted(
        thread_id: SyndicThreadId,
        item_id: SyndicItemId,
    ) -> Self {
        Self::Submitted { thread_id, item_id }
    }

    pub(in crate::cas_projection) const fn accepted(record: AcceptedInputRecord) -> Self {
        Self::Accepted(record)
    }

    pub(super) const fn thread_id(&self) -> SyndicThreadId {
        match self {
            Self::Submitted { thread_id, .. } => *thread_id,
            Self::Accepted(record) => record.thread_id(),
        }
    }

    pub(super) const fn asset_owner(&self) -> AssetOwner {
        match self {
            Self::Submitted { item_id, .. } => AssetOwner::SubmittedTurnItem(*item_id),
            Self::Accepted(record) => AssetOwner::AcceptedInput(record.id()),
        }
    }

    pub(super) fn check_content(
        &self,
        content: ContentReference,
    ) -> Result<(), InputReplayPrepareError> {
        if let Self::Accepted(record) = self
            && record.content() != content
        {
            return Err(InputReplayPrepareError::AcceptedInputContentMismatch {
                input_id: record.id(),
            });
        }
        Ok(())
    }

    pub(super) fn check_durable(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
    ) -> Result<(), InputReplayPrepareError> {
        let Self::Accepted(expected) = self else {
            return Ok(());
        };
        let actual = storage.accepted_input(store, expected.id(), point_limit())?;
        match actual {
            None => Err(InputReplayPrepareError::AcceptedInputMissing {
                input_id: expected.id(),
            }),
            Some(actual) if actual != *expected => {
                Err(InputReplayPrepareError::AcceptedInputChanged {
                    input_id: expected.id(),
                })
            }
            Some(_) => Ok(()),
        }
    }

    pub(super) fn check_durable_source(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
    ) -> Result<(), StreamedInputSourceError> {
        let Self::Accepted(expected) = self else {
            return Ok(());
        };
        match storage.accepted_input(store, expected.id(), point_limit()) {
            Ok(Some(actual)) if actual == *expected => Ok(()),
            Ok(_) => Err(StreamedInputSourceError::InvalidSource),
            Err(_) => Err(StreamedInputSourceError::ReadFailed),
        }
    }

    pub(super) fn hash_into(&self, hasher: &mut Sha256) {
        match self {
            Self::Submitted { thread_id, item_id } => {
                hasher.update([0_u8]);
                hasher.update(thread_id.as_bytes());
                hasher.update(item_id.as_bytes());
            }
            Self::Accepted(record) => {
                let admission = record.admission();
                hasher.update([1_u8]);
                hasher.update(record.id().as_bytes());
                hasher.update(record.thread_id().as_bytes());
                hasher.update(record.ordinal().get().to_be_bytes());
                hasher.update(admission.expected_thread_revision().get().to_be_bytes());
                hasher.update(admission.source_draft_id().as_bytes());
                hasher.update(admission.expected_draft_revision().get().to_be_bytes());
                hasher.update(admission.expected_gate_revision().get().to_be_bytes());
                hasher.update(admission.replacement_draft_id().as_bytes());
                hasher.update(record.route_generation().get().to_be_bytes());
                hasher.update(record.admitted_at().unix_millis().to_be_bytes());
            }
        }
    }
}
