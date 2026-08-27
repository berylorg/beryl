use beryl_home_store::HomeStore;

use crate::SyndicStorage;
use crate::codec::Family;

use super::super::*;

pub(in super::super) struct StagingWindowAcquisitionReader<'a> {
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
    reads: usize,
    encoded_value_bytes: usize,
}

impl<'a> StagingWindowAcquisitionReader<'a> {
    pub(in super::super) const fn new(storage: &'a SyndicStorage, store: &'a HomeStore) -> Self {
        Self {
            storage,
            store,
            reads: 0,
            encoded_value_bytes: 0,
        }
    }

    pub(in super::super) fn point<F: Family>(
        &mut self,
        key: F::Key,
    ) -> Result<Option<F::Value>, DraftPiecePrepareErrorV1> {
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        self.encoded_value_bytes = self
            .encoded_value_bytes
            .checked_add(DRAFT_PIECE_PAGE_MAX_BYTES)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if self.reads > DRAFT_PIECE_BUILD_WINDOW_MAX_READS
            || self.encoded_value_bytes > DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        self.storage
            .point::<F>(
                self.store,
                key,
                crate::SyndicPointReadLimit::new(DRAFT_PIECE_PAGE_MAX_BYTES)
                    .expect("staging acquisition point limit is nonzero"),
            )
            .map_err(DraftPiecePrepareErrorV1::from)
    }

    pub(in super::super) const fn reads(&self) -> usize {
        self.reads
    }

    pub(in super::super) const fn encoded_value_bytes(&self) -> usize {
        self.encoded_value_bytes
    }
}
