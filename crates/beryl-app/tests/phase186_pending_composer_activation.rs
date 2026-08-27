#![cfg(feature = "test-faults")]

#[path = "phase141_syndic_composer_host/support.rs"]
mod composer_base;
#[path = "phase186_pending_composer_activation/support.rs"]
mod support;

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostFlushAdmission, ComposerHostFlushCapture,
        ComposerHostFlushPurpose, ComposerHostFlushState, SyndicComposerHost,
    },
    main_window::{
        MainWindowComposerActivationAdvance, MainWindowComposerMarkerMetadataAuthority,
        MainWindowComposerPublishAdvance, MainWindowComposerSlot,
        MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
        MainWindowConversationComposerMountFlushStart,
        MainWindowConversationComposerMountPublishAdvance, MainWindowConversationComposerService,
    },
};
use beryl_home_store::CommandCancellation;
use gpui::{
    AppContext, Entity, EntityInputHandler, Focusable, IntoElement, ParentElement, Render, div,
};
use gpui_text_input::ensure_text_input_bindings;
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftPieceReplacementV1, DraftPieceV1, SyndicPointReadLimit, SyndicTimestamp,
};

use support::{
    ACTIVATION_DRAFT_BYTES, activation, drive, fixture::Fixture, fixture::operation_id,
    seed_activation_published_draft, widget_config,
};

struct MountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for MountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div().children(self.mount.read(cx).contribution())
    }
}

#[gpui::test]
fn small_seed_promotes_over_a_clean_generation_zero_predecessor(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-small", 184);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected_host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        selected_host
            .test_activate(
                &fixture.store,
                activation(selected_thread, 1, 2, 1, 0),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        selected_host,
        storage,
        marker_authority,
    )
    .unwrap();
    let clean_predecessor = slot.selected_identity().unwrap();
    assert_eq!(
        clean_predecessor
            .binding()
            .candidate()
            .candidate_generation(),
        0
    );
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(store, slot));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let predecessor_id = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap()
        .entity_id();
    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 3, 4, 2, 0),
                operation_id(5),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("small target did not open")
    };
    let mut pending_id = None;
    let mut clean_flush_satisfied = false;
    for _ in 0..16 {
        let start = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        if let Some(pending) = mount.read_with(cx, |mount, _| mount.test_pending_contribution()) {
            pending_id = Some(pending.entity_id());
        }
        match start {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_) => drive(cx, 1),
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(selection) => {
                assert_eq!(selection, clean_predecessor);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch),
            ) => {
                clean_flush_satisfied = true;
                break;
            }
            other => panic!("unexpected clean predecessor flush: {other:?}"),
        }
    }
    assert!(clean_flush_satisfied);
    assert_eq!(service.selected_identity(), Some(clean_predecessor));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );
    let pending_id = pending_id.unwrap();
    let mut published = None;
    for _ in 0..16 {
        let advance = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        match advance {
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(selection) => {
                assert_eq!(selection, clean_predecessor);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected clean activation advance: {other:?}"),
        }
    }
    let published = published.expect("clean activation did not settle within 16 advances");
    assert_eq!(published.claim(), target_claim);
    assert_eq!(service.selected_identity(), Some(published));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        pending_id
    );
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    let promoted = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let promoted_input = promoted.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| promoted_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        promoted_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "x", None, window, input_cx)
        })
    });
    drive(cx, 32);
    let edited = service.selected_identity().unwrap();
    assert_ne!(
        edited,
        published,
        "promoted target edit did not advance: error={:?}, diagnostics={:?}",
        promoted.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        promoted_input.read_with(cx, |input, _| input.realization_diagnostics())
    );
    assert_eq!(
        promoted.read_with(cx, |composer, _| composer.selection_identity()),
        edited
    );
}

#[gpui::test]
fn multi_page_pending_target_primes_then_promotes_the_exact_unpublished_entity(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-promote", 185);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    seed_activation_published_draft(&fixture, target_thread);
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected_host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        selected_host
            .test_activate(
                &fixture.store,
                activation(selected_thread, 11, 12, 1, 0),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        selected_host,
        storage,
        marker_authority,
    )
    .unwrap();
    let store = Arc::new(store);
    let durable_store = store.clone();
    let service = Arc::new(MainWindowConversationComposerService::new(store, slot));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals.clone(),
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
    let predecessor_id = predecessor.entity_id();
    let mut predecessor_selection = service.selected_identity().unwrap();
    let predecessor_candidate_generation = predecessor_selection
        .binding()
        .candidate()
        .candidate_generation();
    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 21, 22, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(23),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("target host did not open")
    };

    let mut pending_id = None;
    let mut priming_rounds = 0;
    let mut flush_ticket = None;
    let mut predecessor_edited = false;
    let mut edited_predecessor_root = None;
    for _ in 0..64 {
        let start = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        if let Some(pending) = mount.read_with(cx, |mount, _| mount.test_pending_contribution()) {
            pending_id.get_or_insert(pending.entity_id());
            assert_ne!(pending.entity_id(), predecessor_id);
            assert!(pending.read_with(cx, |composer, _| composer.is_pending_target()));
        }
        match start {
            MainWindowConversationComposerMountFlushStart::TargetPriming(current) => {
                assert_eq!(current, receipt);
                priming_rounds += 1;
                let residency = mount
                    .read_with(cx, |mount, mount_cx| {
                        mount.test_activation_residency(mount_cx)
                    })
                    .unwrap();
                assert!(residency.current_text_pages() <= residency.bound().text_pages());
                assert!(residency.current_text_bytes() <= residency.bound().text_bytes());
                assert!(residency.current_objects() <= residency.bound().objects());
                assert!(residency.current_object_bytes() <= residency.bound().object_bytes());
                assert_eq!(service.selected_identity(), Some(predecessor_selection));
                assert_eq!(
                    mount
                        .read_with(cx, |mount, _| mount.contribution())
                        .unwrap()
                        .entity_id(),
                    predecessor_id
                );
                if !predecessor_edited {
                    let predecessor_input =
                        predecessor.read_with(cx, |composer, _| composer.gpui_input());
                    cx.update(|window, app| {
                        predecessor_input.update(app, |input, _| input.focus(window))
                    });
                    cx.update(|window, app| {
                        predecessor_input.update(app, |input, input_cx| {
                            input.replace_and_mark_text_in_range(None, "p", None, window, input_cx)
                        })
                    });
                    drive(cx, 32);
                    let edited_selection = service.selected_identity().unwrap();
                    assert_ne!(edited_selection, predecessor_selection);
                    assert!(
                        edited_selection
                            .binding()
                            .candidate()
                            .candidate_generation()
                            > predecessor_candidate_generation
                    );
                    assert_ne!(
                        edited_selection.binding().root(),
                        predecessor_selection.binding().root()
                    );
                    assert_eq!(
                        edited_selection
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes(),
                        1
                    );
                    assert_eq!(
                        predecessor.read_with(cx, |composer, _| composer.selection_identity()),
                        edited_selection
                    );
                    edited_predecessor_root = Some(edited_selection.binding().root());
                    predecessor_selection = edited_selection;
                    predecessor_edited = true;
                }
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(selection) => {
                assert_eq!(selection, predecessor_selection);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Started { ticket, .. },
            ) => {
                flush_ticket = Some(ticket);
                break;
            }
            start => panic!("unexpected predecessor flush admission: {start:?}"),
        }
    }
    assert!(priming_rounds > 0);
    assert!(predecessor_edited);
    if flush_ticket.is_none() {
        let pending = mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .unwrap();
        let (seeds, error, input) = pending.read_with(cx, |composer, _| {
            (
                composer.test_pending_seed_count(),
                composer.last_error().map(str::to_owned),
                composer.gpui_input(),
            )
        });
        let diagnostics = input.read_with(cx, |input, _| input.realization_diagnostics());
        panic!(
            "pending surface did not settle: seeds={seeds}, error={error:?}, diagnostics={diagnostics:?}"
        );
    }
    assert_eq!(service.test_pending_host_request_id(receipt), Some(20));
    let selected_bounds = cx.debug_bounds("conversation-composer-root").unwrap();
    let pending_bounds = cx
        .debug_bounds("conversation-composer-pending-realization")
        .unwrap();
    assert_eq!(pending_bounds.size, selected_bounds.size);
    assert!(pending_bounds.origin.x >= selected_bounds.right());
    let pending_input = mount
        .read_with(cx, |mount, _| mount.test_pending_contribution())
        .unwrap()
        .read_with(cx, |composer, _| composer.gpui_input());
    assert!(!pending_input.read_with(cx, |input, _| input.is_enabled()));
    assert!(!cx.update(|window, app| pending_input.read(app).focus_handle(app).is_focused(window)));
    let pending_id = pending_id.unwrap();
    let published_at = SyndicTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap(),
    );

    let mut published = None;
    let mut captured = false;
    let mut disposal_captured = false;
    for _ in 0..32 {
        drive(cx, 4);
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(selection) => {
                assert_eq!(selection, predecessor_selection);
            }
            MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::Progress(progress),
            ) => {
                if progress == ComposerHostFlushState::CaptureRequired && !captured {
                    let outcome = mount
                        .update(cx, |mount, _| {
                            mount.capture_flush_publication(
                                predecessor_selection,
                                flush_ticket.unwrap(),
                                assets.clone(),
                                &marker_seals,
                                operation_id(24),
                                None,
                                published_at,
                                &CommandCancellation::new(),
                            )
                        })
                        .unwrap();
                    assert!(
                        matches!(
                            outcome,
                            ComposerHostFlushCapture::Captured(_)
                                | ComposerHostFlushCapture::State(
                                    ComposerHostFlushState::DisposalRequired
                                )
                        ),
                        "unexpected predecessor publication capture: {outcome:?}"
                    );
                    captured = true;
                }
                if progress == ComposerHostFlushState::DisposalRequired && !disposal_captured {
                    predecessor_selection = service.selected_identity().unwrap();
                    assert_eq!(
                        predecessor_selection.binding().root(),
                        edited_predecessor_root.unwrap()
                    );
                    assert_eq!(
                        storage
                            .current_draft(
                                &durable_store,
                                selected_thread,
                                SyndicPointReadLimit::new(65_536).unwrap(),
                            )
                            .unwrap()
                            .unwrap()
                            .draft()
                            .piece_root(),
                        edited_predecessor_root.unwrap()
                    );
                    let outcome = mount
                        .update(cx, |mount, _| {
                            mount.capture_flush_disposal(
                                predecessor_selection,
                                flush_ticket.unwrap(),
                                operation_id(25),
                                &CommandCancellation::new(),
                            )
                        })
                        .unwrap();
                    assert!(
                        matches!(
                            outcome,
                            ComposerHostFlushCapture::State(
                                ComposerHostFlushState::DisposalRequired
                            )
                        ),
                        "unexpected predecessor disposal capture: {outcome:?}"
                    );
                    disposal_captured = true;
                }
            }
            MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::ReconciliationPending,
            ) => {}
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected activation publication: {other:?}"),
        }
    }
    let published = published.expect("activation publication did not settle within 32 advances");
    assert_eq!(service.selected_identity(), Some(published));
    assert_eq!(
        published.binding().logical_extent().logical_utf8_bytes(),
        ACTIVATION_DRAFT_BYTES
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        pending_id
    );
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert_eq!(
        storage
            .current_draft(
                &durable_store,
                selected_thread,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
            .draft()
            .piece_root(),
        edited_predecessor_root.unwrap()
    );
}

#[gpui::test]
fn pending_target_is_noninteractive_and_releases_on_cancel_supersession_and_disposal(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-terminal", 186);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    seed_activation_published_draft(&fixture, target_thread);
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected_host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        selected_host
            .test_activate(
                &fixture.store,
                activation(selected_thread, 31, 32, 1, 0),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        selected_host,
        storage,
        marker_authority,
    )
    .unwrap();
    let durable_store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        durable_store.clone(),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    if selection.binding().presentation_generation().get() == 5 {
                        return Err("test pending composer construction failure".to_owned());
                    }
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
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
    let predecessor_id = predecessor.entity_id();
    let predecessor_selection = service.selected_identity().unwrap();
    let cancellation = CommandCancellation::new();
    let activation_cut = cancellation.clone();
    service.test_arm_activation_after_open_fault(move |_, _| activation_cut.cancel());
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.begin_activation(
                target_claim,
                activation(target_thread, 35, 36, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(37),
                &cancellation,
            ))
            .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    assert!(service.pending_receipt().is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    let MainWindowComposerActivationAdvance::Ready(first) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 41, 42, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(43),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("first target did not open")
    };
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(first, window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::TargetPriming(_)
    ));
    let pending = mount
        .read_with(cx, |mount, _| mount.test_pending_contribution())
        .unwrap();
    let pending_binding = pending.read_with(cx, |composer, _| composer.selection_identity());
    let pending_input = pending.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        pending_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "forbidden", None, window, input_cx)
        })
    });
    drive(cx, 16);
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(service.pending_identity(first), Some(pending_binding));
    assert_ne!(pending.entity_id(), predecessor_id);
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );
    cx.update(|window, app| pending_input.update(app, |input, _| input.focus(window)));
    assert!(!cx.update(|window, app| pending_input.read(app).focus_handle(app).is_focused(window)));
    assert_eq!(
        mount
            .read_with(cx, |mount, mount_cx| mount.surface_snapshot(mount_cx))
            .unwrap()
            .selection,
        predecessor_selection
    );
    assert!(
        mount
            .read_with(cx, |mount, mount_cx| {
                mount.hit_test_composite_viewport(gpui::point(gpui::px(1.), gpui::px(1.)), mount_cx)
            })
            .is_none_or(|hit| hit.selection == predecessor_selection)
    );

    let MainWindowComposerActivationAdvance::Ready(second) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 51, 52, 3, ACTIVATION_DRAFT_BYTES),
                operation_id(53),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("superseding target did not open")
    };
    assert!(service.pending_identity(first).is_none());
    assert_eq!(service.pending_receipt(), Some(second));
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(first, window, mount_cx)
        }))
        .is_err()
    );
    assert_eq!(service.pending_receipt(), Some(second));
    assert!(service.pending_identity(second).is_some());
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );
    assert_eq!(
        mount
            .update(cx, |mount, _| mount.retire_pending(second))
            .unwrap(),
        beryl_app::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert!(service.pending_identity(second).is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );

    let MainWindowComposerActivationAdvance::Ready(failed_priming) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 55, 56, 5, ACTIVATION_DRAFT_BYTES),
                operation_id(57),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("priming-failure target did not open")
    };
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(failed_priming, window, mount_cx)
        }))
        .is_err()
    );
    assert!(service.pending_identity(failed_priming).is_none());
    assert!(service.pending_receipt().is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );

    let MainWindowComposerActivationAdvance::Ready(source_drift) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 71, 72, 6, ACTIVATION_DRAFT_BYTES),
                operation_id(74),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("source-drift target did not open")
    };
    let current = storage
        .current_draft(
            &durable_store,
            target_thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let session = match storage
        .draft_editor_candidate_session(
            &durable_store,
            current.draft().id(),
            DraftEditorCandidateSessionIdV1::from_bytes([71; 16]),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("source-drift candidate was not active: {other:?}"),
    };
    let transaction = composer_base::transaction_for_session(
        storage,
        &durable_store,
        session,
        73,
        vec![DraftPieceReplacementV1::new(
            composer_base::point(0),
            composer_base::point(0),
            vec![DraftPieceV1::Text("d".to_owned())],
        )],
        composer_base::point(1),
    );
    composer_base::run_transaction(storage, &durable_store, &transaction, 1);
    let mut drift_rejected = false;
    let mut drift_error = None;
    for _ in 0..32 {
        if let Err(error) = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_publish(source_drift, window, mount_cx)
            })
        }) {
            drift_rejected = true;
            drift_error = Some(error);
            break;
        }
        drive(cx, 1);
    }
    assert!(
        drift_rejected,
        "source drift was not rejected within 32 advances"
    );
    assert!(
        service.pending_identity(source_drift).is_none(),
        "source drift retained pending identity after {drift_error:?}"
    );
    assert!(service.pending_receipt().is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );

    let MainWindowComposerActivationAdvance::Ready(third) = mount
        .update(cx, |mount, _| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 61, 62, 4, ACTIVATION_DRAFT_BYTES),
                operation_id(63),
                &CommandCancellation::new(),
            )
        })
        .unwrap()
    else {
        panic!("disposal target did not open")
    };
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(third, window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::TargetPriming(_)
    ));
    let _ = cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.begin_disposal(window, mount_cx)
        })
    });
    assert!(service.pending_identity(third).is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
}
