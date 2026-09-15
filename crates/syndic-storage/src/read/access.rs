use beryl_home_store::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, DomainHandle,
    HomeCandidateRecoveryAccess, HomeStore, PointReadLimit, ReadError, RecordCodec, StorageDomain,
};
use beryl_model::{BerylHomeId, DomainRevision};

use crate::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    codec::{ExactCodec, Family},
};

#[derive(Clone, Copy)]
pub(crate) enum ReadAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl ReadAccess<'_> {
    pub(crate) fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(store) => store.home_id(),
        }
    }

    pub(crate) fn read_point<D: StorageDomain, R: RecordCodec<D>>(
        self,
        handle: &DomainHandle<D>,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        match self {
            Self::Ordinary(store) => store.read_point::<D, R>(handle, key, limit),
            Self::Candidate(store) => store.read_point::<D, R>(handle, key, limit),
        }
    }

    pub(crate) fn read_cursor<D: StorageDomain, R: RecordCodec<D>>(
        self,
        handle: &DomainHandle<D>,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        match self {
            Self::Ordinary(store) => store.read_cursor::<D, R>(handle, range, direction, limits),
            Self::Candidate(store) => store.read_cursor::<D, R>(handle, range, direction, limits),
        }
    }
}

impl SyndicStorage {
    pub(crate) fn revision_with_access(
        &self,
        store: ReadAccess<'_>,
    ) -> Result<DomainRevision, ReadError> {
        match store {
            ReadAccess::Ordinary(store) => self.revision(store),
            ReadAccess::Candidate(store) => store.domain_revision(&self.handle),
        }
    }

    pub(crate) fn point_with_access<F: Family>(
        &self,
        store: ReadAccess<'_>,
        key: F::Key,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_syndic_point_read();
        store
            .read_point::<crate::domain::SyndicDomain, ExactCodec<F>>(
                &self.handle,
                &key,
                PointReadLimit::new(limit.max_bytes()).expect("point bound is nonzero"),
            )
            .map_err(Into::into)
    }
}
