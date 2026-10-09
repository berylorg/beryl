use super::*;
use crate::app_services::recovery_threadless::ThreadlessRecoveryWindow;
use crate::main_window::RestoredWindowPreparationAttempt;
use crate::theme_runtime::{AppearanceCoordinator, AppearanceCoordinatorConfig};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeRecoveryCandidate,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{DomainRevision, WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::{BerylState, InitializeThreadlessWindow, PreparedThemeAppearance};
use gpui::TestAppContext;
use std::num::NonZeroUsize;

#[path = "recovery_shell_appearance.rs"]
pub(super) mod appearance;

struct Home {
    store: Arc<HomeStore>,
    state: BerylState,
    storage: syndic_storage::SyndicStorage,
    faults: FaultController,
    directory: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("threadless-shell-recovery-")
            .tempdir()
            .unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
        let store = Arc::new(
            candidate
                .prepare_publication(
                    BerylState::required_domains()
                        .unwrap()
                        .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                        .unwrap(),
                )
                .unwrap()
                .publish()
                .unwrap(),
        );
        let session = state.session();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window_id(), placement()),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        Self {
            store,
            state,
            storage,
            faults,
            directory,
        }
    }

    fn recover(
        self,
    ) -> (
        HomeRecoveryCandidate,
        ThreadlessRecoveryWindow,
        tempfile::TempDir,
    ) {
        let Self {
            store,
            state,
            storage,
            faults,
            directory,
        } = self;
        let home = store.home_id();
        let generation = store.health().generation().unwrap();
        drop((state, storage));
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut candidate = Arc::try_unwrap(store)
            .ok()
            .unwrap()
            .recover_same_home()
            .unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        let before = access.home_revision().unwrap();
        let source =
            ThreadlessRecoveryWindow::prepare(&access, &state, home, generation, window_id())
                .unwrap();
        source.revalidate(&access, &state).unwrap();
        assert_eq!(before, access.home_revision().unwrap());
        (candidate, source, directory)
    }
}

fn window_id() -> WindowId {
    WindowId::from_bytes([21; 16])
}
fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 800, 600).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

#[gpui::test]
fn threadless_shell_recovery_adopts_facts_and_preserves_gate_and_reservation(
    cx: &mut TestAppContext,
) {
    run(cx, false, false, AppearanceBinding::None);
}

#[gpui::test]
fn threadless_shell_recovery_refuses_wrong_draft_home_and_unretired_shell(cx: &mut TestAppContext) {
    run(cx, true, false, AppearanceBinding::None);
}

#[gpui::test]
fn threadless_shell_recovery_retained_drafts_preserve_refusals_and_invalidate_readiness(
    cx: &mut TestAppContext,
) {
    run(cx, false, true, AppearanceBinding::None);
}

#[gpui::test]
fn threadless_shell_recovery_replaces_appearance_and_notice_ownership(cx: &mut TestAppContext) {
    run(cx, false, false, AppearanceBinding::Released);
}

#[gpui::test]
fn threadless_shell_recovery_rebinds_retained_published_handle(cx: &mut TestAppContext) {
    run(cx, false, false, AppearanceBinding::Retained);
}

#[derive(Clone, Copy)]
enum AppearanceBinding {
    None,
    Released,
    Retained,
}

fn run(cx: &mut TestAppContext, refuse: bool, aggregate: bool, bindings: AppearanceBinding) {
    let (home, attempt, lifetime, process, prepared, appearance) = std::thread::spawn(|| {
        let home = Home::new();
        let (attempt, lifetime) = RestoredWindowPreparationAttempt::new_for_test(
            home.store.clone(),
            home.state.session(),
            home.storage.clone(),
        )
        .unwrap();
        let revision = home
            .state
            .session()
            .minimal_bootstrap(&home.store)
            .unwrap()
            .unwrap()
            .header()
            .revision();
        let source = attempt
            .begin_threadless(revision, window_id(), home.state.runtime_roots())
            .unwrap();
        let process = RuntimeBackedWindowProcessRegistry::new(Default::default());
        let appearance = AppearanceCoordinator::new(
            AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
            PreparedThemeAppearance::fallback(
                home.state
                    .themes()
                    .settings_identity(DomainRevision::new(1).unwrap(), None),
            ),
        )
        .current();
        let prepared =
            ThreadlessWindowShellPrepared::new(source, &process, appearance.clone()).unwrap();
        (home, attempt, lifetime, process, prepared, appearance)
    })
    .join()
    .unwrap();
    #[cfg(target_os = "windows")]
    let native = std::thread::spawn(|| {
        crate::main_window::prepare_windows_window_placement(window_id(), placement()).unwrap()
    })
    .join()
    .unwrap();
    let mut shell = cx.update(|app| {
        let owner = GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app);
        let host = GpuiMainWindowShellHost::new(app, owner);
        #[cfg(target_os = "windows")]
        let mut host = host.with_prepared_placement(native);
        #[cfg(not(target_os = "windows"))]
        let mut host = host;
        host.construct_threadless_hidden(prepared).unwrap()
    });
    let window = shell.window();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    });
    cx.update(|app| {
        if matches!(bindings, AppearanceBinding::Retained) {
            shell.gate_startup_interaction(app).unwrap();
        }
        shell.publish(app).unwrap();
        if matches!(bindings, AppearanceBinding::Retained) {
            MainWindowShell::release_startup_interaction(std::slice::from_ref(&shell), app)
                .unwrap();
        }
    });
    let mut draft = window
        .update(cx, |root, window, cx| {
            root.set_shutdown_interaction_gated(true, cx).unwrap();
            root.begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    if refuse {
        window
            .update(cx, |root, _, cx| {
                assert!(
                    root.adopt_interrupted_exit_threadless_shell(&draft, &mut None, cx)
                        .is_err()
                );
            })
            .unwrap();
    }
    drop((attempt, lifetime));
    window
        .update(cx, |root, _, cx| {
            assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
        })
        .unwrap();
    let (candidate, source, directory, fresh_appearance) = std::thread::spawn(move || {
        let (candidate, source, directory) = home.recover();
        let fresh =
            (!matches!(bindings, AppearanceBinding::None)).then(|| appearance::prepare(&candidate));
        (candidate, source, directory, fresh)
    })
    .join()
    .unwrap();
    let mut source = Some(source);
    if refuse {
        let (foreign_candidate, foreign, foreign_directory) =
            std::thread::spawn(|| Home::new().recover()).join().unwrap();
        let mut foreign = Some(foreign);
        let other = cx.new(|_| ());
        window
            .update(cx, |root, _, cx| {
                let wrong = MainWindowShutdownDraft {
                    failed: None,
                    claim_operation: None,
                    prepublication_cleanup: std::cell::RefCell::new(None),
                    root: other.entity_id(),
                    retirement: None,
                    detached_source: None,
                    detached_installed: false,
                    composer: None,
                };
                assert!(
                    root.adopt_interrupted_exit_threadless_shell(&wrong, &mut source, cx)
                        .is_err()
                );
                assert!(source.is_some());
                assert!(
                    root.adopt_interrupted_exit_threadless_shell(&draft, &mut foreign, cx)
                        .is_err()
                );
                assert!(foreign.is_some());
                assert!(
                    root.adopt_interrupted_exit_threadless_shell(&draft, &mut None, cx)
                        .is_err()
                );
                assert!(matches!(
                    root.controller.as_ref().unwrap().content,
                    ShellContent::Retired {
                        reservation: Some(_),
                        ..
                    }
                ));
            })
            .unwrap();
        std::thread::spawn(move || {
            drop(foreign_candidate.abort());
            foreign_directory.close().unwrap();
        })
        .join()
        .unwrap();
    }
    let retained_window = window;
    draft = window
        .update(cx, |root, window, cx| {
            if aggregate {
                let mut drafts = crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(
                    retained_window,
                    draft,
                );
                drafts.test_recovery_driving(true);
                assert!(
                    drafts
                        .adopt_recovered_threadless_shell(root, &mut source, window, cx)
                        .is_err()
                );
                assert!(source.is_some());
                drafts.test_recovery_driving(false);
                assert!(drafts.test_recovery_ready());
                assert!(
                    drafts
                        .adopt_recovered_threadless_shell(root, &mut None, window, cx)
                        .is_err()
                );
                assert!(drafts.test_recovery_ready());
                drafts
                    .adopt_recovered_threadless_shell(root, &mut source, window, cx)
                    .unwrap();
                assert!(!drafts.test_recovery_ready());
                assert!(
                    drafts
                        .adopt_recovered_threadless_shell(root, &mut source, window, cx)
                        .is_err()
                );
                assert!(!drafts.test_recovery_ready());
                draft = root.begin_shutdown_draft(window, cx).unwrap();
            } else {
                root.adopt_interrupted_exit_threadless_shell(&draft, &mut source, cx)
                    .unwrap();
            }
            assert!(source.is_none());
            assert!(
                root.adopt_interrupted_exit_threadless_shell(&draft, &mut source, cx)
                    .is_err()
            );
            let controller = root.controller().unwrap();
            assert!(controller.is_threadless());
            assert!(controller.composer_mount().is_none());
            assert_eq!(controller.window_id(), window_id());
            assert_eq!(controller.placement(), &placement());
            assert!(root.creation_target(cx).is_none());
            assert_eq!(
                root.advance_shutdown_draft(&draft, window, cx).unwrap(),
                MainWindowShutdownDraftAdvance::Threadless
            );
            assert!(root.begin_shutdown_draft(window, cx).is_ok());
            assert!(
                root.set_shutdown_interaction_gated(false, cx)
                    .unwrap_err()
                    .contains("fresh appearance")
            );
            assert!(root.shutdown_interaction_gated);
            draft
        })
        .unwrap();
    assert_eq!(process.main_window_occupancy(), 1);
    assert!(!cx.update(|app| shell.ready_to_publish(app)));
    let old_owner = shell.appearance_owner.clone();
    let retained_owner = if matches!(bindings, AppearanceBinding::Retained) {
        Some(appearance::verify_retained(
            &mut shell,
            fresh_appearance.as_ref().unwrap().clone(),
            cx,
        ))
    } else {
        None
    };
    let retained_shell = if matches!(bindings, AppearanceBinding::Retained) {
        Some(shell)
    } else {
        cx.update(|app| shell.release_published_handle(app))
            .unwrap_or_else(|_| panic!("published shell handoff"));
        None
    };
    let fresh_owner = if matches!(bindings, AppearanceBinding::Retained) {
        retained_owner
    } else {
        fresh_appearance.map(|fresh| appearance::verify(window, old_owner, fresh, cx))
    };
    if let Some(owner) = &fresh_owner {
        use crate::theme_runtime::AppearancePublicationTarget;
        let target = cx.update(|app| owner.read(app).target());
        window
            .update(cx, |root, _, cx| {
                root.validate_interrupted_exit_binding(&draft, &target, cx)
                    .unwrap();
                let exact_root = draft.root;
                draft.root = owner.entity_id();
                assert!(
                    root.release_interrupted_exit_mount(&draft, &target, cx)
                        .is_err()
                );
                assert!(
                    root.release_interrupted_exit_draft(&draft, &target, cx)
                        .is_err()
                );
                draft.root = exact_root;
                root.shutdown_interaction_gated = false;
                assert!(
                    root.release_interrupted_exit_mount(&draft, &target, cx)
                        .is_err()
                );
                assert!(
                    root.release_interrupted_exit_draft(&draft, &target, cx)
                        .is_err()
                );
                root.shutdown_interaction_gated = true;
                root.validate_interrupted_exit_binding(&draft, &target, cx)
                    .unwrap();
                for _ in 0..2 {
                    assert!(
                        root.release_interrupted_exit_mount(&draft, &target, cx)
                            .unwrap()
                    );
                    assert!(
                        root.release_interrupted_exit_draft(&draft, &target, cx)
                            .unwrap()
                    );
                    assert!(root.shutdown_interaction_gated);
                }
            })
            .unwrap();
        if let Some(shell) = &retained_shell {
            cx.update(|app| shell.validate_interrupted_exit_binding(&draft, owner, app))
                .unwrap();
        }
        cx.update(|app| owner.update(app, |owner, _| owner.retire()));
        window
            .update(cx, |root, _, cx| {
                assert!(
                    root.release_interrupted_exit_mount(&draft, &target, cx)
                        .is_err()
                );
                assert!(
                    root.validate_interrupted_exit_binding(&draft, &target, cx)
                        .is_err()
                );
            })
            .unwrap();
    }
    std::thread::spawn(move || {
        drop(candidate.abort());
        directory.close().unwrap();
    })
    .join()
    .unwrap();
    window
        .update(cx, |root, _, cx| {
            assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(root.controller().unwrap().is_threadless());
        })
        .unwrap();
    assert_eq!(process.main_window_occupancy(), 1);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    drop(retained_shell);
    cx.run_until_parked();
    assert_eq!(process.main_window_occupancy(), 0);
    if let Some(owner) = fresh_owner {
        use crate::theme_runtime::AppearancePublicationTarget;
        assert_eq!(
            cx.update(|app| owner.read(app).target().snapshot().count),
            0
        );
    }
}
