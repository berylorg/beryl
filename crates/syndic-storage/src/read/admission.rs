use crate::{
    AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputRecord,
    AcceptedOrderIndexRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState, CanonicalItemKind,
    CanonicalItemRecord, DraftComposerMaterializationRecordV1, DraftComposerMaterializationsFamily,
    DraftEditHistoryFrontierKeyV1, DraftEditHistoryFrontierV1, DraftEditHistoryFrontiersFamily,
    DraftEditHistoryPolicyV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidateSessionLifecycleV1, DraftEditorCandidateSessionRecordKeyV1,
    DraftEditorCandidateSessionRecordV1, DraftEditorCandidateSessionV1,
    DraftEditorCandidateSessionsFamily, DraftPieceRootKeyV1, DraftPieceRootRecordV1,
    DraftPieceRootsFamily, DraftRecord, FirstAcceptance, FirstAcceptanceKind,
    FirstAcceptanceStatus, ImageLabelAuthorityHeadV1, ImageLabelOriginOwner,
    ImageLabelOriginSpanRecord, InputGateRecord, SyndicReadError, ThreadRecord, TurnItemOrdinal,
    TurnKind, TurnRecord, canonical_empty_draft_edit_history_v1,
    canonical_empty_draft_piece_root_v1, canonical_empty_draft_root_operation_id_v1, codec::*,
    domain::SyndicStorage,
};
use beryl_home_store::HomeStore;
use beryl_model::AcceptedInputRevision;

use super::SyndicPointReadLimit;

#[derive(Clone, Debug, Eq, PartialEq)]
struct FirstAcceptanceObservation {
    thread: Option<ThreadRecord>,
    image_label_authority: Option<ImageLabelAuthorityHeadV1>,
    image_label_origin_span: Option<ImageLabelOriginSpanRecord>,
    source_draft: Option<DraftRecord>,
    next_draft: Option<DraftRecord>,
    gate: Option<InputGateRecord>,
    source_root: Option<DraftPieceRootRecordV1>,
    source_history: Option<DraftEditHistoryFrontierV1>,
    fresh_root: Option<DraftPieceRootRecordV1>,
    fresh_history: Option<DraftEditHistoryFrontierV1>,
    session: Option<DraftEditorCandidateSessionRecordV1>,
    materialization: Option<DraftComposerMaterializationRecordV1>,
    turn: Option<TurnRecord>,
    item: Option<CanonicalItemRecord>,
    input: Option<AcceptedInputRecord>,
    order: Option<AcceptedOrderIndexRecord>,
    route_leaf: Option<AcceptedRouteLeafRecord>,
}

impl SyndicStorage {
    pub fn first_acceptance_status(
        &self,
        store: &HomeStore,
        acceptance: &FirstAcceptance,
        limit: SyndicPointReadLimit,
    ) -> Result<FirstAcceptanceStatus, SyndicReadError> {
        let observed = FirstAcceptanceObservation::read(self, store, acceptance, limit)?;
        let confirmed = FirstAcceptanceObservation::read(self, store, acceptance, limit)?;
        if observed != confirmed {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "first-acceptance reconciliation",
            });
        }
        if confirmed.is_exact_old(acceptance) {
            return Ok(FirstAcceptanceStatus::ExactOld);
        }
        if confirmed.is_exact_new(acceptance)? {
            return Ok(FirstAcceptanceStatus::ExactNew(expected_kind(acceptance)));
        }
        Ok(FirstAcceptanceStatus::Collision)
    }
}

impl FirstAcceptanceObservation {
    fn read(
        storage: &SyndicStorage,
        store: &HomeStore,
        acceptance: &FirstAcceptance,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, SyndicReadError> {
        let source_history = storage.point::<DraftEditHistoryFrontiersFamily>(
            store,
            acceptance.candidate().history().key(),
            limit,
        )?;
        let (fresh_root_key, fresh_history_key) = fresh_keys(acceptance, source_history.as_ref())?;
        let input =
            storage.point::<AcceptedInputsFamily>(store, acceptance.accepted_input_id(), limit)?;
        let order = match input.as_ref() {
            Some(input) => storage.point::<AcceptedOrderFamily>(
                store,
                ThreadAcceptedKey {
                    owner: acceptance.thread_id(),
                    ordinal: input.ordinal(),
                },
                limit,
            )?,
            None => None,
        };
        let route_leaf = storage.point::<AcceptedRouteLeavesFamily>(
            store,
            acceptance.accepted_input_id(),
            limit,
        )?;
        let image_label_origin_span = match acceptance
            .asset_reference_set()
            .and_then(|proof| proof.sequential().maximum_image_label())
        {
            Some(end_label) => storage.point::<ImageLabelOriginSpansFamily>(
                store,
                ImageLabelOriginSpanKey {
                    thread: acceptance.thread_id(),
                    end_label,
                },
                limit,
            )?,
            None => None,
        };
        Ok(Self {
            thread: storage.point::<ThreadsFamily>(store, acceptance.thread_id(), limit)?,
            image_label_authority: storage.point::<ImageLabelAuthorityHeadsFamily>(
                store,
                acceptance.thread_id(),
                limit,
            )?,
            image_label_origin_span,
            source_draft: storage.point::<DraftsFamily>(store, acceptance.draft_id(), limit)?,
            next_draft: storage.point::<DraftsFamily>(store, acceptance.next_draft_id(), limit)?,
            gate: storage.point::<InputGatesFamily>(store, acceptance.thread_id(), limit)?,
            source_root: storage.point::<DraftPieceRootsFamily>(
                store,
                acceptance.candidate().root().key(),
                limit,
            )?,
            source_history,
            fresh_root: match fresh_root_key {
                Some(key) => storage.point::<DraftPieceRootsFamily>(store, key, limit)?,
                None => None,
            },
            fresh_history: match fresh_history_key {
                Some(key) => storage.point::<DraftEditHistoryFrontiersFamily>(store, key, limit)?,
                None => None,
            },
            session: storage.point::<DraftEditorCandidateSessionsFamily>(
                store,
                DraftEditorCandidateSessionRecordKeyV1::Head {
                    draft_id: acceptance.draft_id(),
                    session_id: acceptance.candidate().session_id(),
                },
                limit,
            )?,
            materialization: storage.point::<DraftComposerMaterializationsFamily>(
                store,
                acceptance.materialization().key(),
                limit,
            )?,
            turn: storage.point::<TurnsFamily>(store, acceptance.submitted_turn_id(), limit)?,
            item: storage.point::<CanonicalItemsFamily>(
                store,
                acceptance.idle_user_item_id(),
                limit,
            )?,
            input,
            order,
            route_leaf,
        })
    }

    fn is_exact_old(&self, acceptance: &FirstAcceptance) -> bool {
        self.thread.as_ref().is_some_and(|thread| {
            thread.id() == acceptance.thread_id()
                && thread.revision() == acceptance.expected_thread_revision()
                && thread.current_draft_id() == acceptance.draft_id()
        }) && self.image_label_authority == Some(acceptance.expected_image_label_authority())
            && self.source_draft.as_ref().is_some_and(|draft| {
                draft.id() == acceptance.draft_id()
                    && draft.thread_id() == acceptance.thread_id()
                    && draft.revision() == acceptance.expected_draft_revision()
                    && draft.root_history()
                        == crate::DraftRootHistoryPairV1::new(
                            acceptance.candidate().root(),
                            acceptance.candidate().history(),
                        )
            })
            && self.next_draft.is_none()
            && self.gate.as_ref().is_some_and(|gate| {
                gate.revision() == acceptance.expected_gate_revision()
                    && gate.state() == acceptance.expected_gate_state()
            })
            && self.source_authority_is_exact(acceptance)
            && self.session.as_ref().is_some_and(|record| {
                matches!(record, DraftEditorCandidateSessionRecordV1::Head(session)
                    if active_session_is_exact(session, acceptance))
            })
            && self.turn.is_none()
            && self.input.is_none()
            && self.order.is_none()
            && self.route_leaf.is_none()
            && (expected_kind(acceptance) == FirstAcceptanceKind::Accepted || self.item.is_none())
    }

    fn is_exact_new(&self, acceptance: &FirstAcceptance) -> Result<bool, SyndicReadError> {
        let Some((expected_head, expected_span)) = expected_image_label_records(acceptance) else {
            return Ok(false);
        };
        let session_exact = self.session.as_ref().is_some_and(|record| {
            matches!(record, DraftEditorCandidateSessionRecordV1::Head(session)
                if disposed_session_is_exact(session, acceptance))
        });
        let fresh_exact = fresh_records_are_exact(
            acceptance,
            self.source_history.as_ref(),
            self.fresh_root.as_ref(),
            self.fresh_history.as_ref(),
        )?;
        let common = self.source_draft.is_none()
            && self.image_label_authority == Some(expected_head)
            && expected_span
                .as_ref()
                .is_none_or(|span| self.image_label_origin_span.as_ref() == Some(span))
            && self.source_authority_is_exact(acceptance)
            && session_exact
            && fresh_exact;
        if !common {
            return Ok(false);
        }
        Ok(match expected_kind(acceptance) {
            FirstAcceptanceKind::Idle { user_item_id } => {
                self.input.is_none()
                    && self.order.is_none()
                    && self.turn.as_ref().is_some_and(|turn| {
                        turn.id() == acceptance.submitted_turn_id()
                            && turn.origin_thread_id() == acceptance.thread_id()
                            && turn.kind() == TurnKind::OrdinaryUser
                            && turn.submitted_at() == acceptance.admitted_at()
                    })
                    && self.item.as_ref().is_some_and(|item| {
                        item.id() == user_item_id
                            && item.turn_id() == acceptance.submitted_turn_id()
                            && item.ordinal() == TurnItemOrdinal::FIRST
                            && item.kind() == CanonicalItemKind::UserInput
                            && item.source_event().is_none()
                            && item.cas_source().is_none()
                            && item.presentation_content()
                                == Some(acceptance.materialization().content())
                            && item.presentation().asset_reference_set()
                                == acceptance.asset_reference_set()
                    })
            }
            FirstAcceptanceKind::Accepted => {
                self.turn.is_none()
                    && self
                        .input
                        .as_ref()
                        .is_some_and(|input| exact_input_matches(acceptance, input))
                    && self.order.as_ref().is_some_and(|order| {
                        self.input.as_ref().is_some_and(|input| {
                            order
                                == &AcceptedOrderIndexRecord::new(
                                    input.thread_id(),
                                    input.ordinal(),
                                    input.id(),
                                    input.route_generation(),
                                )
                        })
                    })
                    && self.input.as_ref().is_some_and(|input| {
                        self.route_leaf
                            .as_ref()
                            .is_some_and(|leaf| exact_route_leaf_matches(acceptance, input, leaf))
                    })
            }
        })
    }

    fn source_authority_is_exact(&self, acceptance: &FirstAcceptance) -> bool {
        self.source_root
            .as_ref()
            .is_some_and(|root| root.reference() == acceptance.candidate().root())
            && self
                .source_history
                .as_ref()
                .is_some_and(|history| history.reference() == acceptance.candidate().history())
            && self.materialization.as_ref() == Some(&acceptance.materialization())
            && asset_proof_is_exact(acceptance)
    }
}

fn exact_route_leaf_matches(
    acceptance: &FirstAcceptance,
    input: &AcceptedInputRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> bool {
    let common = leaf.input_id() == acceptance.accepted_input_id()
        && leaf.thread_id() == acceptance.thread_id()
        && leaf.generation() == input.route_generation()
        && leaf.ordinal() == input.ordinal();
    if !common {
        return false;
    }
    if let Some(proof) = leaf.promotion() {
        let Ok(promoted_revision) = proof.expected_input_revision().checked_next() else {
            return false;
        };
        return proof.expected_route().generation() == leaf.generation()
            && leaf.revision() == promoted_revision
            && leaf.state() == AcceptedRouteLeafState::Routed
            && leaf.lifecycle() == AcceptedInputLifecycle::Promoted;
    }
    let Ok(initial_revision) = AcceptedInputRevision::new(1) else {
        return false;
    };
    leaf.revision() == initial_revision
        && leaf.lifecycle() == AcceptedInputLifecycle::Admitted
        && leaf.last_transition().is_none()
        && match acceptance.expected_gate_state() {
            crate::InputGateState::AwaitingSteering(_) | crate::InputGateState::Steerable(_) => {
                leaf.state() == AcceptedRouteLeafState::Routed
            }
            crate::InputGateState::PendingTurn(_)
            | crate::InputGateState::Compacting { .. }
            | crate::InputGateState::Stopping { .. }
            | crate::InputGateState::FinalizingHistory(_)
            | crate::InputGateState::AwaitingTerminal(_) => {
                matches!(leaf.state(), AcceptedRouteLeafState::NextTurn(_))
            }
            crate::InputGateState::Idle => false,
        }
}

fn expected_kind(acceptance: &FirstAcceptance) -> FirstAcceptanceKind {
    if matches!(
        acceptance.expected_gate_state(),
        crate::InputGateState::Idle
    ) {
        FirstAcceptanceKind::Idle {
            user_item_id: acceptance.idle_user_item_id(),
        }
    } else {
        FirstAcceptanceKind::Accepted
    }
}

fn expected_image_label_records(
    acceptance: &FirstAcceptance,
) -> Option<(
    ImageLabelAuthorityHeadV1,
    Option<ImageLabelOriginSpanRecord>,
)> {
    let head = acceptance.expected_image_label_authority();
    if !head.is_exact() || head.thread_id() != acceptance.thread_id() {
        return None;
    }
    let Some(proof) = acceptance.asset_reference_set() else {
        return Some((head, None));
    };
    let end = proof.sequential().maximum_image_label()?;
    if head.permanent().contains(end) {
        return Some((head, None));
    }
    let start = crate::ImageLabelOrdinal::new(head.permanent().get().checked_add(1)?).ok()?;
    let owner = match expected_kind(acceptance) {
        FirstAcceptanceKind::Idle { user_item_id } => {
            ImageLabelOriginOwner::CanonicalItem(user_item_id)
        }
        FirstAcceptanceKind::Accepted => {
            ImageLabelOriginOwner::AcceptedInput(acceptance.accepted_input_id())
        }
    };
    let span =
        ImageLabelOriginSpanRecord::new(acceptance.thread_id(), start, end, owner, proof).ok()?;
    let advanced = head
        .advanced(crate::ImageLabelFrontier::from_raw(end.get()))
        .ok()?;
    Some((advanced, Some(span)))
}

fn active_session_is_exact(
    session: &DraftEditorCandidateSessionV1,
    acceptance: &FirstAcceptance,
) -> bool {
    session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && session.disposal_operation_id().is_none()
        && session.active_operation().is_none()
        && DraftEditorCandidateActivationBindingV1::from_head(session) == acceptance.candidate()
        && session.published_candidate_generation() == session.newest_candidate_generation()
        && session.published_root() == session.newest_root()
        && session.published_history() == session.newest_history()
}

fn disposed_session_is_exact(
    session: &DraftEditorCandidateSessionV1,
    acceptance: &FirstAcceptance,
) -> bool {
    let candidate = acceptance.candidate();
    session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed
        && session.disposal_operation_id() == Some(acceptance.session_disposal_operation_id())
        && session.active_operation().is_none()
        && candidate.session_generation().checked_add(1) == Some(session.session_generation())
        && session.draft_id() == candidate.draft_id()
        && session.session_id() == candidate.session_id()
        && session.published_candidate_generation() == candidate.candidate_generation()
        && session.published_root() == candidate.root()
        && session.published_history() == candidate.history()
        && session.logical_extent() == candidate.logical_extent()
}

fn exact_input_matches(acceptance: &FirstAcceptance, input: &AcceptedInputRecord) -> bool {
    let Ok(proof) = AcceptedInputAdmissionProof::new(
        acceptance.expected_thread_revision(),
        acceptance.draft_id(),
        acceptance.expected_draft_revision(),
        acceptance.expected_gate_revision(),
        acceptance.next_draft_id(),
    ) else {
        return false;
    };
    input.id() == acceptance.accepted_input_id()
        && input.thread_id() == acceptance.thread_id()
        && input.admission() == proof
        && input.content() == acceptance.materialization().content()
        && input.asset_reference_set() == acceptance.asset_reference_set()
        && input.admitted_at() == acceptance.admitted_at()
}

fn asset_proof_is_exact(acceptance: &FirstAcceptance) -> bool {
    let Ok(summary) = acceptance
        .materialization()
        .content()
        .sealed_marker_summary()
    else {
        return false;
    };
    match acceptance.asset_reference_set() {
        None => summary.sequential().marker_count() == 0,
        Some(proof) => {
            proof.sequential() == summary.sequential()
                && proof.ordered_assets().marker_count()
                    == acceptance
                        .materialization()
                        .content()
                        .summary()
                        .image_marker_count()
        }
    }
}

fn fresh_keys(
    acceptance: &FirstAcceptance,
    source_history: Option<&DraftEditHistoryFrontierV1>,
) -> Result<
    (
        Option<DraftPieceRootKeyV1>,
        Option<DraftEditHistoryFrontierKeyV1>,
    ),
    SyndicReadError,
> {
    let Some(source_history) = source_history else {
        return Ok((None, None));
    };
    let fresh_root = canonical_empty_draft_piece_root_v1(
        acceptance.next_draft_id(),
        beryl_model::DraftRevision::new(1).map_err(|_| {
            SyndicReadError::Invariant("first-acceptance fresh draft revision is invalid")
        })?,
        canonical_empty_draft_root_operation_id_v1(acceptance.next_draft_id()),
    );
    let policy = DraftEditHistoryPolicyV1::new(
        source_history.byte_budget(),
        source_history.retention_policy_revision(),
    )
    .ok_or(SyndicReadError::Invariant(
        "first-acceptance source history policy is invalid",
    ))?;
    let fresh_history = canonical_empty_draft_edit_history_v1(fresh_root.reference(), policy);
    Ok((
        Some(fresh_root.reference().key()),
        Some(fresh_history.reference().key()),
    ))
}

fn fresh_records_are_exact(
    acceptance: &FirstAcceptance,
    source_history: Option<&DraftEditHistoryFrontierV1>,
    fresh_root: Option<&DraftPieceRootRecordV1>,
    fresh_history: Option<&DraftEditHistoryFrontierV1>,
) -> Result<bool, SyndicReadError> {
    let Some(source_history) = source_history else {
        return Ok(false);
    };
    let expected_root = canonical_empty_draft_piece_root_v1(
        acceptance.next_draft_id(),
        beryl_model::DraftRevision::new(1).map_err(|_| {
            SyndicReadError::Invariant("first-acceptance fresh draft revision is invalid")
        })?,
        canonical_empty_draft_root_operation_id_v1(acceptance.next_draft_id()),
    );
    let policy = DraftEditHistoryPolicyV1::new(
        source_history.byte_budget(),
        source_history.retention_policy_revision(),
    )
    .ok_or(SyndicReadError::Invariant(
        "first-acceptance source history policy is invalid",
    ))?;
    let expected_history = canonical_empty_draft_edit_history_v1(expected_root.reference(), policy);
    Ok(fresh_root == Some(&expected_root) && fresh_history == Some(&expected_history))
}
