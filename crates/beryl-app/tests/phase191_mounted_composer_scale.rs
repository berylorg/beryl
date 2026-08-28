#![cfg(feature = "test-faults")]

#[path = "phase191_mounted_composer_scale/support.rs"]
mod support;

use std::{
    num::NonZeroU64,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostAutosaveInterval,
        ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushState,
        ComposerHostImageMarkerMetadata, ComposerHostInitialDemand, ComposerHostRequestId,
        ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{
        ComposerMarkerCommand, MainWindowComposerActivationAdvance,
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerPublishAdvance,
        MainWindowComposerSlot, MainWindowConversationComposerAutosavePhase,
        MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
        MainWindowConversationComposerMountDisposalAdvance,
        MainWindowConversationComposerMountFlushStart,
        MainWindowConversationComposerMountPublishAdvance, MainWindowConversationComposerService,
    },
};
use beryl_home_store::CommandCancellation;
use beryl_model::ImageLabelOrdinal;
use gpui::{
    AppContext, Entity, EntityInputHandler, IntoElement, Modifiers, ParentElement, Render,
    SharedString, StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, Styled,
    TextRun, black, div, font, point, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    ClipboardLimits, ExactGeometryLimits, GeometryQuality, InlineObjectGap, InlineObjectId,
    InlineObjectOrder, MutationLimits, ObjectResidencyLimits, PresentationGeneration,
    RangeRealizationCapacityState, RangeRealizationOwnership, RangeSettlementCoordinator,
    RangeSurfaceHit, RangeTextInputConfig, RangeTextInputLimits, ResidencyLimits,
    SegmentationLimits, StreamingGeometryStyle, StreamingOversizePresentation,
    TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme,
    ensure_text_input_bindings,
};
use syndic_storage::{
    DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1,
    DraftPieceTextDemandV1, SyndicPointReadLimit, SyndicTimestamp,
};

use support::{
    LARGE_DRAFT_BYTES, SAME_ANCHOR_MARKERS, assert_candidate_operation_reconciled,
    assert_same_anchor_marker_order, assert_tail_byte, create_third_target, expected_byte,
    fixture::Fixture, fixture::operation_id, marker_object_id, publish_image_asset,
    seed_large_published_draft,
};

const HIT_SAMPLE_STEP: f32 = 4.;
const HIT_SAMPLE_COLUMNS: usize = 80;
const HIT_SAMPLE_ROWS: usize = 160;
const MOUNT_INLINE_EXTENT: f32 = HIT_SAMPLE_STEP * HIT_SAMPLE_COLUMNS as f32;
const MOUNT_BLOCK_EXTENT: f32 = HIT_SAMPLE_STEP * HIT_SAMPLE_ROWS as f32;

struct MountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for MountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .w(px(MOUNT_INLINE_EXTENT))
            .h(px(MOUNT_BLOCK_EXTENT))
            .children(self.mount.read(cx).contribution())
    }
}

#[gpui::test]
fn mounted_multi_mib_activation_retarget_edit_history_autosave_and_disposal_are_bounded(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase191-mounted-scale", 191);
    let (selected_claim, target_claim) = fixture.claims();
    let (third_thread, third_claim, selected_claim) =
        create_third_target(&fixture, 191, selected_claim);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let mut host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        host.test_activate(
            &fixture.store,
            mounted_activation(selected_thread, 11, 12, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    seed_large_published_draft(&fixture, target_thread);
    let marker_asset = publish_image_asset(&fixture, b"phase191-same-anchor-marker");
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets);
    let (_directory, store, storage) = fixture.into_store();
    let slot =
        MainWindowComposerSlot::new(window_id, selected_claim, host, storage, marker_authority)
            .unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
        slot,
    ));
    let settlement_coordinator = RangeSettlementCoordinator::new(4).unwrap();
    assert_eq!(settlement_coordinator.capacity(), 4);
    assert_eq!(settlement_coordinator.retained_count(), 0);
    let mounted_service = service.clone();
    let mounted_marker_seals = marker_seals.clone();
    let mounted_settlement_coordinator = settlement_coordinator.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let settlement_coordinator = mounted_settlement_coordinator.clone();
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(move |selection| {
                    let presentation = selection.binding().presentation_generation().get();
                    let (max_realized_block_extent, viewport_extent) = match presentation {
                        1 => (px(MOUNT_BLOCK_EXTENT), px(MOUNT_BLOCK_EXTENT)),
                        3 => (px(80.), px(480.)),
                        _ => (px(64.), px(MOUNT_BLOCK_EXTENT)),
                    };
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                            max_realized_block_extent,
                            viewport_extent,
                            settlement_coordinator.clone(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                mounted_marker_seals,
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 32);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let predecessor = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let predecessor_input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    assert!(
        settle_without_draw(cx, 4_096, |cx| {
            predecessor_input.read_with(cx, |input, _| input.is_quiescent())
        }),
        "mounted state did not quiesce: owner_error={:?}, diagnostics={:?}, snapshot={:?}",
        predecessor.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        predecessor_input.read_with(cx, |input, _| input.realization_diagnostics()),
        mount.read_with(cx, |mount, app| mount.surface_snapshot(app)),
    );
    cx.update(|window, app| {
        predecessor_input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, "p", None, window, input_cx)
        })
    });
    drive_until(cx, 512, "predecessor edit", |_| {
        service
            .selected_identity()
            .is_some_and(|selection| selection.binding().logical_extent().logical_utf8_bytes() == 1)
    });
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "predecessor autosave", |cx| {
        let selected = service.selected_identity().unwrap();
        storage
            .current_draft(
                &store,
                selected_thread,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .is_some_and(|current| current.draft().piece_root() == selected.binding().root())
            && mount.read_with(cx, |mount, _| {
                mount.autosave_diagnostics().phase()
                    == MainWindowConversationComposerAutosavePhase::Idle
            })
    });
    let predecessor_entity = predecessor.entity_id();
    let predecessor_selection = service.selected_identity().unwrap();
    assert_candidate_operation_reconciled(storage, &store, predecessor_selection.binding());
    assert_eq!(settlement_coordinator.retained_count(), 0);

    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        mount
            .update(cx, |mount, mount_cx| mount.begin_activation(
                target_claim,
                mounted_activation_with_demand_count(
                    target_thread,
                    17,
                    18,
                    2,
                    LARGE_DRAFT_BYTES,
                    15,
                ),
                operation_id(19),
                &cancelled,
                mount_cx,
            ))
            .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_entity
    );
    let over_cap =
        mounted_activation_with_demand_count(target_thread, 19, 20, 2, LARGE_DRAFT_BYTES, 17);
    assert!(matches!(
        SyndicComposerHost::new(storage).test_activate(
            &store,
            over_cap.clone(),
            &CommandCancellation::new(),
        ),
        Err(beryl_app::composer_host::ComposerHostError::TooManyInitialDemands)
    ));
    assert!(
        mount
            .update(cx, |mount, mount_cx| mount.begin_activation(
                target_claim,
                over_cap,
                operation_id(21),
                &CommandCancellation::new(),
                mount_cx,
            ))
            .is_err()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_candidate_operation_reconciled(storage, &store, predecessor_selection.binding());
    assert!(service.pending_receipt().is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );

    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                mounted_activation(target_thread, 23, 24, 3, LARGE_DRAFT_BYTES),
                operation_id(25),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("large target did not become activation-ready");
    };
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(receipt, window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::TargetPriming(current) if current == receipt
    ));
    let primed_pending = mount
        .read_with(cx, |mount, _| mount.test_pending_contribution())
        .unwrap();
    let primed_pending_entity = primed_pending.entity_id();
    assert_ne!(primed_pending_entity, predecessor_entity);
    assert!(primed_pending.read_with(cx, |composer, _| composer.is_pending_target()));
    let primed_selection =
        primed_pending.read_with(cx, |composer, _| composer.selection_identity());
    assert_eq!(service.pending_identity(receipt), Some(primed_selection));
    assert_eq!(
        primed_selection.binding().presentation_generation().get(),
        3
    );
    assert_combined_activation_residency(cx, &mount);
    assert!(predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    drop(primed_pending);

    let third_request = mounted_activation_with_demand_count(third_thread, 27, 28, 4, 0, 15);
    let mut third_receipt = None;
    for _ in 0..256 {
        match mount.update(cx, |mount, mount_cx| {
            mount.begin_activation(
                third_claim,
                third_request.clone(),
                operation_id(29),
                &CommandCancellation::new(),
                mount_cx,
            )
        }) {
            Ok(MainWindowComposerActivationAdvance::Ready(current)) => {
                third_receipt = Some(current);
                break;
            }
            Err(_) => drive(cx, 1),
            Ok(other) => panic!("third-target retarget did not become ready: {other:?}"),
        }
    }
    let third_receipt = third_receipt.expect("large target did not retire within its finite bound");
    assert!(service.pending_identity(receipt).is_none());
    assert_eq!(service.pending_receipt(), Some(third_receipt));
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert_ne!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        primed_pending_entity
    );
    assert_eq!(settlement_coordinator.retained_count(), 0);
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(receipt, window, mount_cx)
        }))
        .is_err()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_entity
    );
    let final_request = mounted_activation(target_thread, 31, 32, 5, LARGE_DRAFT_BYTES);
    let mut final_receipt = None;
    for _ in 0..256 {
        match mount.update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                final_request.clone(),
                operation_id(33),
                &CommandCancellation::new(),
                mount_cx,
            )
        }) {
            Ok(MainWindowComposerActivationAdvance::Ready(current)) => {
                final_receipt = Some(current);
                break;
            }
            Err(_) => drive(cx, 1),
            Ok(other) => panic!("final large-target retarget did not become ready: {other:?}"),
        }
    }
    let receipt = final_receipt.expect("third target did not retire within its finite bound");
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(third_receipt, window, mount_cx)
        }))
        .is_err()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_candidate_operation_reconciled(storage, &store, predecessor_selection.binding());

    let mut flush = None;
    let mut target_priming_advances = 0_usize;
    let mut widget_fence_advances = 0_usize;
    for _ in 0..4_096 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_) => {
                target_priming_advances += 1;
                assert_combined_activation_residency(cx, &mount);
                assert_eq!(
                    mount
                        .read_with(cx, |mount, _| mount.contribution())
                        .unwrap()
                        .entity_id(),
                    predecessor_entity
                );
                drive(cx, 1);
                let pending_diagnostics = mount
                    .read_with(cx, |mount, _| mount.test_pending_contribution())
                    .map(|pending| {
                        let input = pending.read_with(cx, |composer, _| composer.gpui_input());
                        input.read_with(cx, |input, _| input.realization_diagnostics())
                    })
                    .expect("target priming retains its pending contribution");
                assert_eq!(
                    pending_diagnostics.response_rejection_count, 0,
                    "legitimate target priming rejected exact geometry: {pending_diagnostics:?}"
                );
            }
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(selection) => {
                widget_fence_advances += 1;
                assert_eq!(selection, predecessor_selection);
                assert_eq!(
                    mount
                        .read_with(cx, |mount, _| mount.contribution())
                        .unwrap()
                        .entity_id(),
                    predecessor_entity
                );
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Started { ticket, .. },
            ) => {
                flush = Some(ticket);
                break;
            }
            start => panic!("predecessor flush did not start: {start:?}"),
        }
    }
    let flush = flush.unwrap_or_else(|| {
        let pending = mount
            .read_with(cx, |mount, _| mount.test_pending_contribution());
        let pending_diagnostics = pending.as_ref().map(|pending| {
            let input = pending.read_with(cx, |composer, _| composer.gpui_input());
            (
                pending.read_with(cx, |composer, _| {
                    (
                        composer.last_error().map(str::to_owned),
                        composer.test_has_active_flight(),
                        composer.test_pending_seed_count(),
                    )
                }),
                input.read_with(cx, |input, _| {
                    (
                        input.is_semantically_quiescent(),
                        input.is_quiescent(),
                        input.realization_diagnostics(),
                    )
                }),
            )
        });
        panic!(
            "predecessor fence exceeded its finite settle bound: target_priming_advances={target_priming_advances}, widget_fence_advances={widget_fence_advances}, predecessor_error={:?}, predecessor_active_flight={}, predecessor_semantically_quiescent={}, predecessor_quiescent={}, predecessor_pending_realizer={}, predecessor_diagnostics={:?}, pending_diagnostics={pending_diagnostics:?}, settlement_retained={}",
            predecessor.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
            predecessor.read_with(cx, |composer, _| composer.test_has_active_flight()),
            predecessor_input.read_with(cx, |input, _| input.is_semantically_quiescent()),
            predecessor_input.read_with(cx, |input, _| input.is_quiescent()),
            predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()),
            predecessor_input.read_with(cx, |input, _| input.realization_diagnostics()),
            settlement_coordinator.retained_count(),
        )
    });
    let predecessor_capture = mount
        .update(cx, |mount, _| {
            mount.capture_flush_publication(
                predecessor_selection,
                flush,
                assets,
                &marker_seals,
                operation_id(24),
                None,
                current_timestamp(),
                &CommandCancellation::new(),
            )
        })
        .unwrap();
    assert!(
        matches!(
            predecessor_capture,
            ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
        ),
        "unexpected predecessor publication capture: {predecessor_capture:?}"
    );
    for _ in 0..32 {
        let advance = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        if matches!(
            advance,
            MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::Progress(
                    ComposerHostFlushState::DisposalRequired
                )
            )
        ) {
            break;
        }
    }
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_disposal(
                mount.selected_identity().unwrap(),
                flush,
                operation_id(25),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let mut published = None;
    for _ in 0..256 {
        drive(cx, 4);
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(_) => {}
            advance => {
                published = Some(advance);
                break;
            }
        }
    }
    let published = published.expect("large target publication exceeded its finite release bound");
    let MainWindowConversationComposerMountPublishAdvance::Published(target_selection) = published
    else {
        panic!("large target did not publish atomically: {published:?}");
    };
    let target = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_ne!(target.entity_id(), predecessor_entity);
    assert_eq!(service.selected_identity(), Some(target_selection));
    let initial_surface = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap();
    assert_eq!(initial_surface.selection, target_selection);
    assert_eq!(
        initial_surface.binding.extent().byte_len(),
        LARGE_DRAFT_BYTES
    );

    for block in [px(72_000.), px(8_000.), px(160_000.), px(24_000.)] {
        cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.request_absolute_scroll(block, window, mount_cx)
            })
        })
        .unwrap();
    }
    drive_until(cx, 4_096, "absolute scroll", |cx| {
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .is_some_and(|surface| surface.scroll_block >= px(24_000.))
    });
    let scrolled = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap();
    assert_eq!(scrolled.quality, GeometryQuality::Exact);
    assert!(matches!(
        scrolled.capacity,
        RangeRealizationCapacityState::ViewportExceedsRenderingCapacity
            | RangeRealizationCapacityState::CapacitySaturatedViewportExceedsRenderingCapacity
    ));
    assert!(scrolled.fillers.iter().flatten().next().is_some());
    if let Some(filler) = scrolled.fillers.iter().flatten().copied().find(|filler| {
        filler.block_end() > scrolled.scroll_block
            && filler.block_start() < scrolled.scroll_block + px(MOUNT_BLOCK_EXTENT)
    }) {
        let viewport_block = (filler.block_start() - scrolled.scroll_block + px(1.)).max(px(0.));
        cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.request_filler_reanchor(viewport_block, window, mount_cx)
            })
        })
        .unwrap();
        drive(cx, 64);
    }
    let exact_hit = (0..80).find_map(|row| {
        mount.read_with(cx, |mount, app| {
            mount.hit_test_composite_viewport(point(px(4.), px(row as f32 * 8.)), app)
        })
    });
    let exact_hit = exact_hit.expect("realized viewport had no exact composite hit");
    assert_eq!(exact_hit.selection, target_selection);
    assert_eq!(
        exact_hit.binding,
        target_selection.binding().range_binding()
    );
    assert!(matches!(
        exact_hit.hit,
        RangeSurfaceHit::Gap(position) if matches!(position.gap, InlineObjectGap::NoObjects)
    ));

    let input = target.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.simulate_keystrokes("ctrl-end");
    drive_until(cx, 4_096, "ctrl-end", |cx| {
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .is_some_and(|surface| {
                surface.source_selection.head.byte_offset.get() == LARGE_DRAFT_BYTES
            })
    });
    cx.simulate_keystrokes("shift-left");
    drive_until(cx, 256, "shift-left", |cx| {
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .is_some_and(|surface| surface.source_selection.anchor != surface.source_selection.head)
    });
    let before_edit = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap()
        .source_selection;
    let before_edit_root = service.selected_identity().unwrap().binding().root();
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "!", None, window, input_cx)
        })
    });
    let localized_edit_settled = drive_until_result(cx, 4_096, |cx| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().root() != before_edit_root
                && selection.binding().logical_extent().logical_utf8_bytes() == LARGE_DRAFT_BYTES
                && mount
                    .read_with(cx, |mount, app| mount.surface_snapshot(app))
                    .is_some_and(|surface| surface.binding == selection.binding().range_binding())
        })
    });
    assert!(
        localized_edit_settled,
        "localized edit did not settle: selected_state={:?}, owner_error={:?}, response_rejections={:?}, source_selection={:?}, retained_settlements={}",
        service.selected_identity().map(|selected| (
            selected.binding().root() != before_edit_root,
            selected.binding().logical_extent().logical_utf8_bytes(),
            selected.binding().candidate().candidate_generation(),
        )),
        target.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        input.read_with(cx, |input, _| {
            let diagnostics = input.realization_diagnostics();
            (
                diagnostics.response_rejection_count,
                diagnostics.last_response_rejection,
                diagnostics.last_response_rejection_stage,
            )
        }),
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .map(|snapshot| snapshot.source_selection),
        settlement_coordinator.retained_count(),
    );
    let edited_selection = service.selected_identity().unwrap();
    let after_edit = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap()
        .source_selection;
    assert_tail_byte(storage, &store, edited_selection.binding().root(), b'!');

    cx.simulate_keystrokes("ctrl-z");
    let undo_settled = drive_until_result(cx, 4_096, |cx| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().root() == before_edit_root
                && mount
                    .read_with(cx, |mount, app| mount.surface_snapshot(app))
                    .is_some_and(|surface| {
                        surface.binding == selection.binding().range_binding()
                            && surface.source_selection == before_edit
                    })
        })
    });
    assert!(
        undo_settled,
        "undo did not settle: selected_state={:?}, owner_error={:?}, surface_state={:?}, diagnostics={:?}, retained_settlements={}",
        service.selected_identity().map(|selection| (
            selection.binding().root() == before_edit_root,
            selection.binding().range_binding(),
            selection.binding().candidate().candidate_generation(),
        )),
        target.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .map(|surface| (surface.binding, surface.source_selection, surface.quality)),
        input.read_with(cx, |input, _| {
            let diagnostics = input.realization_diagnostics();
            (
                diagnostics.current.pending_rebind_intents,
                diagnostics.current.pending_presentation_intents,
                diagnostics.current.active_geometry_jobs,
                diagnostics.current.scheduled_continuations,
                diagnostics.response_rejection_count,
                diagnostics.last_response_rejection,
                diagnostics.last_response_rejection_stage,
            )
        }),
        settlement_coordinator.retained_count(),
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .unwrap()
            .source_selection,
        before_edit
    );
    assert_tail_byte(
        storage,
        &store,
        before_edit_root,
        expected_byte(LARGE_DRAFT_BYTES - 1),
    );

    cx.simulate_keystrokes("ctrl-y");
    drive_until(cx, 4_096, "redo", |cx| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().root() == edited_selection.binding().root()
                && mount
                    .read_with(cx, |mount, app| mount.surface_snapshot(app))
                    .is_some_and(|surface| {
                        surface.binding == selection.binding().range_binding()
                            && surface.source_selection == after_edit
                    })
        })
    });
    let redone_selection = service.selected_identity().unwrap();
    assert_eq!(
        redone_selection.binding().root(),
        edited_selection.binding().root()
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .unwrap()
            .source_selection,
        after_edit
    );
    assert_candidate_operation_reconciled(storage, &store, redone_selection.binding());

    cx.simulate_keystrokes("ctrl-z");
    drive_until(cx, 4_096, "undo before redo clear", |cx| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().root() == before_edit_root
                && mount
                    .read_with(cx, |mount, app| mount.surface_snapshot(app))
                    .is_some_and(|surface| {
                        surface.binding == selection.binding().range_binding()
                            && surface.source_selection == before_edit
                    })
        })
    });
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "?", None, window, input_cx)
        })
    });
    drive_until(cx, 4_096, "redo-clearing edit", |cx| {
        service.selected_identity().is_some_and(|selection| {
            selection.binding().root() != before_edit_root
                && selection.binding().root() != edited_selection.binding().root()
                && mount
                    .read_with(cx, |mount, app| mount.surface_snapshot(app))
                    .is_some_and(|surface| surface.binding == selection.binding().range_binding())
        })
    });
    let redo_cleared_selection = service.selected_identity().unwrap();
    let redo_cleared_source = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap()
        .source_selection;
    assert_tail_byte(
        storage,
        &store,
        redo_cleared_selection.binding().root(),
        b'?',
    );
    assert_candidate_operation_reconciled(storage, &store, redo_cleared_selection.binding());
    cx.simulate_keystrokes("ctrl-y");
    drive(cx, 32);
    assert_eq!(service.selected_identity(), Some(redo_cleared_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, app| mount.surface_snapshot(app))
            .unwrap()
            .source_selection,
        redo_cleared_source
    );

    for index in 0..SAME_ANCHOR_MARKERS {
        let prior_generation = service
            .selected_identity()
            .unwrap()
            .binding()
            .candidate()
            .candidate_generation();
        target
            .update(cx, |composer, composer_cx| {
                composer.insert_authenticated_image_marker(
                    ComposerHostImageMarkerMetadata::new(
                        InlineObjectId::new(marker_object_id(index)),
                        ImageLabelOrdinal::new(1).unwrap(),
                        marker_asset,
                    ),
                    InlineObjectOrder::new((index + 1) as u128),
                    composer_cx,
                )
            })
            .unwrap_or_else(|error| {
                panic!(
                    "marker insertion {index} rejected: {error}; diagnostics={:?}; owner_error={:?}",
                    input.read_with(cx, |input, _| input.realization_diagnostics()),
                    target.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
                )
            });
        drive_until(cx, 4_096, "marker insertion", |cx| {
            service.selected_identity().is_some_and(|selection| {
                selection.binding().candidate().candidate_generation() > prior_generation
                    && input.read_with(cx, |input, _| input.is_surface_current_and_interactive())
                    && mount
                        .read_with(cx, |mount, app| mount.surface_snapshot(app))
                        .is_some_and(|surface| {
                            surface.binding == selection.binding().range_binding()
                        })
            })
        });
    }
    let marker_selection = service.selected_identity().unwrap();
    assert_eq!(
        marker_selection.binding().root().summary().marker_count(),
        SAME_ANCHOR_MARKERS as u64
    );
    assert_same_anchor_marker_order(
        storage,
        &store,
        marker_selection.binding().root(),
        marker_asset,
    );
    assert_tail_byte(storage, &store, marker_selection.binding().root(), b'?');
    assert_candidate_operation_reconciled(storage, &store, marker_selection.binding());
    drive(cx, 512);
    let marker_surface = mount
        .read_with(cx, |mount, app| mount.surface_snapshot(app))
        .unwrap();
    assert!(marker_surface.realized_object_count > 0);
    assert!(marker_surface.realized_object_count <= 48);
    let scrollbar_hit_lane = ScrollbarStyle::default().hit_lane_thickness;
    let object_hit = (0..HIT_SAMPLE_ROWS).find_map(|row| {
        (0..HIT_SAMPLE_COLUMNS).find_map(|column| {
            let viewport_inline = px(column as f32 * HIT_SAMPLE_STEP);
            if viewport_inline >= px(MOUNT_INLINE_EXTENT) - scrollbar_hit_lane {
                return None;
            }
            let viewport_position = point(viewport_inline, px(row as f32 * HIT_SAMPLE_STEP));
            mount
                .read_with(cx, |mount, app| {
                    mount.hit_test_composite_viewport(viewport_position, app)
                })
                .filter(|hit| matches!(hit.hit, RangeSurfaceHit::Object(_)))
                .map(|hit| (viewport_position, hit))
        })
    });
    let (object_viewport_position, object_hit) =
        object_hit.expect("mounted marker viewport had no exact object hit");
    let RangeSurfaceHit::Object(object_geometry) = object_hit.hit else {
        unreachable!();
    };
    let object_order = object_geometry.order().get();
    assert!((1..=SAME_ANCHOR_MARKERS as u128).contains(&object_order));
    assert_eq!(
        object_geometry.id(),
        InlineObjectId::new(marker_object_id((object_order - 1) as usize))
    );
    assert_eq!(
        object_geometry.leading().byte_offset.get(),
        LARGE_DRAFT_BYTES
    );
    assert_eq!(
        object_geometry.trailing().byte_offset.get(),
        LARGE_DRAFT_BYTES
    );
    assert_object_leading_gap(
        object_geometry.leading().gap,
        object_geometry.id(),
        object_geometry.order(),
    );
    assert_object_trailing_gap(
        object_geometry.trailing().gap,
        object_geometry.id(),
        object_geometry.order(),
    );
    let hit_bounds = object_geometry.hit_bounds();
    let midpoint = point(
        hit_bounds.origin.x + hit_bounds.size.width / 2.,
        hit_bounds.origin.y + hit_bounds.size.height / 2. - marker_surface.scroll_block,
    );
    let repeated_hit = mount
        .read_with(cx, |mount, app| {
            mount.hit_test_composite_viewport(midpoint, app)
        })
        .unwrap();
    assert_eq!(repeated_hit.selection, marker_selection);
    assert_eq!(repeated_hit.hit, object_hit.hit);
    assert!(input.read_with(cx, |input, _| input.is_surface_current_and_interactive()));
    cx.simulate_click(object_viewport_position, Modifiers::none());
    drive_until(cx, 4_096, "marker activation", |cx| {
        input
            .read_with(cx, |input, _| input.active_inline_object())
            .is_some()
    });
    let active_marker = input
        .read_with(cx, |input, _| input.active_inline_object())
        .unwrap();
    let marker_menu = target
        .read_with(cx, |composer, _| composer.marker_menu())
        .unwrap();
    assert_eq!(
        target.read_with(cx, |composer, _| composer.selection_identity()),
        marker_selection
    );
    assert_eq!(active_marker, marker_menu.anchor());
    assert_eq!(
        marker_menu.commands(),
        &[ComposerMarkerCommand::View, ComposerMarkerCommand::Remove]
    );
    assert_eq!(
        cx.update(|window, app| target.update(app, |composer, composer_cx| {
            composer.dismiss_marker_menu(window, composer_cx)
        }))
        .unwrap(),
        Some(beryl_app::main_window::ComposerMarkerFocusTarget::OriginMarker(active_marker))
    );
    drive(cx, 4);

    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                2,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    drive_until(cx, 512, "target autosave", |cx| {
        mount.read_with(cx, |mount, _| {
            mount.autosave_diagnostics().phase()
                == MainWindowConversationComposerAutosavePhase::Idle
        })
    });
    let autosave = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(autosave.last_error(), None);
    assert_eq!(autosave.retained_tasks(), 0);
    let marker_seal_diagnostics = marker_seals.diagnostics();
    assert_eq!(marker_seal_diagnostics.configured_flight_limit(), 1);
    assert!(marker_seal_diagnostics.high_water_flights() > 0);
    assert!(
        marker_seal_diagnostics.high_water_flights()
            <= marker_seal_diagnostics.configured_flight_limit()
    );
    assert_eq!(marker_seal_diagnostics.current_flights(), 0);
    assert_eq!(marker_seal_diagnostics.retained_draft_sized_bytes(), 0);
    let current = storage
        .current_draft(
            &store,
            target_thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        current.draft().piece_root(),
        marker_selection.binding().root()
    );
    assert_eq!(
        current.draft().piece_root().summary().marker_count(),
        SAME_ANCHOR_MARKERS as u64
    );
    let published_current = current.clone();
    cx.executor().advance_clock(Duration::from_secs(5));
    drive(cx, 64);
    assert_eq!(
        storage
            .current_draft(
                &store,
                target_thread,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap(),
        published_current
    );
    let clean_autosave = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(
        clean_autosave.phase(),
        MainWindowConversationComposerAutosavePhase::Idle
    );
    assert_eq!(clean_autosave.last_error(), None);
    assert_eq!(clean_autosave.retained_tasks(), 0);

    let diagnostics = mount
        .read_with(cx, |mount, app| mount.realization_diagnostics(app))
        .unwrap();
    assert_eq!(diagnostics.max_realized_block_extent, px(64.));
    assert_eq!(diagnostics.max_resident_pages, 6);
    assert_eq!(diagnostics.max_resident_objects, 48);
    assert!(diagnostics.high_water.owned_bytes <= diagnostics.max_surface_bytes);
    assert!(diagnostics.high_water.owned_items <= diagnostics.max_surface_items);
    assert!(diagnostics.high_water.resident_pages <= diagnostics.max_owned_pages);
    assert!(diagnostics.high_water.resident_objects <= diagnostics.max_owned_objects);
    assert!(diagnostics.geometry_high_water_bytes <= diagnostics.max_geometry_bytes);
    assert!(diagnostics.geometry_high_water_items <= diagnostics.max_geometry_items);
    assert!(diagnostics.current.owned_bytes <= diagnostics.max_surface_bytes);
    assert!(diagnostics.current.owned_items <= diagnostics.max_surface_items);
    assert!(diagnostics.current.resident_pages <= diagnostics.max_owned_pages);
    assert!(diagnostics.current.resident_objects <= diagnostics.max_owned_objects);
    assert!(diagnostics.high_water.resident_pages > 0);
    assert!(diagnostics.high_water.resident_objects > 0);
    assert!(diagnostics.high_water.owned_bytes < LARGE_DRAFT_BYTES as usize);
    assert!(diagnostics.high_water.resident_page_bytes < LARGE_DRAFT_BYTES as usize);
    assert!(diagnostics.high_water.resident_object_bytes < LARGE_DRAFT_BYTES as usize);
    assert!(diagnostics.high_water.geometry_bytes < LARGE_DRAFT_BYTES as usize);
    assert!(
        diagnostics.high_water.dispatched_page_requests > 0
            || diagnostics.high_water.request_payload_items > 0
            || diagnostics.high_water.target_geometry_page_waits > 0
    );
    assert!(diagnostics.surface_high_water.bytes <= diagnostics.max_surface_bytes);
    assert!(diagnostics.surface_high_water.items <= diagnostics.max_surface_items);
    assert!(diagnostics.surface_high_water.bytes < LARGE_DRAFT_BYTES as usize);
    assert!(input.read_with(cx, |input, _| input.is_quiescent()));
    assert_eq!(settlement_coordinator.retained_count(), 0);

    let disposal_flush = match cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_disposal(window, mount_cx)
            })
        })
        .unwrap()
    {
        MainWindowConversationComposerMountFlushStart::Started(
            ComposerHostFlushAdmission::Started { ticket, .. },
        ) => ticket,
        start => panic!("clean large composer disposal did not start: {start:?}"),
    };
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_disposal(
                mount.selected_identity().unwrap(),
                disposal_flush,
                operation_id(31),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let mut disposed = None;
    for _ in 0..256 {
        drive(cx, 4);
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_disposal(window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(_) => {}
            advance => {
                disposed = Some(advance);
                break;
            }
        }
    }
    let disposed = disposed.expect("large composer disposal exceeded its finite release bound");
    assert_eq!(
        disposed,
        MainWindowConversationComposerMountDisposalAdvance::Disposed
    );
    assert!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .is_none()
    );
    assert_eq!(service.selected_identity(), None);
    let released = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(released.retained_tasks(), 0);
    assert!(
        mount
            .read_with(cx, |mount, app| mount.realization_diagnostics(app))
            .is_none()
    );
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
    let released_diagnostics = input.read_with(cx, |input, _| input.realization_diagnostics());
    assert!(input.read_with(cx, |input, _| input.is_quiescent()));
    assert_eq!(
        released_diagnostics.current,
        RangeRealizationOwnership {
            owned_bytes: released_diagnostics.current.owned_bytes,
            owned_items: released_diagnostics.current.owned_items,
            geometry_bytes: released_diagnostics.current.geometry_bytes,
            geometry_items: released_diagnostics.current.geometry_items,
            ..RangeRealizationOwnership::default()
        }
    );
    assert_eq!(settlement_coordinator.capacity(), 4);
    assert_eq!(settlement_coordinator.retained_count(), 0);
    let weak_target = target.downgrade();
    let weak_input = input.downgrade();
    drop(input);
    drop(target);
    drive(cx, 2);
    assert!(weak_input.upgrade().is_none());
    assert!(weak_target.upgrade().is_none());
}

fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
    }
}

fn current_timestamp() -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap(),
    )
}

fn settle_without_draw(
    cx: &mut gpui::VisualTestContext,
    rounds: usize,
    mut complete: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) -> bool {
    for _ in 0..rounds {
        if complete(cx) {
            return true;
        }
        cx.run_until_parked();
    }
    complete(cx)
}

fn assert_combined_activation_residency(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
) {
    let residency = mount
        .read_with(cx, |mount, app| mount.test_activation_residency(app))
        .unwrap();
    assert!(residency.current_text_pages() > 0);
    assert!(residency.current_text_pages() <= residency.bound().text_pages());
    assert!(residency.current_text_bytes() <= residency.bound().text_bytes());
    assert!(residency.current_object_pages() <= residency.bound().object_pages());
    assert!(residency.current_objects() <= residency.bound().objects());
    assert!(residency.current_object_bytes() <= residency.bound().object_bytes());
    assert!(residency.current_owned_bytes() <= residency.bound().owned_bytes());
    assert!(residency.current_owned_items() <= residency.bound().owned_items());
}

fn assert_object_leading_gap(gap: InlineObjectGap, id: InlineObjectId, order: InlineObjectOrder) {
    let neighbor = match gap {
        InlineObjectGap::Before(neighbor) => neighbor,
        InlineObjectGap::Between { following, .. } => following,
        other => panic!("object leading edge did not retain an exact neighbor gap: {other:?}"),
    };
    assert_eq!(neighbor.id(), id);
    assert_eq!(neighbor.order(), order);
}

fn assert_object_trailing_gap(gap: InlineObjectGap, id: InlineObjectId, order: InlineObjectOrder) {
    let neighbor = match gap {
        InlineObjectGap::After(neighbor) => neighbor,
        InlineObjectGap::Between { preceding, .. } => preceding,
        other => panic!("object trailing edge did not retain an exact neighbor gap: {other:?}"),
    };
    assert_eq!(neighbor.id(), id);
    assert_eq!(neighbor.order(), order);
}

fn mounted_activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    presentation: u64,
    end: u64,
) -> ComposerHostActivationRequest {
    let mut demands = vec![
        ComposerHostInitialDemand::Text {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 49_152,
        },
        ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::Range { start: 0, end },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                48,
                65_536,
            ),
        },
    ];
    if end > 0 {
        for page in 1..8_u64 {
            let start = page * 49_152;
            demands.push(ComposerHostInitialDemand::Text {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(page * 2 + 1).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceTextDemandV1::Forward(start),
                max_bytes: 49_152,
            });
            demands.push(ComposerHostInitialDemand::Markers {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(page * 2 + 2).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::Range {
                        start,
                        end: (start + 49_152).min(end),
                    },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    48,
                    65_536,
                ),
            });
        }
    }
    ComposerHostActivationRequest::new(
        thread,
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        operation_id(operation),
        NonZeroU64::new(presentation).unwrap(),
        None,
        demands.into_boxed_slice(),
    )
}

fn mounted_activation_with_demand_count(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    presentation: u64,
    end: u64,
    demand_count: usize,
) -> ComposerHostActivationRequest {
    let mut demands = Vec::with_capacity(demand_count);
    for index in 0..demand_count {
        let request_id = ComposerHostRequestId::new(NonZeroU64::new((index + 1) as u64).unwrap());
        let start = if end == 0 {
            0
        } else {
            (index as u64 / 2 * 49_152).min(end - 1)
        };
        if index % 2 == 0 {
            demands.push(ComposerHostInitialDemand::Text {
                request_id,
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceTextDemandV1::Forward(start),
                max_bytes: 49_152,
            });
        } else {
            demands.push(ComposerHostInitialDemand::Markers {
                request_id,
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::Range {
                        start,
                        end: (start + 49_152).min(end),
                    },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    48,
                    65_536,
                ),
            });
        }
    }
    ComposerHostActivationRequest::new(
        thread,
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        operation_id(operation),
        NonZeroU64::new(presentation).unwrap(),
        None,
        demands.into_boxed_slice(),
    )
}

fn drive_until(
    cx: &mut gpui::VisualTestContext,
    rounds: usize,
    operation: &str,
    mut complete: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    assert!(
        drive_until_result(cx, rounds, &mut complete),
        "bounded mounted operation did not settle: {operation}"
    );
}

fn drive_until_result(
    cx: &mut gpui::VisualTestContext,
    rounds: usize,
    mut complete: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) -> bool {
    for _ in 0..rounds {
        if complete(cx) {
            return true;
        }
        drive(cx, 1);
    }
    complete(cx)
}

fn widget_config(
    binding: gpui_text_input::RangeBinding,
    presentation: NonZeroU64,
    max_realized_block_extent: gpui::Pixels,
    viewport_extent: gpui::Pixels,
    settlement_coordinator: RangeSettlementCoordinator,
) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 191,
        segment_policy_id: 191,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(MOUNT_INLINE_EXTENT),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: 4096,
            runs: 8,
            decorations: 8,
            glyphs: 4096,
            wraps: 256,
            maps: 4097,
            fragments: 4,
            retained_items: 32_768,
            retained_bytes: 2 * 1024 * 1024,
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
        presentation_generation: PresentationGeneration::new(presentation.get()),
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
        geometry_limits: ExactGeometryLimits::new(49_152, 16, 4 * 1024 * 1024, 65_536).unwrap(),
        residency_limits: ResidencyLimits::new(6, 384 * 1024, 6, 384 * 1024).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(6, 48, 65_536, 65_536, 6, 48, 65_536)
            .unwrap(),
        mutation_limits: MutationLimits::new(64, 65_536).unwrap(),
        clipboard_limits: ClipboardLimits::new(64 * 1024, 49_152).unwrap(),
        segmentation_limits: SegmentationLimits::new(49_152, 4096).unwrap(),
        limits: RangeTextInputLimits::new(
            8 * 1024 * 1024,
            131_072,
            64,
            max_realized_block_extent,
            49_152,
            49_152,
            px(16.),
        )
        .unwrap(),
        settlement_coordinator,
        viewport_extent,
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}
