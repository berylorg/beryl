use super::*;
use crate::syndic_transcript::{
    ResidentContextMenuContentKind, TranscriptGeneratedImage, TranscriptGeneratedImageLifecycle,
};
use beryl_model::{
    AssetId, ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicResourceId,
    SyndicThreadId, SyndicTurnId,
};

fn generated_seed(
    count: u64,
    revision: u64,
    lifecycle: TranscriptGeneratedImageLifecycle,
) -> PreparedTranscriptActivation {
    let mut activation = seed(count);
    let thread = SyndicThreadId::from_bytes([31; 16]).to_string();
    let view = TranscriptViewId(thread);
    activation.view_id = Some(view.clone());
    let Some(TranscriptProviderResponseKind::ViewPage(page)) =
        activation.view_page_response.as_mut()
    else {
        unreachable!()
    };
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    page.view_id = view.clone();
    page.revision = ProviderRevision(revision);
    projections.view_id = view.clone();
    projections.revision = ProviderRevision(revision);
    for (index, (entry, projection)) in page
        .records
        .iter_mut()
        .zip(projections.records.iter_mut())
        .enumerate()
    {
        let resource = SyndicResourceId::from_bytes([index as u8 + 1; 16]);
        let projection_id =
            ProjectionRecordId(SyndicProjectionId::from_bytes([index as u8 + 65; 16]).to_string());
        let source = SyndicSourceProvenance {
            transcript_entry: Some(syndic_storage::CurrentTranscriptEntryProof::new(
                syndic_storage::TranscriptGeneration::new(revision).unwrap(),
                syndic_storage::TranscriptPosition::new(index as u64 + 1).unwrap(),
            )),
            view_id: view.clone(),
            position: Some(TranscriptViewPosition(index as u64 + 1)),
            turn_id: Some(crate::syndic_transcript::SyndicTurnId(
                SyndicTurnId::from_bytes([32; 16]).to_string(),
            )),
            item_id: Some(crate::syndic_transcript::SyndicItemId(
                SyndicItemId::from_bytes([index as u8 + 1; 16]).to_string(),
            )),
            block_id: None,
            projection_id: Some(projection_id.clone()),
            resource_id: Some(ResourceId(resource.to_string())),
            source_range: None,
            resource_range: None,
            copy_source_range: None,
        };
        entry.id = TranscriptViewRecordId(format!("generated-{index}"));
        entry.position = TranscriptViewPosition(index as u64 + 1);
        entry.projection_id = projection_id.clone();
        entry.narrative_kind = TranscriptNarrativeKind::AssistantGeneratedMedia;
        entry.provenance = source.clone();
        projection.id = projection_id;
        projection.revision = ProviderRevision(revision);
        projection.kind = ProjectionRecordKind::ImageMarker;
        projection.provenance = source;
        projection.payload = ProjectionPayload::GeneratedImage(TranscriptGeneratedImage {
            resource_id: resource,
            resource_revision: ProjectionRevision::new(revision).unwrap(),
            source_fingerprint: [revision as u8; 32],
            lifecycle,
        });
    }
    activation
}

#[gpui::test]
fn generated_images_realize_bounded_markers_without_ranges_or_byte_demands(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(
        cx,
        generated_seed(64, 1, TranscriptGeneratedImageLifecycle::Pending),
    );
    draw_frames(cx, window, 8);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.read_with(cx, |panel, _| {
        let snapshot = panel.snapshot();
        assert_eq!(snapshot.records.len(), 64);
        assert!(!panel.inline_parts_geometry.is_empty());
        assert!(panel.inline_parts_geometry.len() < 20);
        assert_eq!(
            panel.inline_parts_geometry.len(),
            panel.last_frame_window.as_ref().unwrap().records.len()
        );
        for record in snapshot.records.iter() {
            assert_eq!(record.image_label().as_deref(), Some("Generated image"));
            let crate::syndic_transcript::ResidentRecordSource::Syndic(source) =
                &record.provenance.source
            else {
                panic!()
            };
            assert!(source.source_range.is_none() && source.resource_range.is_none());
            assert!(source.copy_source_range.is_none());
        }
        assert!(panel.resource_demand().is_none());
        assert!(snapshot.resources.metadata.is_empty());
        assert!(snapshot.resources.slices.is_empty());
    });
}

#[gpui::test]
fn generated_lifecycle_preserves_realized_focus_inspection_and_disabled_menu(
    cx: &mut TestAppContext,
) {
    let pending = generated_seed(1, 1, TranscriptGeneratedImageLifecycle::Pending);
    let window = mounted_panel(cx, pending);
    draw_frames(cx, window, 6);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let (origin, focus, menu_focus) = window
        .update(cx, |_, native, app| {
            panel.update(app, |panel, cx| {
                panel.set_image_marker_focus_fallback(cx.focus_handle());
                let snapshot = panel.host.shared_snapshot();
                let id = snapshot.records[0].id.clone();
                let focus = panel.image_marker_focus_handle(&id).unwrap();
                native.focus(&focus);
                let bounds = panel.image_marker_bounds(&id, native).unwrap();
                let command = resident_context_menu_command_for_realized_record_id(
                    &snapshot,
                    panel.last_frame_window.as_ref().unwrap(),
                    &id,
                )
                .unwrap();
                let record = crate::syndic_transcript::context_menu::resident_context_menu_record(
                    &snapshot.records[0],
                    command.record,
                )
                .unwrap();
                let target = ResidentTranscriptContextMenuTarget::new(
                    snapshot.presentation_revision,
                    record,
                );
                assert!(target.record.image_demand().is_none());
                panel.image_inspection_pending = Some(1);
                let origin = ImageMarkerInspectionRequested {
                    request: 1,
                    target,
                    activation: snapshot.activation_revision,
                    presentation: snapshot.presentation_revision,
                    bounds,
                    focus: focus.clone(),
                };
                panel.open_record_menu(&id, bounds.center(), native, cx);
                let rows = panel.transcript_menu_rows_for_test();
                assert_eq!(rows.len(), 2);
                assert!(rows.iter().all(|(_, enabled, reason)| !enabled
                    && reason.as_deref() == Some("The generated image is still being prepared.")));
                (origin, focus, native.focused(cx))
            })
        })
        .unwrap();
    let unavailable = generated_seed(
        1,
        2,
        TranscriptGeneratedImageLifecycle::Unavailable(
            beryl_model::GeneratedMediaUnavailableReason::LostSourceAuthority,
        ),
    );
    panel.update(cx, |panel, _| panel.stage_activation(&unavailable, None));
    draw_frames(cx, window, 6);
    window
        .update(cx, |_, native, app| {
            panel.update(app, |panel, cx| {
                let refreshed = panel
                    .refreshed_image_marker_inspection(&origin, native)
                    .expect("same resource occurrence remains inspectable");
                assert_eq!(
                    refreshed.target.record.record_id,
                    origin.target.record.record_id
                );
                assert_eq!(refreshed.bounds, origin.bounds);
                assert_eq!(refreshed.focus, focus);
                assert_eq!(native.focused(cx), menu_focus);
                assert_eq!(
                    panel.transcript_menu_focused_id_for_test(),
                    Some("transcript-image-copy")
                );
                assert!(matches!(
                    refreshed.target.record.content_kind,
                    ResidentContextMenuContentKind::GeneratedImage(TranscriptGeneratedImage {
                        lifecycle: TranscriptGeneratedImageLifecycle::Unavailable(_),
                        ..
                    })
                ));
                assert!(refreshed.target.record.image_demand().is_none());
                let colors =
                    crate::main_window::image_inspection::original_commands::composer_colors();
                assert!(panel.render_quote_menu(colors, native, cx).is_some());
                let rows = panel.transcript_menu_rows_for_test();
                assert!(rows.iter().all(|(_, enabled, reason)| !enabled
                    && reason.as_deref() == Some("The generated image is locally unavailable.")));
            })
        })
        .unwrap();
    let replacement = seed(1);
    panel.update(cx, |panel, _| panel.stage_activation(&replacement, None));
    draw_frames(cx, window, 6);
    window
        .update(cx, |_, native, app| {
            panel
                .read(app)
                .image_marker_inspection_current(&origin, native)
        })
        .map(|current| assert!(!current))
        .unwrap();
}

#[gpui::test]
fn pending_generated_inspection_elects_fresh_admitted_authority_without_changing_anchor(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(
        cx,
        generated_seed(1, 1, TranscriptGeneratedImageLifecycle::Pending),
    );
    draw_frames(cx, window, 6);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let origin = window
        .update(cx, |_, native, app| {
            panel.update(app, |panel, cx| {
                panel.set_image_marker_focus_fallback(cx.focus_handle());
                let snapshot = panel.host.shared_snapshot();
                let row = &snapshot.records[0];
                let focus = panel.image_marker_focus_handle(&row.id).unwrap();
                native.focus(&focus);
                let bounds = panel.image_marker_bounds(&row.id, native).unwrap();
                let command = resident_context_menu_command_for_realized_record_id(
                    &snapshot,
                    panel.last_frame_window.as_ref().unwrap(),
                    &row.id,
                )
                .unwrap();
                let record = crate::syndic_transcript::context_menu::resident_context_menu_record(
                    row,
                    command.record,
                )
                .unwrap();
                panel.image_inspection_pending = Some(2);
                ImageMarkerInspectionRequested {
                    request: 2,
                    target: ResidentTranscriptContextMenuTarget::new(
                        snapshot.presentation_revision,
                        record,
                    ),
                    activation: snapshot.activation_revision,
                    presentation: snapshot.presentation_revision,
                    bounds,
                    focus,
                }
            })
        })
        .unwrap();
    let asset = AssetId::sha256_v1([23; 32], std::num::NonZeroU64::new(42).unwrap());
    panel.update(cx, |panel, _| {
        panel.stage_activation(
            &generated_seed(1, 2, TranscriptGeneratedImageLifecycle::Admitted(asset)),
            None,
        )
    });
    draw_frames(cx, window, 6);
    window.update(cx, |_, native, app| {
        let panel = panel.read(app);
        let refreshed = panel.refreshed_image_marker_inspection(&origin, native).unwrap();
        assert_eq!(refreshed.target.record.record_id, origin.target.record.record_id);
        assert_eq!(refreshed.focus, origin.focus);
        assert!(origin.focus.is_focused(native));
        assert_eq!(refreshed.bounds, origin.bounds);
        assert!(matches!(refreshed.target.record.image_demand(), Some(crate::image_processing::ImageSourceDemand::TranscriptGeneratedResource {
            image: TranscriptGeneratedImage { resource_revision, source_fingerprint, lifecycle: TranscriptGeneratedImageLifecycle::Admitted(actual), .. }, ..
        }) if actual == asset && resource_revision.get() == 2 && source_fingerprint == [2; 32]));
    }).unwrap();
}

#[test]
fn generated_admitted_demand_keeps_resource_authority_separate_from_marker_storage() {
    let asset = AssetId::sha256_v1([22; 32], std::num::NonZeroU64::new(42).unwrap());
    let prepared = generated_seed(1, 2, TranscriptGeneratedImageLifecycle::Admitted(asset));
    let mut core = crate::syndic_transcript::ResidentTranscriptCore::empty();
    core.apply_prepared_activation(prepared, TranscriptActivationSource::RunningThread);
    let snapshot = core.shared_presentation_snapshot();
    let record = &snapshot.records[0];
    let target = crate::syndic_transcript::context_menu::resident_context_menu_record(
        record,
        crate::syndic_transcript::ResidentSelectionRecordGeometry::new(
            record.id.clone(),
            0.0,
            36.0,
        ),
    )
    .unwrap();
    assert!(
        matches!(target.image_demand(), Some(crate::image_processing::ImageSourceDemand::TranscriptGeneratedResource { image, .. })
        if image.lifecycle == TranscriptGeneratedImageLifecycle::Admitted(asset))
    );
}
