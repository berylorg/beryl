use super::super::*;
use super::{
    read::{SourceReader, StoreSourceReader, authenticate_manifest},
    request::*,
};
use crate::{
    CanonicalItemPresentation, DraftSubmissionIntent, InputGateState, SyndicPointReadLimit,
    SyndicStorage, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{DomainReader, HomeStore, MutationContribution};

impl SyndicStorage {
    pub fn prepare_replacement_draft_import(
        &self,
        store: &HomeStore,
        request: DraftReplacementImportRequestV1,
    ) -> Result<PreparedDraftReplacementImportV1, DraftReplacementErrorV1> {
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let head = r
            .point::<DraftEditorCandidateSessionsFamily>(
                &DraftEditorCandidateSessionRecordKeyV1::head(
                    request.candidate.draft_id(),
                    request.candidate.session_id(),
                ),
            )?
            .ok_or(DraftReplacementErrorV1::Ineligible)?;
        let DraftEditorCandidateSessionRecordV1::Head(head) = head else {
            return Err(DraftReplacementErrorV1::Ineligible);
        };
        if DraftEditorCandidateActivationBindingV1::from_head(&head) != request.candidate
            || request.operation == head.open_operation_id()
            || !session::idle_candidate_closure_is_exact_with_access(
                self,
                crate::read::access::ReadAccess::Ordinary(store),
                &head,
            )?
            || !candidate_session_has_saved_identity(&head)
            || head.newest_root().summary().logical_utf8_bytes() != 0
            || head.newest_root().summary().marker_count() != 0
        {
            return Err(DraftReplacementErrorV1::Ineligible);
        }
        let item = self
            .canonical_item(
                store,
                request.item,
                SyndicPointReadLimit::new(65_540).unwrap(),
            )?
            .ok_or(DraftReplacementErrorV1::Ineligible)?;
        let CanonicalItemPresentation::UserInput {
            content,
            asset_reference_set,
        } = item.presentation()
        else {
            return Err(DraftReplacementErrorV1::Ineligible);
        };
        let gate = self
            .input_gate(
                store,
                request.selector.thread_id(),
                SyndicPointReadLimit::new(65_540).unwrap(),
            )?
            .ok_or(DraftReplacementErrorV1::Ineligible)?;
        if *content != request.content
            || asset_reference_set.as_deref().copied() != request.assets
            || gate.revision() != request.gate_revision
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let basis = DraftSealedImportBasisV1 {
            selector: request.selector,
            gate_revision: request.gate_revision,
            intent: request.intent,
            item_id: request.item,
            item_revision: request.revision,
            content: request.content,
            assets: request.assets,
        };
        if !basis.is_locally_exact()
            || request.operation.as_bytes() == &[0; 16]
            || request.candidate.session_id().as_bytes() == &[0; 16]
            || store.health().generation() != Some(self.home_generation)
            || gate.state() != &InputGateState::Idle
            || gate.live_count() != 0
            || head.thread_id() != request.selector.thread_id()
            || head.draft_id() != request.selector.draft_id()
            || head.published_selector_revision() != request.selector.selector_revision()
            || head.published_root() != request.selector.root()
            || head.published_history() != request.selector.history()
            || item.revision() != request.revision
            || item.turn_id() != request.intent.target_turn_id()
        {
            return Err(DraftReplacementErrorV1::Ineligible);
        }
        authenticate_manifest(&r, *content)?;
        if !super::preflight::source_is_exact(&r, basis)? {
            return Err(DraftReplacementErrorV1::Ineligible);
        }
        validate_position(self, store, head.newest_root(), request.predecessor_caret)?;
        validate_position(
            self,
            store,
            head.newest_root(),
            request.predecessor_selection,
        )?;
        let header = DraftPieceEditHeaderV1::new(
            head.draft_id(),
            head.session_id(),
            head.newest_candidate_generation(),
            head.newest_root(),
            head.newest_history(),
            request.operation,
            request.predecessor_caret,
            request.predecessor_selection,
            request.caret,
            request.selection,
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
        );
        let canonical_header =
            canonical_import_header_bytes(header, head.session_generation(), basis);
        if canonical_header.len() > 65_536 || self.revision(store)? != revision {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(PreparedDraftReplacementImportV1 {
            scope: ImportCommandScope::capture(self, store)?,
            home_generation: self.home_generation,
            request,
            source_session: Box::new(head),
            basis,
            header,
            canonical_header,
        })
    }

    pub fn begin_replacement_draft_import(
        &self,
        revision: beryl_model::DomainRevision,
        prepared: PreparedDraftReplacementImportV1,
    ) -> MutationContribution {
        let generation_matches = prepared.home_generation == self.home_generation;
        prepared.scope.handle.clone().contribution(
            revision,
            super::command::BeginImport {
                prepared,
                generation_matches,
            },
        )
    }
}

pub(super) fn authenticate_source(
    reader: &DomainReader<'_, SyndicDomain>,
    basis: DraftSealedImportBasisV1,
) -> Result<(), DraftReplacementErrorV1> {
    let thread = required::<ThreadsFamily>(reader, &basis.selector.thread_id())?;
    let draft = required::<DraftsFamily>(reader, &basis.selector.draft_id())?;
    let reverse = required::<DraftByThreadFamily>(reader, &basis.selector.thread_id())?;
    let gate = required::<InputGatesFamily>(reader, &basis.selector.thread_id())?;
    if thread.revision() != basis.selector.thread_revision()
        || thread.current_draft_id() != basis.selector.draft_id()
        || draft.thread_id() != thread.id()
        || draft.revision() != basis.selector.selector_revision()
        || draft.piece_root() != basis.selector.root()
        || draft.history() != basis.selector.history()
        || draft.submission_intent() != DraftSubmissionIntent::Ordinary
        || reverse.draft_id() != draft.id()
        || reverse.draft_revision() != draft.revision()
        || gate.revision() != basis.gate_revision
        || gate.state() != &InputGateState::Idle
        || gate.live_count() != 0
    {
        return Err(DraftReplacementErrorV1::Ineligible);
    }
    crate::mutation::discussion_mutation::require_editable(reader, thread.id())?;
    let (target, item) =
        crate::mutation::authenticate_replacement_target(reader, &thread, basis.intent)?;
    if target.id() != basis.intent.target_turn_id()
        || item.turn_id() != basis.intent.target_turn_id()
        || thread.id() != basis.selector.thread_id()
        || gate.thread_id() != thread.id()
        || item.id() != basis.item_id
        || item.revision() != basis.item_revision
        || item.presentation()
            != &CanonicalItemPresentation::user_input(basis.content, basis.assets)
    {
        return Err(DraftReplacementErrorV1::Conflict);
    }
    authenticate_manifest(reader, basis.content)?;
    Ok(())
}

pub(super) fn required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<F::Value, DraftReplacementErrorV1> {
    SourceReader::point::<F>(reader, key)?.ok_or(DraftReplacementErrorV1::Conflict)
}
