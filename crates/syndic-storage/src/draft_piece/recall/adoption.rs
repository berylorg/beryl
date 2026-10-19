use super::super::import::read::{SourceReader, StoreSourceReader};
use super::super::*;
use super::*;
use crate::{SyndicStorage, codec::*};
use beryl_home_store::{HomeStore, MutationContribution};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PreparedDraftComposerRootReplacementV1 {
    pub(crate) scope: RecallScope,
    pub(crate) observation: outcome_state::CommandObservation,
    pub(crate) before: Box<DraftEditorCandidateSessionV1>,
    pub(crate) history: Box<DraftEditHistoryFrontierV1>,
    pub(crate) source: Option<Box<DraftPieceBuildRecordV1>>,
    pub(crate) root: DraftPieceRootReferenceV1,
    pub(crate) purpose: DraftComposerRootReplacementPurposeV1,
    pub(crate) header: DraftPieceEditHeaderV1,
    pub(crate) canonical_header: Vec<u8>,
    pub(crate) result: Arc<Mutex<Option<DraftPieceSettlementV1>>>,
}
impl PreparedDraftComposerRootReplacementV1 {
    pub fn key(&self) -> DraftPieceSettlementKeyV1 {
        DraftPieceSettlementKeyV1::new(
            self.header.draft_id(),
            self.header.session_id(),
            self.header.operation_id(),
        )
    }
    pub fn caret(&self) -> DraftCompositePositionV1 {
        self.header.caret()
    }
    pub fn selection(&self) -> DraftCompositePositionV1 {
        self.header.selection()
    }
    pub fn candidate(&self) -> Option<DraftEditorCandidateActivationBindingV1> {
        self.result
            .lock()
            .ok()?
            .as_ref()
            .and_then(|s| match s.closure() {
                DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) => {
                    Some(DraftEditorCandidateActivationBindingV1::from_head(
                        c.adoption.adopted_session(),
                    ))
                }
                _ => None,
            })
    }
    pub fn settlement(&self) -> Option<DraftPieceSettlementV1> {
        self.result.lock().ok()?.clone()
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
}

impl SyndicStorage {
    pub fn prepare_composer_recall_commit(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRecallV1,
    ) -> Result<PreparedDraftComposerRootReplacementV1, DraftComposerRecallErrorV1> {
        p.scope.require(store)?;
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let source = r
            .boxed_point::<DraftPieceBuildsFamily>(&p.key())?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        let before = r.session_head(&command::head_key(&p.before))?;
        if source.canonical_header() != p.canonical_header
            || !recall_build_is_exact(&source)
            || source.lifecycle() != DraftPieceBuildLifecycleV1::Complete
            || before.active_operation() != Some(&command::custody(&source))
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let root = source
            .successor()
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        authenticate_basis(&r, &p.basis)?;
        let history = r
            .boxed_point::<DraftEditHistoryFrontiersFamily>(&before.newest_history().key())?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        validate_position(self, store, root, p.header.caret())?;
        validate_position(self, store, root, p.header.selection())?;
        if self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(PreparedDraftComposerRootReplacementV1 {
            scope: p.scope.clone(),
            observation: Default::default(),
            before,
            history,
            source: Some(source),
            root,
            purpose: DraftComposerRootReplacementPurposeV1::AcceptedRecall(p.basis.clone()),
            header: p.header,
            canonical_header: p.canonical_header.clone(),
            result: Default::default(),
        })
    }
    pub fn prepare_composer_checkpoint_restore(
        &self,
        store: &HomeStore,
        activation: DraftEditorCandidateActivationBindingV1,
        operation: DraftPieceOperationIdV1,
        checkpoint: DraftComposerBrowseCheckpointV1,
        before_caret: DraftCompositePositionV1,
        before_selection: DraftCompositePositionV1,
    ) -> Result<PreparedDraftComposerRootReplacementV1, DraftComposerRecallErrorV1> {
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let before = r.session_head(&DraftEditorCandidateSessionRecordKeyV1::head(
            activation.draft_id(),
            activation.session_id(),
        ))?;
        if checkpoint.home != store.home_id()
            || checkpoint.activation.draft_id() != activation.draft_id()
            || checkpoint.activation.session_id() != activation.session_id()
            || DraftEditorCandidateActivationBindingV1::from_head(&before) != activation
            || operation.as_bytes() == &[0; 16]
            || operation == before.open_operation_id()
            || !session::idle_candidate_closure_is_exact_with_access(
                self,
                crate::read::access::ReadAccess::Ordinary(store),
                &before,
            )?
        {
            return Err(DraftComposerRecallErrorV1::Ineligible);
        }
        require_destination(&r, &before)?;
        let root = checkpoint.activation.root();
        if required::<DraftPieceRootsFamily>(&r, &root.key())?.reference() != root {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        let protection = required::<DraftImageLabelProtectionHeadsFamily>(&r, &before.thread_id())?;
        if !protection.is_exact()
            || root
                .marker_commitment()
                .maximum_image_label()
                .is_some_and(|l| !protection.protected_maximum().contains(l))
        {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        validate_position(self, store, before.newest_root(), before_caret)?;
        validate_position(self, store, before.newest_root(), before_selection)?;
        validate_position(self, store, root, checkpoint.caret)?;
        validate_position(self, store, root, checkpoint.selection)?;
        let header = DraftPieceEditHeaderV1::new(
            activation.draft_id(),
            activation.session_id(),
            activation.candidate_generation(),
            activation.root(),
            activation.history(),
            operation,
            before_caret,
            before_selection,
            checkpoint.caret,
            checkpoint.selection,
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
        );
        let purpose = DraftComposerRootReplacementPurposeV1::CheckpointRestore {
            activation: checkpoint.activation,
            caret: checkpoint.caret,
            selection: checkpoint.selection,
        };
        let canonical_header =
            canonical_recall_header(header, before.session_generation(), &purpose);
        let history = r
            .boxed_point::<DraftEditHistoryFrontiersFamily>(&before.newest_history().key())?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        if self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(PreparedDraftComposerRootReplacementV1 {
            scope: RecallScope::capture(self, store)?,
            observation: Default::default(),
            before,
            history,
            source: None,
            root,
            purpose,
            header,
            canonical_header,
            result: Default::default(),
        })
    }
    pub fn commit_composer_root_replacement(
        &self,
        revision: beryl_model::DomainRevision,
        p: PreparedDraftComposerRootReplacementV1,
    ) -> MutationContribution {
        p.scope
            .handle
            .clone()
            .contribution(revision, writer::Adopt { p })
    }
}
