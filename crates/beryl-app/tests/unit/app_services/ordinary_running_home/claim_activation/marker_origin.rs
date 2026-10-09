use super::*;
use crate::main_window::MainWindowComposerSelectionIdentity;
use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_state::{
    AssetMetadataRecord, AssetOwner, AssetOwnerHeadRecord, AssetReferenceEntryRecord,
};
use syndic_storage::{
    DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, SyndicStorage,
};

pub(super) struct MarkerEvidence {
    selection: MainWindowComposerSelectionIdentity,
    marker: DraftPieceMarkerAtV1,
    metadata: AssetMetadataRecord,
    original_owner: Option<AssetOwnerHeadRecord>,
    settled_owner: Option<(AssetOwnerHeadRecord, AssetReferenceEntryRecord)>,
}

pub(super) async fn seed(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
) -> MarkerEvidence {
    let resident = composer(window, cx).await;
    let prior = cx
        .update(|app| resident.read(app).selection_identity())
        .unwrap();
    assert_eq!(prior.binding().root().summary().marker_count(), 0);
    window
        .update(cx, |_, window, app| {
            let input = resident.read(app).gpui_input();
            input.update(app, |input, cx| input.focus(window));
            window.dispatch_action(Box::new(gpui_text_input::MoveToEnd), app);
        })
        .unwrap();
    let caret_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let insertion_ready = cx
            .update(|app| {
                let composer = resident.read(app);
                assert_eq!(composer.selection_identity(), prior);
                let input = composer.gpui_input();
                let input = input.read(app);
                !composer.test_has_active_flight()
                    && !composer.paste_pending()
                    && input.is_quiescent()
                    && input.is_surface_current_and_interactive()
                    && input.surface().is_some_and(|surface| {
                        let selection = surface.source_selection();
                        surface.binding() == prior.binding().range_binding()
                            && selection.anchor == selection.head
                            && selection.head.byte_offset.get()
                                == prior.binding().logical_extent().logical_utf8_bytes()
                    })
            })
            .unwrap();
        if insertion_ready {
            break;
        }
        assert!(
            Instant::now() < caret_deadline,
            "authentic marker insertion caret did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    let item = gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        encoded_png(),
    ));
    window
        .update(cx, |_, window, app| {
            resident.update(app, |composer, cx| {
                assert!(!composer.test_has_active_flight());
                assert!(!composer.paste_pending());
                composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
                    Ok(gpui::CheckedClipboardSnapshot {
                        item: item.clone(),
                        sequence: 1,
                    })
                }));
                composer.test_begin_captured_paste(window, cx);
                assert!(
                    composer.paste_pending(),
                    "authentic marker paste was not admitted"
                );
            });
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let selection = loop {
        let ready = cx
            .update(|app| {
                let composer = resident.read(app);
                assert!(
                    composer.last_error().is_none(),
                    "marker paste failed: {:?}",
                    composer.last_error()
                );
                let selected = composer.selection_identity();
                let input = composer.gpui_input();
                let input = input.read(app);
                (selected.binding().root().summary().marker_count() == 1
                    && !composer.paste_pending()
                    && !composer.test_has_active_flight()
                    && input.is_quiescent()
                    && input.is_surface_current_and_interactive()
                    && input.surface().is_some_and(|surface| {
                        surface.binding() == selected.binding().range_binding()
                    }))
                .then_some(selected)
            })
            .unwrap();
        if let Some(selected) = ready {
            break selected;
        }
        if Instant::now() >= deadline {
            cx.update(|app| {
                let composer = resident.read(app);
                eprintln!("authentic marker paste feedback: clipboard={:?} mutation={:?}", composer.clipboard_feedback(), composer.mutation_feedback());
                eprintln!("authentic marker paste flight: {}", composer.test_failed_editor_drain_diagnostics(app));
                let selected = composer.selection_identity();
                let input = composer.gpui_input();
                let input = input.read(app);
                let diagnostics = input.realization_diagnostics();
                eprintln!(
                    "authentic marker geometry: surface={:?} estimate={:?} selected_bytes={} range_binding={:?}",
                    input.surface().map(|surface| (surface.geometry_key(), surface.caret(), surface.selection(), surface.content_height(), surface.quality(), surface.viewport())),
                    input.geometry_estimate(),
                    selected.binding().logical_extent().logical_utf8_bytes(),
                    selected.binding().range_binding(),
                );
                eprintln!("authentic marker realization: {diagnostics:?}");
                eprintln!(
                    "authentic marker geometry delivery: rejected={:?} rejection_count={} rejection_stage={:?} superseded_geometry_objects={}",
                    diagnostics.last_response_rejection,
                    diagnostics.response_rejection_count,
                    diagnostics.last_response_rejection_stage,
                    diagnostics.superseded_geometry_object_responses_settled,
                );
                eprintln!(
                    "authentic marker paste state: markers={} paste_pending={} active_flight={} input_quiescent={} interactive={} surface_matches={} candidate={}",
                    selected.binding().root().summary().marker_count(),
                    composer.paste_pending(),
                    composer.test_has_active_flight(),
                    input.is_quiescent(),
                    input.is_surface_current_and_interactive(),
                    input.surface().is_some_and(|surface| surface.binding() == selected.binding().range_binding()),
                    selected.binding().candidate().candidate_generation(),
                );
            }).unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "authentic image marker paste did not settle"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    };
    assert_eq!(selection.claim(), prior.claim());
    assert_eq!(
        selection.binding().logical_extent().logical_utf8_bytes(),
        prior.binding().logical_extent().logical_utf8_bytes()
    );
    assert!(
        selection.binding().candidate().candidate_generation()
            > prior.binding().candidate().candidate_generation()
    );
    let (home, state, storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (
            graph.home().service_reference(),
            graph.state().clone(),
            graph.syndic().clone(),
        )
    };
    cx.background_executor()
        .spawn(async move {
            let result = storage
                .candidate_draft_piece_marker_demand(
                    &home,
                    selection.binding().candidate(),
                    demand(selection),
                )
                .unwrap();
            assert!(result.value().requested_side_complete());
            assert_eq!(result.value().markers().len(), 1);
            let marker = result.value().markers()[0];
            let assets = state.assets();
            let metadata = assets
                .metadata(&home, marker.marker().asset_id())
                .unwrap()
                .unwrap();
            assert_eq!(metadata.media_type().as_str(), "image/png");
            let dimensions = metadata.dimensions().unwrap();
            assert_eq!(dimensions.width().get(), 2);
            assert_eq!(dimensions.height().get(), 3);
            let current = storage
                .current_draft(
                    &home,
                    selection.claim().thread_id(),
                    SyndicPointReadLimit::new(1024 * 1024).unwrap(),
                )
                .unwrap()
                .unwrap();
            assert_eq!(current.draft().piece_root().summary().marker_count(), 0);
            let original_owner = assets
                .owner_head(
                    &home,
                    AssetOwner::CurrentDraft(selection.binding().candidate().draft_id()),
                )
                .unwrap();
            MarkerEvidence {
                selection,
                marker,
                metadata,
                original_owner,
                settled_owner: None,
            }
        })
        .await
}

impl MarkerEvidence {
    pub(super) async fn verify(
        &mut self,
        owner: &Rc<RefCell<RunningProcessOwner>>,
        cx: &mut gpui::AsyncApp,
    ) {
        let (home, state, storage) = {
            let retained = owner.borrow();
            let graph = retained.test_services().graph().unwrap();
            (
                graph.home().service_reference(),
                graph.state().clone(),
                graph.syndic().clone(),
            )
        };
        let selection = self.selection;
        let marker = self.marker;
        let metadata = self.metadata.clone();
        let settled = cx
            .background_executor()
            .spawn(async move {
                verify_source(&home, &storage, selection, marker);
                let assets = state.assets();
                assert_eq!(
                    assets
                        .metadata(&home, marker.marker().asset_id())
                        .unwrap()
                        .unwrap(),
                    metadata
                );
                let head = assets
                    .owner_head(
                        &home,
                        AssetOwner::CurrentDraft(selection.binding().candidate().draft_id()),
                    )
                    .unwrap()
                    .unwrap();
                assets
                    .sealed_reference_set_manifest(&home, head.set())
                    .unwrap();
                let entries = assets
                    .reference_set_entries(
                        &home,
                        head.set(),
                        None,
                        CursorReadLimits::new(2, 65_536).unwrap(),
                    )
                    .unwrap();
                assert!(!entries.has_more());
                assert_eq!(entries.records().len(), 1);
                let entry = entries.records()[0].clone();
                assert_eq!(entry.marker_id(), marker.marker().marker_id());
                assert_eq!(entry.label(), marker.marker().label());
                assert_eq!(entry.asset_id(), marker.marker().asset_id());
                (head, entry)
            })
            .await;
        assert_ne!(self.original_owner.as_ref(), Some(&settled.0));
        if let Some(original) = &self.settled_owner {
            assert_eq!(original, &settled);
        } else {
            self.settled_owner = Some(settled);
        }
    }
}

fn verify_source(
    home: &HomeStore,
    storage: &SyndicStorage,
    selection: MainWindowComposerSelectionIdentity,
    marker: DraftPieceMarkerAtV1,
) {
    let current = storage
        .current_draft(
            home,
            selection.claim().thread_id(),
            SyndicPointReadLimit::new(1024 * 1024).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().piece_root(), selection.binding().root());
    assert_eq!(
        current.draft().piece_root().marker_commitment(),
        selection.binding().root().marker_commitment()
    );
    let result = storage
        .draft_piece_marker_demand(home, current.draft().piece_root(), demand(selection))
        .unwrap();
    assert!(result.requested_side_complete());
    assert_eq!(result.markers(), &[marker]);
}

fn demand(selection: MainWindowComposerSelectionIdentity) -> DraftPieceMarkerDemandV1 {
    DraftPieceMarkerDemandV1::new(
        DraftPieceMarkerScopeV1::InclusiveRange {
            start: 0,
            end: selection.binding().logical_extent().logical_utf8_bytes(),
        },
        DraftPieceMarkerDirectionV1::Forward,
        None,
        2,
        65_536,
    )
}

fn encoded_png() -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut value = u32::MAX;
        for byte in bytes {
            value ^= u32::from(*byte);
            for _ in 0..8 {
                value = (value >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(value & 1));
            }
        }
        !value
    }
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x03\x08\x06\0\0\0".to_vec();
    bytes.extend_from_slice(&crc(&bytes[12..29]).to_be_bytes());
    let mut data = vec![0x78, 0x01, 0x01, 27, 0, 0xe4, 0xff];
    data.extend_from_slice(&[0; 27]);
    data.extend_from_slice(&[0, 27, 0, 1]);
    for (kind, data) in [(b"IDAT", data.as_slice()), (b"IEND", &[][..])] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = bytes.len();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&crc(&bytes[start..]).to_be_bytes());
    }
    bytes
}

#[test]
fn native_original_ordinary_marker_publication_survives_completed_disposal_recovery() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, window_id, claim) = scenario::recover(
                    owner,
                    faults,
                    Cut::DisposalCommitted,
                    control::Control::Normal,
                    SaveExpectation::DirtyMarker,
                    cx,
                )
                .await;
                *qualified_selection.borrow_mut() = Some((window_id, claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
