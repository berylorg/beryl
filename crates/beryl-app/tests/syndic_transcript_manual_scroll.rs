#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
pub(crate) use syndic_transcript_core::*;

mod dynamic_tools {
    pub(crate) const BERYL_DYNAMIC_TOOL_NAMESPACE: &str = "beryl";
}

mod gui_control_dynamic_tools {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub(crate) struct MarkdownCacheUiState;
}

#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

#[path = "../src/diagnostic_dynamic_tools.rs"]
mod diagnostic_dynamic_tools;

#[path = "../src/shell/syndic_transcript/diagnostics.rs"]
mod diagnostics;

pub(crate) use diagnostics::*;

#[path = "../src/shell/syndic_transcript/host.rs"]
mod host;

use host::SyndicTranscriptHost;

const REVISION: ProviderRevision = ProviderRevision(71);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("manual-scroll-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn record_id(name: &str) -> ResidentPresentationRecordId {
    ResidentPresentationRecordId(format!("record-{name}"))
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
        id: TranscriptViewRecordId(format!("view-record-{name}")),
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

fn presentation_record(
    presentation_revision: u64,
    position: u64,
    name: &str,
) -> ResidentPresentationRecord {
    let projection_id = projection_id(name);
    ResidentPresentationRecord {
        id: record_id(name),
        kind: ResidentPresentationRecordKind::TextChunk {
            narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
            text: format!("text-{name}"),
        },
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::Syndic(provenance(&view_id(), position, &projection_id)),
            projection_id: Some(projection_id),
            projection_revision: Some(REVISION),
            presentation_revision,
            copy_source_range: Some(position..position + 4),
        },
        estimated_bytes: 8,
    }
}

fn snapshot_with_names(presentation_revision: u64, names: &[&str]) -> ResidentTranscriptSnapshot {
    ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision,
        state: ResidentTranscriptSnapshotState::ProviderBacked {
            label: "manual-scroll-test".to_string(),
        },
        records: names
            .iter()
            .enumerate()
            .map(|(index, name)| presentation_record(presentation_revision, index as u64, name))
            .collect(),
        resources: ResidentResourceSnapshot::default(),
        realized_range: None,
        visible_range: None,
    }
}

fn command(snapshot: &ResidentTranscriptSnapshot, delta_px: f32) -> ManualTranscriptScrollCommand {
    ManualTranscriptScrollCommand::new(
        20.0,
        0.0,
        10.0,
        delta_px,
        Some(snapshot.presentation_revision),
    )
}

fn wide_command(
    snapshot: &ResidentTranscriptSnapshot,
    delta_px: f32,
) -> ManualTranscriptScrollCommand {
    ManualTranscriptScrollCommand::new(
        32.0,
        16.0,
        16.0,
        delta_px,
        Some(snapshot.presentation_revision),
    )
}

fn contains_fact(facts: &[DemandFact], expected: DemandFactKind) -> bool {
    facts.iter().any(|fact| fact.kind == expected)
}

fn has_fact(facts: &[DemandFact], predicate: impl Fn(&DemandFactKind) -> bool) -> bool {
    facts.iter().any(|fact| predicate(&fact.kind))
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

fn seed_core(names: &[&str]) -> ResidentTranscriptCore {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        names
            .iter()
            .enumerate()
            .map(|(index, name)| view_record(&view_id, index as u64, name))
            .collect(),
    );
    for (index, name) in names.iter().enumerate() {
        provider.insert_projection_record(text_projection(&view_id, index as u64, name));
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

#[test]
fn manual_scroll_command_preserves_exact_delta_and_reports_clamp_demand() {
    let snapshot = snapshot_with_names(8, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.realize(&snapshot, command(&snapshot, 0.0).frame_request());

    let window = controller.realize(&snapshot, command(&snapshot, 100.0).frame_request());

    assert_eq!(window.manual_delta_px, 100.0);
    assert_eq!(window.manual_scroll_total_px, 100.0);
    assert_eq!(window.visible_range, 1..3);
    assert_eq!(
        window.clamp,
        Some(RealizedFrameClamp {
            direction: TranscriptPageDirection::Forward,
            anchor_index: 2,
        })
    );
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::MissingAfter { anchor_index: 2 }
    ));
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::AdjacentRange {
            anchor_index: 2,
            direction: TranscriptPageDirection::Forward,
        }
    ));
}

#[test]
fn stale_manual_scroll_revision_is_rejected_without_mutating_scroll_state() {
    let snapshot = snapshot_with_names(12, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.realize(&snapshot, command(&snapshot, 10.0).frame_request());
    let before = controller.state_snapshot();
    let stale_command = ManualTranscriptScrollCommand::new(20.0, 0.0, 10.0, 100.0, Some(11));

    let window = controller.realize(&snapshot, stale_command.frame_request());

    assert!(window.records.is_empty());
    assert_eq!(
        window.demand_facts,
        vec![DemandFact::new(
            11,
            DemandFactKind::StaleMeasurement {
                observed_revision: 11
            }
        )]
    );
    assert_eq!(controller.state_snapshot(), before);
}

#[test]
fn manual_scroll_demand_facts_update_resident_ranges_from_snapshot_only() {
    let mut core = seed_core(&["zero", "one", "two", "three", "four", "five"]);
    let mut controller = RealizedFrameScrollController::new();
    let snapshot = core.presentation_snapshot();

    let first_window = controller.realize(&snapshot, wide_command(&snapshot, 0.0).frame_request());
    for fact in &first_window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    let first_snapshot = core.core_snapshot();
    assert_eq!(first_snapshot.presentation.visible_range, Some(0..2));
    assert_eq!(first_snapshot.presentation.realized_range, Some(0..3));

    let snapshot = core.presentation_snapshot();
    let second_window =
        controller.realize(&snapshot, wide_command(&snapshot, 200.0).frame_request());
    for fact in &second_window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    let second_snapshot = core.core_snapshot();

    assert!(has_fact(&second_window.demand_facts, |kind| {
        matches!(kind, DemandFactKind::MissingAfter { .. })
    }));
    assert!(has_fact(&second_window.demand_facts, |kind| {
        matches!(
            kind,
            DemandFactKind::AdjacentRange {
                direction: TranscriptPageDirection::Forward,
                ..
            }
        )
    }));
    assert!(has_fact(&second_window.demand_facts, |kind| {
        matches!(kind, DemandFactKind::ObsoleteRange { range } if range == &(0..3))
    }));
    assert_eq!(second_snapshot.resident.obsolete_ranges, vec![0..3]);
}

#[test]
fn host_scroll_input_diagnostics_do_not_consume_unanchored_frames() {
    let mut host = SyndicTranscriptHost::empty();
    let empty_snapshot = ResidentTranscriptSnapshot::empty();

    host.manual_scroll(command(&empty_snapshot, 40.0));

    let metrics = host.frame_metrics_snapshot();
    let empty_event = metrics
        .scroll_inputs
        .events
        .last()
        .expect("empty manual scroll should record a scroll input");
    assert!(!empty_event.consumed);
    assert!(!empty_event.changed);
    assert_eq!(empty_event.requested_delta, 40.0);
    assert_eq!(empty_event.consumed_delta, 0.0);
    assert_eq!(empty_event.residual_delta, 40.0);
    assert!(empty_event.before_anchor.is_none());
    assert!(empty_event.after_anchor.is_none());

    let view_id = view_id();
    let prepared = PreparedTranscriptActivation::new(
        view_id.clone(),
        TranscriptActivationPlacement::Start,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view_id.clone(),
            revision: REVISION,
            history_state: TranscriptProviderHistoryState::Complete,
            records: vec![
                view_record(&view_id, 0, "zero"),
                view_record(&view_id, 1, "one"),
                view_record(&view_id, 2, "two"),
            ],
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: view_id.clone(),
                revision: REVISION,
                records: vec![
                    text_projection(&view_id, 0, "zero"),
                    text_projection(&view_id, 1, "one"),
                    text_projection(&view_id, 2, "two"),
                ],
                rejections: Vec::new(),
            },
        )),
    );
    let _ = host.apply_prepared_activation(prepared, TranscriptActivationSource::Test);
    let anchored_snapshot = host.snapshot();
    host.realize_frame(wide_command(&anchored_snapshot, 0.0).frame_request());
    host.manual_scroll(ManualTranscriptScrollCommand::new(
        32.0,
        16.0,
        16.0,
        100.0,
        Some(REVISION.0.saturating_sub(1)),
    ));

    let metrics = host.frame_metrics_snapshot();
    let stale_event = metrics
        .scroll_inputs
        .events
        .last()
        .expect("stale manual scroll should record a scroll input");
    assert!(!stale_event.consumed);
    assert!(!stale_event.changed);
    assert_eq!(stale_event.requested_delta, 100.0);
    assert_eq!(stale_event.consumed_delta, 0.0);
    assert_eq!(stale_event.residual_delta, 100.0);
    assert!(stale_event.before_anchor.is_some());
    assert!(stale_event.after_anchor.is_none());
    assert_eq!(
        stale_event
            .after_visible_segment_range
            .as_ref()
            .map(|range| { (range.start, range.end) }),
        Some((0, 0))
    );
}
