use super::*;
use confirmation::{edit_prior, selected_window, selection, settled_confirmation};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "primary/acquisition.rs"]
mod acquisition;

#[path = "primary/inspection.rs"]
mod inspection;

fn admit(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) {
    let invoking = selection(window, cx).window_id();
    let lease =
        cx.update(|app| Arc::new(owner.borrow().admit_thread_creation(invoking, app).unwrap()));
    window
        .update(cx, |root, _, _| root.test_thread_confirmation_lease(lease))
        .unwrap();
}

fn invoke(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut TestAppContext) {
    window
        .update(cx, |root, window, cx| {
            root.test_primary_thread_command(window, cx)
        })
        .unwrap();
}

fn focus(window: gpui::WindowHandle<MainWindowShellRoot>, primary: bool, cx: &mut TestAppContext) {
    window
        .update(cx, |root, window, _| {
            root.test_new_thread_focus(primary).focus(window)
        })
        .unwrap();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| {
            window.draw_and_present_for_test(app)
        })
        .unwrap();
    });
}

fn assert_target(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    selected: MainWindowComposerSelectionIdentity,
    expected: RootId,
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
        assert_eq!(execution.execution().root_id(), expected);
        assert_eq!(
            execution.execution().runtime_id(),
            RuntimeId::from_bytes(if expected == RootId::from_bytes([75; 16]) {
                [74; 16]
            } else {
                *expected.as_bytes()
            })
        );
        let record = graph
            .state()
            .session()
            .capture_window_removal(graph.home(), selected.window_id())
            .unwrap();
        assert_eq!(record.window().selected_thread(), Some(selected.claim()));
        assert_eq!(
            record.window().remembered_target(),
            Some(RememberedTarget::new(
                execution.execution().runtime_id(),
                expected
            ))
        );
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
}

#[gpui::test]
fn primary_zero_runtime_is_focusable_and_refuses_every_entrance_while_secondary_opens(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = mounted(cx);
    let explanation = "Add a runtime with the … button before creating a thread.";
    assert_eq!(
        window
            .read_with(cx, |root, app| root.test_primary_thread_reason(app))
            .unwrap()
            .as_deref(),
        Some(explanation)
    );
    let handles = window
        .read_with(cx, |root, _| {
            (
                root.test_new_thread_focus(true),
                root.test_new_thread_focus(false),
            )
        })
        .unwrap();
    assert_ne!(handles.0, handles.1);
    focus(window, true, cx);
    window
        .update(cx, |root, window, _| {
            window.focus_next();
            assert!(root.test_new_thread_focus(false).is_focused(window));
            window.focus_prev();
            assert!(root.test_new_thread_focus(true).is_focused(window));
        })
        .unwrap();
    {
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        let bounds = visual
            .debug_bounds("main-window-new-thread-primary")
            .unwrap();
        visual.simulate_click(bounds.center(), gpui::Modifiers::none());
        visual.simulate_keystrokes("enter space");
    }
    invoke(window, cx);
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none()
                && !root.test_runtime_setup_state().0
                && root.controller().unwrap().is_threadless())
            .unwrap()
    );
    focus(window, false, cx);
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes("enter");
    let picker = window
        .read_with(cx, |root, _| root.test_runtime_setup_picker().unwrap())
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.dismiss(window, cx))
        })
        .unwrap();
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn primary_pointer_creates_in_exact_current_binding_without_long_press_flyout(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    edit_prior(window, "saved pointer predecessor", cx);
    assert!(
        window
            .read_with(cx, |root, app| root
                .test_primary_thread_reason(app)
                .is_none())
            .unwrap()
    );
    admit(&owner, window, cx);
    focus(window, true, cx);
    let bounds = gpui::VisualTestContext::from_window(window.into(), cx)
        .debug_bounds("main-window-new-thread-primary")
        .unwrap();
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_down(
        bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none())
            .unwrap()
    );
    gpui::VisualTestContext::from_window(window.into(), cx).simulate_mouse_up(
        bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    settled_confirmation(&owner, window, cx);
    let current = selection(window, cx);
    assert_ne!(current.claim().thread_id(), prior.claim().thread_id());
    assert_eq!(current.window_id(), prior.window_id());
    assert_eq!(cx.windows().len(), 1);
    assert_target(&owner, current, RootId::from_bytes([75; 16]));
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let draft = graph
            .syndic()
            .current_draft_piece_text_demand(
                graph.home(),
                prior.claim().thread_id(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                4096,
            )
            .unwrap()
            .unwrap();
        assert_eq!(draft.value().bytes(), b"saved pointer predecessor");
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn focused_primary_enter_and_space_create_without_secondary_activation(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    for key in ["enter", "space"] {
        let prior = selection(window, cx);
        edit_prior(window, "x", cx);
        admit(&owner, window, cx);
        focus(window, true, cx);
        gpui::VisualTestContext::from_window(window.into(), cx).simulate_keystrokes(key);
        settled_confirmation(&owner, window, cx);
        let selected = selection(window, cx);
        assert_ne!(selected.claim().thread_id(), prior.claim().thread_id());
        assert_eq!(selected.binding().logical_extent().logical_utf8_bytes(), 0);
        assert_target(&owner, selected, RootId::from_bytes([75; 16]));
        assert!(
            window
                .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none())
                .unwrap()
        );
    }
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn primary_current_pristine_noop_preserves_claim_editor_and_transcript(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    let visible = window
        .read_with(cx, |root, app| {
            root.test_thread_confirmation_visible_identity(app)
        })
        .unwrap();
    let saves = Arc::new(AtomicUsize::new(0));
    let observed = saves.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                None,
                Some(Box::new(move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                })),
                None,
            )
        })
        .unwrap();
    admit(&owner, window, cx);
    invoke(window, cx);
    settled_confirmation(&owner, window, cx);
    let current = selection(window, cx);
    assert_eq!(saves.load(Ordering::SeqCst), 1);
    assert_eq!(current.claim(), prior.claim());
    assert_eq!(
        current.binding().host_generation(),
        prior.binding().host_generation()
    );
    assert_eq!(
        window
            .read_with(cx, |root, app| root
                .test_thread_confirmation_visible_identity(app))
            .unwrap(),
        visible
    );
    assert_eq!(cx.windows().len(), 1);
    dispose_recovered(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn programmatic_primary_ignores_pending_root_scope_and_search_and_suppresses_duplicates(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = selected_window(cx);
    let prior = selection(window, cx);
    edit_prior(window, "preserve current scope", cx);
    catalog::populate(&owner, 1);
    let picker = open(window, cx);
    catalog::settled(&picker, 2, cx);
    let key = PickerRowKey(format!("root:{}", RootId::from_bytes([1; 16])));
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
            cx,
        )
    });
    catalog::settled(&picker, 1, cx);
    picker.update(cx, |picker, cx| picker.activate(&key, cx));
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-001", cx));
    catalog::settled(&picker, 1, cx);
    let commits = Arc::new(AtomicUsize::new(0));
    let observed = commits.clone();
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                Some(Box::new(move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                })),
                None,
                None,
            )
        })
        .unwrap();
    admit(&owner, window, cx);
    window
        .update(cx, |root, window, cx| {
            root.test_primary_thread_command(window, cx);
            assert!(root.test_primary_thread_reason(cx).is_some());
            root.test_primary_thread_command(window, cx);
            root.test_thread_confirmation_command(key, window, cx);
        })
        .unwrap();
    settled_confirmation(&owner, window, cx);
    let selected = selection(window, cx);
    assert_ne!(selected.claim().thread_id(), prior.claim().thread_id());
    assert_eq!(commits.load(Ordering::SeqCst), 1);
    assert_target(&owner, selected, RootId::from_bytes([75; 16]));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none())
            .unwrap()
    );
    dispose_recovered(owner, window, cx);
    drop(directory);
}
