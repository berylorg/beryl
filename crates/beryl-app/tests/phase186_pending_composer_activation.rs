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
    AppContext, Entity, EntityInputHandler, Focusable, IntoElement, ParentElement, Render, Styled,
    div, px,
};
use gpui_text_input::ensure_text_input_bindings;
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftPieceReplacementV1, DraftPieceV1, SyndicPointReadLimit, SyndicTimestamp,
};

use support::{
    ACTIVATION_DRAFT_BYTES, activation, activation_with_marker_proof, drive, fixture::Fixture,
    fixture::operation_id, seed_activation_published_draft, seed_activation_published_draft_chunks,
    widget_config,
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

struct StableMountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for StableMountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .w(px(320.))
            .children(self.mount.read(cx).contribution())
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
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 3, 4, 2, 0),
                operation_id(5),
                &CommandCancellation::new(),
                mount_cx,
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
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(current) => {
                assert_eq!(current, receipt);
                drive(cx, 1);
            }
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
    drive(cx, 16);
    let promoted_input = promoted.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| promoted_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        promoted_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "x", None, window, input_cx)
        })
    });
    let mut edited = None;
    for _ in 0..128 {
        drive(cx, 1);
        let current = service.selected_identity().unwrap();
        if current != published {
            edited = Some(current);
            break;
        }
    }
    let edited = edited.unwrap_or(published);
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
fn multi_page_pending_target_promotes_the_exact_unpublished_entity(cx: &mut gpui::TestAppContext) {
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
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 21, 22, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(23),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("target host did not open")
    };

    let predecessor_input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| predecessor_input.update(app, |input, _| input.focus(window)));
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
    let edited_predecessor_root = edited_selection.binding().root();
    predecessor_selection = edited_selection;

    let mut pending_id = None;
    let mut flush_ticket = None;
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
                let residency = mount
                    .read_with(cx, |mount, mount_cx| {
                        mount.test_activation_residency(mount_cx)
                    })
                    .unwrap();
                assert!(residency.current_text_pages() <= residency.bound().text_pages());
                assert!(residency.current_text_bytes() <= residency.bound().text_bytes());
                assert!(residency.current_object_pages() <= residency.bound().object_pages());
                assert!(residency.current_objects() <= residency.bound().objects());
                assert!(residency.current_object_bytes() <= residency.bound().object_bytes());
                assert!(residency.current_owned_bytes() <= residency.bound().owned_bytes());
                assert!(residency.current_owned_items() <= residency.bound().owned_items());
                assert_eq!(service.selected_identity(), Some(predecessor_selection));
                assert_eq!(
                    mount
                        .read_with(cx, |mount, _| mount.contribution())
                        .unwrap()
                        .entity_id(),
                    predecessor_id
                );
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
    assert!(
        service
            .test_pending_host_request_id(receipt)
            .is_some_and(|request_id| request_id > 16),
        "typed seed miss did not continue through ordinary pending dispatch"
    );
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
                        edited_predecessor_root
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
                        edited_predecessor_root
                    );
                    let outcome = mount
                        .update(cx, |mount, _| {
                            mount.capture_flush_disposal(
                                predecessor_selection,
                                flush_ticket.expect("dirty predecessor flush ticket disappeared"),
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
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(current) => {
                assert_eq!(current, receipt);
            }
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
    assert!(
        mount
            .read_with(cx, |mount, mount_cx| mount
                .test_activation_residency(mount_cx))
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
        edited_predecessor_root
    );
}

#[gpui::test]
fn prior_flush_failure_detaches_pending_presentation_while_retirement_is_pending(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-final-drift", 188);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected_host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        selected_host
            .test_activate(
                &fixture.store,
                activation(selected_thread, 91, 92, 1, 0),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    selected_host.test_arm_publication_before_execute_fault(move |store, storage| {
        composer_base::bump_home_revision(storage, store, 97);
    });
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let durable_store = Arc::new(store);
    let mut slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        selected_host,
        storage,
        marker_authority,
    )
    .unwrap();
    slot.test_arm_abandonment_before_execute_fault(move |store, storage| {
        composer_base::bump_home_revision(storage, store, 98);
    });
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
    drive(cx, 24);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let predecessor = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let predecessor_id = predecessor.entity_id();
    let original_predecessor = service.selected_identity().unwrap();
    let predecessor_input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| predecessor_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        predecessor_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "a", None, window, input_cx)
        })
    });
    let mut predecessor_selection = original_predecessor;
    for _ in 0..128 {
        drive(cx, 1);
        predecessor_selection = service.selected_identity().unwrap();
        if predecessor_selection != original_predecessor {
            break;
        }
    }
    assert_ne!(predecessor_selection, original_predecessor);
    let (predecessor_source_selection, predecessor_composition) =
        predecessor_input.read_with(cx, |input, _| {
            let surface = input.surface().unwrap();
            (surface.source_selection(), surface.composition())
        });
    let durable_selected_root = storage
        .current_draft(
            &durable_store,
            selected_thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .draft()
        .piece_root();

    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 93, 94, 2, 0),
                operation_id(95),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("post-flush drift target did not open")
    };
    let mut flush_ticket = None;
    for _ in 0..32 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_) => drive(cx, 1),
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
            other => panic!("unexpected post-flush drift admission: {other:?}"),
        }
    }
    let flush_ticket = flush_ticket.expect("dirty predecessor did not start a switch flush");
    let published_at = SyndicTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap(),
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );

    assert!(matches!(
        mount
            .update(cx, |mount, _| {
                mount.capture_flush_publication(
                    predecessor_selection,
                    flush_ticket,
                    assets.clone(),
                    &marker_seals,
                    operation_id(96),
                    None,
                    published_at,
                    &CommandCancellation::new(),
                )
            })
            .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    assert!(predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(
        cx.debug_bounds("conversation-composer-pending-realization")
            .is_some()
    );
    let failure_advance = cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.advance_publish(receipt, window, mount_cx)
        })
    });
    assert!(
        matches!(
            failure_advance,
            Ok(MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::PriorFlushFailed
            ))
        ),
        "unexpected prior flush failure advance: {failure_advance:?}"
    );
    assert_eq!(service.pending_receipt(), Some(receipt));
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(!predecessor.read_with(cx, |composer, app| {
        composer.test_has_pending_render_child(app)
    }));
    assert!(
        mount
            .read_with(cx, |mount, mount_cx| mount
                .test_activation_residency(mount_cx))
            .is_none()
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert_eq!(service.pending_receipt(), Some(receipt));
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(
        mount
            .read_with(cx, |mount, mount_cx| mount
                .test_activation_residency(mount_cx))
            .is_none()
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );
    assert!(predecessor_input.read_with(cx, |input, _| input.is_enabled()));
    predecessor_input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert_eq!(surface.source_selection(), predecessor_source_selection);
        assert_eq!(surface.composition(), predecessor_composition);
    });
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
        durable_selected_root
    );
    assert_eq!(
        predecessor.read_with(cx, |composer, _| composer.selection_identity()),
        predecessor_selection
    );
}

#[gpui::test]
fn predispatch_pending_flight_loss_settles_custody_and_keeps_promoted_editor_usable(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-predispatch", 188);
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
                activation(selected_thread, 101, 102, 1, 0),
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
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    let mut widget = widget_config(
                        selection.binding().range_binding(),
                        selection.binding().presentation_generation(),
                    );
                    widget.viewport_extent = px(64.);
                    widget.overscan = px(0.);
                    MainWindowConversationComposerConfig::new(selection, widget)
                        .map_err(|error| error.to_string())
                }),
                marker_seals,
                window,
                mount_cx,
            )
            .unwrap()
        });
        StableMountRoot { mount }
    });
    drive(cx, 24);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 103, 104, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(105),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("predispatch target did not open")
    };
    let mut pending = None;
    let mut admitted = false;
    for _ in 0..64 {
        let started = match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_) => false,
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(_) => false,
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch),
            ) => true,
            other => panic!("unexpected predispatch priming advance: {other:?}"),
        };
        pending = mount.read_with(cx, |mount, _| mount.test_pending_contribution());
        if started {
            admitted = true;
            break;
        }
        drive(cx, 1);
        if pending.as_ref().is_some_and(|pending| {
            pending.read_with(cx, |pending, app| pending.pending_surface_ready(app))
        }) {
            continue;
        }
    }
    assert!(
        admitted,
        "predispatch pending target was not admitted: diagnostics={:?}",
        pending
            .as_ref()
            .map(|pending| pending
                .read_with(cx, |pending, app| pending.realization_diagnostics(app)))
    );
    let pending = pending
        .or_else(|| mount.read_with(cx, |mount, _| mount.test_pending_contribution()))
        .unwrap();
    let pending_id = pending.entity_id();
    let release = service.test_block_next_pending_dispatch();
    let pending_input = pending.read_with(cx, |composer, _| composer.gpui_input());
    pending_input.update(cx, |input, input_cx| {
        input
            .platform_text_for_range(
                0..usize::try_from(ACTIVATION_DRAFT_BYTES).unwrap(),
                input_cx,
            )
            .unwrap()
    });
    for _ in 0..32 {
        if release.is_blocked() {
            break;
        }
        drive(cx, 1);
    }
    assert!(
        release.is_blocked(),
        "pending dispatch gate was not entered"
    );
    assert!(pending.read_with(cx, |composer, _| composer.test_has_active_flight()));
    assert!(pending.read_with(cx, |composer, app| composer.pending_surface_ready(app)));

    let mut published = None;
    for _ in 0..24 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(current) => {
                assert_eq!(current, receipt);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(_) => {
                drive(cx, 1)
            }
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected predispatch publication: {other:?}"),
        }
    }
    let published = published.expect("predispatch pending entity was not promoted");
    assert_eq!(service.selected_identity(), Some(published));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        pending_id
    );

    release.release();
    for _ in 0..64 {
        drive(cx, 1);
        if !pending.read_with(cx, |composer, _| composer.test_has_active_flight()) {
            break;
        }
    }
    assert!(!pending.read_with(cx, |composer, _| composer.test_has_active_flight()));
    assert!(pending.read_with(cx, |composer, _| composer.last_error().is_none()));
    let settled = pending.read_with(cx, |composer, app| composer.realization_diagnostics(app));
    assert_eq!(settled.current.dispatched_page_requests, 0);
    assert_eq!(settled.current.dispatched_object_requests, 0);
    assert_eq!(settled.current.response_custody_count, 0);
    assert_eq!(settled.current.response_processing_bytes, 0);
    assert_eq!(settled.current.response_processing_items, 0);
    assert_eq!(settled.current.deferred_response_bytes, 0);
    assert_eq!(settled.current.deferred_response_items, 0);

    let promoted_input = pending.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| promoted_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        promoted_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "z", None, window, input_cx)
        })
    });
    let mut edited = published;
    for _ in 0..128 {
        drive(cx, 1);
        edited = service.selected_identity().unwrap();
        if edited != published {
            break;
        }
    }
    assert_ne!(edited, published);
    drive(cx, 16);
    assert!(pending.read_with(cx, |composer, _| composer.last_error().is_none()));
    assert!(promoted_input.read_with(cx, |input, _| input.is_enabled()));
}

#[gpui::test]
fn current_predecessor_release_does_not_wait_for_its_remaining_sparse_index(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase187-predecessor-index-release", 189);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let predecessor_extent = seed_activation_published_draft_chunks(&fixture, selected_thread, 32);
    let (selected_claim, target_claim) = fixture.claims();
    let mut selected_host = SyndicComposerHost::new(fixture.storage);
    assert!(matches!(
        selected_host
            .test_activate(
                &fixture.store,
                activation(selected_thread, 111, 112, 1, predecessor_extent),
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
    let predecessor_selection = slot.selected_identity().unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    let mut widget = widget_config(
                        selection.binding().range_binding(),
                        selection.binding().presentation_generation(),
                    );
                    widget.viewport_extent = px(64.);
                    widget.overscan = px(0.);
                    MainWindowConversationComposerConfig::new(selection, widget)
                        .map_err(|error| error.to_string())
                }),
                marker_seals,
                window,
                mount_cx,
            )
            .unwrap()
        });
        StableMountRoot { mount }
    });
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let predecessor = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let predecessor_id = predecessor.entity_id();
    let predecessor_input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    let mut relayout = widget_config(
        predecessor_selection.binding().range_binding(),
        predecessor_selection.binding().presentation_generation(),
    );
    relayout.layout.wrap_width = px(160.);
    predecessor_input
        .update(cx, |input, input_cx| {
            input.set_layout(relayout.layout, relayout.style, input_cx)
        })
        .unwrap();
    let mut unfinished_surface_observed = false;
    for _ in 0..512 {
        unfinished_surface_observed = predecessor_input.read_with(cx, |input, _| {
            input.is_surface_current_and_interactive()
                && !input.is_quiescent()
                && input.surface().is_some_and(|surface| {
                    surface.quality() == gpui_text_input::GeometryQuality::Estimated
                })
        });
        if unfinished_surface_observed {
            break;
        }
        let _ = cx.executor().tick();
        cx.update(|window, app| window.draw(app).clear());
    }
    assert!(
        unfinished_surface_observed,
        "selected predecessor did not expose a current estimated surface before its sparse index finished: error={:?}, diagnostics={:?}",
        predecessor.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        predecessor_input.read_with(cx, |input, _| input.realization_diagnostics())
    );
    assert_eq!(service.selected_identity(), Some(predecessor_selection));

    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 113, 114, 2, 0),
                operation_id(115),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("small successor target did not open")
    };
    let mut successor_id = None;
    let mut admitted = false;
    let mut admission_advances = 0;
    for _ in 0..16 {
        admission_advances += 1;
        let start = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        successor_id = successor_id.or_else(|| {
            mount
                .read_with(cx, |mount, _| mount.test_pending_contribution())
                .map(|successor| successor.entity_id())
        });
        match start {
            MainWindowConversationComposerMountFlushStart::TargetPriming(current) => {
                assert_eq!(current, receipt);
                let _ = cx.executor().tick();
                cx.update(|window, app| window.draw(app).clear());
            }
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(selection) => {
                assert_eq!(selection, predecessor_selection);
                let _ = cx.executor().tick();
                cx.update(|window, app| window.draw(app).clear());
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch),
            ) => {
                admitted = true;
                break;
            }
            other => panic!("unexpected sparse-index successor admission: {other:?}"),
        }
    }
    assert!(
        admitted,
        "successor waited for the predecessor's remaining sparse index: error={:?}, diagnostics={:?}",
        predecessor.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        predecessor_input.read_with(cx, |input, _| input.realization_diagnostics())
    );
    assert!(admission_advances <= 16);
    predecessor_input.read_with(cx, |input, _| {
        assert!(!input.is_quiescent());
        assert_eq!(
            input.surface().unwrap().quality(),
            gpui_text_input::GeometryQuality::Estimated
        );
    });
    assert!(predecessor.read_with(cx, |composer, _| composer.last_error().is_none()));

    let mut published = None;
    let mut publication_advances = 0;
    for _ in 0..16 {
        publication_advances += 1;
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(current) => {
                assert_eq!(current, receipt);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(selection) => {
                assert_eq!(selection, predecessor_selection);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected sparse-index successor publication: {other:?}"),
        }
    }
    let published =
        published.expect("successor did not publish before the bounded release advance limit");
    assert!(publication_advances <= 16);
    assert_eq!(published.claim(), target_claim);
    assert_eq!(service.selected_identity(), Some(published));
    assert_ne!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        successor_id.unwrap()
    );
    assert!(predecessor.read_with(cx, |composer, _| composer.last_error().is_none()));
    predecessor_input.read_with(cx, |input, _| {
        assert!(input.surface().is_none());
        assert!(input.is_quiescent());
    });
}

#[gpui::test]
fn promoted_pending_flight_failure_settles_the_exact_widget_request(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase186-pending-flight-failure", 187);
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
                activation(selected_thread, 81, 82, 1, 0),
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
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    let mut widget = widget_config(
                        selection.binding().range_binding(),
                        selection.binding().presentation_generation(),
                    );
                    widget.viewport_extent = px(64.);
                    widget.overscan = px(0.);
                    MainWindowConversationComposerConfig::new(selection, widget)
                        .map_err(|error| error.to_string())
                }),
                marker_seals,
                window,
                mount_cx,
            )
            .unwrap()
        });
        StableMountRoot { mount }
    });
    drive(cx, 24);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 83, 84, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(85),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("delayed-failure target did not open")
    };
    let mut pending = None;
    let mut admitted = false;
    for _ in 0..64 {
        let started = match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_) => false,
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(_) => false,
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch),
            ) => true,
            other => panic!("unexpected delayed-flight flush: {other:?}"),
        };
        pending = mount.read_with(cx, |mount, _| mount.test_pending_contribution());
        if started {
            admitted = true;
            break;
        }
        drive(cx, 1);
        if pending.as_ref().is_some_and(|pending| {
            pending.read_with(cx, |pending, app| pending.pending_surface_ready(app))
        }) {
            continue;
        }
    }
    assert!(
        admitted,
        "pending target was not admitted before the delayed flight: diagnostics={:?}",
        pending
            .as_ref()
            .map(|pending| pending
                .read_with(cx, |pending, app| pending.realization_diagnostics(app)))
    );
    let pending = pending
        .or_else(|| mount.read_with(cx, |mount, _| mount.test_pending_contribution()))
        .unwrap();
    let pending_id = pending.entity_id();
    let release = service.test_block_next_pending_completion();
    let pending_input = pending.read_with(cx, |composer, _| composer.gpui_input());
    pending_input.update(cx, |input, input_cx| {
        input
            .platform_text_for_range(
                0..usize::try_from(ACTIVATION_DRAFT_BYTES).unwrap(),
                input_cx,
            )
            .unwrap()
    });
    for _ in 0..32 {
        if release.is_blocked() {
            break;
        }
        drive(cx, 1);
    }
    assert!(
        release.is_blocked(),
        "pending flight did not enter its completion gate"
    );
    let mut published = None;
    for _ in 0..24 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(current) => {
                assert_eq!(current, receipt);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(_) => {
                drive(cx, 1)
            }
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected delayed-flight publication: {other:?}"),
        }
    }
    let published = published.unwrap_or_else(|| {
        let (ready, error, diagnostics) = pending.read_with(cx, |composer, app| {
            (
                composer.pending_surface_ready(app),
                composer.last_error().map(str::to_owned),
                composer.realization_diagnostics(app),
            )
        });
        panic!(
            "pending entity was not promoted around its delayed flight: ready={ready}, error={error:?}, diagnostics={diagnostics:?}"
        )
    });
    assert_eq!(service.selected_identity(), Some(published));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        pending_id
    );
    release.release();
    drive(cx, 16);
    let promoted = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_eq!(promoted.entity_id(), pending_id);
    assert_eq!(
        promoted.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        Some("pending composer dispatched completion failed".to_owned())
    );
    let diagnostics = promoted.read_with(cx, |composer, app| composer.realization_diagnostics(app));
    assert_eq!(diagnostics.current.dispatched_page_requests, 0);
    assert_eq!(diagnostics.current.dispatched_object_requests, 0);
    assert_eq!(diagnostics.current.response_custody_count, 0);
    assert_eq!(diagnostics.current.response_processing_bytes, 0);
    assert_eq!(diagnostics.current.response_processing_items, 0);
    assert_eq!(diagnostics.current.deferred_response_bytes, 0);
    assert_eq!(diagnostics.current.deferred_response_items, 0);
}

#[gpui::test]
fn pending_target_releases_on_cancel_supersession_and_disposal(cx: &mut gpui::TestAppContext) {
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
    service.test_append_impossible_pending_initial_response();
    let MainWindowComposerActivationAdvance::Ready(malformed) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation_with_marker_proof(target_thread, 33, 34, 8, ACTIVATION_DRAFT_BYTES),
                operation_id(35),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("malformed-seed target did not open")
    };
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(malformed, window, mount_cx)
        }))
        .is_err()
    );
    assert!(service.pending_receipt().is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(!predecessor.read_with(cx, |composer, app| {
        composer.test_has_pending_render_child(app)
    }));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
    cx.refresh().unwrap();
    drive(cx, 1);
    assert!(
        cx.debug_bounds("conversation-composer-pending-realization")
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
    let cancellation = CommandCancellation::new();
    let activation_cut = cancellation.clone();
    service.test_arm_activation_after_open_fault(move |_, _| activation_cut.cancel());
    assert!(matches!(
        mount
            .update(cx, |mount, mount_cx| mount.begin_activation(
                target_claim,
                activation(target_thread, 35, 36, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(37),
                &cancellation,
                mount_cx,
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
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 41, 42, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(43),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("first target did not open")
    };
    let MainWindowComposerActivationAdvance::Ready(second) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 51, 52, 3, ACTIVATION_DRAFT_BYTES),
                operation_id(53),
                &CommandCancellation::new(),
                mount_cx,
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
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(second, window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::TargetPriming(current) if current == second
    ));
    assert!(predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_some()
    );
    cx.refresh().unwrap();
    drive(cx, 1);
    assert!(
        cx.debug_bounds("conversation-composer-pending-realization")
            .is_some()
    );
    assert_eq!(
        mount
            .update(cx, |mount, mount_cx| mount.retire_pending(second, mount_cx))
            .unwrap(),
        beryl_app::main_window::MainWindowComposerRetirementAdvance::Retired
    );
    assert!(service.pending_identity(second).is_none());
    assert!(
        mount
            .read_with(cx, |mount, _| mount.test_pending_contribution())
            .is_none()
    );
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(!predecessor.read_with(cx, |composer, app| {
        composer.test_has_pending_render_child(app)
    }));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );

    let MainWindowComposerActivationAdvance::Ready(failed_priming) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 55, 56, 5, ACTIVATION_DRAFT_BYTES),
                operation_id(57),
                &CommandCancellation::new(),
                mount_cx,
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
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));

    let MainWindowComposerActivationAdvance::Ready(source_drift) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 71, 72, 6, ACTIVATION_DRAFT_BYTES),
                operation_id(74),
                &CommandCancellation::new(),
                mount_cx,
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
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));

    let MainWindowComposerActivationAdvance::Ready(third) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 61, 62, 4, ACTIVATION_DRAFT_BYTES),
                operation_id(63),
                &CommandCancellation::new(),
                mount_cx,
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
        MainWindowConversationComposerMountFlushStart::TargetPriming(current) if current == third
    ));
    assert!(predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
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
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
}

#[gpui::test]
fn primed_seed_retarget_uses_live_queue_then_final_target_publishes(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase188-single-pending-authority", 190);
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
                activation(selected_thread, 81, 82, 1, 0),
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
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
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
        StableMountRoot { mount }
    });
    drive(cx, 32);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let predecessor = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let predecessor_id = predecessor.entity_id();
    let predecessor_selection = service.selected_identity().unwrap();
    let predecessor_surface = predecessor
        .read_with(cx, |composer, app| composer.surface_snapshot(app))
        .unwrap();

    let MainWindowComposerActivationAdvance::Ready(first_receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 83, 84, 2, ACTIVATION_DRAFT_BYTES),
                operation_id(85),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("first pending target did not open")
    };
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(first_receipt, window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::TargetPriming(first)
            if first == first_receipt
    ));
    let first_pending = mount
        .read_with(cx, |mount, _| mount.test_pending_contribution())
        .unwrap();
    assert!(predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_some()
    );
    assert!(first_pending.read_with(cx, |composer, _| composer.test_pending_seed_count()) > 0);
    cx.update(|window, app| {
        first_pending
            .update(app, |composer, composer_cx| {
                composer.request_absolute_scroll(px(16_000.), window, composer_cx)
            })
            .unwrap()
    });
    drive(cx, 64);
    assert!(first_pending.read_with(cx, |composer, _| composer.last_error().is_none()));
    assert!(
        service
            .test_pending_host_request_id(first_receipt)
            .is_some_and(|request_id| request_id > 16),
        "retargeted widget demand did not continue through ordinary pending dispatch"
    );

    let final_request = activation(target_thread, 87, 88, 3, ACTIVATION_DRAFT_BYTES);
    let mut final_receipt = None;
    for _ in 0..128 {
        match mount.update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                final_request.clone(),
                operation_id(89),
                &CommandCancellation::new(),
                mount_cx,
            )
        }) {
            Ok(MainWindowComposerActivationAdvance::Ready(receipt)) => {
                final_receipt = Some(receipt);
                break;
            }
            Err(_) => drive(cx, 1),
            Ok(other) => panic!("unexpected final activation outcome: {other:?}"),
        }
    }
    let final_receipt = final_receipt.expect("primed target did not retire within its bound");
    assert!(service.pending_identity(first_receipt).is_none());
    assert_eq!(service.pending_receipt(), Some(final_receipt));
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_publish(first_receipt, window, mount_cx)
        }))
        .is_err()
    );
    assert_eq!(service.pending_receipt(), Some(final_receipt));
    assert_eq!(service.selected_identity(), Some(predecessor_selection));
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
    assert_eq!(
        predecessor.read_with(cx, |composer, app| composer.surface_snapshot(app)),
        Some(predecessor_surface)
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        predecessor_id
    );

    let mut flush_started = false;
    for _ in 0..128 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(final_receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountFlushStart::TargetPriming(receipt) => {
                assert_eq!(receipt, final_receipt);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::WidgetFencePending(selection) => {
                assert_eq!(selection, predecessor_selection);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch),
            ) => {
                flush_started = true;
                break;
            }
            other => panic!("unexpected final flush start: {other:?}"),
        }
    }
    assert!(flush_started);
    let pending_id = mount
        .read_with(cx, |mount, _| mount.test_pending_contribution())
        .unwrap()
        .entity_id();
    let mut published = None;
    for _ in 0..128 {
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(final_receipt, window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(receipt) => {
                assert_eq!(receipt, final_receipt);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(selection) => {
                assert_eq!(selection, predecessor_selection);
                drive(cx, 1);
            }
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                published = Some(selection);
                break;
            }
            other => panic!("unexpected final publication advance: {other:?}"),
        }
    }
    let published = published.expect("final target did not publish within its bound");
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
    assert!(
        mount
            .read_with(cx, |mount, app| mount.test_activation_residency(app))
            .is_none()
    );
    assert!(!predecessor.read_with(cx, |composer, _| composer.test_has_pending_realizer()));
}
