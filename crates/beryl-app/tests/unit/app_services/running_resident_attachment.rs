use crate::main_window::*;

#[test]
fn native_recovery_owner_attaches_once_and_preserves_fences() {
    resident_run(ResidentScenario::Attach);
}

#[test]
fn native_recovery_owner_refuses_request_changed_after_ready() {
    resident_run(ResidentScenario::StaleAttachment);
}

#[test]
fn native_recovery_owner_refuses_cancelled_ready_attachment() {
    resident_run(ResidentScenario::CancelledAttachment);
}

#[test]
fn native_recovery_owner_retains_ready_resources_after_capacity_refusal() {
    resident_run(ResidentScenario::CapacityAttachment);
}

pub(super) fn attempt(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    key: &crate::running_owner::ResidentPreparationKey,
    mount: &gpui::Entity<MainWindowConversationComposerMount>,
    root: &mut MainWindowShellRoot,
    drafts: &Rc<RefCell<crate::running_owner::RunningShutdownDrafts>>,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    close: MainWindowConversationComposerCloseTicket,
    adapters: &mut Option<crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters>,
    mut current: gpui_text_input::RangePrepublicationCurrent,
    scenario: ResidentScenario,
    window: &mut gpui::Window,
    app: &mut gpui::Context<MainWindowShellRoot>,
) -> bool {
    let input = resident.read(app).gpui_input();
    input.update(app, |input, _| input.focus(window));
    let focus = window.focused(app);
    let mut configurator: Option<MainWindowConversationComposerConfigurator> =
        Some(Box::new(resident_fixture::configure));
    let foreign = request.test_foreign();
    assert!(
        owner
            .borrow_mut()
            .attach_interrupted_exit_resident(
                &foreign,
                key,
                root,
                adapters,
                &mut configurator,
                current,
                window,
                app,
            )
            .is_err()
    );
    assert!(adapters.is_some() && configurator.is_some());
    let retained = owner.borrow_mut().test_replace_recovery_drafts(None);
    assert!(
        owner
            .borrow_mut()
            .attach_interrupted_exit_resident(
                request,
                key,
                root,
                adapters,
                &mut configurator,
                current,
                window,
                app,
            )
            .err()
            .unwrap()
            .contains("drafts are unavailable")
    );
    owner.borrow_mut().test_replace_recovery_drafts(retained);
    let borrowed = drafts.borrow_mut();
    assert!(
        owner
            .borrow_mut()
            .attach_interrupted_exit_resident(
                request,
                key,
                root,
                adapters,
                &mut configurator,
                current,
                window,
                app,
            )
            .err()
            .unwrap()
            .contains("drafts are busy")
    );
    drop(borrowed);
    assert_eq!(
        owner
            .borrow()
            .test_captured_recovery_ticket(resident.entity_id()),
        Some(close)
    );
    assert!(root.test_shell_construction_retired());
    if scenario == ResidentScenario::StaleAttachment {
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign);
    }
    if scenario == ResidentScenario::CancelledAttachment {
        RunningProcessOwner::cancel_interrupted_exit_resident(owner, key, app).unwrap();
    }
    if scenario == ResidentScenario::CapacityAttachment {
        current.available_capacity = gpui_text_input::RangeSurfaceCharge { bytes: 0, items: 0 };
    }
    let result = owner.borrow_mut().attach_interrupted_exit_resident(
        request,
        key,
        root,
        adapters,
        &mut configurator,
        current,
        window,
        app,
    );
    assert_eq!(window.focused(app), focus);
    assert_eq!(resident.read(app).gpui_input(), input);
    assert!(!input.read(app).is_enabled());
    assert_eq!(mount.read(app).contribution().as_ref(), Some(resident));
    if scenario != ResidentScenario::Attach {
        assert!(result.is_err());
        assert!(adapters.is_some() && configurator.is_some());
        assert_eq!(
            resident
                .read(app)
                .recovery_snapshot()
                .unwrap()
                .close_ticket(),
            close
        );
        assert!(mount.read(app).selected_identity().is_none());
        assert!(root.test_shell_construction_retired());
        assert!(drafts.borrow().test_recovery_ready());
        assert_eq!(
            owner
                .borrow()
                .test_captured_recovery_ticket(resident.entity_id()),
            Some(close)
        );
        assert!(
            drafts.borrow().recovery_residents()
                == vec![(window.window_handle(), resident.entity_id(), close)]
        );
        return false;
    }
    let (fresh_close, record) =
        result.unwrap_or_else(|error| panic!("attachment refused: {error}"));
    assert_ne!(fresh_close, close);
    assert!(!drafts.borrow().test_recovery_ready());
    assert!(!root.test_shell_construction_retired());
    assert_eq!(
        owner
            .borrow()
            .test_captured_recovery_ticket(resident.entity_id()),
        Some(fresh_close)
    );
    assert!(
        drafts.borrow().recovery_residents()
            == vec![(window.window_handle(), resident.entity_id(), fresh_close)]
    );
    assert!(
        root.set_shutdown_interaction_gated(false, app)
            .unwrap_err()
            .contains("fresh appearance")
    );
    let selection = resident.read(app).selection_identity();
    assert_eq!(record.window_id(), selection.window_id());
    assert_eq!(record.selected_thread(), Some(selection.claim()));
    assert!(record.remembered_target().is_some());
    assert!(adapters.is_none() && configurator.is_none());
    assert!(resident.read(app).recovery_snapshot().is_none());
    assert_eq!(
        mount.read(app).selected_identity(),
        Some(resident.read(app).selection_identity())
    );
    assert!(
        owner
            .borrow()
            .interrupted_exit_resident_result(key)
            .is_err()
    );
    assert!(
        owner
            .borrow_mut()
            .attach_interrupted_exit_resident(
                request,
                key,
                root,
                adapters,
                &mut configurator,
                current,
                window,
                app,
            )
            .is_err()
    );
    assert!(
        owner
            .borrow_mut()
            .take_cancelled_resident_preparation(key)
            .is_none()
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    assert!(
        mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .is_err()
    );
    true
}
