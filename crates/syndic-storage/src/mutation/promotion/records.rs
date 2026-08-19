use beryl_home_store::{DomainReader, MutationBuilder};
use beryl_model::{ProjectionRevision, SyndicAcceptedInputId, SyndicDraftId};

use crate::{
    AcceptedInputLifecycle, AcceptedNextCandidateBasis, AcceptedNextSourceRecord,
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteTarget, ActivityQueryHeadRecord,
    ActivityQuerySource, ActivityQuerySourceRecord, BindingHeadRecord, BindingLifecycle,
    BindingRecord, BindingState, CanonicalItemRecord, ConversationParent, DraftByThreadRecord,
    HistorySummaryRecord, InputGateRecord, InputGateState, NextTurnReason, ProjectionLifecycle,
    SelectedPathProof, SyndicMutationError, ThreadParentIndexRecord, ThreadRecord,
    TranscriptBuildRecord, TranscriptViewHeadRecord, TurnChildIndexRecord, TurnItemIndexRecord,
    TurnItemOrdinal, TurnKind, TurnLifecycle, TurnRecord, TurnStateRecord, TurnStateRevision,
    codec::*, domain::SyndicDomain,
};

use super::PromoteAcceptedInput;
use crate::mutation::{point, required};

mod projection;
mod route;
mod validation;

use projection::projection_records;
use route::{promotion_route_records, validate_promotion_source};
use validation::{validate_current_basis, validate_fresh_identities};

pub(super) struct PromotionRecords {
    source_key: ThreadRouteKey,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    route_generation: AcceptedRouteGenerationRecord,
    route_leaf: AcceptedRouteLeafRecord,
    next_source: Option<AcceptedNextSourceRecord>,
    thread: ThreadRecord,
    draft_index: DraftByThreadRecord,
    turn: TurnRecord,
    turn_state: TurnStateRecord,
    child_index: TurnChildIndexRecord,
    item: CanonicalItemRecord,
    item_index: TurnItemIndexRecord,
    transcript_head: TranscriptViewHeadRecord,
    transcript_build: Option<TranscriptBuildRecord>,
    summary: HistorySummaryRecord,
    gate: InputGateRecord,
    activity_head: ActivityQueryHeadRecord,
    activity_source: ActivityQuerySourceRecord,
    binding: BindingRecord,
    binding_head: BindingHeadRecord,
    thread_parent_index: Option<ThreadParentIndexRecord>,
}

impl PromotionRecords {
    pub(super) fn build(
        reader: &DomainReader<'_, SyndicDomain>,
        promotion: &PromoteAcceptedInput,
    ) -> Result<Self, SyndicMutationError> {
        let basis = promotion.candidate().basis();
        validate_current_basis(reader, basis)?;
        validate_promotion_source(basis, promotion)?;
        validate_fresh_identities(reader, basis, promotion)?;

        let parent_id = basis
            .thread()
            .committed_tail()
            .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
        let parent = ConversationParent::Turn(parent_id);
        let (depth, digest, ancestor_skip) = crate::mutation::admission_helpers::turn_shape(
            reader,
            promotion.successor_turn_id(),
            parent,
        )?;
        let thread_revision = basis.thread().revision().checked_next()?;
        let selected_path =
            SelectedPathProof::new(Some(promotion.successor_turn_id()), thread_revision, digest);
        let thread = ThreadRecord::new(
            basis.thread().id(),
            selected_path,
            basis.thread().current_draft_id(),
            basis.thread().lineage(),
            basis.thread().image_label_frontiers(),
            basis.thread().context_owner_id(),
        );
        let draft_index = DraftByThreadRecord::new(
            thread.id(),
            basis.draft_by_thread().draft_id(),
            basis.draft_by_thread().draft_revision(),
            thread_revision,
        );
        let turn = TurnRecord::new(
            promotion.successor_turn_id(),
            thread.id(),
            TurnKind::OrdinaryUser,
            parent,
            ancestor_skip,
            depth,
            digest,
            promotion.promoted_at(),
        );
        let turn_state = TurnStateRecord::with_capture_frontiers(
            turn.id(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            1,
            0,
            1,
            0,
            None,
            promotion.promoted_at(),
        )?;
        let child_index = TurnChildIndexRecord::new(parent_id, turn.id(), depth, digest);
        let item_revision = ProjectionRevision::new(1)?;
        let item = CanonicalItemRecord::local_user_input(
            promotion.successor_item_id(),
            turn.id(),
            TurnItemOrdinal::FIRST,
            item_revision,
            basis.input().content(),
            basis.input().asset_reference_set(),
        );
        let item_index =
            TurnItemIndexRecord::new(turn.id(), TurnItemOrdinal::FIRST, item.id(), item_revision);

        let route = promotion_route_records(basis, promotion)?;
        let projection = projection_records(reader, basis, promotion, &thread, selected_path)?;
        Ok(Self {
            source_key: route.source_key,
            route_head: route.route_head,
            route_generation: route.generation,
            route_leaf: route.leaf,
            next_source: route.next_source,
            thread,
            draft_index,
            turn,
            turn_state,
            child_index,
            item,
            item_index,
            transcript_head: projection.transcript_head,
            transcript_build: projection.transcript_build,
            summary: projection.summary,
            gate: projection.gate,
            activity_head: projection.activity_head,
            activity_source: projection.activity_source,
            binding: projection.binding,
            binding_head: projection.binding_head,
            thread_parent_index: projection.thread_parent_index,
        })
    }
}

impl PromotionRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let Some(head) = &self.route_head {
            mutations.put::<AcceptedRouteGenerationHeadsCodec>(&head.thread_id(), head)?;
        }
        mutations.put::<AcceptedRouteGenerationsCodec>(&self.source_key, &self.route_generation)?;
        mutations.put::<AcceptedRouteLeavesCodec>(&self.route_leaf.input_id(), &self.route_leaf)?;
        match &self.next_source {
            Some(source) => {
                mutations.put::<AcceptedNextSourcesCodec>(&self.source_key, source)?;
            }
            None => mutations.delete::<AcceptedNextSourcesCodec>(&self.source_key)?,
        }
        mutations.put::<ThreadsCodec>(&self.thread.id(), &self.thread)?;
        mutations.put::<DraftByThreadCodec>(&self.thread.id(), &self.draft_index)?;
        mutations.put::<TurnsCodec>(&self.turn.id(), &self.turn)?;
        mutations.put::<TurnStatesCodec>(&self.turn.id(), &self.turn_state)?;
        mutations.put::<TurnChildrenCodec>(
            &TurnPairKey {
                parent: self.child_index.parent_id(),
                child: self.child_index.child_id(),
            },
            &self.child_index,
        )?;
        mutations.put::<CanonicalItemsCodec>(&self.item.id(), &self.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: self.turn.id(),
                ordinal: TurnItemOrdinal::FIRST,
            },
            &self.item_index,
        )?;
        mutations.put::<TranscriptHeadsCodec>(&self.thread.id(), &self.transcript_head)?;
        if let Some(build) = &self.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations.put::<HistorySummariesCodec>(&self.thread.id(), &self.summary)?;
        mutations.put::<InputGatesCodec>(&self.thread.id(), &self.gate)?;
        mutations
            .put::<ActivityQueryHeadsCodec>(&self.activity_head.thread_id(), &self.activity_head)?;
        mutations.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: self.activity_source.thread_id(),
                work_period: self.activity_source.work_period(),
                source_thread: self.activity_source.source().thread_id(),
                source_turn: self.activity_source.source().turn_id(),
            },
            &self.activity_source,
        )?;
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: self.binding.thread_id(),
                revision: self.binding.revision(),
            },
            &self.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&self.thread.id(), &self.binding_head)?;
        if let Some(index) = &self.thread_parent_index {
            mutations.put::<ThreadParentCodec>(
                &ThreadPairKey {
                    first: index.parent_thread_id(),
                    second: index.child_thread_id(),
                },
                index,
            )?;
        }
        Ok(())
    }
}
