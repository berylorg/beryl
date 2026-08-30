use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, MutationContribution, ReadError,
    ReconciliationReservation,
};
use beryl_model::DomainRevision;

use crate::{
    ProviderObservationStageBatch, ProviderObservationStageBatchError, SyndicStorage, codec::*,
    domain::SyndicDomain,
};

/// Why an unpublished provider-observation transition was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Batch(#[from] ProviderObservationStageBatchError),
    #[error("provider-observation build identity is already occupied")]
    BuildIdentityCollision,
    #[error("provider-observation build is missing")]
    BuildMissing,
    #[error("provider-observation build does not equal the batch's expected frontier")]
    BuildConflict,
    #[error("provider-observation chunk identity is already occupied")]
    ChunkIdentityCollision,
}

impl DomainCallbackError for ProviderObservationMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl SyndicStorage {
    /// Atomically stages one bounded unpublished observation transition.
    #[must_use]
    pub fn stage_provider_observation_batch(
        &self,
        expected_domain_revision: DomainRevision,
        batch: ProviderObservationStageBatch,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StageProviderObservationMutation { batch },
        )
    }

    /// Stages one batch against writer-admitted current domain state.
    #[must_use]
    pub fn current_stage_provider_observation_batch(
        &self,
        batch: ProviderObservationStageBatch,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(StageProviderObservationMutation { batch })
    }
}

struct StageProviderObservationMutation {
    batch: ProviderObservationStageBatch,
}

#[cfg(feature = "test-faults")]
pub(crate) fn provider_observation_stage_fault_scope() -> beryl_home_store::test_faults::FaultScope
{
    beryl_home_store::test_faults::FaultScope::of::<StageProviderObservationMutation>()
}

impl DomainMutation<SyndicDomain> for StageProviderObservationMutation {
    type Error = ProviderObservationMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.batch.validate_shape()?;
        let identity = self.batch.next_build().identity();
        let current = point::<ProviderObservationBuildsFamily>(reader, &identity)?;
        match self.batch.expected_build() {
            None if current.is_some() => {
                return Err(ProviderObservationMutationError::BuildIdentityCollision);
            }
            None => {}
            Some(_) if current.is_none() => {
                return Err(ProviderObservationMutationError::BuildMissing);
            }
            Some(expected) if current.as_ref() != Some(expected) => {
                return Err(ProviderObservationMutationError::BuildConflict);
            }
            Some(_) => {}
        }
        if let Some(chunk) = self.batch.chunk() {
            let key = ProviderObservationChunkKey::new(chunk.identity(), chunk.ordinal());
            if point::<ProviderObservationChunksFamily>(reader, &key)?.is_some() {
                return Err(ProviderObservationMutationError::ChunkIdentityCollision);
            }
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if self.batch.chunk().is_some() {
            reservation.reserve_records::<ProviderObservationChunksCodec>(1)?;
        }
        reservation.reserve_records::<ProviderObservationBuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(chunk) = prepared.batch.chunk() {
            mutations.put::<ProviderObservationChunksCodec>(
                &ProviderObservationChunkKey::new(chunk.identity(), chunk.ordinal()),
                chunk,
            )?;
        }
        mutations.put::<ProviderObservationBuildsCodec>(
            &prepared.batch.next_build().identity(),
            prepared.batch.next_build(),
        )?;
        Ok(())
    }
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, ProviderObservationMutationError> {
    reader
        .point::<ExactCodec<F>>(key, crate::codec::family_point_limit::<F>())
        .map_err(Into::into)
}
