use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, HomeCandidateRecoveryAccess, HomeStore,
};
use beryl_model::SyndicItemId;

use super::access::ReadAccess;
use crate::{
    ItemProjectionBuildRecord, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    codec::{ItemProjectionBuildsCodec, ItemProjectionSetKey},
    domain::SyndicDomain,
};

impl SyndicStorage {
    pub fn latest_item_projection_build(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionBuildRecord>, SyndicReadError> {
        self.latest_item_projection_build_with_access(ReadAccess::Ordinary(store), item, limit)
    }

    pub fn latest_item_projection_build_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        item: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionBuildRecord>, SyndicReadError> {
        self.latest_item_projection_build_with_access(ReadAccess::Candidate(store), item, limit)
    }

    fn latest_item_projection_build_with_access(
        &self,
        store: ReadAccess<'_>,
        item: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionBuildRecord>, SyndicReadError> {
        let page = store.read_cursor::<SyndicDomain, ItemProjectionBuildsCodec>(
            &self.handle,
            &CursorRange::closed(
                ItemProjectionSetKey::first_for_item(item),
                ItemProjectionSetKey::last_for_item(item),
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, limit.max_bytes())
                .expect("projection progress bound is nonzero"),
        )?;
        Ok(page.records().first().map(|record| record.value().clone()))
    }
}
