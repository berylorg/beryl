use super::*;
use crate::main_window::*;
use crate::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore, test_faults::FaultPoint};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use gpui::{AppContext, EntityInputHandler, TestAppContext};
use gpui_text_input::*;
use std::num::NonZeroUsize;
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[path = "../pending_composer_activation/support.rs"]
mod composer_support;
#[path = "../main_window_shell/support.rs"]
mod home_support;
#[path = "../initial_composer/support.rs"]
mod support;
use support::*;

#[gpui::test]
fn shell_recovery_rebinds_selection_and_draft_without_reopening(cx: &mut TestAppContext) {
    run(cx, false, false);
}

#[gpui::test]
fn shell_recovery_preserves_custody_after_stale_draft_and_capacity_refusal(
    cx: &mut TestAppContext,
) {
    run(cx, true, false);
}

#[gpui::test]
fn retained_drafts_adopt_shell_and_invalidate_readiness(cx: &mut TestAppContext) {
    run(cx, false, true);
}

#[gpui::test]
fn retained_drafts_preserve_custody_after_refused_adoption(cx: &mut TestAppContext) {
    run(cx, true, true);
}

#[gpui::test]
fn retained_mount_settlement_refusal_preserves_selected_tickets(cx: &mut TestAppContext) {
    run_with_settlement(cx, false, true, true);
}

fn drive(shell: &MainWindowShell, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update(|app| {
        app.update_window(shell.window().into(), |_, window, app| {
            window.draw(app).clear()
        })
        .unwrap();
    });
}

fn run(cx: &mut TestAppContext, refuse: bool, aggregate: bool) {
    run_with_settlement(cx, refuse, aggregate, false);
}

fn run_with_settlement(
    cx: &mut TestAppContext,
    refuse: bool,
    aggregate: bool,
    settle_mounts: bool,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, prepared) = home_support::join(
        home_support::worker(|| {
            let fixture = Fixture::new(151);
            let mut initial = fixture.begin(152);
            initial.advance(&CommandCancellation::new()).unwrap();
            let prepared = prepared_shell(&fixture, initial);
            (fixture, prepared)
        }),
        cx,
    );
    let mut shell = cx.update(|app| {
        let appearance = GpuiAppearanceWindowSet::new(
            prepared.appearance().clone(),
            NonZeroUsize::new(4).unwrap(),
            app,
        );
        GpuiMainWindowShellHost::new(app, appearance)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell construction"))
    });
    for _ in 0..32 {
        drive(&shell, cx);
    }
    cx.update(|app| shell.publish(app)).unwrap();
    let mount = shell
        .window()
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let resident = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = resident.read_with(cx, |resident, _| resident.gpui_input());
    let interval =
        crate::composer_host::ComposerHostAutosaveInterval::new(if aggregate { 17 } else { 30 })
            .unwrap();
    let mut draft = shell
        .window()
        .update(cx, |root, window, cx| {
            if aggregate {
                mount.update(cx, |mount, cx| {
                    mount
                        .publish_autosave_interval(7, interval, window, cx)
                        .unwrap();
                });
            }
            input.update(cx, |input, cx| {
                input.focus(window);
                input.replace_text_in_range(None, "preserved shell draft", window, cx);
            });
            root.set_shutdown_interaction_gated(true, cx).unwrap();
            root.begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    let old_close = draft.test_ticket().unwrap();
    let mut ready = false;
    for _ in 0..512 {
        drive(&shell, cx);
        ready = shell
            .window()
            .update(cx, |root, window, cx| {
                root.advance_shutdown_draft(&draft, window, cx).unwrap()
                    == MainWindowShutdownDraftAdvance::Resident(
                        MainWindowConversationComposerCloseAdvance::Ready,
                    )
                    && input.read(cx).is_quiescent()
            })
            .unwrap();
        if ready {
            break;
        }
    }
    assert!(ready);
    let original = resident.read_with(cx, |resident, _| resident.selection_identity());
    let Fixture {
        directory,
        store,
        state,
        storage,
        service,
        process,
        faults,
        ..
    } = fixture;
    drop((state, storage, service));
    let store = home_support::join(
        home_support::worker(move || {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            store
        }),
        cx,
    );
    let retired = shell
        .window()
        .update(cx, |root, _, cx| {
            assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
            mount
                .update(cx, |mount, cx| {
                    mount.take_interrupted_exit_retirement(old_close, cx)
                })
                .unwrap()
                .unwrap()
        })
        .unwrap();
    let (seed, protection) = resident.read_with(cx, |resident, _| {
        let snapshot = resident.recovery_snapshot().unwrap();
        (*snapshot.restoration(), snapshot.protection())
    });
    let (candidate, mut adapters, state, storage, fresh_appearance) = home_support::join(
        home_support::worker(move || {
            let mut candidate = Arc::try_unwrap(store)
                .ok()
                .unwrap()
                .recover_same_home()
                .unwrap();
            let state = BerylState::reacquire_candidate(&candidate).unwrap();
            let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
            let adapters =
                crate::app_services::recovery_composer::test_support::adapters(&mut candidate);
            let appearance = super::threadless_tests::appearance::prepare(&candidate);
            (candidate, Some(adapters), state, storage, appearance)
        }),
        cx,
    );
    let mut preparation = cx.update(|app| {
        MainWindowComposerRecoveryPreparation::prepare(
            candidate,
            retired,
            storage,
            state,
            seed,
            app,
            |_| {},
        )
    });
    cx.run_until_parked();
    let (fresh, selection) = preparation.authenticated_source().unwrap().unwrap();
    let record = preparation.authenticated_window().unwrap();
    let mut config = composer_support::widget_config(
        fresh.binding,
        selection.binding().presentation_generation(),
    );
    config.viewport_extent = px(96.);
    config.limits.max_realized_block_extent = config.viewport_extent;
    let current = RangePrepublicationCurrent {
        binding: fresh.binding,
        history: fresh.history,
        available_capacity: RangeSurfaceCharge {
            bytes: config.limits.max_surface_bytes,
            items: config.limits.max_surface_items,
        },
    };
    let combined = RangeSurfaceCharge {
        bytes: current.available_capacity.bytes * 2,
        items: current.available_capacity.items * 2,
    };
    let environment = shell
        .window()
        .update(cx, |_, window, _| {
            RangePrepublicationEnvironment::new(
                12,
                config,
                window.text_system(),
                RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap(),
            )
            .unwrap()
        })
        .unwrap();
    input.read_with(cx, |input, _| {
        preparation
            .admit(input, protection, environment, combined)
            .unwrap()
    });
    ready = false;
    for _ in 0..512 {
        let progress = shell
            .window()
            .update(cx, |_, window, cx| {
                input.update(cx, |input, cx| {
                    preparation.advance(input, window.text_system(), cx, |_| {})
                })
            })
            .unwrap()
            .unwrap();
        if progress == MainWindowComposerRecoveryProgress::Ready {
            ready = true;
            break;
        }
        cx.run_until_parked();
    }
    assert!(ready);
    let mut configurator: Option<MainWindowConversationComposerConfigurator> =
        Some(Box::new(support::config));
    let occupancy = process.main_window_occupancy();
    let mut draft = Some(draft);
    let mut retained = aggregate.then(|| {
        crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(
            shell.window(),
            draft.take().unwrap(),
        )
    });
    let candidate = shell
        .window()
        .update(cx, |root, window, cx| {
            let focus = window.focused(cx);
            if let Some(retained) = retained.as_mut() {
                let foreign = MainWindowConversationComposerCloseTicket::for_test(
                    mount.entity_id(),
                    u64::MAX,
                    original,
                );
                for (driving, editor, ticket) in [
                    (true, resident.entity_id(), old_close),
                    (false, input.entity_id(), old_close),
                    (false, resident.entity_id(), foreign),
                ] {
                    retained.test_recovery_driving(driving);
                    assert!(
                        retained
                            .adopt_recovered_shell(
                                root,
                                editor,
                                ticket,
                                &mut preparation,
                                &mut adapters,
                                &mut configurator,
                                current,
                                window,
                                cx,
                            )
                            .is_err()
                    );
                    assert!(
                        retained.recovery_residents()
                            == vec![(shell.window().into(), resident.entity_id(), old_close),]
                    );
                    assert!(adapters.is_some() && configurator.is_some());
                    assert!(root.test_shell_construction_retired());
                }
                retained.test_recovery_driving(false);
                let mut capacity = current;
                if refuse {
                    capacity.available_capacity = RangeSurfaceCharge { bytes: 0, items: 0 };
                }
                let result = retained.adopt_recovered_shell(
                    root,
                    resident.entity_id(),
                    old_close,
                    &mut preparation,
                    &mut adapters,
                    &mut configurator,
                    capacity,
                    window,
                    cx,
                );
                assert_eq!(window.focused(cx), focus);
                assert_eq!(process.main_window_occupancy(), occupancy);
                assert_eq!(resident.read(cx).gpui_input(), input);
                assert!(!input.read(cx).is_enabled());
                if refuse {
                    assert!(result.is_err());
                    assert!(retained.test_recovery_ready());
                    assert!(
                        retained.recovery_residents()
                            == vec![(shell.window().into(), resident.entity_id(), old_close),]
                    );
                    assert!(root.test_shell_construction_retired());
                    assert!(adapters.is_some() && configurator.is_some());
                    return None;
                }
                let (candidate, close) = result.unwrap();
                assert_ne!(close, old_close);
                assert!(!retained.test_recovery_ready());
                assert!(
                    retained.recovery_residents()
                        == vec![(shell.window().into(), resident.entity_id(), close),]
                );
                assert_eq!(resident.read(cx).selection_identity(), selection);
                assert!(adapters.is_none() && configurator.is_none());
                assert!(!root.test_shell_construction_retired());
                assert!(
                    retained
                        .adopt_recovered_shell(
                            root,
                            resident.entity_id(),
                            old_close,
                            &mut preparation,
                            &mut adapters,
                            &mut configurator,
                            current,
                            window,
                            cx,
                        )
                        .is_err()
                );
                assert!(root.set_shutdown_interaction_gated(false, cx).is_err());
                return Some(candidate);
            }
            let mut draft = draft.as_mut().unwrap();
            if refuse {
                let root_id = draft.root;
                draft.root = input.entity_id();
                assert!(
                    root.adopt_interrupted_exit_shell(
                        &mut draft,
                        &mut preparation,
                        &mut adapters,
                        &mut configurator,
                        current,
                        window,
                        cx,
                    )
                    .unwrap_err()
                    .contains("exact gated draft")
                );
                draft.root = root_id;
                let foreign = MainWindowConversationComposerCloseTicket::for_test(
                    mount.entity_id(),
                    u64::MAX,
                    original,
                );
                draft.composer.as_mut().unwrap().2 = foreign;
                assert!(
                    root.adopt_interrupted_exit_shell(
                        &mut draft,
                        &mut preparation,
                        &mut adapters,
                        &mut configurator,
                        current,
                        window,
                        cx
                    )
                    .is_err()
                );
                assert_eq!(draft.test_ticket(), Some(foreign));
                draft.composer.as_mut().unwrap().2 = old_close;
                let mut insufficient = current;
                insufficient.available_capacity = RangeSurfaceCharge { bytes: 0, items: 0 };
                assert!(
                    root.adopt_interrupted_exit_shell(
                        &mut draft,
                        &mut preparation,
                        &mut adapters,
                        &mut configurator,
                        insufficient,
                        window,
                        cx
                    )
                    .is_err()
                );
                assert_eq!(draft.test_ticket(), Some(old_close));
                assert!(root.test_shell_construction_retired());
                assert!(adapters.is_some() && configurator.is_some());
                assert_eq!(resident.read(cx).selection_identity(), original);
                assert!(resident.read(cx).recovery_snapshot().is_some());
                assert_eq!(process.main_window_occupancy(), occupancy);
                assert_eq!(window.focused(cx), focus);
                assert!(!input.read(cx).is_enabled());
                return None;
            }
            let (candidate, fresh_close) = root
                .adopt_interrupted_exit_shell(
                    &mut draft,
                    &mut preparation,
                    &mut adapters,
                    &mut configurator,
                    current,
                    window,
                    cx,
                )
                .unwrap();
            assert_ne!(fresh_close, old_close);
            assert_eq!(draft.test_ticket(), Some(fresh_close));
            assert_eq!(resident.read(cx).selection_identity(), selection);
            assert!(adapters.is_none() && configurator.is_none());
            assert!(!root.test_shell_construction_retired());
            let controller = root.controller().unwrap();
            assert_eq!(controller.window_id(), record.window_id());
            assert_eq!(controller.placement(), record.placement());
            let ShellContent::Recovered {
                window: installed,
                selection: selected,
                ..
            } = &controller.content
            else {
                panic!("shell was not rebound")
            };
            assert_eq!(installed, &record);
            assert_eq!(*selected, selection);
            assert_eq!(process.main_window_occupancy(), occupancy);
            assert_eq!(window.focused(cx), focus);
            assert_eq!(resident.read(cx).gpui_input(), input);
            assert!(
                root.set_shutdown_interaction_gated(false, cx)
                    .unwrap_err()
                    .contains("fresh appearance")
            );
            assert!(!input.read(cx).is_enabled());
            assert!(
                root.adopt_interrupted_exit_shell(
                    &mut draft,
                    &mut preparation,
                    &mut adapters,
                    &mut configurator,
                    current,
                    window,
                    cx
                )
                .is_err()
            );
            assert_eq!(draft.test_ticket(), Some(fresh_close));
            Some(candidate)
        })
        .unwrap();
    let candidate = candidate.unwrap_or_else(|| {
        preparation.cancel();
        for _ in 0..128 {
            cx.run_until_parked();
            if preparation.advance_cleanup().unwrap() {
                break;
            }
        }
        assert!(preparation.advance_cleanup().unwrap());
        let (candidate, source) = preparation.take_cancelled_resources().unwrap();
        assert!(source.is_ok());
        drop(source);
        candidate
    });
    let window = shell.window();
    let old_owner = shell.appearance_owner.clone();
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("published selected shell handoff"));
    let fresh_owner = (!refuse).then(|| {
        cx.update(|app| old_owner.update(app, |owner, _| owner.retire()));
        let owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(
                fresh_appearance.clone(),
                NonZeroUsize::new(4).unwrap(),
                app,
            )
        });
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(window, &owner, app))
            .unwrap();
        if let Some(draft) = draft.as_mut() {
            use crate::theme_runtime::AppearancePublicationTarget;
            let target = cx.update(|app| owner.read(app).target());
            window
                .update(cx, |root, _, cx| {
                    root.validate_interrupted_exit_binding(draft, &target, cx)
                        .unwrap();
                    let fresh = draft.composer.as_ref().unwrap().2;
                    draft.composer.as_mut().unwrap().2 = old_close;
                    assert!(
                        root.validate_interrupted_exit_binding(draft, &target, cx)
                            .is_err()
                    );
                    draft.composer.as_mut().unwrap().2 = fresh;
                    let release = root.appearance_release.take();
                    assert!(
                        root.validate_interrupted_exit_binding(draft, &target, cx)
                            .is_err()
                    );
                    root.appearance_release = release;
                    root.validate_interrupted_exit_binding(draft, &target, cx)
                        .unwrap();
                })
                .unwrap();
        }
        window
            .read_with(cx, |root, app| {
                assert!(root.shutdown_interaction_gated);
                assert!(!resident.read(app).is_live());
                assert!(Arc::ptr_eq(
                    &root.controller().unwrap().appearance.generation,
                    &fresh_appearance
                ));
            })
            .unwrap();
        let fresh = resident.read_with(cx, |resident, _| {
            old_close.with_recovered_selection(resident.selection_identity())
        });
        let service = mount.read_with(cx, |mount, _| mount.bound_service().unwrap().clone());
        assert!(service.test_window_close_is_current(fresh));
        assert!(
            service
                .prepare_recovered_autosave(old_close.selection(), None)
                .is_err()
        );
        assert!(
            service
                .prepare_recovered_autosave(
                    fresh.selection(),
                    Some((
                        0,
                        crate::composer_host::ComposerHostAutosaveInterval::new(19).unwrap()
                    )),
                )
                .is_err()
        );
        if let Some(draft) = draft.as_mut() {
            let target = cx.update(|app| owner.read(app).target());
            window
                .update(cx, |root, _, cx| {
                    draft.composer.as_mut().unwrap().2 = old_close;
                    assert!(
                        root.release_interrupted_exit_draft(draft, &target, cx)
                            .is_err()
                    );
                    draft.composer.as_mut().unwrap().2 = fresh;
                    assert!(
                        root.release_interrupted_exit_mount(draft, &target, cx)
                            .is_err()
                    );
                    assert!(!resident.read(cx).is_live());
                    service.test_with_close_slot_locked(|| {
                        assert!(
                            !root
                                .release_interrupted_exit_draft(draft, &target, cx)
                                .unwrap()
                        );
                    });
                    assert!(service.test_window_close_is_current(fresh));
                })
                .unwrap();
        }
        cx.update(|app| {
            mount.update(app, |mount, cx| {
                assert!(mount.release_interrupted_exit_mount(fresh, cx).is_err());
                assert!(mount.recovery_binding_current(fresh));
                assert!(mount.release_interrupted_exit_draft(old_close, cx).is_err());
                let retained = mount.test_window_close_worker(|| {}).unwrap();
                assert!(mount.release_interrupted_exit_draft(fresh, cx).is_err());
                assert!(service.test_window_close_is_current(fresh));
                drop(retained);
                let autosave_worker = mount.test_autosave_worker(|| {}).unwrap();
                assert!(mount.release_interrupted_exit_draft(fresh, cx).is_err());
                assert!(service.test_window_close_is_current(fresh));
                drop(autosave_worker);
                service.test_with_close_slot_locked(|| {
                    assert_eq!(
                        mount.release_interrupted_exit_draft(fresh, cx).unwrap(),
                        MainWindowConversationComposerCloseRelease::Pending
                    );
                });
                assert!(service.test_window_close_is_current(fresh));
                assert!(mount.recovery_binding_current(fresh));
            });
        });
        for _ in 0..2 {
            if let Some(draft) = draft.as_ref() {
                let target = cx.update(|app| owner.read(app).target());
                window
                    .update(cx, |root, _, cx| {
                        assert!(
                            root.release_interrupted_exit_draft(draft, &target, cx)
                                .unwrap()
                        );
                    })
                    .unwrap();
            } else {
                cx.update(|app| {
                    mount.update(app, |mount, cx| {
                        assert_eq!(
                            mount.release_interrupted_exit_draft(fresh, cx).unwrap(),
                            MainWindowConversationComposerCloseRelease::Released
                        );
                    })
                });
            }
            assert!(!service.test_window_close_is_current(fresh));
            assert!(mount.read_with(cx, |mount, _| mount.recovery_binding_current(fresh)));
            service.test_with_selected_host(|host| {
                assert_eq!(host.autosave_interval(), interval);
                assert!(host.autosave_timer().is_none());
            });
            assert!(
                service
                    .prepare_recovered_autosave(
                        fresh.selection(),
                        aggregate.then_some((7, interval)),
                    )
                    .unwrap()
            );
            let autosave = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
            assert_eq!(
                autosave.phase(),
                MainWindowConversationComposerAutosavePhase::Idle
            );
            assert_eq!(autosave.retained_tasks(), 0);
            assert_eq!(autosave.retained_workers(), 0);
            assert!(!autosave.fenced());
            assert!(autosave.last_error().is_none());
        }
        cx.update(|app| {
            mount.update(app, |mount, cx| {
                assert!(mount.release_interrupted_exit_draft(old_close, cx).is_err());
            });
        });
        window
            .update(cx, |root, window, cx| {
                assert!(root.shutdown_interaction_gated);
                assert!(!resident.read(cx).is_live());
                assert!(resident.read(cx).recovery_binding_current(fresh));
                assert!(!input.read(cx).is_enabled());
                assert_eq!(resident.read(cx).selection_identity(), fresh.selection());
                mount.update(cx, |mount, cx| {
                    assert!(mount.advance_window_close(fresh, window, cx).is_err());
                    assert!(mount.release_window_close(fresh, window, cx).is_err());
                    assert!(
                        mount
                            .release_window_close_with_evidence(fresh, window, cx)
                            .is_err()
                    );
                });
            })
            .unwrap();
        drop(service);
        owner
    });
    cx.update(|app| {
        if settle_mounts {
            use crate::theme_runtime::AppearancePublicationTarget;
            let retained = retained.as_ref().unwrap();
            let calls = std::cell::Cell::new(0);
            let target = fresh_owner.as_ref().unwrap().read(app).target();
            let fresh = old_close.with_recovered_selection(resident.read(app).selection_identity());
            let worker = mount.update(app, |mount, _| {
                mount.test_window_close_worker(|| {}).unwrap()
            });
            assert!(
                retained
                    .test_release_prepared_recovered_mounts_after(&target, app, || panic!(
                        "unprepared mount reached settlement"
                    ))
                    .is_err()
            );
            drop(worker);
            for _ in 0..2 {
                assert_eq!(
                    retained
                        .test_release_prepared_recovered_mounts_after(&target, app, || {
                            calls.set(calls.get() + 1);
                            Err("process admission is busy".into())
                        })
                        .unwrap_err(),
                    "process admission is busy"
                );
                assert!(mount.read(app).recovery_binding_current(fresh));
                assert!(resident.read(app).recovery_binding_current(fresh));
                assert!(!resident.read(app).is_live());
                assert!(resident.read(app).mutation_gated());
                assert!(input.read(app).is_enabled());
                assert!(window.read(app).unwrap().shutdown_interaction_gated);
            }
            let service = mount.read(app).bound_service().unwrap().clone();
            service.test_with_close_slot_locked(|| {
                assert!(
                    retained
                        .test_release_prepared_recovered_mounts_after(&target, app, || {
                            calls.set(calls.get() + 1);
                            Ok(())
                        })
                        .unwrap()
                );
            });
            assert_eq!(calls.get(), 3);
            assert!(resident.read(app).is_live());
            assert!(!resident.read(app).recovery_binding_current(fresh));
            assert!(!mount.read(app).recovery_binding_current(fresh));
            assert!(resident.read(app).mutation_gated());
            assert!(window.read(app).unwrap().shutdown_interaction_gated);
            assert!(
                retained
                    .test_release_prepared_recovered_mounts_after(&target, app, || panic!(
                        "consumed ticket reached settlement"
                    ))
                    .is_err()
            );
        }
        window
            .update(app, |root, window, cx| {
                if fresh_owner.is_some() && !settle_mounts {
                    let fresh =
                        old_close.with_recovered_selection(resident.read(cx).selection_identity());
                    mount.update(cx, |mount, cx| {
                        assert!(mount.release_interrupted_exit_mount(old_close, cx).is_err());
                        assert!(!input.read(cx).is_enabled());
                        let retained = mount.test_window_close_worker(|| {}).unwrap();
                        assert!(mount.prepare_interrupted_exit_mount(fresh, cx).is_err());
                        assert!(mount.release_interrupted_exit_mount(fresh, cx).is_err());
                        assert!(!input.read(cx).is_enabled());
                        assert!(mount.recovery_binding_current(fresh));
                        assert!(resident.read(cx).recovery_binding_current(fresh));
                        drop(retained);
                    });
                    let service = mount.read(cx).bound_service().unwrap().clone();
                    if let Some(draft) = draft.as_mut() {
                        let target = fresh_owner.as_ref().unwrap().read(cx).target();
                        let old_target = old_owner.read(cx).target();
                        assert!(
                            root.release_interrupted_exit_mount(draft, &old_target, cx)
                                .is_err()
                        );
                        draft.composer.as_mut().unwrap().2 = old_close;
                        assert!(
                            root.release_interrupted_exit_mount(draft, &target, cx)
                                .is_err()
                        );
                        draft.composer.as_mut().unwrap().2 = fresh;
                        let worker = mount.update(cx, |mount, _| {
                            mount.test_window_close_worker(|| {}).unwrap()
                        });
                        assert!(
                            root.prepare_interrupted_exit_mount(draft, &target, cx)
                                .is_err()
                        );
                        assert!(
                            root.release_interrupted_exit_mount(draft, &target, cx)
                                .is_err()
                        );
                        assert!(resident.read(cx).recovery_binding_current(fresh));
                        assert!(!input.read(cx).is_enabled());
                        drop(worker);
                        service.test_with_close_slot_locked(|| {
                            for _ in 0..2 {
                                assert!(
                                    root.prepare_interrupted_exit_mount(draft, &target, cx)
                                        .unwrap()
                                );
                                assert!(input.read(cx).is_enabled());
                                assert!(!resident.read(cx).is_live());
                                assert!(resident.read(cx).recovery_binding_current(fresh));
                                assert!(mount.read(cx).recovery_binding_current(fresh));
                                assert!(resident.read(cx).mutation_gated());
                                assert!(root.shutdown_interaction_gated);
                            }
                            assert!(
                                root.release_interrupted_exit_mount(draft, &target, cx)
                                    .unwrap()
                            );
                        });
                        assert!(
                            root.release_interrupted_exit_mount(draft, &target, cx)
                                .is_err()
                        );
                    } else {
                        mount.update(cx, |mount, cx| {
                            service.test_with_close_slot_locked(|| {
                                assert!(mount.prepare_interrupted_exit_mount(fresh, cx).unwrap());
                                assert!(mount.recovery_binding_current(fresh));
                                assert!(resident.read(cx).recovery_binding_current(fresh));
                                assert!(!resident.read(cx).is_live());
                                assert!(input.read(cx).is_enabled());
                                assert!(mount.release_interrupted_exit_mount(fresh, cx).unwrap());
                            });
                            assert!(!mount.recovery_binding_current(fresh));
                            assert!(mount.release_interrupted_exit_mount(fresh, cx).is_err());
                        });
                    }
                    resident.update(cx, |resident, cx| {
                        assert!(
                            resident
                                .release_interrupted_exit_resident(old_close, cx)
                                .is_err()
                        );
                        assert!(resident.is_live());
                        assert!(!resident.recovery_binding_current(fresh));
                        assert!(resident.mutation_gated());
                        assert!(
                            !resident
                                .release_window_close_gate(fresh, window, cx)
                                .unwrap()
                        );
                        assert!(
                            resident
                                .release_interrupted_exit_resident(fresh, cx)
                                .is_err()
                        );
                        assert_eq!(resident.selection_identity(), fresh.selection());
                    });
                    assert!(input.read(cx).is_enabled());
                    assert!(root.shutdown_interaction_gated);
                    assert!(!mount.read(cx).recovery_binding_current(fresh));
                }
                assert!(
                    input
                        .update(cx, |input, cx| input.dispose(window, cx))
                        .is_empty()
                );
                drop(root.controller.take());
                window.remove_window();
            })
            .unwrap();
    });
    drop((
        draft,
        retained,
        mount,
        resident,
        input,
        preparation,
        adapters,
        configurator,
    ));
    cx.run_until_parked();
    if let Some(owner) = fresh_owner {
        use crate::theme_runtime::AppearancePublicationTarget;
        assert_eq!(
            cx.update(|app| owner.read(app).target().snapshot().count),
            0
        );
    }
    home_support::join(
        home_support::worker(move || candidate.abort().close().unwrap()),
        cx,
    );
    assert_eq!(process.main_window_occupancy(), 0);
    drop(directory);
}
