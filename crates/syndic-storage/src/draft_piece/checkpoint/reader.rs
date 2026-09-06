use super::*;

pub(super) trait CheckpointReader {
    type Error;

    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error>;

    fn authenticate_frontier(
        &self,
        frontier: &DraftEditHistoryFrontierV1,
    ) -> Result<bool, Self::Error>;

    fn authenticate_progress(
        &self,
        receipt: &DraftPieceBuildProgressReceiptV1,
    ) -> Result<bool, Self::Error>;
}

pub(super) struct StoreCheckpointReader<'a> {
    pub storage: &'a SyndicStorage,
    pub store: &'a HomeStore,
}

impl CheckpointReader for StoreCheckpointReader<'_> {
    type Error = SyndicReadError;

    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error> {
        self.storage
            .point::<F>(self.store, key.clone(), point_limit())
    }

    fn authenticate_frontier(
        &self,
        frontier: &DraftEditHistoryFrontierV1,
    ) -> Result<bool, Self::Error> {
        draft_edit_history_frontier_is_authenticated_v1(self.storage, self.store, frontier)
    }

    fn authenticate_progress(
        &self,
        receipt: &DraftPieceBuildProgressReceiptV1,
    ) -> Result<bool, Self::Error> {
        session::progress_receipt_closure_is_exact(self.storage, self.store, receipt)
    }
}

impl CheckpointReader for DomainReader<'_, SyndicDomain> {
    type Error = SyndicMutationError;

    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, Self::Error> {
        crate::mutation::point::<F>(self, key)
    }

    fn authenticate_frontier(
        &self,
        frontier: &DraftEditHistoryFrontierV1,
    ) -> Result<bool, Self::Error> {
        authenticate_draft_edit_history_frontier_v1(self, frontier).map(|()| true)
    }

    fn authenticate_progress(
        &self,
        receipt: &DraftPieceBuildProgressReceiptV1,
    ) -> Result<bool, Self::Error> {
        mutation::authenticate_progress_receipt(self, receipt).map(|()| true)
    }
}
