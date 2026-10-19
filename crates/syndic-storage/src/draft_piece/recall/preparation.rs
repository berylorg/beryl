use super::super::import::read::{SourceReader, StoreSourceReader};
use super::super::*;
use super::*;
use crate::SyndicStorage;
use beryl_home_store::{HomeStore, MutationContribution};

impl SyndicStorage {
    pub fn prepare_composer_recall(
        &self,
        store: &HomeStore,
        request: DraftComposerRecallRequestV1,
    ) -> Result<PreparedDraftComposerRecallV1, DraftComposerRecallErrorV1> {
        let revision = self.revision(store)?;
        let basis = self.composer_recall_basis(store, &request.source)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let before = r.session_head(&DraftEditorCandidateSessionRecordKeyV1::head(
            request.activation.draft_id(),
            request.activation.session_id(),
        ))?;
        if DraftEditorCandidateActivationBindingV1::from_head(&before) != request.activation
            || request.source.thread_id() != before.thread_id()
            || before.active_operation().is_some()
            || request.operation.as_bytes() == &[0; 16]
            || request.operation == before.open_operation_id()
            || !session::idle_candidate_closure_is_exact_with_access(
                self,
                crate::read::access::ReadAccess::Ordinary(store),
                &before,
            )?
        {
            return Err(DraftComposerRecallErrorV1::Ineligible);
        }
        require_destination(&r, &before)?;
        validate_position(self, store, before.newest_root(), request.before_caret)?;
        validate_position(self, store, before.newest_root(), request.before_selection)?;
        let end = request.source.content().summary().logical_utf8_bytes();
        let gap = if request.source.content().summary().image_marker_count() == 0 {
            DraftCompositeGapWitnessV1::Unambiguous
        } else {
            let edge = self.draft_piece_marker_demand(
                store,
                request.source.acceptance.candidate().root(),
                DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(end),
                    DraftPieceMarkerDirectionV1::Backward,
                    None,
                    1,
                    65_536,
                ),
            )?;
            if edge.markers().is_empty() {
                DraftCompositeGapWitnessV1::Unambiguous
            } else {
                DraftCompositeGapWitnessV1::AfterAll
            }
        };
        let caret = DraftCompositePositionV1::new(end, gap);
        let header = DraftPieceEditHeaderV1::new(
            before.draft_id(),
            before.session_id(),
            before.newest_candidate_generation(),
            before.newest_root(),
            before.newest_history(),
            request.operation,
            request.before_caret,
            request.before_selection,
            caret,
            caret,
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
        );
        let canonical_header = canonical_recall_header(
            header,
            before.session_generation(),
            &DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis.clone()),
        );
        if canonical_header.len() > 65_536 || self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(PreparedDraftComposerRecallV1 {
            home_generation: self.home_generation,
            scope: RecallScope::capture(self, store)?,
            before,
            source: request.source,
            basis,
            header,
            canonical_header,
        })
    }
    pub fn begin_composer_recall(
        &self,
        revision: beryl_model::DomainRevision,
        prepared: PreparedDraftComposerRecallV1,
    ) -> MutationContribution {
        let generation_matches = prepared.home_generation == self.home_generation;
        prepared.scope.handle.clone().contribution(
            revision,
            command::BeginRecall {
                prepared,
                generation_matches,
            },
        )
    }
}
