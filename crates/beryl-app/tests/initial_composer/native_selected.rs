use super::*;
use gpui::Application;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[path = "../support/native_shell_appearance.rs"]
mod appearance;
#[path = "../support/desktop_placement_native.rs"]
mod native;

#[test]
fn native_restored_editor_realizes_on_windows_worker() {
    let prepared = home_support::worker(|| {
        [
            None,
            Some(gpui_text_input::MutationKind::Undo),
            Some(gpui_text_input::MutationKind::Redo),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, history)| {
            let PreparedFixture {
                prepared,
                attempt,
                _service,
                fixture,
                appearance: _,
                seals,
                snapshot,
                draft,
            } = prepared_fixture_with_history(211 + index as u8 * 10, history);
            let appearance = AppearanceCoordinator::new(
                AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                appearance::system_font_appearance(&fixture.state),
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
                fixture, attempt, _service, shell, geometry, appearance, snapshot, draft, selection,
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
        let (control, _) = native::open(app, "selected-native-worker-control");
        app.spawn(async move |cx| {
            for (
                fixture,
                attempt,
                service,
                prepared,
                geometry,
                appearance,
                before,
                draft,
                selection,
            ) in prepared
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
                            .unwrap_or_else(|_| panic!("native selected mount"))
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                while !cx.update(|app| shell.ready_to_publish(app)).unwrap() {
                    assert!(
                        Instant::now() < deadline,
                        "native selected first-presentable timeout"
                    );
                    shell
                        .window()
                        .update(cx, |_, window, _| window.refresh())
                        .unwrap();
                    native::pump(cx).await;
                }
                shell
                    .window()
                    .read_with(cx, |root, app| {
                        let mount = root
                            .controller()
                            .unwrap()
                            .composer_mount()
                            .unwrap()
                            .read(app);
                        let composer = mount.contribution().unwrap().read(app);
                        assert_eq!(composer.selection_identity(), selection);
                        assert!(
                            composer
                                .gpui_input()
                                .read(app)
                                .surface()
                                .unwrap()
                                .pages()
                                .iter()
                                .any(|page| page.text() == SAVED_TEXT)
                        );
                    })
                    .unwrap();
                let unpublished = cx
                    .update(|app| shell.close_restored_before_publication(app))
                    .unwrap()
                    .unwrap_or_else(|_| panic!("native selected cleanup"));
                let cleanup = home_support::worker(move || {
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
                    drop((attempt, service));
                });
                let deadline = Instant::now() + Duration::from_secs(15);
                while !cleanup.is_finished() {
                    assert!(Instant::now() < deadline, "native selected cleanup timeout");
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
