use std::{
    collections::BTreeSet,
    sync::{Mutex, OnceLock},
};

use beryl_home_store::MutationContribution;
use beryl_model::DomainRevision;

use crate::SyndicStorage;

use super::*;

pub struct DraftPieceUnadmittedMarkerBuilderForTest<'storage> {
    storage: &'storage SyndicStorage,
}

impl SyndicStorage {
    pub fn unadmitted_marker_builder_for_test(
        &self,
    ) -> DraftPieceUnadmittedMarkerBuilderForTest<'_> {
        DraftPieceUnadmittedMarkerBuilderForTest { storage: self }
    }
}

impl DraftPieceUnadmittedMarkerBuilderForTest<'_> {
    pub fn prepare_staging_page_batch(
        &self,
        head: &DraftMutationStagingHeadV1,
        session: &DraftEditorCandidateSessionV1,
        inputs: Box<[DraftMutationStagingPageInputV1]>,
    ) -> Result<PreparedDraftMutationStagingBatchV1, DraftMutationStagingErrorV1> {
        let prepared = self
            .storage
            .prepare_unadmitted_marker_staging_page_batch_for_test(head, session, inputs)?;
        authorize_unadmitted_marker_builder_for_test(DraftPieceSettlementKeyV1::new(
            head.identity().draft_id(),
            head.identity().session_id(),
            head.identity().operation_id().as_piece_operation(),
        ));
        Ok(prepared)
    }

    pub fn prepare_fragment(
        &self,
        prepared: &PreparedDraftPieceEditV1,
        ordinal: u64,
        preceding_chain: DraftPieceDigestV1,
        replacement: DraftPieceReplacementV1,
    ) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
        let fragment = self.storage.prepare_unadmitted_marker_fragment_for_test(
            prepared,
            ordinal,
            preceding_chain,
            replacement,
        )?;
        authorize_unadmitted_marker_builder_for_test(DraftPieceSettlementKeyV1::new(
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        ));
        Ok(fragment)
    }

    pub fn stage_fragment(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceEditV1,
        fragment: DraftPieceBuildFragmentV1,
    ) -> MutationContribution {
        self.storage.stage_unadmitted_marker_fragment_for_test(
            expected_domain_revision,
            prepared,
            fragment,
        )
    }
}

fn unadmitted_marker_builder_authorities_for_test()
-> &'static Mutex<BTreeSet<DraftPieceSettlementKeyV1>> {
    static AUTHORITIES: OnceLock<Mutex<BTreeSet<DraftPieceSettlementKeyV1>>> = OnceLock::new();
    AUTHORITIES.get_or_init(|| Mutex::new(BTreeSet::new()))
}

fn authorize_unadmitted_marker_builder_for_test(key: DraftPieceSettlementKeyV1) {
    unadmitted_marker_builder_authorities_for_test()
        .lock()
        .expect("unadmitted-marker test authority lock is available")
        .insert(key);
}

pub(crate) fn unadmitted_marker_builder_is_authorized_for_test(
    key: DraftPieceSettlementKeyV1,
) -> bool {
    unadmitted_marker_builder_authorities_for_test()
        .lock()
        .expect("unadmitted-marker test authority lock is available")
        .contains(&key)
}
