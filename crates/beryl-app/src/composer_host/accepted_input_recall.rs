mod drive;
mod recovery;

pub use recovery::ComposerHostRecallRestoration;
pub(in crate::composer_host) use recovery::RetiredAcceptedInputRecall;

use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};
use beryl_home_store::HomeStore;
use gpui_text_input::{MutationPositions, SourcePosition};
use syndic_storage::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostRecallTicket {
    binding: ComposerHostBinding,
    operation: DraftPieceOperationIdV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostRecallRefusal {
    CapacityUnavailable,
    SourceUnavailable,
    SourceChanged,
    ProofRejected,
    CommandRefused,
}

#[derive(Clone, Debug)]
pub struct ComposerHostRecallCompletion {
    previous: ComposerHostBinding,
    binding: ComposerHostBinding,
    positions: MutationPositions,
    checkpoint: Option<DraftComposerBrowseCheckpointV1>,
    replaced: bool,
    refusal: Option<ComposerHostRecallRefusal>,
}

impl ComposerHostRecallCompletion {
    pub const fn previous_binding(&self) -> ComposerHostBinding {
        self.previous
    }
    pub const fn binding(&self) -> ComposerHostBinding {
        self.binding
    }
    pub const fn positions(&self) -> MutationPositions {
        self.positions
    }
    pub fn checkpoint(&self) -> Option<&DraftComposerBrowseCheckpointV1> {
        self.checkpoint.as_ref()
    }
    pub const fn replaced(&self) -> bool {
        self.replaced
    }
    pub const fn refusal(&self) -> Option<ComposerHostRecallRefusal> {
        self.refusal
    }
}

#[derive(Clone, Debug)]
pub enum ComposerHostRecallAdvance {
    Progress,
    Ready(ComposerHostRecallCompletion),
    NotCommitted,
}

pub(super) struct PendingAcceptedInputRecall {
    ticket: ComposerHostRecallTicket,
    build: Option<PreparedDraftComposerRecallV1>,
    checkpoint: Option<DraftComposerBrowseCheckpointV1>,
    stage: Stage,
    cancelled: bool,
    assets: Option<beryl_state::AssetState>,
    before_positions: MutationPositions,
    refusal: Option<ComposerHostRecallRefusal>,
}

enum Stage {
    Begin,
    Streaming,
    Quantum {
        beginning: bool,
        outcome: Quantum,
    },
    Commit {
        prepared: PreparedDraftComposerRootReplacementV1,
        attempted: bool,
    },
    Terminate {
        prepared: PreparedDraftComposerRecallTerminationV1,
        attempted: bool,
    },
    Ready(ComposerHostRecallCompletion),
    NotCommitted,
}

type Quantum = super::publication::RetainedComposerCommandOutcome;

impl SyndicComposerHost {
    pub fn accepted_input_recall_pending(&self) -> bool {
        self.accepted_input_recall.is_some()
    }

    pub fn begin_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        source: ComposerAcceptedInputReferenceV1,
        positions: MutationPositions,
        operation: DraftPieceOperationIdV1,
        capture_checkpoint: bool,
    ) -> Result<ComposerHostRecallTicket, ComposerHostError> {
        self.validate_recall_admission(store, binding)?;
        if positions.selection_head() != positions.caret() {
            return Err(ComposerHostError::MutationMalformed);
        }
        if source.thread_id()
            != self
                .active_thread_id()
                .ok_or(ComposerHostError::OldBinding)?
        {
            return Err(ComposerHostError::OldBinding);
        }
        let caret = canonical_position(positions.caret())?;
        let selection = canonical_position(positions.selection_anchor())?;
        let checkpoint = if capture_checkpoint {
            Some(self.storage.capture_composer_browse_checkpoint(
                store,
                binding.candidate(),
                caret,
                selection,
            )?)
        } else {
            None
        };
        let build = self.storage.prepare_composer_recall(
            store,
            DraftComposerRecallRequestV1::new(
                binding.candidate(),
                operation,
                source,
                caret,
                selection,
            ),
        )?;
        let ticket = ComposerHostRecallTicket { binding, operation };
        self.accepted_input_recall = Some(Box::new(PendingAcceptedInputRecall {
            ticket,
            build: Some(build),
            checkpoint,
            stage: Stage::Begin,
            cancelled: false,
            assets: None,
            before_positions: positions,
            refusal: None,
        }));
        Ok(ticket)
    }

    pub fn begin_composer_checkpoint_restore(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        checkpoint: DraftComposerBrowseCheckpointV1,
        positions: MutationPositions,
        operation: DraftPieceOperationIdV1,
    ) -> Result<ComposerHostRecallTicket, ComposerHostError> {
        self.validate_recall_admission(store, binding)?;
        if positions.selection_head() != positions.caret() {
            return Err(ComposerHostError::MutationMalformed);
        }
        let prepared = self.storage.prepare_composer_checkpoint_restore(
            store,
            binding.candidate(),
            operation,
            checkpoint,
            canonical_position(positions.caret())?,
            canonical_position(positions.selection_anchor())?,
        )?;
        let ticket = ComposerHostRecallTicket { binding, operation };
        self.accepted_input_recall = Some(Box::new(PendingAcceptedInputRecall {
            ticket,
            build: None,
            checkpoint: None,
            stage: Stage::Commit {
                prepared,
                attempted: false,
            },
            cancelled: false,
            assets: None,
            before_positions: positions,
            refusal: None,
        }));
        Ok(ticket)
    }

    fn validate_recall_admission(
        &self,
        store: &HomeStore,
        binding: ComposerHostBinding,
    ) -> Result<(), ComposerHostError> {
        super::request::validate_store(binding, store)?;
        if self.lifecycle.freezes_admission()
            || self.live_operation_pending()
            || self.submission_pending()
            || self.publication.lane.is_some()
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        self.reserve_settlement_custody()?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != binding
            || active.unavailable
            || active.session_disposed
            || active.submission_intent != Some(DraftSubmissionIntent::Ordinary)
        {
            return Err(ComposerHostError::OldBinding);
        }
        Ok(())
    }

    pub fn acknowledge_accepted_input_recall(
        &mut self,
        ticket: ComposerHostRecallTicket,
        binding: ComposerHostBinding,
    ) -> Result<(), ComposerHostError> {
        let pending = self
            .accepted_input_recall
            .as_ref()
            .filter(|pending| pending.ticket == ticket)
            .ok_or(ComposerHostError::OldBinding)?;
        if !matches!(&pending.stage, Stage::Ready(completion) if completion.binding == binding) {
            return Err(ComposerHostError::OldBinding);
        }
        self.accepted_input_recall = None;
        Ok(())
    }

    fn adopt_recalled_root(
        &mut self,
        store: &HomeStore,
        pending: &PendingAcceptedInputRecall,
        prepared: &PreparedDraftComposerRootReplacementV1,
        candidate: DraftEditorCandidateActivationBindingV1,
    ) -> Result<ComposerHostRecallCompletion, ComposerHostError> {
        let previous = pending.ticket.binding;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != previous || Some(candidate) != prepared.candidate() {
            return Err(ComposerHostError::OldBinding);
        }
        let binding = ComposerHostBinding::new(
            previous.home_id(),
            previous.home_generation(),
            previous.host_generation(),
            candidate,
            previous.presentation_generation(),
        );
        let caret = self.history_position(store, binding, prepared.caret())?;
        let anchor = self.history_position(store, binding, prepared.selection())?;
        let completion = ComposerHostRecallCompletion {
            previous,
            binding,
            positions: MutationPositions::new(caret, anchor, caret),
            checkpoint: pending.checkpoint.clone(),
            replaced: true,
            refusal: None,
        };
        let active = self.active.as_mut().unwrap();
        active.binding = binding;
        active.storage_candidate = candidate;
        active.initial_responses.clear();
        self.pending.clear();
        self.last_mutation_identity = None;
        self.last_history_identity = None;
        self.last_history_outcome = None;
        self.lifecycle.adopted(binding, true);
        Ok(completion)
    }
}

fn canonical_position(
    position: SourcePosition,
) -> Result<DraftCompositePositionV1, ComposerHostError> {
    use gpui_text_input::InlineObjectGap;
    let gap = match position.gap {
        InlineObjectGap::NoObjects => DraftCompositeGapWitnessV1::Unambiguous,
        InlineObjectGap::Before(_) => DraftCompositeGapWitnessV1::BeforeAll,
        InlineObjectGap::After(_) => DraftCompositeGapWitnessV1::AfterAll,
        InlineObjectGap::Between {
            preceding,
            following,
        } => DraftCompositeGapWitnessV1::Between {
            left_order_key: u64::try_from(preceding.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?,
            left_marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(
                preceding.id().get().to_be_bytes(),
            ),
            right_order_key: u64::try_from(following.order().get())
                .map_err(|_| ComposerHostError::MutationMalformed)?,
            right_marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(
                following.id().get().to_be_bytes(),
            ),
        },
    };
    Ok(DraftCompositePositionV1::new(
        position.byte_offset.get(),
        gap,
    ))
}
