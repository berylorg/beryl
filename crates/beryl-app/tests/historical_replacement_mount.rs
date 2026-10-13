#![cfg(all(target_os = "windows", feature = "test-faults"))]

#[path = "historical_replacement_mount/config.rs"]
mod config;
#[path = "historical_replacement_host/support.rs"]
mod history;
#[path = "syndic_composer_host/support.rs"]
mod host_support;
#[path = "main_window_composer_slot/support.rs"]
mod slot_support;

use beryl_app::main_window::*;
use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId, SyndicItemId};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads};
use gpui::{
    AppContext, Application, AsyncApp, Entity, EntityInputHandler, WindowHandle, WindowOptions,
};
use gpui_text_input::{ClipboardWriteOutcome, InlineObjectGap};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use syndic_storage::{ComposerAtom, ComposerPayload, DraftSubmissionIntent};

fn with_native<R>(
    window: WindowHandle<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
    action: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
) -> R {
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| action(native, app))
            .unwrap()
    })
    .unwrap()
}

async fn settle(
    window: WindowHandle<MainWindowConversationComposer>,
    composer: &Entity<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
) {
    for _ in 0..1000 {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app);
            native.minimize_window();
        });
        let ready = composer
            .read_with(cx, |owner, app| {
                assert_eq!(owner.last_error(), None);
                !owner.test_historical_replacement_state().0
                    && owner.gpui_input().read(app).is_quiescent()
                    && owner
                        .gpui_input()
                        .read(app)
                        .is_surface_current_and_interactive()
                    && owner.test_edit_position_proof_ready(app)
            })
            .unwrap();
        if ready {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    let diagnostic = composer
        .read_with(cx, |owner, app| {
            owner.test_edit_position_proof_diagnostics(app)
        })
        .unwrap();
    panic!("Native historical replacement did not reach coherent quiescence: {diagnostic}");
}

#[test]
fn native_deferred_import_and_marker_undo_preserve_exact_baseline_and_cancel_state() {
    qualify_native(true, false);
}

#[test]
fn native_text_import_reissues_unchanged_position_proof_before_the_first_edit() {
    qualify_native(false, false);
}

#[test]
fn native_unpresented_marker_import_recovers_on_the_same_protected_widget() {
    qualify_native(true, true);
}

fn qualify_native(marker_only: bool, recover_unpresented: bool) {
    eprintln!("historical mount: preparing fixture");
    let prepared = std::thread::Builder::new()
        .name("historical-mount-fixture".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let fixture = slot_support::Fixture::new("native-historical-replacement", 181);
            let claim = fixture.claims().0;
            let window_id = fixture.window_id;
            let thread = fixture.selected_thread;
            let assets = fixture.assets();
            let marker_seals = fixture.marker_seals();
            let marker = SyndicDraftMarkerId::from_bytes([230; 16]);
            let label = ImageLabelOrdinal::new(7).unwrap();
            let proof = marker_only.then(|| {
                let asset = history::publish_asset(&fixture.store, &assets, b"native-marker-image");
                let proof =
                    history::seal_marker_set(&fixture.store, &assets, &[(marker, label)], asset);
                host_support::committed(host_support::execute(
                    &fixture.store,
                    assets.update_owner_heads(
                        assets.revision(&fixture.store).unwrap(),
                        UpdateAssetOwnerHeads::new(Box::new([AssetOwnerHeadUpdate::replace(
                            AssetOwner::SubmittedTurnItem(SyndicItemId::from_bytes([204; 16])),
                            None,
                            Some(proof),
                        )]))
                        .unwrap(),
                    ),
                ));
                proof
            });
            let payload = ComposerPayload::new(vec![if marker_only {
                ComposerAtom::image_marker(marker, label)
            } else {
                ComposerAtom::Text("historical text".into())
            }])
            .unwrap();
            let target = history::seed_source_with_assets(
                &fixture.store,
                &fixture.storage,
                thread,
                &payload,
                proof,
            );
            let faults = fixture.faults.clone();
            let (directory, store, storage) = fixture.into_store();
            let host = history::activate(&store, storage.clone(), thread);
            let slot = MainWindowComposerSlot::new(
                window_id,
                claim,
                host,
                storage.clone(),
                MainWindowComposerMarkerMetadataAuthority::new(assets),
            )
            .unwrap();
            let service = Arc::new(MainWindowConversationComposerService::new(
                store.service_reference(),
                slot,
            ));
            (
                directory,
                store,
                storage,
                service,
                marker_seals,
                target,
                faults,
            )
        })
        .unwrap()
        .join()
        .unwrap();
    eprintln!("historical mount: fixture ready");
    let finished = Arc::new(AtomicBool::new(false));
    let observed = finished.clone();
    Application::new().run(move |app| {
        gpui_text_input::ensure_text_input_bindings(app);
        let (directory, store, storage, service, marker_seals, target, faults) = prepared;
        let selection = service.selected_identity().unwrap();
        let widget = config::widget(selection.binding().range_binding());
        let configuration = MainWindowConversationComposerConfig::new(selection, widget).unwrap();
        let mounted_service = service.clone();
        let window = app
            .open_window(
                WindowOptions {
                    focus: false,
                    show: true,
                    ..Default::default()
                },
                move |window, app| {
                    app.new(|cx| {
                        MainWindowConversationComposer::new(
                            configuration,
                            mounted_service,
                            Box::new(|_, _| ClipboardWriteOutcome::Failed),
                            window,
                            cx,
                        )
                        .unwrap()
                    })
                },
            )
            .unwrap();
        app.spawn(async move |cx| {
            let composer = window.entity(cx).unwrap();
            settle(window, &composer, cx).await;
            eprintln!("historical mount: initial editor ready");
            window
                .update(cx, |owner, native, cx| {
                    owner.test_hold_historical_realization(recover_unpresented);
                    owner
                        .test_start_historical_replacement(target, marker_seals, true, native, cx)
                        .unwrap();
                    assert!(owner.test_historical_replacement_state().0);
                    assert!(owner.test_historical_replacement_state().1.is_none());
                })
                .unwrap();
            eprintln!("historical mount: import requested");
            let (store, storage, service) = if recover_unpresented {
                recover_unpresented_import(window, &composer, cx, store, storage, service, faults)
                    .await
            } else {
                (store, storage, service)
            };
            settle(window, &composer, cx).await;
            eprintln!("historical mount: imported editor ready");
            let input = composer
                .read_with(cx, |owner, _| {
                    let state = owner.test_historical_replacement_state();
                    assert_eq!(state.1.unwrap().target_turn_id(), target.turn_id);
                    assert!(
                        recover_unpresented || state.2 > 0,
                        "A real Busy rebind must settle through native prepaint"
                    );
                    owner.gpui_input()
                })
                .unwrap();
            let (baseline, positions) = input
                .read_with(cx, |input, _| {
                    let surface = input.surface().unwrap();
                    assert_eq!(surface.realized_objects().len(), usize::from(marker_only));
                    let positions = surface.source_selection();
                    if marker_only {
                        assert!(matches!(positions.head.gap, InlineObjectGap::Before(_)));
                    } else {
                        assert_eq!(positions.head.gap, InlineObjectGap::NoObjects);
                        assert_eq!(positions.head.byte_offset.get(), 0);
                    }
                    assert_eq!(positions.anchor, positions.head);
                    assert!(!input.history_frontier().undo_available);
                    (surface.binding(), positions)
                })
                .unwrap();
            with_native(window, cx, |native, app| {
                input.update(app, |input, cx| {
                    input.focus(native);
                    input.replace_text_in_range(None, "🙂", native, cx);
                })
            });
            settle(window, &composer, cx).await;
            input
                .read_with(cx, |input, _| {
                    assert!(input.history_frontier().undo_available)
                })
                .unwrap();
            eprintln!("historical mount: first edited candidate ready");
            with_native(window, cx, |native, app| {
                native.dispatch_action(Box::new(gpui_text_input::Undo), app)
            });
            settle(window, &composer, cx).await;
            input
                .read_with(cx, |input, _| {
                    let surface = input.surface().unwrap();
                    assert_eq!(surface.source_selection(), positions);
                    assert_eq!(surface.realized_objects().len(), usize::from(marker_only));
                    assert_eq!(surface.binding().extent(), baseline.extent());
                    assert!(input.history_frontier().redo_available);
                })
                .unwrap();
            eprintln!("historical mount: Undo baseline ready");
            with_native(window, cx, |native, app| {
                native.dispatch_action(Box::new(gpui_text_input::Redo), app)
            });
            settle(window, &composer, cx).await;
            let before = input
                .read_with(cx, |input, _| {
                    input
                        .export_restoration(Some(input.history_frontier()))
                        .unwrap()
                })
                .unwrap();
            let before_identity = service.selected_identity().unwrap();
            with_native(window, cx, |native, app| {
                native.dispatch_input_for_test(
                    gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse("escape").unwrap(),
                        is_held: false,
                    }),
                    app,
                );
            });
            settle(window, &composer, cx).await;
            input
                .read_with(cx, |input, _| {
                    assert_eq!(
                        input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap(),
                        before
                    );
                })
                .unwrap();
            let after_identity = service.selected_identity().unwrap();
            assert_eq!(
                before_identity.binding().range_binding(),
                after_identity.binding().range_binding()
            );
            assert_eq!(
                before_identity.binding().range_history_frontier(),
                after_identity.binding().range_history_frontier()
            );
            assert!(
                after_identity.binding().candidate().session_generation()
                    > before_identity.binding().candidate().session_generation()
            );
            assert_eq!(
                host_support::current(storage.clone(), &store, target.thread_id)
                    .draft()
                    .submission_intent(),
                DraftSubmissionIntent::Ordinary
            );
            composer
                .read_with(cx, |owner, _| {
                    assert!(owner.test_historical_replacement_state().1.is_none())
                })
                .unwrap();
            with_native(window, cx, |native, _| native.remove_window());
            drop(composer);
            drop(input);
            drop(service);
            drop(storage);
            drop(store);
            drop(directory);
            observed.store(true, Ordering::SeqCst);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(finished.load(Ordering::SeqCst));
}

async fn recover_unpresented_import(
    window: WindowHandle<MainWindowConversationComposer>,
    composer: &Entity<MainWindowConversationComposer>,
    cx: &mut AsyncApp,
    store: beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    service: Arc<MainWindowConversationComposerService>,
    faults: beryl_home_store::test_faults::FaultController,
) -> (
    beryl_home_store::HomeStore,
    syndic_storage::SyndicStorage,
    Arc<MainWindowConversationComposerService>,
) {
    use gpui_text_input::{
        RangePrepublicationCleanupLedger, RangePrepublicationCurrent,
        RangePrepublicationEnvironment, RangeSurfaceCharge,
    };
    let input = composer
        .read_with(cx, |owner, _| owner.gpui_input())
        .unwrap();
    let prior = composer
        .read_with(cx, |owner, _| owner.selection_identity())
        .unwrap();
    let mut durable_ready = false;
    for _ in 0..1000 {
        with_native(window, cx, |native, app| {
            native.draw_and_present_for_test(app)
        });
        durable_ready = composer
            .read_with(cx, |owner, app| {
                owner.last_error().is_none()
                    && !owner.test_has_active_flight()
                    && service
                        .selected_identity()
                        .is_some_and(|selected| selected.binding().root() != prior.binding().root())
                    && owner.gpui_input().read(app).is_quiescent()
            })
            .unwrap();
        if durable_ready {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(
        durable_ready,
        "original import did not reach durable unpresented cut"
    );
    assert_eq!(
        input
            .read_with(cx, |input, _| input.surface().unwrap().binding())
            .unwrap(),
        prior.binding().range_binding()
    );
    faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let ticket = composer
        .update(cx, |owner, cx| owner.begin_failed_resident(cx))
        .unwrap()
        .unwrap();
    let capture = composer
        .update(cx, |owner, cx| owner.capture_failed_resident(ticket, cx))
        .unwrap()
        .unwrap()
        .unwrap();
    let predecessor = capture.restoration();
    assert_eq!(predecessor.binding, prior.binding().range_binding());
    assert_ne!(capture.selection(), prior);
    let mut resources = composer
        .update(cx, |owner, cx| {
            owner.detach_failed_resident_resources(&capture, cx)
        })
        .unwrap()
        .unwrap();
    let owned_service = resources.service.take().unwrap();
    drop((resources, service, storage));
    let retired = owned_service
        .retire_failed_resident()
        .ok()
        .expect("failed historical operation retained service aliases");
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let source = MainWindowFailedResidentCandidateSource::new(
        &mut candidate,
        retired,
        storage.clone(),
        &state,
        predecessor,
        None,
    )
    .unwrap_or_else(|(_, error)| panic!("historical candidate source refused: {error}"));
    let mut preparation = MainWindowFailedResidentPreparation::new(candidate, source, capture)
        .ok()
        .unwrap();
    let (seed, selected) = preparation.authenticated_source().unwrap();
    assert!(matches!(seed.caret.gap, InlineObjectGap::Before(_)));
    assert_eq!(seed.selection.anchor, seed.caret);
    let mut configuration = config::widget(seed.binding);
    let layout = input
        .read_with(cx, |input, _| input.resident_layout_snapshot())
        .unwrap();
    configuration.layout = layout.layout;
    configuration.style = layout.style;
    configuration.viewport_extent = layout.viewport_extent;
    configuration.presentation_generation = gpui_text_input::PresentationGeneration::new(
        selected.binding().presentation_generation().get(),
    );
    let available = RangeSurfaceCharge {
        bytes: configuration.limits.max_surface_bytes,
        items: configuration.limits.max_surface_items,
    };
    let capacity = RangeSurfaceCharge {
        bytes: available.bytes * 2,
        items: available.items * 2,
    };
    with_native(window, cx, |native, app| {
        let ledger = RangePrepublicationCleanupLedger::new(native.text_system(), 64).unwrap();
        let environment =
            RangePrepublicationEnvironment::new(391, configuration, native.text_system(), ledger)
                .unwrap();
        preparation
            .admit(input.read(app), environment, capacity)
            .unwrap();
    });
    let mut ready = false;
    for _ in 0..1000 {
        let progress = with_native(window, cx, |native, app| {
            input.update(app, |input, cx| {
                preparation.advance(input, native.text_system(), cx, |_| {})
            })
        })
        .unwrap();
        if progress == MainWindowComposerRecoveryProgress::Ready {
            ready = true;
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(
        ready,
        "historical changed-source prepublication did not become ready"
    );
    let current = RangePrepublicationCurrent {
        binding: seed.binding,
        history: seed.history,
        available_capacity: available,
    };
    let (candidate, service, adoption) = with_native(window, cx, |native, app| {
        composer.update(app, |owner, cx| {
            preparation.adopt_resident(owner, current, native, cx)
        })
    })
    .unwrap();
    assert_eq!(
        composer
            .read_with(cx, |owner, _| owner.gpui_input().entity_id())
            .unwrap(),
        input.entity_id()
    );
    assert_eq!(
        input
            .read_with(cx, |input, _| input
                .export_restoration(seed.history)
                .unwrap())
            .unwrap(),
        seed
    );
    assert!(
        input
            .read_with(cx, |input, _| input
                .resident_protection_is_current(adoption.protection()))
            .unwrap()
    );
    let store = candidate.publish().unwrap();
    composer
        .update(cx, |owner, cx| {
            owner.publish_failed_resident(&adoption, &store, cx)?;
            owner.test_hold_historical_realization(false);
            Ok::<_, String>(())
        })
        .unwrap()
        .unwrap();
    drop((adoption, state, preparation));
    (store, storage, service)
}
