use super::*;

#[gpui::test]
fn dismissed_and_superseded_real_query_replies_cannot_replace_the_current_picker(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let before = selection(window, cx);
    seed(&owner, before, 100, 2);
    prepare_reader(&owner, window, 3, cx);
    let reader = owner.borrow().catalog_query_reader().unwrap();
    let pause = reader.test_pause_next_request().unwrap();
    let old_picker = open_switcher(window, cx);
    wait(
        cx,
        |_| pause.has_entered(),
        "opening query did not enter its real worker",
    );
    dismiss(window, cx);
    let picker = open_switcher(window, cx);
    assert_ne!(old_picker.entity_id(), picker.entity_id());
    pause.release();
    settled(&picker, 3, cx);
    assert!(
        window
            .read_with(cx, |root, _| {
                root.test_thread_switcher_picker().as_ref() == Some(&picker)
                    && root.test_thread_switcher_failure().is_none()
            })
            .unwrap()
    );

    let pause = reader.test_pause_next_request().unwrap();
    search(window, &picker, "absent-query", cx);
    wait(
        cx,
        |_| pause.has_entered(),
        "refined query did not enter its real worker",
    );
    search(window, &picker, "", cx);
    pause.release();
    settled(&picker, 3, cx);
    assert_eq!(
        cx.update(|app| picker.read(app).search_input().read(app).text().to_owned()),
        ""
    );
    assert_eq!(selection(window, cx), before);
    assert!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_failure().is_none())
            .unwrap()
    );
    drop(pause);
    drop(old_picker);
    dismiss(window, cx);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

fn make_selected_runtime_unavailable(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    selected: MainWindowComposerSelectionIdentity,
) {
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let committed_revision = loop {
            let execution = graph
                .syndic()
                .thread_execution(
                    graph.home(),
                    selected.claim().thread_id(),
                    syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap();
            let roots = graph.state().runtime_roots();
            let source = roots
                .catalog_source(
                    graph.home(),
                    execution.execution().runtime_id(),
                    execution.execution().root_id(),
                )
                .unwrap();
            let mut command = HomeCommand::new(graph.home().home_revision().unwrap());
            command
                .add(
                    roots.set_runtime_availability(
                        roots.revision(graph.home()).unwrap(),
                        beryl_state::SetRuntimeAvailability::new(
                            source.runtime().runtime_id(),
                            source.runtime().revision(),
                            AvailabilitySnapshot::observed(
                                Availability::Unavailable(
                                    beryl_model::UnavailableReason::BackendUnavailable,
                                ),
                                UnixMillis::new(100),
                            )
                            .unwrap(),
                        ),
                    ),
                )
                .unwrap();
            match graph.home().execute(command) {
                beryl_home_store::CommandOutcome::Committed { .. } => {
                    break graph.home().home_revision().unwrap();
                }
                beryl_home_store::CommandOutcome::NotCommitted { evidence: error } => {
                    assert!(
                        Instant::now() < deadline,
                        "availability preparation remained conflicted: {error}"
                    );
                }
                _ => panic!("availability preparation returned uncertain custody"),
            }
        };
        loop {
            match graph
                .catalog_source_reader()
                .retain_source(graph.home(), &beryl_home_store::CommandCancellation::new())
            {
                Ok(source) => {
                    let fresh = source.home_revision() >= committed_revision;
                    graph.home().release_frozen_read(&source).unwrap();
                    if fresh {
                        break;
                    }
                }
                Err(crate::catalog_readiness::CatalogSourceReadError::NotReady) => {}
                Err(error) => panic!("unavailable source qualification failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "updated unavailable source did not certify"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
}

#[gpui::test]
fn unavailable_current_thread_still_accepts_as_an_exact_live_noop(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let before = selection(window, cx);
    make_selected_runtime_unavailable(&owner, before);
    let running_reader = install_running_reader(&owner, window, cx);
    prepare_reader(&owner, window, 1, cx);
    let history = window
        .read_with(cx, |root, _| root.test_thread_navigation_history())
        .unwrap();
    let picker = open_switcher(window, cx);
    settled(&picker, 1, cx);
    let key = window
        .read_with(cx, |root, _| {
            root.test_thread_switcher_thread_key(before.claim().thread_id())
                .unwrap()
        })
        .unwrap();
    assert_eq!(selection(window, cx), before);
    let visible = window
        .read_with(cx, |root, app| {
            root.test_thread_confirmation_visible_identity(app)
        })
        .unwrap();
    picker.update(cx, |picker, pcx| picker.activate(&key, pcx));
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_thread_switcher_picker().is_none())
                .unwrap()
        },
        "unavailable current row did not authenticate the live no-op",
    );
    assert_eq!(selection(window, cx), before);
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_navigation_history())
            .unwrap(),
        history
    );
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    assert_eq!(cx.windows().len(), 1);
    drop(picker);
    drop(running_reader);
    dispose_recovered(owner, window, cx);
    drop(directory);
}
