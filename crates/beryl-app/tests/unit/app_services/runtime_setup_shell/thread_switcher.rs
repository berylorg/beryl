use super::*;
use confirmation::{selected_window, selection};
use gpui::{EntityInputHandler, Focusable};

#[path = "thread_switcher/configuration.rs"]
mod configuration;
#[path = "thread_switcher/retention.rs"]
mod retention;

fn seed(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    selected: MainWindowComposerSelectionIdentity,
    first: u8,
    count: u8,
) {
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let execution = graph
            .syndic()
            .thread_execution(
                graph.home(),
                selected.claim().thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        for index in first..first + count {
            let thread = SyndicThreadId::from_bytes([index; 16]);
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let mut command = HomeCommand::new(graph.home().home_revision().unwrap());
                command
                    .add(graph.syndic().create_thread(
                        graph.syndic().revision(graph.home()).unwrap(),
                        CreateThread::ordinary(
                            thread,
                            SyndicDraftId::from_bytes([index; 16]),
                            execution.execution().clone(),
                            syndic_storage::SyndicTimestamp::from_unix_millis(index as u64),
                            DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1).unwrap(),
                        ),
                    ))
                    .unwrap();
                match graph.home().execute(command) {
                    beryl_home_store::CommandOutcome::Committed { .. } => break,
                    beryl_home_store::CommandOutcome::NotCommitted { evidence: error } => {
                        assert!(
                            Instant::now() < deadline,
                            "fixture thread seed conflicts: {error}"
                        );
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    _ => panic!("fixture thread seed has uncertain original custody"),
                }
            }
            loop {
                let prepared = match crate::catalog_projection::prepare_thread_catalog_projection(
                    graph.home(), graph.syndic(), graph.state(), thread,
                ) {
                    Ok(prepared) => prepared,
                    Err(crate::catalog_projection::CatalogProjectionBuildError::ConcurrentPreparation) => {
                        assert!(Instant::now() < deadline, "source projection preparation remained conflicted");
                        continue;
                    }
                    Err(error) => panic!("canonical source projection preparation failed: {error}"),
                };
                let command = match prepared {
                    crate::catalog_projection::ThreadCatalogProjectionPreparation::ExactCurrent => break,
                    crate::catalog_projection::ThreadCatalogProjectionPreparation::Publish(command) => command,
                    crate::catalog_projection::ThreadCatalogProjectionPreparation::ThreadMissing => {
                        panic!("newly created canonical thread is missing")
                    }
                };
                match graph.home().execute(command) {
                    beryl_home_store::CommandOutcome::Committed { later_failure: None, .. } => break,
                    beryl_home_store::CommandOutcome::NotCommitted { evidence } if evidence.conflicts().is_some() => {
                        assert!(Instant::now() < deadline, "canonical source projection remained conflicted: {evidence}");
                    }
                    outcome => panic!("canonical source projection did not commit: {outcome:?}"),
                }
            }
        }
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
}

fn prepare_reader(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    count: usize,
    cx: &mut TestAppContext,
) {
    let diagnostic_at = Instant::now() + Duration::from_secs(2);
    let mut reported = false;
    wait(
        cx,
        |_| {
            let actual = owner
                .borrow()
                .test_services()
                .graph()
                .unwrap()
                .catalog_source_reader()
                .certified_threads();
            if !reported && Instant::now() >= diagnostic_at {
                reported = true;
                eprintln!("switcher source expected {count}, certified {actual:?}");
            }
            match actual {
                Ok(actual) => actual == count,
                Err(crate::catalog_readiness::CatalogSourceReadError::NotReady) => false,
                Err(error) => panic!("switcher source certification failed: {error}"),
            }
        },
        "complete source catalog did not certify",
    );
    let reader = owner.borrow().catalog_query_reader().unwrap();
    assert!(reader.is_ready());
    window
        .update(cx, |root, _, _| root.test_thread_switcher_reader(reader))
        .unwrap();
}
fn install_running_reader(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> Arc<crate::app_services::PublishedRunningThreadsReader> {
    let reader = Arc::new(owner.borrow().running_threads_reader().unwrap());
    window
        .update(cx, |root, window, cx| {
            root.test_running_thread_reader(&reader, window, cx)
        })
        .unwrap()
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    root.coherent_selected_thread_title(app).is_some()
                })
                .unwrap()
        },
        "same-owner selected transcript and title did not become coherent",
    );
    reader
}
fn open_switcher(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> gpui::Entity<ThreadRootPicker> {
    window
        .update(cx, |root, window, cx| root.open_thread_switcher(window, cx))
        .unwrap();
    window
        .read_with(cx, |root, _| root.test_thread_switcher_picker().unwrap())
        .unwrap()
}
fn settled(picker: &gpui::Entity<ThreadRootPicker>, count: usize, cx: &mut TestAppContext) {
    let diagnostic_at = Instant::now() + Duration::from_secs(2);
    let mut reported = false;
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                let picker = picker.read(app);
                let diagnostics = picker.diagnostics();
                if !reported && Instant::now() >= diagnostic_at {
                    reported = true;
                    eprintln!("switcher expected {count}, primary {diagnostics:?}, runtime {:?}, search {:?}",
                        picker.runtime_diagnostics(), picker.search_input().read(app).text());
                }
                diagnostics.total_count == count
                    && diagnostics.pending_page_count == 0
                    && !diagnostics.collection_failed
                    && picker.runtime_diagnostics().is_some_and(|diagnostics| {
                        diagnostics.pending_page_count == 0 && !diagnostics.collection_failed
                    })
            })
        },
        "switcher paired visible pages did not settle",
    );
}
fn search(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    picker: &gpui::Entity<ThreadRootPicker>,
    text: &str,
    cx: &mut TestAppContext,
) {
    let input = cx.update(|app| picker.read(app).search_input());
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.focus(window, cx);
                let len = input.text().len();
                input.replace_text_in_range(Some(0..len), text, window, cx);
            })
        })
        .unwrap();
}
fn dismiss(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut TestAppContext) {
    window
        .update(cx, |root, window, cx| {
            root.test_thread_switcher_dismiss(window, cx)
        })
        .unwrap();
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_retained())
            .unwrap(),
        (false, false, 0)
    );
}

#[gpui::test]
fn toolbar_opens_immediate_picker_and_current_acceptance_preserves_exact_view(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let before = selection(window, cx);
    let running_reader = install_running_reader(&owner, window, cx);
    prepare_reader(&owner, window, 1, cx);
    let history = window
        .read_with(cx, |root, _| root.test_thread_navigation_history())
        .unwrap();
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_toolbar_height())
            .unwrap(),
        (true, 44.)
    );
    let bounds = gpui::VisualTestContext::from_window(window.into(), cx)
        .debug_bounds("main-window-thread-selector")
        .unwrap();
    assert!(bounds.size.width > gpui::px(0.));
    assert_eq!(bounds.size.height, gpui::px(32.));
    assert!(bounds.origin.y >= gpui::px(0.));
    window
        .update(cx, |root, window, _| {
            root.test_thread_switcher_focus().focus(window)
        })
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes("enter");
    let picker = window
        .read_with(cx, |root, _| root.test_thread_switcher_picker().unwrap())
        .unwrap();
    settled(&picker, 1, cx);
    assert!(cx.update(|app| picker.read(app).confirmation_state().is_none()));
    let key = window
        .read_with(cx, |root, _| root.test_thread_switcher_keys()[0].clone())
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
        "current row acceptance did not dismiss",
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
    drop(running_reader);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn large_switcher_pages_reach_end_and_partial_thirty_row_page_has_no_navigation_gap(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let before = selection(window, cx);
    seed(&owner, before, 100, 80);
    prepare_reader(&owner, window, 81, cx);
    let picker = open_switcher(window, cx);
    settled(&picker, 81, cx);
    assert_eq!(
        cx.update(|app| picker.read(app).diagnostics().resident_row_count),
        30
    );
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, pcx| {
                picker.focus_row(29, window, pcx);
                picker.navigate(PickerNavigation::Down, window, pcx);
                picker.navigate(PickerNavigation::Down, window, pcx);
            })
        })
        .unwrap();
    wait(
        cx,
        |cx| cx.update(|app| picker.read(app).focused_position() == Some(31)),
        "partial page lost exact offscreen navigation",
    );
    let focused = cx
        .update(|app| picker.read(app).focused_key().cloned())
        .unwrap();
    assert!(
        window
            .read_with(cx, |root, _| root
                .test_thread_switcher_keys()
                .contains(&focused))
            .unwrap()
    );
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, pcx| {
                picker.navigate(PickerNavigation::End, window, pcx)
            })
        })
        .unwrap();
    wait(
        cx,
        |cx| cx.update(|app| picker.read(app).focused_position() == Some(80)),
        "end did not reach complete logical catalog",
    );
    let diagnostics = cx.update(|app| picker.read(app).diagnostics());
    assert!(diagnostics.realized_row_count <= PICKER_MAX_REALIZED_ROWS);
    assert!(diagnostics.resident_page_count <= PICKER_MAX_RESIDENT_PAGES);
    assert_eq!(selection(window, cx), before);
    dismiss(window, cx);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn frozen_opening_keeps_search_scope_and_zero_thread_options_coherent_under_new_publication(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    catalog::populate(&owner, 2);
    let selected = selection(window, cx);
    seed(&owner, selected, 100, 3);
    prepare_reader(&owner, window, 4, cx);
    let picker = open_switcher(window, cx);
    settled(&picker, 4, cx);
    let opening = window
        .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
        .unwrap();
    seed(&owner, selected, 104, 1);
    prepare_reader(&owner, window, 5, cx);
    search(window, &picker, "missing-everywhere", cx);
    settled(&picker, 0, cx);
    assert!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_picker().is_some())
            .unwrap()
    );
    search(window, &picker, "", cx);
    settled(&picker, 4, cx);
    let runtime = PickerRowKey(format!("runtime:{}", RuntimeId::from_bytes([1; 16])));
    picker.update(cx, |picker, pcx| {
        picker.dispatch_command(PickerCommand::BrowseRoots(runtime), pcx)
    });
    settled(&picker, 1, cx);
    assert!(cx.update(|app| picker.read(app).query_text().is_empty()));
    let root = window
        .read_with(cx, |root, _| {
            root.test_thread_switcher_root_keys()[0].clone()
        })
        .unwrap();
    picker.update(cx, |picker, pcx| picker.activate(&root, pcx));
    settled(&picker, 0, cx);
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap())
            .unwrap(),
        opening
    );
    picker.update(cx, |picker, pcx| {
        picker.dispatch_command(PickerCommand::Return, pcx)
    });
    settled(&picker, 4, cx);
    assert_eq!(selection(window, cx), selected);
    dismiss(window, cx);
    let reopened = open_switcher(window, cx);
    settled(&reopened, 5, cx);
    assert!(
        window
            .read_with(cx, |root, _| root.test_thread_switcher_revision().unwrap()
                > opening)
            .unwrap()
    );
    dismiss(window, cx);
    dispose_recovered(owner, window, cx);
    drop(directory);
}
