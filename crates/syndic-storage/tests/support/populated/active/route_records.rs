use beryl_model::{
    CasThreadId, CasTurnId, SyndicAcceptedInputId, SyndicDraftId, SyndicPathDigest, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};

use super::*;

pub(super) struct RouteRecordFacts {
    pub(super) thread: SyndicThreadId,
    pub(super) current_draft: SyndicDraftId,
    pub(super) steering: SyndicAcceptedInputId,
    pub(super) steering_gate_revision: InputGateRevision,
    pub(super) steering_content: ContentReference,
    pub(super) steering_asset_reference_set: SealedAssetReferenceSetProof,
    pub(super) marker_label: ImageLabelOrdinal,
    pub(super) steering_target: SteeringTargetProof,
    pub(super) next: SyndicAcceptedInputId,
    pub(super) gate_revision: InputGateRevision,
    pub(super) empty_content: ContentReference,
    pub(super) binding_one: BindingRevision,
    pub(super) binding_two: BindingRevision,
    pub(super) binding_three: BindingRevision,
    pub(super) selected: SelectedPathProof,
    pub(super) usable: UsableCasBinding,
    pub(super) active_binding: ActiveCasBinding,
    pub(super) digest: SyndicPathDigest,
    pub(super) turn: SyndicTurnId,
    pub(super) cas_thread: CasThreadId,
    pub(super) represented: CasRepresentedPrefixProof,
    pub(super) lineage: CasLineageProof,
    pub(super) cas_turn: CasTurnId,
}

pub(super) fn records(facts: RouteRecordFacts) -> Vec<FixtureRecord> {
    let RouteRecordFacts {
        thread,
        current_draft,
        steering,
        steering_gate_revision,
        steering_content,
        steering_asset_reference_set,
        marker_label,
        steering_target,
        next,
        gate_revision,
        empty_content,
        binding_one,
        binding_two,
        binding_three,
        selected,
        usable,
        active_binding,
        digest,
        turn,
        cas_thread,
        represented,
        lineage,
        cas_turn,
    } = facts;
    let accepted_revision = AcceptedInputRevision::new(1).unwrap();
    let steering_generation = AcceptedRouteGeneration::FIRST;

    vec![
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                steering,
                thread,
                AcceptedInputOrdinal::FIRST,
                AcceptedInputAdmissionProof::new(
                    ThreadRevision::new(1).unwrap(),
                    SyndicDraftId::from_bytes(*steering.as_bytes()),
                    DraftRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    SyndicDraftId::from_bytes(*next.as_bytes()),
                )
                .unwrap(),
                steering_generation,
                steering_content,
                Some(steering_asset_reference_set),
                timestamp(8),
            )
            .unwrap(),
        ),
        FixtureRecord::ImageLabelOriginSpan(
            ImageLabelOriginSpanRecord::new(
                thread,
                marker_label,
                marker_label,
                ImageLabelOriginOwner::AcceptedInput(steering),
                steering_asset_reference_set,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            AcceptedInputOrdinal::FIRST,
            steering,
            steering_generation,
        )),
        FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
            thread,
            AcceptedRouteHeadProof::new(
                steering_generation,
                AcceptedRouteRevision::new(2).unwrap(),
            ),
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                steering_generation,
                AcceptedRouteRevision::new(2).unwrap(),
                AcceptedRouteTarget::Steering(steering_target),
                Some(AcceptedInputOrdinal::FIRST),
                Some(AcceptedInputOrdinal::new(2).unwrap()),
                2,
                1,
                0,
                1,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedReadySource(AcceptedReadySourceRecord::new(
            thread,
            gate_revision,
            steering_generation,
            AcceptedRouteRevision::new(2).unwrap(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(2).unwrap(),
        )),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            steering,
            thread,
            steering_generation,
            AcceptedInputOrdinal::FIRST,
            accepted_revision,
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                next,
                thread,
                AcceptedInputOrdinal::new(2).unwrap(),
                AcceptedInputAdmissionProof::new(
                    ThreadRevision::new(2).unwrap(),
                    SyndicDraftId::from_bytes(*next.as_bytes()),
                    DraftRevision::new(1).unwrap(),
                    steering_gate_revision,
                    current_draft,
                )
                .unwrap(),
                steering_generation,
                empty_content,
                None,
                timestamp(8),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread,
            AcceptedInputOrdinal::new(2).unwrap(),
            next,
            steering_generation,
        )),
        FixtureRecord::AcceptedRouteLeaf(fixture_route_leaf_with_transition(
            AcceptedRouteLeafRecord::new(
                next,
                thread,
                steering_generation,
                AcceptedInputOrdinal::new(2).unwrap(),
                accepted_revision.checked_next().unwrap(),
                AcceptedRouteLeafState::NextTurn(NextTurnReason::SteeringRejected),
                AcceptedInputLifecycle::Retryable,
            ),
            AcceptedRouteLeafTransitionProof::new(
                InputGateRevision::new(3).unwrap(),
                AcceptedRouteHeadProof::new(steering_generation, AcceptedRouteRevision::FIRST),
                accepted_revision,
                AcceptedRouteLeafTransitionKind::SteeringRejected,
            ),
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            steering_generation,
            AcceptedRouteRevision::new(2).unwrap(),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputOrdinal::new(2).unwrap(),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_one,
            selected,
            BindingState::unbound("active fixture history").unwrap(),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_two,
            selected,
            BindingState::valid(usable),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_three,
            selected,
            BindingState::active(active_binding),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_three,
            BindingLifecycle::Active,
            digest,
        )),
        FixtureRecord::ExecutionSnapshot(ExecutionSnapshotRecord::new(
            active_snapshot(),
            thread,
            binding_three,
            steering_gate_revision,
            turn,
            cas_thread.clone(),
            selected,
            represented,
            CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
            execution_binding(),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(8),
        )),
        FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
            active_snapshot(),
            thread,
            turn,
            binding_three,
            cas_thread.clone(),
            cas_turn.clone(),
            timestamp(8),
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
            cas_thread.clone(),
            thread,
            binding_two,
            binding_three,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            cas_thread.clone(),
            thread,
            binding_two,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            cas_thread.clone(),
            thread,
            binding_three,
        )),
        FixtureRecord::CasTurn(CasTurnIndexRecord::new(
            cas_thread,
            cas_turn,
            thread,
            turn,
            binding_three,
            active_snapshot(),
            CasNativeTurnCount::new(1),
        )),
    ]
}
