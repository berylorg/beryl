use super::restoration_support::*;

#[path = "support/restored_recovery.rs"]
mod recovery_support;
pub(super) use recovery_support::{shell_for_empty_recovery, shell_for_recovery};

#[cfg(target_os = "windows")]
#[path = "restored_desktop.rs"]
mod desktop;
#[cfg(target_os = "windows")]
#[path = "../support/desktop_placement_native.rs"]
mod native_retirement;
#[cfg(target_os = "windows")]
#[path = "native_selected.rs"]
mod native_selected;
#[cfg(target_os = "windows")]
#[path = "startup_construction.rs"]
mod startup_construction;
#[cfg(target_os = "windows")]
#[path = "startup_disposal.rs"]
mod startup_disposal;
use super::*;
use beryl_app::composer_host::*;
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
    GpuiAppearanceWindowSet,
};
use gpui::AppContext;
use std::num::NonZeroUsize;

#[path = "../syndic_composer_history/support.rs"]
mod editor;

const SAVED_TEXT: &str = "Saved restored draft 🦀";

#[path = "../support/window_placement.rs"]
mod placement_support;

struct PreparedFixture {
    prepared: RestoredWindowComposerPrepared,
    attempt: RestoredWindowPreparationAttempt,
    _service: RestoredWindowServiceTestLifetime,
    fixture: Fixture,
    appearance: Arc<AppearanceGeneration>,
    seals: DraftMarkerSealService,
    snapshot: beryl_state::MinimalSessionBootstrap,
    draft: SyndicDraftId,
}

fn appearance(fixture: &Fixture) -> Arc<AppearanceGeneration> {
    AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        beryl_state::PreparedThemeAppearance::fallback(
            fixture
                .state
                .themes()
                .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        ),
    )
    .current()
}

fn seals(fixture: &Fixture) -> DraftMarkerSealService {
    DraftMarkerSealService::test_new(
        &fixture.store,
        fixture.store.health().generation().unwrap(),
        fixture.storage.clone(),
        fixture.state.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn submission() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    )
}

fn prepared_fixture(seed: u8) -> PreparedFixture {
    prepared_fixture_with_history(seed, None)
}

fn prepared_fixture_with_history(
    seed: u8,
    history: Option<gpui_text_input::MutationKind>,
) -> PreparedFixture {
    prepared_fixture_with_text(seed, history, SAVED_TEXT)
}

fn prepared_fixture_with_text(
    seed: u8,
    history: Option<gpui_text_input::MutationKind>,
    text: &str,
) -> PreparedFixture {
    let fixture = Fixture::new(seed);
    let acquired = fixture.acquire(seed + 1);
    let thread = acquired.thread_id();
    let window = acquired.window_id();
    let draft = acquired.draft_id();
    drop(acquired);
    let seals = seals(&fixture);
    let (mut host, binding) = editor::activated(
        fixture.storage.clone(),
        &fixture.store,
        thread,
        seed + 2,
        seed + 3,
    );
    let written = if text.is_empty() {
        binding
    } else {
        editor::commit_text(
            &mut host,
            &fixture.store,
            binding,
            1,
            0,
            0,
            text,
            text.len() as u64,
            1,
        )
    };
    match history {
        Some(gpui_text_input::MutationKind::Undo) => {
            let length = SAVED_TEXT.len() as u64;
            let appended = editor::commit_text(
                &mut host,
                &fixture.store,
                written,
                2,
                length,
                length,
                "!",
                length + 1,
                1,
            );
            editor::select_history(
                &mut host,
                &fixture.store,
                appended,
                3,
                gpui_text_input::MutationKind::Undo,
            );
        }
        Some(gpui_text_input::MutationKind::Redo) => {
            let undone = editor::select_history(
                &mut host,
                &fixture.store,
                written,
                2,
                gpui_text_input::MutationKind::Undo,
            );
            editor::select_history(
                &mut host,
                &fixture.store,
                undone,
                3,
                gpui_text_input::MutationKind::Redo,
            );
        }
        None => {}
        Some(_) => panic!("unsupported native history fixture"),
    }
    let ComposerHostFlushAdmission::Started { ticket, .. } = host
        .begin_flush(ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("seed flush admission")
    };
    let captured = host
        .capture_flush_publication(
            &fixture.store,
            ticket,
            fixture.state.assets(),
            &seals,
            editor::operation_id(if history.is_some() { 20 } else { 2 }),
            None,
            SyndicTimestamp::from_unix_millis(1000),
            &CommandCancellation::new(),
        )
        .unwrap();
    if text.is_empty() {
        assert!(
            matches!(
                captured,
                ComposerHostFlushCapture::State(
                    beryl_app::composer_host::ComposerHostFlushState::CloseReady
                )
            ),
            "{captured:?}"
        );
    } else {
        assert!(matches!(captured, ComposerHostFlushCapture::Captured(_)));
    }
    host.advance_flush(&fixture.store, ticket).unwrap();
    assert!(!host.is_dirty());
    assert!(host.release_window_close(ticket).unwrap());
    host.dispose_composer_service(&fixture.store).unwrap();
    drop(host);
    begin_restore(&fixture);
    let (attempt, service) = attempt(&fixture);
    let snapshot_before = snapshot(&fixture);
    let mut custody = attempt
        .begin(
            snapshot_before.header().revision(),
            window,
            composer_support::activation_with_marker_proof(
                thread,
                seed + 4,
                seed + 5,
                1,
                text.len() as u64,
            ),
            composer_support::fixture::operation_id(seed + 6),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets()),
        )
        .unwrap();
    open(&mut custody, &attempt);
    assert_eq!(
        custody
            .activate_claim(&attempt, &CommandCancellation::new())
            .unwrap(),
        RestoredClaimActivationProgress::Active
    );
    let prepared = custody
        .prepare(&attempt, &mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    assert_eq!(
        prepared
            .selection_identity()
            .binding()
            .logical_extent()
            .logical_utf8_bytes(),
        text.len() as u64
    );
    let snapshot = snapshot(&fixture);
    let appearance = appearance(&fixture);
    PreparedFixture {
        prepared,
        attempt,
        _service: service,
        fixture,
        appearance,
        seals,
        snapshot,
        draft,
    }
}

#[gpui::test]
fn restored_hidden_native_mount_and_failure_retire_only_transient_custody(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    for (failed_mount, placement_case) in [
        (false, "valid"),
        (true, "valid"),
        #[cfg(target_os = "windows")]
        (false, "missing"),
        #[cfg(target_os = "windows")]
        (false, "foreign"),
        #[cfg(target_os = "windows")]
        (false, "changed"),
    ] {
        let (fixture, attempt, service, shell_prepared, appearance, before, draft, selection) =
            home_support::join(
                home_support::worker(move || {
                    let PreparedFixture {
                        prepared,
                        attempt,
                        _service,
                        fixture,
                        appearance,
                        seals,
                        snapshot,
                        draft,
                    } = prepared_fixture(if failed_mount { 61 } else { 41 });
                    let selection = prepared.selection_identity();
                    assert_eq!(fixture.process.main_window_occupancy(), 0);
                    let shell = RestoredWindowShellPrepared::prepare(
                        prepared,
                        &attempt,
                        &fixture.process,
                        Box::new(config),
                        seals,
                        submission(),
                        appearance.clone(),
                    )
                    .unwrap_or_else(|failure| panic!("{}", failure.error));
                    assert_eq!(fixture.process.main_window_occupancy(), 1);
                    (
                        fixture, attempt, _service, shell, appearance, snapshot, draft, selection,
                    )
                }),
                cx,
            );
        let appearance_owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app)
        });
        let native_window = if placement_case == "foreign" {
            beryl_model::WindowId::from_bytes([199; 16])
        } else {
            shell_prepared.window_id()
        };
        let native_saved = if placement_case == "changed" {
            beryl_model::WindowPlacement::new(
                beryl_model::WindowBounds::new(321, 123, 640, 480).unwrap(),
                beryl_model::WindowDisplayState::Maximized,
                None,
                None,
            )
        } else {
            shell_prepared.placement().clone()
        };
        let native_placement = placement_support::prepare(native_window, &native_saved);
        let windows_before = cx.update(|app| app.windows().len());
        let result = cx.update(|app| {
            let host = GpuiMainWindowShellHost::new(app, appearance_owner.clone());
            let mut host = if placement_case == "missing" {
                host
            } else {
                placement_support::attach(host, native_placement)
            };
            if failed_mount {
                host.test_reject_mount_after_native();
            }
            host.construct_restored_hidden(shell_prepared)
        });
        let unpublished = if placement_case != "valid" {
            let failure = result
                .err()
                .expect("invalid placement must fail before construction");
            assert!(matches!(
                failure,
                RestoredWindowShellHostFailure::BeforeConstruction { .. }
            ));
            assert_eq!(cx.update(|app| app.windows().len()), windows_before);
            failure.into_unpublished()
        } else if failed_mount {
            let failure = result.err().expect("forced failure after native creation");
            assert!(matches!(
                failure,
                RestoredWindowShellHostFailure::Construction { .. }
            ));
            failure.into_unpublished()
        } else {
            let shell = result.unwrap_or_else(|_| panic!("restored hidden native construction"));
            for _ in 0..32 {
                cx.run_until_parked();
                cx.update(|app| {
                    app.update_window(shell.window().into(), |_, window, app| {
                        window.draw(app).clear()
                    })
                    .unwrap()
                });
            }
            assert!(!cx.window_visibility(shell.window().into()).is_visible);
            shell
                .window()
                .read_with(cx, |root, app| {
                    let mount_entity = root.controller().unwrap().composer_mount().unwrap();
                    let mount = mount_entity.read(app);
                    assert!(mount.selected_first_presentable(app));
                    let composer_entity = mount.contribution().unwrap();
                    let composer = composer_entity.read(app);
                    assert_eq!(composer.selection_identity(), selection);
                    let input_entity = composer.gpui_input();
                    let input = input_entity.read(app);
                    assert!(input.is_quiescent());
                    assert_eq!(
                        input.surface().unwrap().binding().extent().byte_len(),
                        SAVED_TEXT.len() as u64
                    );
                    assert!(
                        input
                            .surface()
                            .unwrap()
                            .pages()
                            .iter()
                            .any(|page| page.text() == SAVED_TEXT)
                    );
                })
                .unwrap();
            cx.update(|app| shell.close_restored_before_publication(app))
                .unwrap_or_else(|_| panic!("close hidden restored shell"))
        };
        assert_eq!(fixture.process.main_window_occupancy(), 1);
        home_support::join(
            home_support::worker(move || {
                let unpublished = if failed_mount {
                    unpublished
                } else {
                    let cancel = CommandCancellation::new();
                    cancel.cancel();
                    let RestoredWindowShellRetirement::Pending { unpublished, .. } =
                        unpublished.retire(cancel)
                    else {
                        panic!("cancelled retirement must retain the restored shell reservation")
                    };
                    assert_eq!(fixture.process.main_window_occupancy(), 1);
                    assert_eq!(snapshot(&fixture), before);
                    unpublished
                };
                assert!(matches!(
                    unpublished.retire(CommandCancellation::new()),
                    RestoredWindowShellRetirement::Retired
                ));
                assert_eq!(fixture.process.main_window_occupancy(), 0);
                assert_eq!(snapshot(&fixture), before);
                let current = fixture
                    .storage
                    .current_draft(
                        &fixture.store,
                        selection.claim().thread_id(),
                        syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(current.draft().id(), draft);
                assert_eq!(
                    current.draft().piece_root().summary().logical_utf8_bytes(),
                    SAVED_TEXT.len() as u64
                );
                assert!(fixture.store.pending_reconciliations().is_empty());
                drop((attempt, service));
            }),
            cx,
        );
    }
}

#[test]
fn foreign_appearance_refuses_restored_shell_preparation_without_deleting_saved_state() {
    let PreparedFixture {
        prepared,
        attempt,
        _service,
        fixture,
        seals,
        snapshot: before,
        ..
    } = prepared_fixture(81);
    let foreign = Fixture::new(101);
    let failure = RestoredWindowShellPrepared::prepare(
        prepared,
        &attempt,
        &fixture.process,
        Box::new(config),
        seals,
        submission(),
        appearance(&foreign),
    )
    .err()
    .expect("foreign appearance must refuse");
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert!(matches!(
        failure.prepared.retire(CommandCancellation::new()),
        RestoredWindowComposerRetirement::Retired
    ));
    assert_eq!(snapshot(&fixture), before);
}

#[gpui::test]
fn expired_restore_attempt_refuses_native_construction_and_returns_original_editor(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, service, prepared, appearance, before) = home_support::join(
        home_support::worker(|| {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance,
                seals,
                snapshot,
                ..
            } = prepared_fixture(111);
            let prepared = RestoredWindowShellPrepared::prepare(
                prepared,
                &attempt,
                &fixture.process,
                Box::new(config),
                seals,
                submission(),
                appearance.clone(),
            )
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            drop(attempt);
            (fixture, _service, prepared, appearance, snapshot)
        }),
        cx,
    );
    let appearance_owner = cx
        .update(|app| GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app));
    let native_placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    let failure = cx
        .update(|app| {
            placement_support::attach(
                GpuiMainWindowShellHost::new(app, appearance_owner),
                native_placement,
            )
            .construct_restored_hidden(prepared)
        })
        .err()
        .expect("expired attempt cannot construct native shell");
    assert!(matches!(
        failure,
        RestoredWindowShellHostFailure::BeforeConstruction { .. }
    ));
    let unpublished = failure.into_unpublished();
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    home_support::join(
        home_support::worker(move || {
            assert!(matches!(
                unpublished.retire(CommandCancellation::new()),
                RestoredWindowShellRetirement::Retired
            ));
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            assert_eq!(snapshot(&fixture), before);
            drop(service);
        }),
        cx,
    );
}

#[cfg(target_os = "windows")]
#[test]
fn native_worker_retires_restored_shell_without_changing_saved_session_or_draft() {
    retire_restored_shell_on_native_worker(231, None);
}

#[cfg(target_os = "windows")]
#[test]
fn native_worker_retires_restored_shell_with_saved_undo_history() {
    retire_restored_shell_on_native_worker(232, Some(gpui_text_input::MutationKind::Undo));
}

#[cfg(target_os = "windows")]
#[test]
fn native_worker_retires_restored_shell_with_saved_redo_history() {
    retire_restored_shell_on_native_worker(233, Some(gpui_text_input::MutationKind::Redo));
}

#[cfg(target_os = "windows")]
fn retire_restored_shell_on_native_worker(
    seed: u8,
    history: Option<gpui_text_input::MutationKind>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let (fixture, attempt, service, unpublished, before, draft, selection) =
        home_support::worker(move || {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance,
                seals,
                snapshot,
                draft,
            } = prepared_fixture_with_history(seed, history);
            let selection = prepared.selection_identity();
            let prepared = RestoredWindowShellPrepared::prepare(
                prepared,
                &attempt,
                &fixture.process,
                Box::new(config),
                seals,
                submission(),
                appearance,
            )
            .unwrap_or_else(|failure| panic!("{}", failure.error));
            (
                fixture,
                attempt,
                _service,
                prepared.into_unpublished(),
                snapshot,
                draft,
                selection,
            )
        })
        .join()
        .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    gpui::Application::new().run(move |app| {
        let (control, _) = native_retirement::open(app, "restored-retirement-control");
        app.spawn(async move |cx| {
            assert_eq!(fixture.process.main_window_occupancy(), 1);
            let cancellation = CommandCancellation::new();
            let retirement = cx
                .background_executor()
                .spawn(async move { unpublished.retire(cancellation) })
                .await;
            assert!(matches!(retirement, RestoredWindowShellRetirement::Retired));
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            cx.background_executor()
                .spawn(async move {
                    assert_eq!(snapshot(&fixture), before);
                    let current = fixture
                        .storage
                        .current_draft(
                            &fixture.store,
                            selection.claim().thread_id(),
                            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(current.draft().id(), draft);
                    assert_eq!(
                        current.draft().piece_root().summary().logical_utf8_bytes(),
                        SAVED_TEXT.len() as u64
                    );
                    assert!(fixture.store.pending_reconciliations().is_empty());
                    drop((attempt, service));
                })
                .await;
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(completed.load(Ordering::SeqCst));
}
