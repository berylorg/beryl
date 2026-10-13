use std::{collections::BTreeMap, sync::Arc};

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::*;

fn snapshot(count: usize) -> Arc<ResidentTranscriptSnapshot> {
    Arc::new(ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision: 7,
        state: ResidentTranscriptSnapshotState::ProviderBacked {
            label: "retention".into(),
        },
        records: (0..count)
            .map(|index| ResidentPresentationRecord {
                id: ResidentPresentationRecordId(format!("record-{index}")),
                kind: ResidentPresentationRecordKind::TextChunk {
                    plain_text: None,
                    narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
                    text: "bounded layout".into(),
                },
                provenance: ResidentRecordProvenance {
                    source: ResidentRecordSource::LocalUi,
                    projection_id: None,
                    projection_revision: None,
                    presentation_revision: 7,
                    copy_source_range: None,
                },
                estimated_bytes: 16,
            })
            .collect::<Vec<_>>()
            .into(),
        resources: ResidentResourceSnapshot::default(),
        realized_range: None,
        visible_range: None,
    })
}

fn preparation(
    snapshot: Arc<ResidentTranscriptSnapshot>,
    index: usize,
    delta: f32,
) -> MeasuredFramePreparation {
    let anchor = RealizedFrameAnchor {
        record_id: snapshot.records[index].id.clone(),
        index,
        viewport_y_px: 0.0,
        position: None,
    };
    MeasuredFramePreparation::new(
        snapshot,
        RealizedFrameScrollStateSnapshot {
            anchor: Some(anchor),
            scroll_mode: RealizedFrameScrollMode::DetachedManual,
            presentation_revision: Some(7),
            manual_scroll_total_px: 0.0,
        },
        RealizedFrameRequest {
            viewport_height_px: 60.0,
            overscan_height_px: 20.0,
            default_record_height_px: 9000.0,
            manual_delta_px: delta,
            observed_presentation_revision: Some(7),
        },
    )
}

#[test]
fn large_displacement_releases_crossed_candidate_facts_before_the_next_observation() {
    for (start, delta) in [(0, 4000.0), (499, -4000.0)] {
        let mut preparation = preparation(snapshot(500), start, delta);
        let mut reservations = BTreeMap::<String, usize>::new();
        let mut released = 0;
        let mut observations = 0;
        let mut complete = false;
        for _ in 0..40 {
            let ready =
                preparation.advance_with_retained_observations(32, |record, _, retained| {
                    reservations.retain(|id, _| {
                        let keep =
                            id == &record.id.0 || retained.iter().any(|row| &row.record_id.0 == id);
                        released += usize::from(!keep);
                        keep
                    });
                    reservations.insert(record.id.0.clone(), 1024 * 1024);
                    assert!(reservations.values().sum::<usize>() < 8 * 1024 * 1024);
                    observations += 1;
                    (Some(20.0), 1)
                });
            reservations.retain(|id, _| {
                let id = ResidentPresentationRecordId(id.clone());
                let keep = ready.as_ref().map_or_else(
                    || preparation.retains_record(&id),
                    |frame| frame.records.iter().any(|row| row.record_id == id),
                );
                released += usize::from(!keep);
                keep
            });
            assert!(preparation.work_last_step() <= 32);
            if let Some(frame) = ready {
                assert_eq!(reservations.len(), frame.records.len());
                assert!(observations >= 200);
                assert!(released >= 199);
                complete = true;
                break;
            }
        }
        assert!(complete);
    }
}

#[test]
fn partial_current_record_survives_pruning_without_retaining_crossed_records() {
    let snapshot = snapshot(10);
    let mut preparation = preparation(snapshot.clone(), 0, 40.0);
    let mut attempts = BTreeMap::<String, usize>::new();
    let mut candidate = None;
    for _ in 0..8 {
        assert!(
            preparation
                .advance_with_retained_observations(1, |record, _, retained| {
                    assert!(retained.is_empty());
                    let attempt = attempts.entry(record.id.0.clone()).or_default();
                    *attempt += 1;
                    candidate = Some(record.id.clone());
                    ((*attempt == 3).then_some(20.0), 1)
                })
                .is_none()
        );
        if candidate
            .as_ref()
            .is_some_and(|id| !preparation.retains_record(id))
        {
            candidate = None;
        }
        if attempts.get("record-0").copied().unwrap_or(0) < 3 {
            assert_eq!(candidate.as_ref(), Some(&snapshot.records[0].id));
        }
        assert!(!preparation.retains_record(&snapshot.records[9].id));
    }
    assert_eq!(candidate.as_ref(), Some(&snapshot.records[2].id));
    assert!(!preparation.retains_record(&snapshot.records[0].id));
    assert!(!preparation.retains_record(&snapshot.records[1].id));
}
