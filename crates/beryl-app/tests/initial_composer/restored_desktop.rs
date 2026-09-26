use super::*;
use gpui::Application;
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "../support/shell_desktop_flight.rs"]
mod flight;
use flight::{Case, native};

#[test]
fn native_restored_desktop_completion_preserves_original_editor_cleanup() {
    let fixtures = home_support::worker(|| {
        [Case::Ready, Case::CancelDuring, Case::RemoveDuring]
            .into_iter()
            .enumerate()
            .map(|(index, case)| {
                let PreparedFixture {
                    prepared,
                    attempt,
                    _service,
                    fixture,
                    appearance: _,
                    seals,
                    snapshot,
                    draft,
                } = prepared_fixture(170 + index as u8 * 10);
                let appearance = AppearanceCoordinator::new(
                    AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                    flight::system_font_appearance(&fixture.state),
                )
                .current();
                let selection = prepared.selection_identity();
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
                let geometry = beryl_app::main_window::prepare_windows_window_placement(
                    shell.window_id(),
                    shell.placement().clone(),
                )
                .unwrap();
                (
                    case, fixture, attempt, _service, shell, geometry, appearance, snapshot, draft,
                    selection,
                )
            })
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    Application::new().run(move |app| {
        gpui_text_input::ensure_text_input_bindings(app);
        let (control, _) = native::open(app, "restored-flight-control");
        app.spawn(async move |cx| {
            for (
                case,
                fixture,
                attempt,
                service,
                prepared,
                geometry,
                appearance,
                before,
                draft,
                selection,
            ) in fixtures
            {
                let shell = cx
                    .update(|app| {
                        let owner = GpuiAppearanceWindowSet::new(
                            appearance,
                            NonZeroUsize::new(4).unwrap(),
                            app,
                        );
                        GpuiMainWindowShellHost::new(app, owner)
                            .with_prepared_placement(geometry)
                            .construct_restored_hidden(prepared)
                            .unwrap_or_else(|_| panic!("restored native mount"))
                    })
                    .unwrap();
                let window = shell.window();
                let (shell, raw) = flight::flight(shell, case, cx).await;
                if matches!(case, Case::Ready) {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "restored first-presentable timeout"
                        );
                        window.update(cx, |_, window, _| window.refresh()).unwrap();
                        native::pump(cx).await;
                    }
                    window
                        .read_with(cx, |root, app| {
                            let mount = root
                                .controller()
                                .unwrap()
                                .composer_mount()
                                .unwrap()
                                .read(app);
                            let composer = mount.contribution().unwrap().read(app);
                            assert_eq!(composer.selection_identity(), selection);
                            let input = composer.gpui_input().read(app);
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
                }
                let unpublished = cx
                    .update(|app| shell.close_restored_before_publication(app))
                    .unwrap()
                    .unwrap_or_else(|_| panic!("original restored shell cleanup"));
                flight::wait_destroyed(raw, cx).await;
                assert_eq!(fixture.process.main_window_occupancy(), 1);
                let cleanup = home_support::worker(move || {
                    let cancellation = CommandCancellation::new();
                    cancellation.cancel();
                    let RestoredWindowShellRetirement::Pending { unpublished, .. } =
                        unpublished.retire(cancellation)
                    else {
                        panic!("cancelled cleanup retains original custody")
                    };
                    assert_eq!(snapshot(&fixture), before);
                    assert_eq!(fixture.process.main_window_occupancy(), 1);
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
                });
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                while !cleanup.is_finished() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "restored cleanup timeout"
                    );
                    native::pump(cx).await;
                }
                cleanup.join().unwrap();
            }
            result.store(true, Ordering::SeqCst);
            control
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        })
        .detach();
    });
    assert!(completed.load(Ordering::SeqCst));
}
