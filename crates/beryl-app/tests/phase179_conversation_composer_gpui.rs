#![cfg(feature = "test-faults")]

#[path = "phase166_syndic_composer_history/support.rs"]
mod composer_support;
#[path = "phase172_syndic_composer_publication/support.rs"]
mod publication_support;
#[path = "phase177_main_window_composer_slot/support.rs"]
mod support;

use std::{
    num::NonZeroU64,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostError,
        ComposerHostImageMarkerMetadata, ComposerHostInitialDemand, ComposerHostMutationOutcome,
        ComposerHostRequestId, ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{
        ComposerImagePresentationState, ComposerImagePreviewCommandState,
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowConversationComposer, MainWindowConversationComposerConfig,
        MainWindowConversationComposerService,
    },
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{AssetId, ImageLabelOrdinal};
use gpui::{
    EntityInputHandler, Modifiers, SharedString, StreamingLayoutBinding, StreamingLayoutLimits,
    StreamingLayoutPosition, TextRun, black, font, point, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    BindingId, ByteOffset, ClipboardLimits, ClipboardWriteOutcome, ExactGeometryLimits,
    InlineObjectGap, InlineObjectId, InlineObjectNeighbor, InlineObjectOrder, LogicalExtent,
    MutationBeginRequest, MutationCommitRequest, MutationCursor, MutationFinishInput,
    MutationIdentity, MutationKind, MutationLane, MutationLimits, MutationPage, MutationPageItem,
    MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, ObjectChange, ObjectResidencyLimits, OperationId,
    PresentationGeneration, RangeSettlementCoordinator, RangeSourceSelection, RangeTextInputConfig,
    RangeTextInputLimits, ResidencyLimits, SegmentationLimits, SourcePosition, SourceRange,
    SourceRevision, StreamingGeometryStyle, StreamingOversizePresentation, SuccessorObject,
    TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme,
    ensure_text_input_bindings,
};
use syndic_storage::{
    DraftCompositeSearchKeyV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, DraftPieceTextDemandV1,
};

use support::Fixture;

#[gpui::test]
fn production_owner_settles_committed_edit_and_failed_cut_through_widget(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase179-owner", 71);
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    let activation = ComposerHostActivationRequest::new(
        thread,
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([72; 16]),
        support::operation_id(73),
        NonZeroU64::new(1).unwrap(),
        None,
        vec![
            ComposerHostInitialDemand::Text {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 32,
            },
            ComposerHostInitialDemand::Markers {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::Range { start: 0, end: 0 },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    32,
                    65_536,
                ),
            },
        ]
        .into_boxed_slice(),
    );
    assert!(matches!(
        host.test_activate(&store, activation, &CommandCancellation::new(),)
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(store, slot));
    let writes = Arc::new(AtomicUsize::new(0));
    let observed_writes = writes.clone();
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 1024),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(move |_, _| {
                observed_writes.fetch_add(1, Ordering::SeqCst);
                ClipboardWriteOutcome::Failed
            }),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });

    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    input.read_with(cx, |input, _| assert!(input.is_quiescent()));
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "x", None, window, cx)
        })
    });
    drive_owner(cx, 24);
    let committed = service.selected_identity().unwrap();
    let owner_error =
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned));
    assert!(
        committed.binding().candidate().candidate_generation()
            > selection.binding().candidate().candidate_generation(),
        "owner error: {owner_error:?}"
    );
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let settled_error =
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned));
    input.read_with(cx, |input, _| {
        assert_eq!(
            input.surface().unwrap().binding(),
            committed.binding().range_binding(),
            "owner error: {settled_error:?}"
        );
    });
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));

    cx.simulate_keystrokes("ctrl-z");
    drive_owner(cx, 32);
    let undone = service.selected_identity().unwrap();
    assert_eq!(undone.binding().root(), selection.binding().root());
    assert_eq!(
        undone.binding().logical_extent(),
        selection.binding().logical_extent()
    );
    assert!(undone.binding().range_history_frontier().redo_available);
    input.read_with(cx, |input, _| {
        assert!(input.history_frontier().redo_available);
        assert!(input.surface().is_some());
    });
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));

    cx.simulate_keystrokes("ctrl-y");
    drive_owner(cx, 32);
    let redone = service.selected_identity().unwrap();
    assert_eq!(redone.binding().root(), committed.binding().root());
    assert!(redone.binding().range_history_frontier().undo_available);
    assert!(!redone.binding().range_history_frontier().redo_available);
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));

    cx.simulate_keystrokes("ctrl-a");
    drive_owner(cx, 12);
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 24);
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(service.selected_identity(), Some(redone));
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
}

#[gpui::test]
fn composite_clipboard_orders_markers_enforces_cap_and_cuts_only_after_write(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase180-composite-clipboard", 101);
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([102; 16]),
                support::operation_id(103),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 104, 0, 0, "AB", 2, 1);
    let (binding, marker_before, _) =
        composer_support::insert_marker(&mut host, &store, binding, 105, true);
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        102,
        103,
        2,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(
                0x8001_0203_0405_0607_0809_0a0b_0c0d_0eff_u128.to_be_bytes(),
            ),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let binding = rebound;
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    assert_eq!(selection.binding(), binding);
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let writes = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed_writes = writes.clone();
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 1024),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(move |text, _| {
                observed_writes.lock().unwrap().push(text.to_owned());
                ClipboardWriteOutcome::Written
            }),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input
                .rebind(
                    selection.binding().range_binding(),
                    Some(RangeSourceSelection {
                        anchor: marker_before,
                        head: composer_support::position(1),
                    }),
                    window,
                    cx,
                )
                .unwrap()
        })
    });
    drive_owner(cx, 12);
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 64);
    let owner_error =
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned));
    assert_eq!(
        &*writes.lock().unwrap(),
        &["[Image A]A"],
        "owner error: {owner_error:?}"
    );
    assert_eq!(owner_error, None);
    let cut = service.selected_identity().unwrap();
    let input_state = input.read_with(cx, |input, _| {
        (
            input.is_quiescent(),
            input.surface().map(|surface| surface.selection()),
        )
    });
    assert_ne!(cut, selection, "input state: {input_state:?}");
    assert_eq!(cut.binding().logical_extent().logical_utf8_bytes(), 1);
    assert_eq!(cut.binding().root().summary().marker_count(), 0);
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));

    let fixture = Fixture::new("phase180-composite-cap", 111);
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([112; 16]),
                support::operation_id(113),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 114, 0, 0, "A", 1, 1);
    let (binding, marker_before, _) =
        composer_support::insert_marker(&mut host, &store, binding, 115, true);
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        112,
        113,
        1,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(
                0x8001_0203_0405_0607_0809_0a0b_0c0d_0eff_u128.to_be_bytes(),
            ),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let binding = rebound;
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    assert_eq!(selection.binding(), binding);
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let writes = Arc::new(AtomicUsize::new(0));
    let observed_writes = writes.clone();
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 5),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(move |_, _| {
                observed_writes.fetch_add(1, Ordering::SeqCst);
                ClipboardWriteOutcome::Written
            }),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input
                .rebind(
                    selection.binding().range_binding(),
                    Some(RangeSourceSelection {
                        anchor: marker_before,
                        head: composer_support::position(1),
                    }),
                    window,
                    cx,
                )
                .unwrap()
        })
    });
    drive_owner(cx, 12);
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 32);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(service.selected_identity(), Some(selection));
}

#[gpui::test]
fn marker_menu_and_preview_mount_and_dismiss_through_real_gpui_surfaces(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase181-marker-surfaces", 131);
    let marker_asset = publication_support::publish_image_asset(
        &fixture.store,
        fixture.assets(),
        b"phase181-marker-surface",
    );
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([132; 16]),
                support::operation_id(133),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 134, 0, 0, "AB", 2, 1);
    let binding = insert_marker_at_text_end(&mut host, &store, binding, 135, marker_asset);
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        132,
        133,
        2,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 1,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(0x1001_u128.to_be_bytes()),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 1024),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(|_, _| ClipboardWriteOutcome::Written),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let editor_focus = cx.update(|window, app| window.focused(app).unwrap());
    let marker = input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert!(
            !surface.realized_objects().is_empty(),
            "marker geometry missing: quiescent={}, pages={}, object_pages={}, objects={}, presentations={}",
            input.is_quiescent(),
            surface.pages().len(),
            surface.object_pages().len(),
            surface
                .object_pages()
                .iter()
                .map(|page| page.objects().len())
                .sum::<usize>(),
            surface
                .realized_presentations(surface.publication_key())
                .unwrap()
                .count(),
        );
        surface.realized_objects()[0]
    });
    let marker_click = marker.hit_bounds().origin + point(px(1.0), px(1.0));

    cx.simulate_click(marker_click, Modifiers::none());
    drive_owner(cx, 4);
    let anchor = composer.read_with(cx, |composer, _| composer.marker_menu().unwrap().anchor());
    assert_eq!(
        input.read_with(cx, |input, _| input.active_inline_object()),
        Some(anchor)
    );
    assert_ne!(
        cx.update(|window, app| window.focused(app).unwrap()),
        editor_focus
    );
    let root_bounds = cx.debug_bounds("conversation-composer-root").unwrap();
    let menu_bounds = cx
        .debug_bounds("conversation-composer-marker-menu")
        .unwrap();
    assert!(root_bounds.contains(&menu_bounds.origin));
    assert!(menu_bounds.right() <= root_bounds.right());
    assert!(menu_bounds.bottom() <= root_bounds.bottom());
    assert!(
        cx.debug_bounds("conversation-composer-marker-remove")
            .is_some()
    );

    let view = cx
        .debug_bounds("conversation-composer-marker-view")
        .unwrap();
    cx.simulate_click(view.center(), Modifiers::none());
    drive_owner(cx, 4);
    composer.read_with(cx, |composer, _| {
        assert!(composer.marker_menu().is_none());
        let preview = composer.image_preview().unwrap();
        assert_eq!(preview.state(), ComposerImagePresentationState::Pending);
        assert_eq!(
            preview.command_state(),
            ComposerImagePreviewCommandState::DisabledPending
        );
    });
    assert!(
        cx.debug_bounds("conversation-composer-image-preview")
            .is_some()
    );
    let copy = cx
        .debug_bounds("conversation-composer-image-preview-copy")
        .unwrap();
    let save = cx
        .debug_bounds("conversation-composer-image-preview-save")
        .unwrap();
    cx.simulate_click(copy.center(), Modifiers::none());
    cx.simulate_click(save.center(), Modifiers::none());
    drive_owner(cx, 2);
    composer.read_with(cx, |composer, _| {
        assert!(composer.image_preview().is_some())
    });

    cx.simulate_keystrokes("escape");
    drive_owner(cx, 4);
    composer.read_with(cx, |composer, _| {
        assert!(composer.image_preview().is_none())
    });
    assert_eq!(
        cx.update(|window, app| window.focused(app).unwrap()),
        editor_focus
    );
    assert_eq!(
        input.read_with(cx, |input, _| input.active_inline_object()),
        Some(anchor)
    );

    cx.simulate_click(marker_click, Modifiers::none());
    drive_owner(cx, 4);
    let menu_bounds = cx
        .debug_bounds("conversation-composer-marker-menu")
        .unwrap();
    let outside = point(
        root_bounds.right() - px(2.0),
        root_bounds.bottom() - px(2.0),
    );
    assert!(!menu_bounds.contains(&outside));
    cx.simulate_click(outside, Modifiers::none());
    drive_owner(cx, 4);
    composer.read_with(cx, |composer, _| assert!(composer.marker_menu().is_none()));

    cx.simulate_click(marker_click, Modifiers::none());
    drive_owner(cx, 4);
    cx.update(|window, app| {
        composer.update(app, |composer, cx| {
            composer
                .invoke_marker_view(ComposerImagePresentationState::LocalUnavailable, window, cx)
                .unwrap();
        })
    });
    drive_owner(cx, 4);
    composer.read_with(cx, |composer, _| {
        let preview = composer.image_preview().unwrap();
        assert_eq!(
            preview.command_state(),
            ComposerImagePreviewCommandState::DisabledUnavailable
        );
    });
    let close = cx
        .debug_bounds("conversation-composer-image-preview-close")
        .unwrap();
    cx.simulate_click(close.center(), Modifiers::none());
    drive_owner(cx, 4);
    composer.read_with(cx, |composer, _| {
        assert!(composer.image_preview().is_none());
        assert_eq!(composer.last_error(), None);
    });
    assert_eq!(
        cx.update(|window, app| window.focused(app).unwrap()),
        editor_focus
    );

    cx.simulate_click(marker_click, Modifiers::none());
    drive_owner(cx, 4);
    cx.update(|window, app| {
        composer.update(app, |composer, composer_cx| {
            composer
                .insert_authenticated_image_marker(
                    ComposerHostImageMarkerMetadata::new(
                        InlineObjectId::new(0x1001),
                        ImageLabelOrdinal::new(1).unwrap(),
                        marker_asset,
                    ),
                    InlineObjectOrder::new(2),
                    composer_cx,
                )
                .unwrap();
            assert!(composer.invoke_marker_remove(composer_cx).is_err());
            assert!(composer.marker_menu().is_some());
            assert!(
                composer
                    .dismiss_marker_menu(window, composer_cx)
                    .unwrap()
                    .is_some()
            );
        })
    });
    drive_owner(cx, 32);
    assert_eq!(
        service
            .selected_identity()
            .unwrap()
            .binding()
            .root()
            .summary()
            .marker_count(),
        1
    );
    input.read_with(cx, |input, _| {
        let markers = input.surface().unwrap().realized_objects();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].id(), InlineObjectId::new(0x1001));
        assert_eq!(markers[0].order(), InlineObjectOrder::new(2));
    });
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
}

#[gpui::test]
fn cancelled_marker_removal_releases_the_exact_surface_attachment(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase181-marker-remove-noncommit", 151);
    let marker_asset = publication_support::publish_image_asset(
        &fixture.store,
        fixture.assets(),
        b"phase181-marker-remove-noncommit",
    );
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([152; 16]),
                support::operation_id(153),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 154, 0, 0, "AB", 2, 1);
    let binding = insert_marker_at_text_end(&mut host, &store, binding, 155, marker_asset);
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        152,
        153,
        2,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 1,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(0x1001_u128.to_be_bytes()),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 1024),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(|_, _| ClipboardWriteOutcome::Written),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let marker = input.read_with(cx, |input, _| {
        input.surface().unwrap().realized_objects()[0]
    });
    let click = marker.hit_bounds().origin + point(px(1.0), px(1.0));
    cx.simulate_click(click, Modifiers::none());
    drive_owner(cx, 4);
    composer
        .update(cx, |composer, composer_cx| {
            composer.invoke_marker_remove(composer_cx)
        })
        .unwrap();
    service.test_cancel_next_mutation_commit();
    drive_owner(cx, 64);
    composer.read_with(cx, |composer, _| {
        assert!(composer.marker_menu().is_none());
    });
    assert!(input.read_with(cx, |input, _| input.active_inline_object().is_none()));
}

#[gpui::test]
fn multi_page_marker_cut_streams_after_one_successful_clipboard_write(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase181-streamed-cut", 121);
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([122; 16]),
                support::operation_id(123),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 124, 0, 0, "Z", 1, 1);
    let (binding, before, after) =
        composer_support::insert_markers(&mut host, &store, binding, 125, 9);
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage);
    let last = match after.gap {
        gpui_text_input::InlineObjectGap::After(last) => last,
        _ => unreachable!(),
    };
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        122,
        123,
        1,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: last.order().get().try_into().unwrap(),
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(last.id().get().to_be_bytes()),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let selection = slot.selected_identity().unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let writes = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed_writes = writes.clone();
    let configuration = MainWindowConversationComposerConfig::new(
        selection,
        widget_config(selection.binding().range_binding(), 4096),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let composer = MainWindowConversationComposer::new(
            configuration,
            service.clone(),
            Box::new(move |text, _| {
                observed_writes.lock().unwrap().push(text.to_owned());
                ClipboardWriteOutcome::Written
            }),
            window,
            cx,
        )
        .unwrap();
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input
                .rebind(
                    selection.binding().range_binding(),
                    Some(RangeSourceSelection {
                        anchor: before,
                        head: composer_support::position(1),
                    }),
                    window,
                    cx,
                )
                .unwrap()
        })
    });
    drive_owner(cx, 16);
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 128);

    let copied = writes.lock().unwrap();
    assert_eq!(
        copied.len(),
        1,
        "composer error: {:?}",
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned))
    );
    assert!(copied[0].starts_with("[Image A]"));
    assert!(copied[0].ends_with('Z'));
    drop(copied);
    let cut = service.selected_identity().unwrap();
    assert_eq!(
        cut.binding().candidate().candidate_generation(),
        selection
            .binding()
            .candidate()
            .candidate_generation()
            .checked_add(1)
            .unwrap()
    );
    assert_eq!(cut.binding().logical_extent().logical_utf8_bytes(), 0);
    assert_eq!(cut.binding().root().summary().marker_count(), 0);
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
}

fn insert_marker_at_text_end(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
    asset: AssetId,
) -> beryl_app::composer_host::ComposerHostBinding {
    let object = InlineObjectId::new(0x1001);
    let order = InlineObjectOrder::new(1);
    let point = SourcePosition::new(ByteOffset::new(1), InlineObjectGap::NoObjects);
    let after = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::after(InlineObjectNeighbor::new(object, order)),
    );
    let key = gpui_text_input::MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(point),
                SourceRange::new(point, point).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(object, ByteOffset::new(1), order, 17, 5),
        })],
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page),
        vec![ComposerHostImageMarkerMetadata::new(
            object,
            ImageLabelOrdinal::new(1).unwrap(),
            asset,
        )]
        .into_boxed_slice(),
    )
    .unwrap();
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            MutationStreamFinish {
                next_cursor: MutationCursor::new(0),
                next_ordinal: 0,
                cumulative_identity: MutationIdentity::ROOT,
                totals: MutationTotals::default(),
            },
            proposal_finish,
            LogicalExtent::new(0, 1),
            MutationPositions::collapsed(after),
        ),
    )
    .unwrap();
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("marker mutation did not commit: {other:?}"),
        }
    }
    panic!("marker mutation remained pending")
}

fn drive_owner(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
    }
}

fn activate_with_initial_pages(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    end: u64,
    marker_continuation: Option<DraftCompositeSearchKeyV1>,
) -> beryl_app::composer_host::ComposerHostBinding {
    let mut demands = vec![
        ComposerHostInitialDemand::Text {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 32,
        },
        ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::Range { start: 0, end },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                32,
                65_536,
            ),
        },
    ];
    if let Some(cursor) = marker_continuation {
        demands.push(ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(3).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::Range { start: 0, end },
                DraftPieceMarkerDirectionV1::Forward,
                Some(cursor),
                32,
                65_536,
            ),
        });
    }
    let outcome = host
        .test_activate(
            store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
                support::operation_id(operation),
                NonZeroU64::new(1).unwrap(),
                None,
                demands.into_boxed_slice(),
            ),
            &CommandCancellation::new(),
        )
        .unwrap();
    let ComposerHostActivationOutcome::Activated { binding, .. } = outcome else {
        panic!("composer did not reactivate exact candidate: {outcome:?}")
    };
    binding
}

fn widget_config(
    binding: gpui_text_input::RangeBinding,
    clipboard_bytes: usize,
) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 11,
        segment_policy_id: 13,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(320.),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: 32,
            runs: 8,
            decorations: 8,
            glyphs: 256,
            wraps: 128,
            maps: 257,
            fragments: 1,
            retained_items: 4096,
            retained_bytes: 256 * 1024,
        },
    };
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding,
        presentation_generation: PresentationGeneration::new(1),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout,
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(12.),
                px(16.),
                px(12.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(32, 8, 512 * 1024, 8192).unwrap(),
        residency_limits: ResidencyLimits::new(8, 128 * 1024, 8, 256).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(
            4,
            32,
            65_536,
            32 * 1024,
            4,
            32,
            65_536,
        )
        .unwrap(),
        mutation_limits: MutationLimits::new(8, 256).unwrap(),
        clipboard_limits: ClipboardLimits::new(clipboard_bytes, 32).unwrap(),
        segmentation_limits: SegmentationLimits::new(32, 64).unwrap(),
        limits: RangeTextInputLimits::new(2 * 1024 * 1024, 32768, 32, 32, px(16.)).unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(80.),
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}
