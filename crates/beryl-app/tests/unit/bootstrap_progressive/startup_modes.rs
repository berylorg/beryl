use super::*;

#[test]
fn native_empty_restore_acquires_one_empty_editor_through_production_bootstrap() {
    let fixture = support::selected(1, false, true);
    let catalog_hold =
        ProducerHold::install(Producer::CatalogCertification, *fixture.home_id.as_bytes());
    let catalog = catalog_hold.gate();
    let runtime_hold = ProducerHold::install(Producer::RuntimeLaunch, *fixture.runtime.as_bytes());
    let runtime = runtime_hold.gate();
    let captured = Rc::new(RefCell::new(None));
    let receipt = captured.clone();
    let mut configuration = support::configuration(&fixture);
    let expected_window = configuration.startup.initial_window;
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            support::wait(
                cx,
                |app| running(app),
                "empty runtime-backed restore never publishes ordinary replacement",
            )
            .await;
            let window = cx
                .update(|app| {
                    assert_eq!(app.windows().len(), 1);
                    let window = support::shells(app)[0];
                    let selected = identity(window, app);
                    assert_eq!(selected.0, expected_window);
                    assert!(selected.1.is_some());
                    assert!(
                        !window
                            .read_with(app, |root, _| root.controller().unwrap().is_threadless())
                            .unwrap()
                    );
                    *receipt.borrow_mut() = Some(selected);
                    window
                })
                .unwrap();
            support::wait(
                cx,
                |app| assert_local_editor(window, app, 0),
                "replacement first editor was not coherently empty",
            )
            .await;
            cx.update(|app| support::type_character(window, app))
                .unwrap();
            support::wait(
                cx,
                |app| assert_local_editor(window, app, 1),
                "replacement accepts actual native input while progressive work waits",
            )
            .await;
            catalog.release();
            runtime.release();
            support::wait(
                cx,
                |app| {
                    catalog.returned() > 0
                        && runtime.returned() > 0
                        && window
                            .read_with(app, |root, app| {
                                root.test_progressive_readiness(app).0
                                    && root.test_runtime_failure_snapshot().is_some()
                            })
                            .unwrap()
                },
                "released acquired-shell catalog and runtime producers did not settle before Exit",
            )
            .await;
            cx.update(exit).unwrap();
        })
        .detach();
    }));
    run(configuration).unwrap();
    let (window, thread) = captured.borrow_mut().take().unwrap();
    let mut fixture = fixture;
    fixture
        .members
        .push((window, thread.unwrap(), String::new()));
    support::assert_saved(&fixture, 1);
    drop((catalog_hold, runtime_hold));
    fixture.directory.close().unwrap();
}

#[test]
fn native_zero_runtime_restore_publishes_one_threadless_ordinary_shell() {
    let fixture = support::threadless();
    let catalog_hold =
        ProducerHold::install(Producer::CatalogCertification, *fixture.home_id.as_bytes());
    let catalog = catalog_hold.gate();
    let checked = Rc::new(Cell::new(false));
    let receipt = checked.clone();
    let mut configuration = support::configuration(&fixture);
    let expected = configuration.startup.initial_window;
    configuration.app_hook = Some(Box::new(move |app| {
        app.spawn(async move |cx| {
            support::wait(
                cx,
                |app| running(app) && catalog.entered() > 0,
                "zero-runtime ordinary shell did not publish with real catalog work outstanding",
            )
            .await;
            let window = cx
                .update(|app| {
                    assert_eq!(app.windows().len(), 1);
                    let window = support::shells(app)[0];
                    assert_eq!(identity(window, app), (expected, None));
                    window
                        .read_with(app, |root, app| {
                            assert!(root.controller().unwrap().is_threadless());
                            assert!(root.test_submission_composer(app).is_none());
                            assert!(!root.test_progressive_readiness(app).0);
                            assert!(
                                root.new_window_disabled_reason(app)
                                    .unwrap()
                                    .contains("runtime")
                            );
                        })
                        .unwrap();
                    window
                })
                .unwrap();
            catalog.release();
            support::wait(
                cx,
                |app| {
                    window
                        .read_with(app, |root, app| root.test_progressive_readiness(app).0)
                        .unwrap()
                },
                "zero-runtime catalog did not certify independently",
            )
            .await;
            cx.update(|app| {
                assert_eq!(identity(window, app), (expected, None));
                support::minimize(window, app);
            })
            .unwrap();
            receipt.set(true);
            cx.update(exit).unwrap();
        })
        .detach();
    }));
    run(configuration).unwrap();
    assert!(checked.get());
    let HomeOpenOutcome::Ready {
        candidate,
        state,
        syndic,
    } = support::open(fixture.directory.path())
    else {
        unreachable!()
    };
    let store = candidate.publish().unwrap();
    let snapshot = state.session().minimal_bootstrap(&store).unwrap().unwrap();
    assert_eq!(snapshot.windows().len(), 1);
    assert_eq!(snapshot.windows()[0].window_id(), expected);
    assert!(snapshot.windows()[0].selected_thread().is_none());
    drop((state, syndic));
    store.close().unwrap();
    drop(catalog_hold);
    fixture.directory.close().unwrap();
}
