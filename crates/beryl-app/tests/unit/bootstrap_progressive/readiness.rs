use super::*;

#[test]
fn native_ordinary_shell_progresses_catalog_history_and_backend_independently() {
    let fixture = support::selected(2, false, false);
    let catalog_hold =
        ProducerHold::install(Producer::CatalogCertification, *fixture.home_id.as_bytes());
    let catalog = catalog_hold.gate();
    let runtime_hold = ProducerHold::install(Producer::RuntimeLaunch, *fixture.runtime.as_bytes());
    let runtime = runtime_hold.gate();
    let history_holds = fixture
        .members
        .iter()
        .map(|(_, thread, _)| ProducerHold::install(Producer::SelectedHistory, *thread.as_bytes()))
        .collect::<Vec<_>>();
    let history = history_holds
        .iter()
        .map(ProducerHold::gate)
        .collect::<Vec<_>>();
    let expected = fixture.members.clone();
    let checked = Rc::new(Cell::new(false));
    let receipt = checked.clone();
    let mut configuration = support::configuration(&fixture);
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            support::wait(cx, |app| running(app) && catalog.entered() > 0 && runtime.entered() > 0 && history.iter().all(|gate| gate.entered() > 0) && support::shells(app).iter().all(|window| window.read_with(app, |root, _| root.test_selected_runtime_warmup_retained()).unwrap()), "ordinary shell did not retain independent real producers and their original view interests").await;
            let windows = cx.update(|app| {
                assert_eq!(app.windows().len(), expected.len(), "no loading/failure window substitutes for ordinary shells");
                let windows = support::shells(app);
                for window in &windows {
                    let selected = identity(*window, app);
                    assert!(expected.iter().any(|(id, thread, _)| selected == (*id, Some(*thread))));
                    window.read_with(app, |root, app| {
                        assert_eq!(root.test_progressive_readiness(app), (false, false));
                        assert_submission_gated(root, app);
                    }).unwrap();
                    window.update(app, |root, native, cx| {
                        root.open_thread_switcher(native, cx);
                        assert!(root.test_thread_switcher_picker().is_none(), "loading selector stays inert");
                        let panel = root.test_transcript_panel();
                        panel.update(cx, |panel, panel_cx| {
                            assert!(panel.initial_history_loading_for_test());
                            native.focus(&gpui::Focusable::focus_handle(panel, panel_cx));
                            panel.open_loading_menu_for_test(native, panel_cx);
                            assert!(!panel.quote_menu_open_for_test());
                        });
                        native.dispatch_action(Box::new(crate::syndic_transcript::OpenTranscriptContextMenu), cx);
                    }).unwrap();
                }
                windows
            }).unwrap();
            let editor_ids = cx.update(|app| windows.iter().map(|window| {
                window.read_with(app, |root, app| root.test_submission_composer(app).unwrap().entity_id()).unwrap()
            }).collect::<Vec<_>>()).unwrap();
            for window in &windows {
                let selection = cx.update(|app| identity(*window, app)).unwrap();
                let text = &expected.iter().find(|(id, _, _)| *id == selection.0).unwrap().2;
                support::wait(cx, |app| assert_local_editor(*window, app, text.len()), "local draft unavailable during progressive startup").await;
                cx.update(|app| {
                    support::configure_autosave(*window, app, 1);
                    support::type_character(*window, app);
                }).unwrap();
                support::wait(cx, |app| assert_local_editor(*window, app, text.len() + 1), "native draft input failed while independent producers were held").await;
                cx.update(|app| support::press_enter(*window, app)).unwrap();
                support::wait(cx, |app| window.read_with(app, |root, app| {
                    assert_submission_gated(root, app);
                    root.test_submission_composer(app).unwrap().read(app).submission_disabled_outcome().is_some()
                }).unwrap(), "normal native Enter bypassed the genuine backend readiness gate").await;
                support::wait(cx, |app| assert_saved_editor(*window, app, text.len() + 1), "original autosave did not preserve native input while independent producers were held").await;
            }
            assert_eq!(catalog.returned(), 0);
            assert_eq!(runtime.returned(), 0);
            catalog.release();
            support::wait(cx, |app| windows.iter().all(|window| window.read_with(app, |root, app| root.test_progressive_readiness(app) == (true, false)).unwrap()), "catalog release did not clear only selector readiness").await;
            assert!(history.iter().all(|gate| gate.returned() == 0));
            assert_eq!(runtime.returned(), 0);
            for gate in &history { gate.release(); }
            support::wait(cx, |app| windows.iter().all(|window| window.read_with(app, |root, app| root.test_progressive_readiness(app) == (true, true)).unwrap()), "initial history release did not independently publish measured history").await;
            assert_eq!(runtime.returned(), 0);
            runtime.release();
            support::wait(cx, |app| windows.iter().all(|window| window.read_with(app, |root, app| {
                assert_eq!(root.test_progressive_readiness(app), (true, true));
                assert_submission_gated(root, app);
                root.test_runtime_failure_snapshot().is_some() && root.notice_projection().is_some_and(|notice| notice.kind == crate::main_window::NoticeKind::RuntimeUnavailable)
            }).unwrap()), "real backend launch failure did not remain a per-window notice").await;
            for (window, editor_id) in windows.into_iter().zip(editor_ids) {
                let selection = cx.update(|app| {
                    window.read_with(app, |root, app| {
                        assert_eq!(root.test_submission_composer(app).unwrap().entity_id(), editor_id, "backend failure preserves the same local editor");
                    }).unwrap();
                    identity(window, app)
                }).unwrap();
                let text = &expected.iter().find(|(id, _, _)| *id == selection.0).unwrap().2;
                support::wait(cx, |app| assert_local_editor(window, app, text.len() + 1), "local editor became unavailable after the backend notice").await;
                cx.update(|app| {
                    support::configure_autosave(window, app, 2);
                    support::type_character_at(window, app, 1);
                }).unwrap();
                support::wait(cx, |app| assert_local_editor(window, app, text.len() + 2), "normal native input did not settle after backend failure").await;
                support::wait(cx, |app| assert_saved_editor(window, app, text.len() + 2), "original autosave did not preserve native input after backend failure").await;
                cx.update(|app| {
                    let selected = identity(window, app);
                    assert!(expected.iter().any(|(id, thread, _)| selected == (*id, Some(*thread))));
                    window.read_with(app, |root, app| {
                        assert_eq!(root.test_submission_composer(app).unwrap().entity_id(), editor_id);
                        assert_eq!(root.test_progressive_readiness(app), (true, true));
                        assert!(root.test_runtime_failure_snapshot().is_some());
                        assert_submission_gated(root, app);
                    }).unwrap();
                    support::minimize(window, app);
                }).unwrap();
            }
            receipt.set(true);
            cx.update(exit).unwrap();
        }).detach();
    }));
    run(configuration).unwrap();
    assert!(checked.get());
    support::assert_saved(&fixture, 2);
    drop((catalog_hold, runtime_hold, history_holds));
    fixture.directory.close().unwrap();
}
