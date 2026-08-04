use beryl_home_store::HomeStore;
use beryl_model::{ProjectionRevision, SyndicAcceptedInputId, SyndicDraftId};

use crate::{
    AcceptedInputLifecycle, AcceptedInputPromotionStatus, AcceptedNextCandidateBasis,
    AcceptedNextSourceRecord, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    ActivityQueryHeadRecord, ActivityQuerySource, ActivityQuerySourceRecord, BindingHeadRecord,
    BindingLifecycle, BindingRecord, BindingState, CanonicalItemRecord, ConversationParent,
    DraftByThreadRecord, DraftRecord, HistorySummaryRecord, InputGateRecord, InputGateState,
    ProjectionLifecycle, PromoteAcceptedInput, SelectedPathProof, SyndicReadError, SyndicStorage,
    ThreadParentIndexRecord, ThreadRecord, TranscriptBuildRecord, TranscriptViewHeadRecord,
    TurnChildIndexRecord, TurnItemIndexRecord, TurnItemOrdinal, TurnKind, TurnLifecycle,
    TurnRecord, TurnStateRecord, TurnStateRevision, codec::*,
};

use super::SyndicPointReadLimit;

mod exact;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PromotionObservation {
    source: Option<AcceptedNextSourceRecord>,
    gate: Option<InputGateRecord>,
    thread: Option<ThreadRecord>,
    draft_index: Option<DraftByThreadRecord>,
    current_draft: Option<DraftRecord>,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    generation: Option<AcceptedRouteGenerationRecord>,
    leaf: Option<AcceptedRouteLeafRecord>,
    input: Option<crate::AcceptedInputRecord>,
    order: Option<crate::AcceptedOrderIndexRecord>,
    binding_head: Option<BindingHeadRecord>,
    source_binding: Option<BindingRecord>,
    successor_binding: Option<BindingRecord>,
    transcript_head: Option<TranscriptViewHeadRecord>,
    transcript_build: Option<TranscriptBuildRecord>,
    summary: Option<HistorySummaryRecord>,
    activity_head: Option<ActivityQueryHeadRecord>,
    activity_source: Option<ActivityQuerySourceRecord>,
    parent_turn: Option<TurnRecord>,
    parent_turn_state: Option<TurnStateRecord>,
    successor_ancestor_skip: Option<beryl_model::SyndicTurnId>,
    turn: Option<TurnRecord>,
    turn_state: Option<TurnStateRecord>,
    item: Option<CanonicalItemRecord>,
    item_index: Option<TurnItemIndexRecord>,
    child_index: Option<TurnChildIndexRecord>,
    thread_parent_index: Option<ThreadParentIndexRecord>,
    raw_turn_draft: Option<crate::DraftRecord>,
    raw_turn_accepted: Option<crate::AcceptedInputRecord>,
}

impl SyndicStorage {
    /// Reconciles one accepted-input promotion against stable fixed-work storage points.
    ///
    /// An exact result survives coherent current-draft saves, accepted admissions against the
    /// promoted pending turn, transcript construction or invalidation, and child-activity
    /// publication. Two fixed observations stabilize only the records relevant to this promotion;
    /// unrelated Syndic commits do not invalidate the read. The immutable promotion witness,
    /// successor identity, and dispatch-relevant authority must still agree.
    pub fn accepted_input_promotion_status(
        &self,
        store: &HomeStore,
        promotion: &PromoteAcceptedInput,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputPromotionStatus, SyndicReadError> {
        let observed = PromotionObservation::read(self, store, promotion, limit)?;
        let confirmed = PromotionObservation::read(self, store, promotion, limit)?;
        if observed != confirmed {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "accepted-input promotion reconciliation",
            });
        }
        if observed.is_exact(promotion)? {
            Ok(AcceptedInputPromotionStatus::Exact)
        } else if observed.is_prior(promotion) {
            Ok(AcceptedInputPromotionStatus::Prior)
        } else {
            Ok(AcceptedInputPromotionStatus::Collision)
        }
    }
}

impl PromotionObservation {
    fn read(
        storage: &SyndicStorage,
        store: &HomeStore,
        promotion: &PromoteAcceptedInput,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        let basis = promotion.candidate().basis();
        let thread = basis.thread().id();
        let route_key = ThreadRouteKey {
            thread,
            generation: basis.generation().generation(),
        };
        let order_key = ThreadAcceptedKey {
            owner: thread,
            ordinal: basis.order().ordinal(),
        };
        let successor_binding_revision =
            basis
                .binding_head()
                .revision()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "accepted-input promotion binding revision is exhausted",
                    )
                })?;
        let activity_work_period = expected_activity_work_period(basis)?;
        let activity_source = ActivityQuerySource::new(thread, promotion.successor_turn_id());
        let parent = basis
            .thread()
            .committed_tail()
            .ok_or(SyndicReadError::Invariant(
                "accepted-input promotion candidate has no committed tail",
            ))?;
        let raw_draft = SyndicDraftId::from_bytes(*promotion.successor_turn_id().as_bytes());
        let raw_accepted =
            SyndicAcceptedInputId::from_bytes(*promotion.successor_turn_id().as_bytes());
        let thread_record = storage.point::<ThreadsFamily>(store, thread, limit)?;
        let draft_index = storage.point::<DraftByThreadFamily>(store, thread, limit)?;
        let current_draft = draft_index
            .as_ref()
            .map(|index| storage.point::<DraftsFamily>(store, index.draft_id(), limit))
            .transpose()?
            .flatten();
        let transcript_head = storage.point::<TranscriptHeadsFamily>(store, thread, limit)?;
        let transcript_build = transcript_head
            .as_ref()
            .map(|head| {
                storage.point::<TranscriptBuildsFamily>(
                    store,
                    ThreadTranscriptBuildKey {
                        thread,
                        generation: head.generation(),
                    },
                    limit,
                )
            })
            .transpose()?
            .flatten();
        let thread_parent_index = match (
            basis.thread().parent_thread_id(),
            basis.thread().context_owner_id(),
        ) {
            (Some(parent), Some(_)) => storage.point::<ThreadParentFamily>(
                store,
                ThreadPairKey {
                    first: parent,
                    second: thread,
                },
                limit,
            )?,
            _ => None,
        };
        let parent_turn = storage.point::<TurnsFamily>(store, parent, limit)?;
        let successor_ancestor_skip = parent_turn
            .as_ref()
            .map(|parent| {
                let child_depth = parent.depth().checked_next().map_err(|_| {
                    SyndicReadError::Invariant("accepted-input promotion turn depth is exhausted")
                })?;
                crate::selected_path::child_ancestor_skip(
                    parent.clone(),
                    child_depth,
                    |turn_id| {
                        storage.point::<TurnsFamily>(store, turn_id, limit)?.ok_or(
                            SyndicReadError::Invariant(
                                "accepted-input promotion ancestor is missing",
                            ),
                        )
                    },
                    SyndicReadError::Invariant,
                )
            })
            .transpose()?;
        Ok(Self {
            source: storage.point::<AcceptedNextSourcesFamily>(store, route_key, limit)?,
            gate: storage.point::<InputGatesFamily>(store, thread, limit)?,
            thread: thread_record,
            draft_index,
            current_draft,
            route_head: storage
                .point::<AcceptedRouteGenerationHeadsFamily>(store, thread, limit)?,
            generation: storage.point::<AcceptedRouteGenerationsFamily>(store, route_key, limit)?,
            leaf: storage.point::<AcceptedRouteLeavesFamily>(store, basis.input().id(), limit)?,
            input: storage.point::<AcceptedInputsFamily>(store, basis.input().id(), limit)?,
            order: storage.point::<AcceptedOrderFamily>(store, order_key, limit)?,
            binding_head: storage.point::<BindingHeadsFamily>(store, thread, limit)?,
            source_binding: storage.point::<BindingsFamily>(
                store,
                BindingKey {
                    thread,
                    revision: basis.binding().revision(),
                },
                limit,
            )?,
            successor_binding: storage.point::<BindingsFamily>(
                store,
                BindingKey {
                    thread,
                    revision: successor_binding_revision,
                },
                limit,
            )?,
            transcript_head,
            transcript_build,
            summary: storage.point::<HistorySummariesFamily>(store, thread, limit)?,
            activity_head: storage.point::<ActivityQueryHeadsFamily>(store, thread, limit)?,
            activity_source: storage.point::<ActivityQuerySourcesFamily>(
                store,
                ActivityQuerySourceKey {
                    thread,
                    work_period: activity_work_period,
                    source_thread: activity_source.thread_id(),
                    source_turn: activity_source.turn_id(),
                },
                limit,
            )?,
            parent_turn,
            parent_turn_state: storage.point::<TurnStatesFamily>(store, parent, limit)?,
            successor_ancestor_skip,
            turn: storage.point::<TurnsFamily>(store, promotion.successor_turn_id(), limit)?,
            turn_state: storage.point::<TurnStatesFamily>(
                store,
                promotion.successor_turn_id(),
                limit,
            )?,
            item: storage.point::<CanonicalItemsFamily>(
                store,
                promotion.successor_item_id(),
                limit,
            )?,
            item_index: storage.point::<TurnItemsFamily>(
                store,
                TurnItemKey {
                    owner: promotion.successor_turn_id(),
                    ordinal: TurnItemOrdinal::FIRST,
                },
                limit,
            )?,
            child_index: storage.point::<TurnChildrenFamily>(
                store,
                TurnPairKey {
                    parent,
                    child: promotion.successor_turn_id(),
                },
                limit,
            )?,
            thread_parent_index,
            raw_turn_draft: storage.point::<DraftsFamily>(store, raw_draft, limit)?,
            raw_turn_accepted: storage.point::<AcceptedInputsFamily>(store, raw_accepted, limit)?,
        })
    }

    fn is_prior(&self, promotion: &PromoteAcceptedInput) -> bool {
        let basis = promotion.candidate().basis();
        let expected_parent_index = match (
            basis.thread().parent_thread_id(),
            basis.thread().context_owner_id(),
        ) {
            (Some(parent), Some(owner)) => Some(ThreadParentIndexRecord::new(
                parent,
                basis.thread().id(),
                basis.thread().revision(),
                owner,
            )),
            _ => None,
        };
        self.source.as_ref() == Some(basis.source())
            && self.gate.as_ref() == Some(basis.gate())
            && self.thread.as_ref() == Some(basis.thread())
            && self.draft_index.as_ref() == Some(basis.draft_by_thread())
            && self.current_draft.as_ref().is_some_and(|draft| {
                draft.id() == basis.draft_by_thread().draft_id()
                    && draft.thread_id() == basis.thread().id()
                    && draft.revision() == basis.draft_by_thread().draft_revision()
                    && matches!(
                        draft.submission_intent(),
                        crate::DraftSubmissionIntent::Ordinary
                    )
                    && draft.created_at() <= draft.updated_at()
                    && draft.updated_at() <= basis.summary().last_activity_at()
            })
            && self.route_head.as_ref() == basis.route_head()
            && self.generation.as_ref() == Some(basis.generation())
            && self.leaf.as_ref() == Some(basis.leaf())
            && self.input.as_ref() == Some(basis.input())
            && self.order.as_ref() == Some(basis.order())
            && self.binding_head.as_ref() == Some(basis.binding_head())
            && self.source_binding.as_ref() == Some(basis.binding())
            && self.successor_binding.is_none()
            && self.transcript_head.as_ref() == Some(basis.transcript_head())
            && self.summary.as_ref() == Some(basis.summary())
            && self.activity_head.as_ref() == Some(basis.activity_head())
            && self.activity_source.is_none()
            && self.source_parent_agrees(basis)
            && self.turn.is_none()
            && self.turn_state.is_none()
            && self.item.is_none()
            && self.item_index.is_none()
            && self.child_index.is_none()
            && self.thread_parent_index == expected_parent_index
            && self.raw_turn_draft.is_none()
            && self.raw_turn_accepted.is_none()
    }

    fn source_parent_agrees(&self, basis: &AcceptedNextCandidateBasis) -> bool {
        let Some(parent_id) = basis.thread().committed_tail() else {
            return false;
        };
        self.parent_turn.as_ref().is_some_and(|parent| {
            parent.id() == parent_id
                && parent.chain_digest() == basis.thread().selected_path_digest()
        }) && self.parent_turn_state.as_ref().is_some_and(|state| {
            state.turn_id() == parent_id && state.lifecycle().is_proven_terminal()
        }) && self.successor_ancestor_skip.is_some()
    }
}

fn expected_activity_work_period(
    basis: &AcceptedNextCandidateBasis,
) -> Result<crate::ActivityWorkPeriod, SyndicReadError> {
    if basis.activity_head().source().is_none() {
        Ok(basis.activity_head().work_period())
    } else {
        basis
            .activity_head()
            .work_period()
            .checked_next()
            .map_err(|_| {
                SyndicReadError::Invariant(
                    "accepted-input promotion activity work period is exhausted",
                )
            })
    }
}
