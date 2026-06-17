use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::*;

fn view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn record_id(name: &str) -> ResidentPresentationRecordId {
    ResidentPresentationRecordId(format!("record-{name}"))
}

fn request(manual_delta_px: f32) -> RealizedFrameRequest {
    RealizedFrameRequest {
        viewport_height_px: 20.0,
        overscan_height_px: 0.0,
        default_record_height_px: 10.0,
        manual_delta_px,
        observed_presentation_revision: None,
    }
}

fn snapshot_with_names(presentation_revision: u64, names: &[&str]) -> ResidentTranscriptSnapshot {
    ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision,
        state: ResidentTranscriptSnapshotState::Fixture {
            label: "frame-test".to_string(),
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
            source: ResidentRecordSource::Syndic(SyndicSourceProvenance {
                view_id: view_id(),
                position: Some(TranscriptViewPosition(position)),
                turn_id: Some(SyndicTurnId(format!("turn-{name}"))),
                item_id: Some(SyndicItemId(format!("item-{name}"))),
                projection_id: Some(projection_id.clone()),
                resource_id: None,
                source_range: Some(position..position + 4),
                resource_range: None,
                copy_source_range: Some(position..position + 4),
            }),
            projection_id: Some(projection_id),
            projection_revision: Some(ProviderRevision(0)),
            presentation_revision,
            copy_source_range: Some(position..position + 4),
        },
        estimated_bytes: 8,
    }
}

fn contains_fact(facts: &[DemandFact], expected: DemandFactKind) -> bool {
    facts.iter().any(|fact| fact.kind == expected)
}

fn obsolete_ranges(facts: &[DemandFact]) -> Vec<Range<usize>> {
    facts
        .iter()
        .filter_map(|fact| match &fact.kind {
            DemandFactKind::ObsoleteRange { range } => Some(range.clone()),
            _ => None,
        })
        .collect()
}

fn measured_record_count(facts: &[DemandFact]) -> usize {
    facts
        .iter()
        .filter(|fact| matches!(fact.kind, DemandFactKind::MeasuredRecord { .. }))
        .count()
}

fn anchor_record_id(window: &RealizedFrameWindow) -> Option<ResidentPresentationRecordId> {
    window
        .anchor
        .as_ref()
        .map(|anchor| anchor.record_id.clone())
}

fn anchor_viewport_y(window: &RealizedFrameWindow) -> Option<f32> {
    window.anchor.as_ref().map(|anchor| anchor.viewport_y_px)
}

#[test]
fn empty_snapshot_reports_empty_ranges_and_anchor_fact() {
    let snapshot = ResidentTranscriptSnapshot::empty();
    let mut controller = RealizedFrameScrollController::new();

    let window = controller.realize(&snapshot, request(0.0));

    assert!(window.records.is_empty());
    assert_eq!(window.visible_range, 0..0);
    assert_eq!(window.overscan_range, 0..0);
    assert_eq!(window.anchor, None);
    assert_eq!(window.clamp, None);
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::VisibleRange { range: 0..0 }
    ));
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::OverscanRange { range: 0..0 }
    ));
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::CurrentAnchor {
            record_id: None,
            position: None,
        }
    ));
}

#[test]
fn manual_scroll_on_empty_snapshot_clamps_and_reports_demand() {
    let snapshot = ResidentTranscriptSnapshot::empty();
    let mut controller = RealizedFrameScrollController::new();

    let window = controller.realize(&snapshot, request(40.0));

    assert!(window.records.is_empty());
    assert_eq!(window.visible_range, 0..0);
    assert_eq!(window.overscan_range, 0..0);
    assert_eq!(window.anchor, None);
    assert_eq!(window.manual_delta_px, 40.0);
    assert_eq!(window.manual_scroll_total_px, 40.0);
    assert_eq!(
        window.clamp,
        Some(RealizedFrameClamp {
            direction: TranscriptPageDirection::Forward,
            anchor_index: 0,
        })
    );
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::MissingAfter { anchor_index: 0 }
    ));
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::AdjacentRange {
            anchor_index: 0,
            direction: TranscriptPageDirection::Forward,
        }
    ));
}

#[test]
fn bounded_snapshot_realizes_visible_and_overscan_records() {
    let snapshot = snapshot_with_names(7, &["a", "b", "c", "d", "e"]);
    let mut controller = RealizedFrameScrollController::new();
    let mut frame_request = request(0.0);
    frame_request.overscan_height_px = 10.0;

    let window = controller.realize(&snapshot, frame_request);

    assert_eq!(window.visible_range, 0..2);
    assert_eq!(window.overscan_range, 0..3);
    assert_eq!(window.records.len(), 3);
    assert_eq!(window.records[0].record_id, record_id("a"));
    assert_eq!(window.records[1].top_px, 10.0);
    assert_eq!(measured_record_count(&window.demand_facts), 3);
    assert!(contains_fact(
        &window.demand_facts,
        DemandFactKind::CurrentAnchor {
            record_id: Some(record_id("a")),
            position: Some(TranscriptViewPosition(0)),
        }
    ));
}

#[test]
fn live_tail_following_places_initial_tail() {
    let snapshot = snapshot_with_names(21, &["a", "b", "c", "d"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();

    let window = controller.realize(&snapshot, request(0.0));

    assert_eq!(window.visible_range, 2..4);
    assert_eq!(anchor_record_id(&window), Some(record_id("c")));
    assert_eq!(anchor_viewport_y(&window), Some(0.0));
    assert_eq!(
        controller.state_snapshot().scroll_mode,
        RealizedFrameScrollMode::LiveTailFollowing
    );
}

#[test]
fn live_tail_following_tracks_coherent_tail_growth() {
    let first_snapshot = snapshot_with_names(22, &["a", "b", "c"]);
    let second_snapshot = snapshot_with_names(23, &["a", "b", "c", "d"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();

    let first_window = controller.realize(&first_snapshot, request(0.0));
    assert_eq!(first_window.visible_range, 1..3);
    assert_eq!(anchor_record_id(&first_window), Some(record_id("b")));

    let second_window = controller.realize(&second_snapshot, request(0.0));

    assert_eq!(second_window.visible_range, 2..4);
    assert_eq!(anchor_record_id(&second_window), Some(record_id("c")));
    assert_eq!(anchor_viewport_y(&second_window), Some(0.0));
    assert_eq!(
        controller.state_snapshot().scroll_mode,
        RealizedFrameScrollMode::LiveTailFollowing
    );
}

#[test]
fn manual_scroll_detaches_live_tail_before_tail_growth() {
    let first_snapshot = snapshot_with_names(24, &["a", "b", "c", "d"]);
    let second_snapshot = snapshot_with_names(25, &["a", "b", "c", "d", "e"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();
    controller.realize(&first_snapshot, request(0.0));

    let manual_window = controller.realize(&first_snapshot, request(-10.0));
    assert_eq!(
        controller.state_snapshot().scroll_mode,
        RealizedFrameScrollMode::DetachedManual
    );
    assert_eq!(anchor_record_id(&manual_window), Some(record_id("b")));

    let grown_window = controller.realize(&second_snapshot, request(0.0));

    assert_eq!(anchor_record_id(&grown_window), Some(record_id("b")));
    assert_ne!(anchor_record_id(&grown_window), Some(record_id("e")));
}

#[test]
fn stale_live_tail_request_is_rejected_without_mutating_scroll_state() {
    let snapshot = snapshot_with_names(26, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();
    controller.realize(&snapshot, request(0.0));
    let before = controller.state_snapshot();
    let mut stale_request = request(100.0);
    stale_request.observed_presentation_revision = Some(25);

    let window = controller.realize(&snapshot, stale_request);

    assert!(window.records.is_empty());
    assert_eq!(
        window.demand_facts,
        vec![DemandFact::new(
            25,
            DemandFactKind::StaleMeasurement {
                observed_revision: 25
            }
        )]
    );
    assert_eq!(controller.state_snapshot(), before);
}

#[test]
fn empty_live_tail_snapshot_keeps_pending_tail_placement() {
    let empty_snapshot = ResidentTranscriptSnapshot::empty();
    let filled_snapshot = snapshot_with_names(27, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();

    let empty_window = controller.realize(&empty_snapshot, request(0.0));
    assert!(empty_window.records.is_empty());
    assert_eq!(
        controller.state_snapshot().scroll_mode,
        RealizedFrameScrollMode::LiveTailFollowing
    );

    let filled_window = controller.realize(&filled_snapshot, request(0.0));

    assert_eq!(filled_window.visible_range, 1..3);
    assert_eq!(anchor_record_id(&filled_window), Some(record_id("b")));
}

#[test]
fn live_tail_following_preserves_anchor_for_non_tail_mutation() {
    let first_snapshot = snapshot_with_names(28, &["a"]);
    let mutated_snapshot = snapshot_with_names(29, &["x", "a", "b", "c", "d"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.begin_live_tail_following();

    let first_window = controller.realize(&first_snapshot, request(0.0));
    assert_eq!(anchor_record_id(&first_window), Some(record_id("a")));

    let mutated_window = controller.realize(&mutated_snapshot, request(0.0));

    assert_eq!(mutated_window.visible_range, 1..3);
    assert_eq!(anchor_record_id(&mutated_window), Some(record_id("a")));
    assert_eq!(
        mutated_window.anchor.as_ref().map(|anchor| anchor.index),
        Some(1)
    );
    assert_ne!(mutated_window.visible_range, 3..5);
}

#[test]
fn manual_scroll_clamps_at_trailing_edge_and_reports_missing_demand() {
    let snapshot = snapshot_with_names(8, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.realize(&snapshot, request(0.0));

    let window = controller.realize(&snapshot, request(100.0));

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
fn anchor_is_preserved_when_record_remains_resident() {
    let first_snapshot = snapshot_with_names(9, &["a", "b", "c", "d"]);
    let second_snapshot = snapshot_with_names(10, &["prefix", "a", "b", "c", "d"]);
    let mut controller = RealizedFrameScrollController::new();

    let first_window = controller.realize(&first_snapshot, request(15.0));
    assert_eq!(
        first_window
            .anchor
            .as_ref()
            .map(|anchor| anchor.record_id.clone()),
        Some(record_id("b"))
    );
    assert_eq!(
        first_window
            .anchor
            .as_ref()
            .map(|anchor| anchor.viewport_y_px),
        Some(-5.0)
    );

    let second_window = controller.realize(&second_snapshot, request(0.0));

    assert_eq!(
        second_window
            .anchor
            .as_ref()
            .map(|anchor| anchor.record_id.clone()),
        Some(record_id("b"))
    );
    assert_eq!(
        second_window.anchor.as_ref().map(|anchor| anchor.index),
        Some(2)
    );
    assert_eq!(
        second_window
            .anchor
            .as_ref()
            .map(|anchor| anchor.viewport_y_px),
        Some(-5.0)
    );
    assert!(contains_fact(
        &second_window.demand_facts,
        DemandFactKind::CurrentAnchor {
            record_id: Some(record_id("b")),
            position: Some(TranscriptViewPosition(2)),
        }
    ));
}

#[test]
fn stale_revision_request_is_rejected_without_mutating_scroll_state() {
    let snapshot = snapshot_with_names(12, &["a", "b", "c"]);
    let mut controller = RealizedFrameScrollController::new();
    controller.realize(&snapshot, request(10.0));
    let before = controller.state_snapshot();
    let mut stale_request = request(100.0);
    stale_request.observed_presentation_revision = Some(11);

    let window = controller.realize(&snapshot, stale_request);

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

    let measurement_fact = controller.observe_record_measurement(
        &snapshot,
        RealizedRecordMeasurement {
            presentation_revision: 11,
            record_id: record_id("a"),
            height_px: 42.0,
        },
    );
    assert_eq!(
        measurement_fact,
        DemandFact::new(
            11,
            DemandFactKind::StaleMeasurement {
                observed_revision: 11
            }
        )
    );
}

#[test]
fn obsolete_range_is_reported_when_overscan_window_moves() {
    let snapshot = snapshot_with_names(13, &["a", "b", "c", "d", "e", "f"]);
    let mut controller = RealizedFrameScrollController::new();

    let first_window = controller.realize(&snapshot, request(0.0));
    assert_eq!(first_window.overscan_range, 0..2);

    let second_window = controller.realize(&snapshot, request(30.0));

    assert_eq!(second_window.overscan_range, 3..5);
    assert_eq!(obsolete_ranges(&second_window.demand_facts), vec![0..2]);
    assert!(contains_fact(
        &second_window.demand_facts,
        DemandFactKind::VisibleRange { range: 3..5 }
    ));
    assert!(contains_fact(
        &second_window.demand_facts,
        DemandFactKind::OverscanRange { range: 3..5 }
    ));
}
