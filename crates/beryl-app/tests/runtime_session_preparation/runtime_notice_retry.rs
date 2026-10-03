use super::*;
use beryl_app::cas_projection::{RuntimeFailureSnapshot, SelectedRuntimeFailureObservation};
use beryl_app::main_window::*;
use gpui::EntityInputHandler;

#[derive(Default)]
struct RetryWorkerCustody(Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>);

impl Drop for RetryWorkerCustody {
    fn drop(&mut self) {
        if let Some(worker) = self.0.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

struct ProjectionRelease(std::path::PathBuf);

impl Drop for ProjectionRelease {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, "ready");
    }
}

fn mount_worker(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) -> (Arc<()>, RetryWorkerCustody) {
    let lifetime = Arc::new(());
    let custody = RetryWorkerCustody::default();
    window
        .update(cx, |root, window, cx| {
            root.test_use_runtime_retry_native_worker(Arc::downgrade(&custody.0));
            root.test_mount_exact_status_worker_with_retry(
                fixture.service().exact_stop_worker(),
                fixture.service().selected_runtime_retry_worker(sessions),
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
    (lifetime, custody)
}

fn fail_runtime(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
) -> RuntimeFailureSnapshot {
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    begin(fixture, 1);
    wait_until(|| {
        sessions
            .runtime_failure(binding(fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    sessions
        .runtime_failure(binding(fixture, 1).runtime_id())
        .unwrap()
}

fn retry_token(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) -> NoticeVisibleToken {
    notice_shell::wait(window, cx, |root, _| {
        root.notice_projection().is_some_and(|projection| {
            projection.kind == NoticeKind::RuntimeUnavailable
                && projection.content.commands().next().unwrap().state()
                    == NoticeCommandState::Enabled
        })
    });
    window
        .read_with(cx, |root, _| {
            root.notice_projection().unwrap().token.clone()
        })
        .unwrap()
}

fn ingress(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) -> MainWindowNoticeIngress {
    window
        .update(cx, |root, window, cx| root.notice_ingress(window, cx))
        .unwrap()
}

fn activate(
    ingress: &MainWindowNoticeIngress,
    token: &NoticeVisibleToken,
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|app| {
        ingress.dispatch(
            MainWindowNoticeWidgetEvent::Command {
                token: token.clone(),
                command: NoticeCommandId::new(1),
            },
            app,
        )
    })
    .unwrap();
}

fn remove_window(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut gpui::TestAppContext) {
    window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}

#[gpui::test]
fn mounted_retry_preserves_live_editor_edits_focus_and_history_and_ignores_old_failure(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, sessions, _attention) = fixture(12);
    let (prepared, _custody) = notice_shell::prepare(&fixture);
    let shell = notice_shell::mount(prepared, cx);
    let window = shell.window();
    let (lifetime, _worker_custody) = mount_worker(&fixture, &sessions, window, cx);
    let original = fail_runtime(&fixture, &sessions);
    let requested = retry_token(window, cx);
    let ingress = ingress(window, cx);
    let mount = window
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let editor = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = editor.read_with(cx, |editor, _| editor.gpui_input());
    let before = editor.read_with(cx, |editor, _| editor.selection_identity());
    fs::write(fixture.root(1).join("fixture-mode"), "pause-projection").unwrap();
    let _release = ProjectionRelease(fixture.root(1).join("release-projection"));
    let idle_election = sessions.install_idle_election_pause_for_test(before.claim().thread_id());
    activate(&ingress, &requested, cx);
    window
        .read_with(cx, |root, _| {
            let pending = root.notice_projection().unwrap();
            assert!(requested.record().same_identity(pending.token.record()));
            assert_eq!(pending.content.dismissal, NoticeDismissal::Persistent);
            assert_eq!(
                pending.content.commands().next().unwrap().state(),
                NoticeCommandState::Disabled
            );
            assert!(
                pending
                    .content
                    .commands()
                    .next()
                    .unwrap()
                    .disabled_reason()
                    .unwrap()
                    .as_str()
                    .contains("pending")
            );
        })
        .unwrap();
    activate(&ingress, &requested, cx);
    notice_shell::wait(window, cx, |_, _| {
        fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    });
    let focus = window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.focus(window);
                input.replace_text_in_range(None, "local draft", window, cx);
            });
            window.focused(app).unwrap()
        })
        .unwrap();
    notice_shell::wait(window, cx, |_, app| {
        let current = editor.read(app).selection_identity();
        current.binding().candidate().candidate_generation()
            > before.binding().candidate().candidate_generation()
            && input
                .read(app)
                .surface()
                .is_some_and(|surface| surface.binding() == current.binding().range_binding())
    });
    let edited = editor.read_with(cx, |editor, app| {
        (
            editor.selection_identity(),
            editor.surface_snapshot(app).unwrap().source_selection,
            input.read(app).history_frontier(),
        )
    });
    assert_eq!(
        edited.0.binding().logical_extent().logical_utf8_bytes(),
        "local draft".len() as u64
    );
    window
        .read_with(cx, |root, _| {
            assert!(
                root.test_runtime_retry_diagnostics().0,
                "local edits cancelled Retry presentation: {:?}",
                root.test_runtime_retry_diagnostics()
            );
        })
        .unwrap();
    fs::write(fixture.root(1).join("release-projection"), "ready").unwrap();
    notice_shell::wait(window, cx, |_, _| idle_election.try_entered());
    notice_shell::wait(window, cx, |root, _| {
        !root.test_runtime_retry_diagnostics().0
    });
    if window
        .read_with(cx, |root, _| root.notice_projection().is_some())
        .unwrap()
    {
        window
            .read_with(cx, |root, _| {
                assert!(
                    root.test_runtime_retry_diagnostics()
                        .2
                        .as_ref()
                        .is_some_and(
                            |feedback| feedback.contains("no longer has current authority")
                        )
                );
            })
            .unwrap();
        let explicit_retry = retry_token(window, cx);
        activate(&ingress, &explicit_retry, cx);
    }
    notice_shell::wait(window, cx, |root, _| {
        let diagnostics = root.test_runtime_retry_diagnostics();
        assert!(
            diagnostics.2.is_none(),
            "Retry completed without usability: {diagnostics:?}; notice={:?}",
            root.notice_projection()
                .map(|notice| notice.content.detail().as_str().to_owned())
        );
        root.notice_projection().is_none()
    });
    idle_election.release();
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.contribution()),
        Some(editor.clone())
    );
    editor.read_with(cx, |editor, app| {
        assert_eq!(editor.gpui_input().entity_id(), input.entity_id());
        assert_eq!(
            (
                editor.selection_identity(),
                editor.surface_snapshot(app).unwrap().source_selection,
                input.read(app).history_frontier()
            ),
            edited
        );
    });
    window
        .update(cx, |_, window, _| assert!(focus.is_focused(window)))
        .unwrap();
    let stamp = window
        .read_with(cx, |root, app| {
            root.test_exact_status_observation_stamp(app)
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            for observation in [
                SelectedRuntimeFailureObservation::Unknown,
                SelectedRuntimeFailureObservation::Unavailable {
                    execution: binding(&fixture, 1),
                    failure: original,
                },
            ] {
                root.test_apply_runtime_failure_observation(stamp, observation, window, cx);
                assert!(root.notice_projection().is_none());
            }
        })
        .unwrap();
    let evidence: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("runtime-projection-evidence.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["method"], "thread/start");
    remove_window(window, cx);
    drop((editor, input, mount, ingress, lifetime, shell));
    close(&mut fixture, &sessions);
}

#[gpui::test]
fn mounted_retry_failure_revises_one_notice_and_requires_another_explicit_activation(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, sessions, _attention) = fixture(12);
    let (prepared, _custody) = notice_shell::prepare(&fixture);
    let shell = notice_shell::mount(prepared, cx);
    let window = shell.window();
    let (lifetime, _worker_custody) = mount_worker(&fixture, &sessions, window, cx);
    let original = fail_runtime(&fixture, &sessions);
    let requested = retry_token(window, cx);
    let ingress = ingress(window, cx);
    activate(&ingress, &requested, cx);
    notice_shell::wait(window, cx, |root, _| {
        root.test_runtime_failure_snapshot()
            .is_some_and(|current| current != original && current.retry_ready())
            && root.notice_projection().is_some_and(|projection| {
                projection.content.commands().next().unwrap().state() == NoticeCommandState::Enabled
            })
    });
    let latest = window
        .read_with(cx, |root, _| {
            let projection = root.notice_projection().unwrap();
            assert!(requested.record().same_identity(projection.token.record()));
            assert!(projection.content.detail().as_str().contains("Retry"));
            assert_eq!(root.notice_diagnostics().retained_records, 1);
            root.test_runtime_failure_snapshot().unwrap()
        })
        .unwrap();
    for _ in 0..8 {
        notice_shell::tick(window, cx);
    }
    window
        .read_with(cx, |root, _| {
            assert_eq!(root.test_runtime_failure_snapshot(), Some(latest))
        })
        .unwrap();
    assert!(
        cx.update(|app| ingress.dispatch(
            MainWindowNoticeWidgetEvent::Command {
                token: requested,
                command: NoticeCommandId::new(1),
            },
            app
        ))
        .is_err()
    );
    remove_window(window, cx);
    drop((ingress, lifetime, shell));
    close(&mut fixture, &sessions);
}

#[gpui::test]
fn mounted_retry_retirement_revokes_presentation_without_retiring_shared_runtime(
    cx: &mut gpui::TestAppContext,
) {
    for close_window in [false, true] {
        let (mut fixture, sessions, _attention) = fixture(12);
        let (prepared, _custody) = notice_shell::prepare(&fixture);
        let shell = notice_shell::mount(prepared, cx);
        let window = shell.window();
        let (lifetime, _worker_custody) = mount_worker(&fixture, &sessions, window, cx);
        fail_runtime(&fixture, &sessions);
        let requested = retry_token(window, cx);
        let ingress = ingress(window, cx);
        fs::write(fixture.root(1).join("fixture-mode"), "pause-projection").unwrap();
        let _release = ProjectionRelease(fixture.root(1).join("release-projection"));
        activate(&ingress, &requested, cx);
        notice_shell::wait(window, cx, |_, _| {
            fixture
                .root(1)
                .join("runtime-projection-evidence.json")
                .exists()
        });
        let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
        if close_window {
            window
                .update(cx, |root, window, cx| root.retire_notices(window, cx))
                .unwrap();
        }
        drop(lifetime);
        notice_shell::wait(window, cx, |root, _| root.notice_projection().is_none());
        assert!(process.running());
        fs::write(fixture.root(1).join("release-projection"), "ready").unwrap();
        for _ in 0..8 {
            notice_shell::tick(window, cx);
        }
        window
            .read_with(cx, |root, _| assert!(root.notice_projection().is_none()))
            .unwrap();
        remove_window(window, cx);
        drop((ingress, shell));
        close(&mut fixture, &sessions);
        process.assert_exited();
    }
}
