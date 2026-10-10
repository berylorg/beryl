use std::sync::Arc;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::*;

fn snapshot(count: usize) -> Arc<ResidentTranscriptSnapshot> {
    Arc::new(ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision: 7,
        state: ResidentTranscriptSnapshotState::ProviderBacked {
            label: "measured-frame".into(),
        },
        records: (0..count)
            .map(|index| ResidentPresentationRecord {
                id: ResidentPresentationRecordId(format!("record-{index}")),
                kind: ResidentPresentationRecordKind::TextChunk {
                    narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
                    text: "variable geometry".into(),
                },
                provenance: ResidentRecordProvenance {
                    source: ResidentRecordSource::LocalUi,
                    projection_id: None,
                    projection_revision: None,
                    presentation_revision: 7,
                    copy_source_range: None,
                },
                estimated_bytes: 24,
            })
            .collect::<Vec<_>>()
            .into(),
        resources: ResidentResourceSnapshot::default(),
        realized_range: None,
        visible_range: None,
    })
}

fn request(viewport: f32, overscan: f32, delta: f32) -> RealizedFrameRequest {
    RealizedFrameRequest {
        viewport_height_px: viewport,
        overscan_height_px: overscan,
        default_record_height_px: 9_000.0,
        manual_delta_px: delta,
        observed_presentation_revision: Some(7),
    }
}

fn state(
    snapshot: &ResidentTranscriptSnapshot,
    index: usize,
    top: f32,
) -> RealizedFrameScrollStateSnapshot {
    RealizedFrameScrollStateSnapshot {
        anchor: Some(RealizedFrameAnchor {
            record_id: snapshot.records[index].id.clone(),
            index,
            viewport_y_px: top,
            position: None,
        }),
        scroll_mode: RealizedFrameScrollMode::DetachedManual,
        presentation_revision: Some(6),
        manual_scroll_total_px: 0.0,
    }
}

fn fresh(mode: RealizedFrameScrollMode) -> RealizedFrameScrollStateSnapshot {
    RealizedFrameScrollStateSnapshot {
        anchor: None,
        scroll_mode: mode,
        presentation_revision: None,
        manual_scroll_total_px: 0.0,
    }
}

fn index(record: &ResidentPresentationRecord) -> usize {
    record
        .id
        .0
        .strip_prefix("record-")
        .unwrap()
        .parse()
        .unwrap()
}

fn complete(
    preparation: &mut MeasuredFramePreparation,
    budget: usize,
    mut height: impl FnMut(&ResidentPresentationRecord) -> f32,
) -> RealizedFrameWindow {
    for _ in 0..2_000 {
        let mut observations = 0;
        let result = preparation.advance(budget, |record| {
            observations += 1;
            height(record)
        });
        assert_eq!(observations, preparation.work_last_step());
        assert!(observations <= budget.min(32));
        if let Some(window) = result {
            return window;
        }
    }
    panic!("measured preparation did not complete");
}

fn assert_actual_geometry(window: &RealizedFrameWindow, height: impl Fn(usize) -> f32) {
    assert!(!window.records.is_empty());
    for record in &window.records {
        assert_eq!(record.height_px, height(record.index));
        assert!(window.demand_facts.iter().any(|fact| matches!(&fact.kind,
            DemandFactKind::MeasuredRecord { record_id, height_px }
                if record_id == &record.record_id && *height_px == record.height_px)));
    }
    for records in window.records.windows(2) {
        assert_eq!(records[0].index + 1, records[1].index);
        assert_eq!(records[0].top_px + records[0].height_px, records[1].top_px);
    }
    assert_eq!(
        window
            .demand_facts
            .iter()
            .filter(|fact| matches!(fact.kind, DemandFactKind::MeasuredRecord { .. }))
            .count(),
        window.records.len()
    );
}

fn qualify_large_shrink(budget: usize) {
    let snapshot = snapshot(500);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 0, -10_000.0),
        request(640.0, 320.0, 0.0),
    );
    let mut frames = 0;
    let window = loop {
        frames += 1;
        let mut observations = 0;
        let result = preparation.advance(budget, |_| {
            observations += 1;
            36.0
        });
        assert_eq!(preparation.work_last_step(), observations);
        assert!(observations <= budget);
        assert!(preparation.retained_measurement_count() <= 37);
        if let Some(window) = result {
            break window;
        }
        assert!(frames < 400);
    };
    assert!(frames > 1);
    assert_eq!(window.visible_range, 277..296);
    assert_eq!(window.overscan_range, 268..305);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, -28.0);
    assert_eq!(window.manual_scroll_total_px, 0.0);
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn shrink_traversal_obeys_single_observation_budget() {
    qualify_large_shrink(1);
}

#[test]
fn shrink_traversal_obeys_frame_budget_without_retaining_crossed_rows() {
    qualify_large_shrink(32);
}

#[test]
fn fractional_manual_displacement_is_applied_once_across_preparation_steps() {
    let snapshot = snapshot(100);
    let mut saved = state(&snapshot, 20, -0.25);
    saved.manual_scroll_total_px = 10.5;
    let mut preparation =
        MeasuredFramePreparation::new(snapshot, saved, request(80.0, 30.0, 95.75));
    let window = complete(&mut preparation, 1, |_| 12.5);
    assert_eq!(window.anchor.as_ref().unwrap().index, 27);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, -8.5);
    assert_eq!(window.manual_delta_px, 95.75);
    assert_eq!(window.manual_scroll_total_px, 106.25);
    assert_actual_geometry(&window, |_| 12.5);
}

#[test]
fn natural_tail_geometry_is_ready_on_first_publication() {
    let snapshot = snapshot(30);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot,
        fresh(RealizedFrameScrollMode::LiveTailFollowing),
        request(100.0, 30.0, 0.0),
    );
    let window = complete(&mut preparation, 1, |record| {
        if index(record) == 29 { 153.75 } else { 11.25 }
    });
    let tail = window.records.last().unwrap();
    assert_eq!(tail.index, 29);
    assert_eq!(tail.top_px, -53.75);
    assert_eq!(tail.top_px + tail.height_px, 100.0);
    assert_eq!(window.visible_range, 29..30);
    assert_actual_geometry(&window, |index| if index == 29 { 153.75 } else { 11.25 });
}

#[test]
fn short_tail_content_starts_at_leading_edge() {
    let snapshot = snapshot(3);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot,
        fresh(RealizedFrameScrollMode::LiveTailFollowing),
        request(200.0, 100.0, 0.0),
    );
    let window = complete(&mut preparation, 1, |_| 36.0);
    assert_eq!(window.records.first().unwrap().top_px, 0.0);
    assert_eq!(window.records.last().unwrap().top_px + 36.0, 108.0);
    assert_eq!(window.visible_range, 0..3);
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn zero_input_shrink_of_last_anchor_clamps_and_refills_measured_rows() {
    let snapshot = snapshot(21);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 20, -1_000.0),
        request(640.0, 320.0, 0.0),
    );
    let window = complete(&mut preparation, 1, |_| 36.0);
    assert_eq!(window.visible_range, 3..21);
    assert_eq!(window.records.last().unwrap().top_px + 36.0, 640.0);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, -8.0);
    assert_eq!(
        window.clamp.as_ref().unwrap().direction,
        TranscriptPageDirection::Forward
    );
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn trailing_clamp_qualifies_every_newly_revealed_row() {
    let snapshot = snapshot(10);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 4, -16.0),
        request(200.0, 100.0, 200.0),
    );
    let mut observed = Vec::new();
    let window = complete(&mut preparation, 1, |record| {
        observed.push(index(record));
        36.0
    });
    assert_eq!(window.visible_range, 4..10);
    assert_eq!(window.overscan_range, 1..10);
    assert_eq!(window.records.last().unwrap().top_px + 36.0, 200.0);
    for record in &window.records {
        assert!(observed.contains(&record.index));
    }
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn leading_edge_clamps_fractional_backward_input() {
    let snapshot = snapshot(20);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 2, -1.5),
        request(100.0, 30.0, -300.25),
    );
    let window = complete(&mut preparation, 1, |_| 36.5);
    assert_eq!(window.anchor.as_ref().unwrap().index, 0);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, 0.0);
    assert_eq!(
        window.clamp.as_ref().unwrap().direction,
        TranscriptPageDirection::Backward
    );
    assert_actual_geometry(&window, |_| 36.5);
}

#[test]
fn detached_initial_position_uses_selected_snapshot_range() {
    let mut snapshot = snapshot(100);
    Arc::make_mut(&mut snapshot).visible_range = Some(40..41);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot,
        fresh(RealizedFrameScrollMode::DetachedManual),
        request(100.0, 30.0, 0.0),
    );
    let window = complete(&mut preparation, 1, |_| 36.0);
    assert_eq!(window.anchor.as_ref().unwrap().index, 40);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, 0.0);
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn excessive_budget_is_capped_and_ready_capability_is_consumed_once() {
    let snapshot = snapshot(500);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 0, -10_000.0),
        request(640.0, 320.0, 0.0),
    );
    assert!(preparation.advance(usize::MAX, |_| 36.0).is_none());
    assert_eq!(preparation.work_last_step(), 32);
    let window = complete(&mut preparation, 32, |_| 36.0);
    assert_actual_geometry(&window, |_| 36.0);
    assert!(
        preparation
            .advance(32, |_| panic!("consumed preparation measured again"))
            .is_none()
    );
    assert_eq!(preparation.retained_measurement_count(), 0);
}

#[test]
fn accepted_frame_installs_only_final_anchor_and_manual_total() {
    let snapshot = snapshot(50);
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 4, -2.25),
        request(100.0, 30.0, 78.5),
    );
    let window = complete(&mut preparation, 1, |_| 36.0);
    let mut controller = RealizedFrameScrollController::new();
    controller.accept_measured_frame(&snapshot, &window, RealizedFrameScrollMode::DetachedManual);
    let accepted = controller.state_snapshot();
    assert_eq!(accepted.anchor, window.anchor);
    assert_eq!(accepted.manual_scroll_total_px, 78.5);
    assert_eq!(accepted.presentation_revision, Some(7));
    assert_eq!(
        accepted.scroll_mode,
        RealizedFrameScrollMode::DetachedManual
    );
}

#[test]
fn empty_preparation_has_no_measurements_and_emits_coherent_ranges() {
    let mut preparation = MeasuredFramePreparation::new(
        snapshot(0),
        fresh(RealizedFrameScrollMode::DetachedManual),
        request(100.0, 30.0, 2.25),
    );
    let window = preparation
        .advance(0, |_| panic!("empty record measured"))
        .unwrap();
    assert_eq!(window.visible_range, 0..0);
    assert_eq!(window.overscan_range, 0..0);
    assert!(window.anchor.is_none());
    assert_eq!(window.manual_scroll_total_px, 2.25);
    assert_eq!(preparation.work_last_step(), 0);
}

#[test]
fn subpixel_natural_heights_are_not_replaced_by_default_extents() {
    let mut preparation = MeasuredFramePreparation::new(
        snapshot(8),
        fresh(RealizedFrameScrollMode::DetachedManual),
        request(1.0, 0.5, 0.125),
    );
    let window = complete(&mut preparation, 1, |_| 0.25);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, -0.125);
    assert_actual_geometry(&window, |_| 0.25);
}

#[test]
fn invalid_observation_does_not_publish_guessed_geometry() {
    let mut preparation = MeasuredFramePreparation::new(
        snapshot(3),
        fresh(RealizedFrameScrollMode::DetachedManual),
        request(100.0, 30.0, 0.0),
    );
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(preparation.advance(32, |_| invalid).is_none());
        assert_eq!(preparation.work_last_step(), 1);
        assert_eq!(preparation.retained_measurement_count(), 0);
    }
    let window = complete(&mut preparation, 1, |_| 36.0);
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn initial_tail_manual_input_uses_measured_tail_as_its_starting_position() {
    let mut preparation = MeasuredFramePreparation::new(
        snapshot(100),
        fresh(RealizedFrameScrollMode::LiveTailFollowing),
        request(100.0, 30.0, -25.25),
    );
    let window = complete(&mut preparation, 1, |_| 36.0);
    let tail = window.records.last().unwrap();
    assert_eq!(tail.index, 99);
    assert_eq!(tail.top_px + tail.height_px, 125.25);
    assert_eq!(window.manual_scroll_total_px, -25.25);
    assert_actual_geometry(&window, |_| 36.0);
}

#[test]
fn mixed_heights_preserve_exact_positions_across_cursor_crossings() {
    let snapshot = snapshot(100);
    let heights = [7.25, 83.5, 19.75, 4.5];
    let mut preparation = MeasuredFramePreparation::new(
        snapshot.clone(),
        state(&snapshot, 12, -1.25),
        request(120.0, 60.0, 122.125),
    );
    let window = complete(&mut preparation, 1, |record| heights[index(record) % 4]);
    assert_eq!(window.anchor.as_ref().unwrap().index, 17);
    assert_eq!(window.anchor.as_ref().unwrap().viewport_y_px, -1.125);
    assert_actual_geometry(&window, |index| heights[index % 4]);
}
