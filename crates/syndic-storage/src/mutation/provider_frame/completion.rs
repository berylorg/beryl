use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, MutationContribution, ReadError,
};
use beryl_model::DomainRevision;

use crate::{
    ProviderItemBuildRecord, SyndicStorage,
    codec::*,
    domain::SyndicDomain,
    validation::{ProviderFrameStorageValidationError, advance_provider_completion_comparison},
};

/// Why one bounded completion-equality mutation was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ProviderCompletionComparisonMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error("provider-item completion build is missing")]
    BuildMissing,
    #[error("provider-item completion build changed before comparison")]
    BuildConflict,
    #[error("provider-item completion comparison state is invalid")]
    ComparisonConflict,
    #[error(transparent)]
    StorageRecord(#[from] crate::ProviderStorageRecordError),
}

impl DomainCallbackError for ProviderCompletionComparisonMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl SyndicStorage {
    /// Advances one writer-admitted bounded completion equality page.
    #[must_use]
    pub fn current_compare_provider_completion(
        &self,
        expected: ProviderItemBuildRecord,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(CompareProviderCompletionMutation { expected })
    }

    /// Advances one bounded completion equality page at an exact domain revision.
    #[must_use]
    pub fn compare_provider_completion(
        &self,
        expected_domain_revision: DomainRevision,
        expected: ProviderItemBuildRecord,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            CompareProviderCompletionMutation { expected },
        )
    }
}

struct CompareProviderCompletionMutation {
    expected: ProviderItemBuildRecord,
}

impl DomainMutation<SyndicDomain> for CompareProviderCompletionMutation {
    type Error = ProviderCompletionComparisonMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = current_build(reader, self.expected.item_id())?;
        if current != self.expected {
            return Err(ProviderCompletionComparisonMutationError::BuildConflict);
        }
        derive_next(reader, &current)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let current = current_build(reader, self.expected.item_id())?;
        if current != self.expected {
            return Err(ProviderCompletionComparisonMutationError::BuildConflict);
        }
        let next = derive_next(reader, &current)?;
        mutations.put::<ProviderItemBuildsCodec>(&next.item_id(), &next)?;
        Ok(())
    }
}

fn derive_next(
    reader: &DomainReader<'_, SyndicDomain>,
    current: &ProviderItemBuildRecord,
) -> Result<ProviderItemBuildRecord, ProviderCompletionComparisonMutationError> {
    let state =
        advance_provider_completion_comparison(reader, current).map_err(|error| match error {
            ProviderFrameStorageValidationError::Read(source) => {
                ProviderCompletionComparisonMutationError::Read(source)
            }
            ProviderFrameStorageValidationError::Invariant(_) => {
                ProviderCompletionComparisonMutationError::ComparisonConflict
            }
        })?;
    current.advance_completion(state).map_err(Into::into)
}

fn current_build(
    reader: &DomainReader<'_, SyndicDomain>,
    item_id: beryl_model::SyndicItemId,
) -> Result<ProviderItemBuildRecord, ProviderCompletionComparisonMutationError> {
    reader
        .point::<ProviderItemBuildsCodec>(
            &item_id,
            crate::codec::family_point_limit::<ProviderItemBuildsFamily>(),
        )?
        .ok_or(ProviderCompletionComparisonMutationError::BuildMissing)
}
