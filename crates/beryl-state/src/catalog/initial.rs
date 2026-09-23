use std::sync::Arc;

use beryl_home_store::{
    DomainMutation, DomainReader, HomeCandidateRecoveryAccess, MutationBuilder,
    ReconciliationReservation, RecordCodec,
};
use beryl_model::BerylHomeId;

use super::*;

#[derive(Clone)]
pub struct CatalogInitialPublication {
    home_id: BerylHomeId,
    row: Arc<CatalogRow>,
}

impl CatalogInitialPublication {
    pub fn row(&self) -> &CatalogRow {
        &self.row
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogInitialStatus {
    Absent,
    Exact,
    Collision,
}

pub struct PreparedInitialCatalogPublication {
    handle: DomainHandle<CatalogDomain>,
    revision: DomainRevision,
    publication: CatalogInitialPublication,
}

impl PreparedInitialCatalogPublication {
    pub fn publication(&self) -> &CatalogInitialPublication {
        &self.publication
    }

    pub fn contribution(self) -> MutationContribution {
        self.handle.clone().contribution(self.revision, self)
    }
}

impl CatalogState {
    pub fn prepare_initial_publication(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        sources: CatalogSourceRevisions,
        facts: CatalogFacts,
    ) -> Result<PreparedInitialCatalogPublication, CatalogMutationError> {
        let revision = self.revision(store)?;
        let row = CatalogRow::current(thread_id, sources, facts, CatalogRevision::INITIAL)?;
        Ok(PreparedInitialCatalogPublication {
            handle: self.handle.clone(),
            revision,
            publication: CatalogInitialPublication {
                home_id: store.home_id(),
                row: Arc::new(row),
            },
        })
    }

    pub fn initial_publication_status(
        &self,
        store: &HomeStore,
        publication: &CatalogInitialPublication,
    ) -> Result<CatalogInitialStatus, CatalogReadError> {
        self.initial_status(ReadAccess::Ordinary(store), publication)
    }

    pub fn initial_publication_status_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        publication: &CatalogInitialPublication,
    ) -> Result<CatalogInitialStatus, CatalogReadError> {
        self.initial_status(ReadAccess::Candidate(access), publication)
    }

    fn initial_status(
        &self,
        access: ReadAccess<'_>,
        publication: &CatalogInitialPublication,
    ) -> Result<CatalogInitialStatus, CatalogReadError> {
        let before = access.revision(&self.handle)?;
        if access.home_id() != publication.home_id {
            return Err(CatalogReadError::Invariant(
                "initial catalog publication belongs to another home",
            ));
        }
        let expected = publication.row();
        let row = access.point::<CatalogRowCodec>(&self.handle, &expected.thread_id())?;
        let index =
            access.point::<CatalogRecencyCodec>(&self.handle, &expected.recency_cursor())?;
        let after = access.revision(&self.handle)?;
        if before != after {
            return Err(CatalogReadError::RevisionChanged {
                expected: before,
                current: after,
            });
        }
        Ok(match (row, index) {
            (None, None) => CatalogInitialStatus::Absent,
            (Some(row), Some(index)) if &row == expected && &index == expected => {
                CatalogInitialStatus::Exact
            }
            _ => CatalogInitialStatus::Collision,
        })
    }
}

impl DomainMutation<CatalogDomain> for PreparedInitialCatalogPublication {
    type Error = CatalogMutationError;
    type Prepared = CatalogInitialPublication;

    fn prepare(
        self,
        reader: &DomainReader<'_, CatalogDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let row = self.publication.row();
        let thread_id = row.thread_id();
        if reader
            .point::<CatalogRowCodec>(&thread_id, point_limit())?
            .is_some()
        {
            return Err(CatalogMutationError::RowExists { thread_id });
        }
        if reader
            .point::<CatalogRecencyCodec>(&row.recency_cursor(), point_limit())?
            .is_some()
        {
            return Err(CatalogMutationError::IndexExists { thread_id });
        }
        Ok(self.publication)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CatalogRowCodec>(1)?;
        reservation.reserve_records::<CatalogRecencyCodec>(1)?;
        Ok(())
    }

    fn contribute(
        publication: Self::Prepared,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        let row = publication.row();
        mutations.put::<CatalogRowCodec>(&row.thread_id(), row)?;
        mutations.put::<CatalogRecencyCodec>(&row.recency_cursor(), row)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ReadAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl ReadAccess<'_> {
    fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(access) => access.home_id(),
        }
    }

    fn revision(self, handle: &DomainHandle<CatalogDomain>) -> Result<DomainRevision, ReadError> {
        match self {
            Self::Ordinary(store) => store.domain_revision(handle),
            Self::Candidate(access) => access.domain_revision(handle),
        }
    }

    fn point<R: RecordCodec<CatalogDomain, Value = CatalogRow>>(
        self,
        handle: &DomainHandle<CatalogDomain>,
        key: &R::Key,
    ) -> Result<Option<CatalogRow>, ReadError> {
        match self {
            Self::Ordinary(store) => {
                store.read_point::<CatalogDomain, R>(handle, key, point_limit())
            }
            Self::Candidate(access) => {
                access.read_point::<CatalogDomain, R>(handle, key, point_limit())
            }
        }
    }
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(CATALOG_POINT_READ_MAX_BYTES).expect("catalog schema limit is nonzero")
}
