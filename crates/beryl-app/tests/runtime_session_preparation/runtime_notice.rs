use super::*;
use beryl_app::cas_projection::{RuntimeFailure, SelectedRuntimeFailureObservation};
use beryl_model::{WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::{InitializeThreadlessWindow, RememberedTarget, ReplaceWindowClaim};

pub(super) fn select(fixture: &Fixture) -> (WindowId, beryl_state::WindowClaimSelection) {
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let session = fixture.state.session();
    let window = WindowId::from_bytes([200; 16]);
    let placement = WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    );
    notice_shell::commit(
        home,
        session.initialize_threadless(
            session.revision(home).unwrap(),
            InitializeThreadlessWindow::new(window, placement),
        ),
    );
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    let execution = binding(fixture, 1);
    notice_shell::commit(
        home,
        session.replace_claim(
            session.revision(home).unwrap(),
            ReplaceWindowClaim::new(
                bootstrap.header().revision(),
                window,
                bootstrap.windows()[0].revision(),
                None,
                RememberedTarget::new(execution.runtime_id(), execution.root_id()),
                thread_id(1),
            ),
        ),
    );
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    (window, bootstrap.windows()[0].selected_thread().unwrap())
}

#[test]
fn actual_runtime_failure_reader_preserves_exact_failure_and_weak_ownership() {
    let (mut fixture, sessions, _attention) = fixture(6);
    eprintln!(
        "owned runtime-notice fixture: {}",
        fixture.root(1).parent().unwrap().display()
    );
    let (window, claim) = select(&fixture);
    let session = fixture.state.session();
    let reader = fixture.service().runtime_failure_reader();
    let counts = reader.test_resource_strong_counts();
    let cloned = reader.clone();
    assert_eq!(reader.test_resource_strong_counts(), counts);
    assert_eq!(
        reader.observe(&session, window, claim),
        SelectedRuntimeFailureObservation::Unknown
    );
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| {
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    let original = sessions
        .runtime_failure(binding(&fixture, 1).runtime_id())
        .unwrap();
    assert_eq!(original.failure(), RuntimeFailure::Admission);
    assert_eq!(
        reader.observe(&session, window, claim),
        SelectedRuntimeFailureObservation::Unavailable {
            execution: binding(&fixture, 1),
            failure: original,
        }
    );
    assert_eq!(
        reader.observe(&session, WindowId::from_bytes([201; 16]), claim),
        SelectedRuntimeFailureObservation::Unknown
    );
    {
        let live = fixture.service().live_home_command().unwrap();
        let home = live.home();
        let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
        let replacement = binding(&fixture, 2);
        notice_shell::commit(
            home,
            session.replace_claim(
                session.revision(home).unwrap(),
                ReplaceWindowClaim::new(
                    bootstrap.header().revision(),
                    window,
                    bootstrap.windows()[0].revision(),
                    Some(claim),
                    RememberedTarget::new(replacement.runtime_id(), replacement.root_id()),
                    thread_id(2),
                ),
            ),
        );
        let current = session.minimal_bootstrap(home).unwrap().unwrap().windows()[0]
            .selected_thread()
            .unwrap();
        assert_eq!(
            reader.observe(&session, window, claim),
            SelectedRuntimeFailureObservation::Unknown
        );
        assert_eq!(
            reader.observe(&session, window, current),
            SelectedRuntimeFailureObservation::Unavailable {
                execution: replacement,
                failure: original,
            }
        );
    }
    close(&mut fixture, &sessions);
    assert_eq!(
        cloned.observe(&session, window, claim),
        SelectedRuntimeFailureObservation::Unknown
    );
    assert_eq!(reader.test_resource_strong_counts(), [0, 0]);
}

#[gpui::test]
fn actual_runtime_failure_mount_is_persistent_and_publication_fenced(
    cx: &mut gpui::TestAppContext,
) {
    use beryl_app::main_window::{NoticeCommandState, NoticeDismissal, NoticeKind};
    let (mut fixture, sessions, _attention) = fixture(6);
    eprintln!(
        "owned runtime-notice fixture: {}",
        fixture.root(1).parent().unwrap().display()
    );
    let (prepared, _custody) = notice_shell::prepare(&fixture);
    let shell = notice_shell::mount(prepared, cx);
    let window = shell.window();
    let lifetime = Arc::new(());
    let worker = fixture.service().exact_stop_worker();
    window
        .update(cx, |root, window, cx| {
            root.test_mount_exact_status_worker(
                worker,
                Arc::downgrade(&lifetime),
                fixture.state.session(),
                window,
                cx,
            )
        })
        .unwrap();
    notice_shell::wait(window, cx, |root, app| {
        root.test_exact_status_selection_present(app)
    });
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    begin(&fixture, 1);
    wait_until(|| {
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    notice_shell::wait(window, cx, |root, _| {
        root.notice_projection()
            .is_some_and(|projection| projection.kind == NoticeKind::RuntimeUnavailable)
    });
    let token = window
        .read_with(cx, |root, _| {
            let projection = root.notice_projection().unwrap();
            assert_eq!(projection.content.dismissal, NoticeDismissal::Persistent);
            assert!(
                projection
                    .content
                    .title()
                    .as_str()
                    .contains(&binding(&fixture, 1).runtime_id().to_string())
            );
            let command = projection.content.commands().next().unwrap();
            assert_eq!(command.label().as_str(), "Retry");
            assert_eq!(command.state(), NoticeCommandState::Disabled);
            assert!(
                command
                    .disabled_reason()
                    .unwrap()
                    .as_str()
                    .contains("does not currently qualify for recovery")
            );
            projection.token.clone()
        })
        .unwrap();
    let stamp = window
        .read_with(cx, |root, app| {
            root.test_exact_status_observation_stamp(app)
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            root.test_apply_runtime_failure_observation(
                stamp,
                SelectedRuntimeFailureObservation::Unknown,
                window,
                cx,
            )
        })
        .unwrap();
    for _ in 0..3 {
        notice_shell::tick(window, cx);
    }
    window
        .read_with(cx, |root, _| {
            assert_eq!(root.notice_projection().unwrap().token, token)
        })
        .unwrap();

    let original = sessions
        .runtime_failure(binding(&fixture, 1).runtime_id())
        .unwrap();
    sessions
        .retry_runtime_session(original, thread_id(1), binding(&fixture, 1))
        .unwrap();
    begin(&fixture, 1);
    wait_until(|| {
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready() && failure != original)
    });
    let latest = sessions
        .runtime_failure(binding(&fixture, 1).runtime_id())
        .unwrap();
    notice_shell::wait(window, cx, |root, _| {
        root.test_runtime_failure_snapshot() == Some(latest)
    });
    let current_stamp = window
        .read_with(cx, |root, app| {
            root.test_exact_status_observation_stamp(app)
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            root.test_apply_runtime_failure_observation(
                current_stamp,
                SelectedRuntimeFailureObservation::Unavailable {
                    execution: binding(&fixture, 1),
                    failure: original,
                },
                window,
                cx,
            );
            assert_eq!(root.test_runtime_failure_snapshot(), Some(latest));
        })
        .unwrap();

    let mount = window
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let selected = mount.read_with(cx, |mount, _| mount.selected_identity().unwrap());
    let control = beryl_app::cas_projection::NativeLineageRecoveryControl::for_test(
        std::num::NonZeroUsize::new(1).unwrap(),
    );
    control
        .install_route_for_test(
            selected.claim().thread_id(),
            selected.claim().thread_id(),
            beryl_model::BindingRevision::new(1).unwrap(),
            beryl_app::cas_projection::NativeLineageOperation::Resume,
            1,
            beryl_app::cas_projection::NativeLineageHistoryRecovery::Available,
        )
        .unwrap();
    mount.update(cx, |mount, cx| {
        mount.attach_native_lineage_recovery(control, cx)
    });
    notice_shell::wait(window, cx, |_, app| {
        mount
            .read(app)
            .test_native_lineage_mount_diagnostics()
            .prompt_published
    });
    window
        .read_with(cx, |root, app| {
            assert!(root.test_exact_status_selection_present(app));
            assert!(mount.read(app).contribution().is_none());
            assert_eq!(
                root.notice_projection().unwrap().kind,
                NoticeKind::RuntimeUnavailable
            );
        })
        .unwrap();

    drop(lifetime);
    notice_shell::wait(window, cx, |root, _| root.notice_projection().is_none());
    window
        .update(cx, |root, window, cx| {
            root.test_apply_runtime_failure_observation(
                current_stamp,
                SelectedRuntimeFailureObservation::Unavailable {
                    execution: binding(&fixture, 1),
                    failure: latest,
                },
                window,
                cx,
            );
            assert!(root.notice_projection().is_none());
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    drop(shell);
    close(&mut fixture, &sessions);
}
