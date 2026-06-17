#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(91);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("status-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(position..position + 4),
        resource_range: None,
        copy_source_range: Some(position..position + 4),
    }
}

fn view_record(view_id: &TranscriptViewId, position: u64, name: &str) -> TranscriptViewRecord {
    let projection_id = projection_id(name);
    TranscriptViewRecord {
        id: TranscriptViewRecordId(format!("record-{name}")),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
        provenance: provenance(view_id, position, &projection_id),
    }
}

fn text_projection(view_id: &TranscriptViewId, position: u64, name: &str) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: REVISION,
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: format!("text-{name}"),
        },
        provenance: provenance(view_id, position, &projection_id),
    }
}

fn handle_provider_request(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    request: TranscriptProviderRequest,
) -> ResidentProviderResponseEffect {
    let response = provider
        .handle_request(request)
        .expect("fixture provider request should not fail");
    core.handle_provider_response(response)
}

fn seed_text_core(names: &[&str]) -> ResidentTranscriptCore {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        names
            .iter()
            .enumerate()
            .map(|(index, name)| view_record(&view_id, (index as u64 + 1) * 10, name))
            .collect(),
    );
    for (index, name) in names.iter().enumerate() {
        provider.insert_projection_record(text_projection(&view_id, (index as u64 + 1) * 10, name));
    }

    let mut core = ResidentTranscriptCore::empty();
    let page_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert!(matches!(
        handle_provider_request(&mut core, &mut provider, page_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { .. }
    ));
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should request projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: names.len(),
            rejected_count: 0
        }
    );
    core
}

fn frame_request(snapshot: &ResidentTranscriptSnapshot) -> RealizedFrameRequest {
    RealizedFrameRequest {
        viewport_height_px: 32.0,
        overscan_height_px: 16.0,
        default_record_height_px: 16.0,
        manual_delta_px: 0.0,
        observed_presentation_revision: Some(snapshot.presentation_revision),
    }
}

fn status_facts(
    core: &ResidentTranscriptCore,
    scroll: &RealizedFrameScrollController,
) -> ResidentTranscriptStatusFacts {
    ResidentTranscriptStatusFacts::from_core_snapshot(
        &core.core_snapshot(),
        scroll.state_snapshot(),
    )
}

#[test]
fn empty_status_facts_are_unknown_for_turn_view() {
    let core = ResidentTranscriptCore::empty();
    let scroll = RealizedFrameScrollController::new();

    let facts = status_facts(&core, &scroll);

    assert_eq!(facts.state, ResidentTranscriptStatusState::Empty);
    assert_eq!(
        facts.scroll_mode,
        ResidentTranscriptStatusScrollMode::DetachedManual
    );
    assert_eq!(facts.activation_revision, 0);
    assert_eq!(facts.presentation_revision, 0);
    assert_eq!(facts.resident_presentation_record_count, 0);
    assert_eq!(facts.pending_demand_fact_count, 0);
    assert_eq!(facts.pending_provider_request_count, 0);
    assert_eq!(facts.rejected_demand_count, 0);
    assert_eq!(facts.turn_view, ResidentTranscriptTurnViewFacts::unknown());
    assert_eq!(
        ResidentTranscriptStatusFacts::unknown().state,
        ResidentTranscriptStatusState::Unknown
    );
}

#[test]
fn fixture_status_facts_publish_scroll_anchor_and_resident_counts() {
    let mut core = seed_text_core(&["alpha", "beta", "gamma"]);
    let mut scroll = RealizedFrameScrollController::new();
    scroll.begin_live_tail_following();
    let snapshot = core.presentation_snapshot();
    let window = scroll.realize(&snapshot, frame_request(&snapshot));
    for fact in &window.demand_facts {
        core.push_demand_fact(fact.clone());
    }

    let facts = status_facts(&core, &scroll);

    assert_eq!(
        facts.state,
        ResidentTranscriptStatusState::FixtureBacked {
            label: "resident-syndic-projections".to_string()
        }
    );
    assert_eq!(
        facts.scroll_mode,
        ResidentTranscriptStatusScrollMode::LiveTailFollowing
    );
    assert_eq!(facts.anchor_position, Some(TranscriptViewPosition(20)));
    assert!(facts.anchor_record_id.is_some());
    assert_eq!(facts.resident_presentation_record_count, 3);
    assert_eq!(facts.resident_view_record_count, 3);
    assert_eq!(facts.resident_projection_record_count, 3);
    assert_eq!(facts.turn_view, ResidentTranscriptTurnViewFacts::unknown());
}

#[test]
fn pending_and_rejected_demands_are_status_facts_not_content() {
    let mut core = ResidentTranscriptCore::empty();
    let scroll = RealizedFrameScrollController::new();
    let view_id = view_id();
    let outcome = core.begin_activation(TranscriptActivationSeed::new(
        view_id.clone(),
        TranscriptActivationSource::Test,
        TranscriptActivationPlacement::Tail,
    ));
    let request = outcome
        .provider_request
        .expect("view-backed activation should reserve a provider request");
    core.push_demand_fact(DemandFact::new(
        outcome.presentation_revision,
        DemandFactKind::Viewport {
            width_px: 640.0,
            height_px: 480.0,
        },
    ));
    core.handle_provider_response(TranscriptProviderResponse {
        request_id: request.id,
        kind: TranscriptProviderResponseKind::Rejected(TranscriptProviderRejection {
            target: TranscriptProviderTarget::View(view_id),
            reason: TranscriptProviderRejectionReason::MissingView,
            revision: None,
            message: Some("missing view".to_string()),
        }),
    });

    let facts = status_facts(&core, &scroll);

    assert_eq!(facts.state, ResidentTranscriptStatusState::Empty);
    assert_eq!(facts.activation_revision, 1);
    assert_eq!(facts.pending_demand_fact_count, 1);
    assert_eq!(facts.pending_provider_request_count, 0);
    assert_eq!(facts.rejected_demand_count, 1);
    assert_eq!(facts.resident_presentation_record_count, 0);
    assert_eq!(facts.turn_view, ResidentTranscriptTurnViewFacts::unknown());
}
