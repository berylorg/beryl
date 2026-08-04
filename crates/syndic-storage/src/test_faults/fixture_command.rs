use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, MutationBuildError, MutationBuilder, MutationContribution,
};
use beryl_model::DomainRevision;

use crate::{codec::*, domain::SyndicDomain, *};

use super::{FixtureDelete, FixtureRecord, fixture_delete::delete_record, fixture_put::put_record};

const MAX_FIXTURE_OPERATIONS: usize = 131_072;

#[derive(Clone, Debug)]
enum FixtureOperation {
    Put(Box<FixtureRecord>),
    Delete(FixtureDelete),
}

/// One bounded exact-domain batch used to seed valid or intentionally inconsistent fixtures.
#[derive(Clone, Debug, Default)]
pub struct FixtureBatch {
    operations: Vec<FixtureOperation>,
}

impl FixtureBatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, record: FixtureRecord) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Put(Box::new(record)))?;
        Ok(self)
    }

    pub fn delete(&mut self, key: FixtureDelete) -> Result<&mut Self, FixtureBuildError> {
        self.push(FixtureOperation::Delete(key))?;
        Ok(self)
    }

    fn push(&mut self, operation: FixtureOperation) -> Result<(), FixtureBuildError> {
        if self.operations.len() == MAX_FIXTURE_OPERATIONS {
            return Err(FixtureBuildError::TooManyOperations);
        }
        self.operations.push(operation);
        Ok(())
    }
}

/// Why a test-only typed fixture batch could not be built.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FixtureBuildError {
    #[error("Syndic fixture exceeds its fixed operation bound")]
    TooManyOperations,
}

#[derive(Debug)]
pub enum FixtureMutationError {
    Build(MutationBuildError),
}

impl fmt::Display for FixtureMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(source) => source.fmt(f),
        }
    }
}

impl Error for FixtureMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(source) => Some(source),
        }
    }
}

impl From<MutationBuildError> for FixtureMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl DomainCallbackError for FixtureMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

impl DomainMutation<SyndicDomain> for FixtureBatch {
    type Error = FixtureMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for operation in &self.operations {
            match operation {
                FixtureOperation::Put(record) => put_record(builder, record.as_ref())?,
                FixtureOperation::Delete(key) => delete_record(builder, key)?,
            }
        }
        Ok(())
    }
}

impl SyndicStorage {
    /// Seals one bounded typed fixture batch against an exact expected domain revision.
    #[must_use]
    pub fn fixture_contribution(
        &self,
        expected_revision: DomainRevision,
        batch: FixtureBatch,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, batch)
    }

    /// Reads one immutable CAS-thread binding membership for fault-cut assertions.
    pub fn fixture_cas_thread_binding_membership(
        &self,
        store: &beryl_home_store::HomeStore,
        cas_thread: beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CasThreadBindingIndexRecord>, SyndicReadError> {
        self.point::<CasThreadBindingIndexFamily>(
            store,
            CasThreadBindingKey::Record(cas_thread, revision),
            limit,
        )
    }

    /// Counts a bounded physical slice of activity entries, including logically retired rows.
    pub fn fixture_activity_query_entry_count(
        &self,
        store: &beryl_home_store::HomeStore,
        thread: beryl_model::SyndicThreadId,
        work_period: ActivityWorkPeriod,
        limits: CursorReadLimits,
    ) -> Result<(usize, bool), SyndicReadError> {
        let page = store.read_cursor::<SyndicDomain, ActivityQueryEntriesCodec>(
            self.handle,
            &CursorRange::closed(
                ActivityQueryEntryKey::first_for_period(thread, work_period),
                ActivityQueryEntryKey::last_for_period(thread, work_period),
            ),
            CursorDirection::Forward,
            limits,
        )?;
        Ok((page.records().len(), page.has_more()))
    }
}
