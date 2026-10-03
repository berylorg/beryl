use super::*;

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Revisions,
    ResolvedFifo,
    VolatileDismissal,
}

#[gpui::test]
fn mounted_stop_notice_tracks_waiting_revision_after_anchor_loss_and_preemption(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted_with_notices(cx, false, false, None, Some(Scenario::Revisions));
}

#[gpui::test]
fn mounted_stop_notices_keep_resolved_outcomes_in_fifo_and_dismiss_only_exact_presentation(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted_with_notices(cx, false, false, None, Some(Scenario::ResolvedFifo));
}

#[gpui::test]
fn mounted_volatile_notice_dismissal_cannot_restore_cached_or_fresh_stop_eligibility(
    cx: &mut gpui::TestAppContext,
) {
    run_mounted_with_notices(cx, false, false, None, Some(Scenario::VolatileDismissal));
}

pub(super) fn exercise(
    scenario: Scenario,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    worker: &ExactStopWorker,
    thread: SyndicThreadId,
    publication: &mut Option<Arc<()>>,
    session: beryl_state::SessionState,
) {
    match scenario {
        Scenario::Revisions => revisions(window, cx, worker, thread, publication),
        Scenario::ResolvedFifo => resolved_fifo(window, cx, worker, thread, publication),
        Scenario::VolatileDismissal => {
            volatile_dismissal(window, cx, worker, thread, publication, session)
        }
    }
}

fn feedback(
    worker: &ExactStopWorker,
    thread: SyndicThreadId,
    state: ExactStopFeedbackState,
    cx: &mut gpui::TestAppContext,
) -> ExactStopFeedback {
    support::join(
        support::worker({
            let worker = worker.clone();
            move || {
                let ExactSoftStopAvailability::Eligible(eligibility) =
                    worker.exact_soft_stop_eligibility(thread)
                else {
                    panic!("exact notice source eligibility");
                };
                eligibility.test_projected_feedback(state).unwrap()
            }
        }),
        cx,
    )
}

fn retain(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    feedback: ExactStopFeedback,
) {
    window
        .update(cx, |root, _, cx| {
            root.test_retain_exact_status_feedback(feedback, cx)
        })
        .unwrap();
    support::draw(window, cx);
}

fn ingress(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) -> MainWindowNoticeIngress {
    window
        .update(cx, |root, window, cx| root.notice_ingress(window, cx))
        .unwrap()
}

fn force_anchor_loss(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) {
    window
        .update(cx, |root, window, cx| {
            let stamp = root.test_exact_status_observation_stamp(cx);
            root.test_apply_exact_status_observation(
                stamp,
                ExactSelectedOperationSnapshot::unavailable(),
                window,
                cx,
            );
        })
        .unwrap();
    support::draw(window, cx);
}

fn close_notice(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut gpui::TestAppContext) {
    support::draw(window, cx);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let close = visual.debug_bounds("main-window-notice-close").unwrap();
    visual.simulate_click(close.center(), gpui::Modifiers::none());
}

fn revisions(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    worker: &ExactStopWorker,
    thread: SyndicThreadId,
    publication: &mut Option<Arc<()>>,
) {
    let feedback = feedback(worker, thread, ExactStopFeedbackState::Waiting, cx);
    retain(window, cx, feedback.clone());
    support::wait(window, cx, |d| {
        d.0 == "working" && d.4 == Some(ExactStopFeedbackState::Waiting)
    });
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let anchor = visual.debug_bounds("main-window-status-turn").unwrap();
    visual.simulate_click(anchor.center(), gpui::Modifiers::none());
    visual.simulate_keystrokes("escape");
    assert!(
        window
            .read_with(&visual, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    drop(visual);
    force_anchor_loss(window, cx);
    let first = window
        .read_with(cx, |root, _| {
            let notice = root.notice_projection().unwrap();
            assert_eq!(notice.kind, NoticeKind::ExactStopFeedback);
            assert_eq!(notice.content.variant, NoticeVariant::Warning);
            assert_eq!(notice.content.dismissal, NoticeDismissal::Persistent);
            notice.token.clone()
        })
        .unwrap();
    let ingress = ingress(window, cx);
    assert_eq!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(first.clone()), app)),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::Persistent
        ))
    );
    support::wait(window, cx, |d| {
        d.0 == "working" && d.4 == Some(ExactStopFeedbackState::Waiting)
    });
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    drop(publication.take());
    support::wait(window, cx, |d| d.0 == "Unknown");
    let restored = window
        .read_with(cx, |root, _| {
            root.notice_projection().unwrap().token.clone()
        })
        .unwrap();
    assert_eq!(first.record().condition(), restored.record().condition());
    let home = NoticeConditionId::new();
    let home_token = cx.update(|app| {
        let record = NoticeRecord {
            window_id: WindowId::from_bytes([190; 16]),
            condition: home,
            revision: 1,
            kind: NoticeKind::HomeFailure,
            content: NoticeContent::new(
                NoticeVariant::Error,
                NoticeDismissal::Persistent,
                "Home condition",
                "Higher-priority test condition",
            ),
        };
        let NoticeAdmission::Admitted(token) = ingress.admit(record, app) else {
            panic!("home preemption");
        };
        token
    });
    feedback.test_resolve_projected_feedback(ExactStopFeedbackState::Interrupted);
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(250));
        support::draw(window, cx);
    }
    assert_eq!(
        window
            .read_with(cx, |root, _| root.notice_projection().unwrap().kind)
            .unwrap(),
        NoticeKind::HomeFailure
    );
    cx.update(|app| ingress.remove(&home_token, app)).unwrap();
    let resolved = window
        .read_with(cx, |root, _| {
            let notice = root.notice_projection().unwrap();
            assert_eq!(notice.content.variant, NoticeVariant::Info);
            assert_eq!(notice.content.dismissal, NoticeDismissal::Dismissible);
            assert_eq!(notice.content.commands().count(), 0);
            notice.token.clone()
        })
        .unwrap();
    assert!(resolved.record().same_identity(restored.record()));
    assert!(resolved.record().revision() > restored.record().revision());
    close_notice(window, cx);
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_exact_status_diagnostics().3)
            .unwrap(),
        0
    );
    retain(window, cx, feedback.clone());
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_exact_status_diagnostics().3)
            .unwrap(),
        0
    );
}

fn resolved_fifo(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    worker: &ExactStopWorker,
    thread: SyndicThreadId,
    publication: &mut Option<Arc<()>>,
) {
    use ExactStopFeedbackState as State;
    let states = [
        (State::DurableNondispatch, NoticeVariant::Error),
        (State::VolatileNondispatch, NoticeVariant::Error),
        (State::RequestNotAdmitted, NoticeVariant::Error),
        (State::Interrupted, NoticeVariant::Info),
        (State::Completed, NoticeVariant::Info),
        (State::Failed, NoticeVariant::Error),
        (State::UnknownTerminal, NoticeVariant::Warning),
        (State::AuthorityLost, NoticeVariant::Warning),
    ];
    let mut retained = Vec::new();
    for (state, _) in states {
        retained.push(feedback(worker, thread, state, cx));
    }
    window
        .update(cx, |root, _, cx| {
            for feedback in retained {
                root.test_retain_exact_status_feedback(feedback, cx);
            }
        })
        .unwrap();
    drop(publication.take());
    support::wait(window, cx, |d| d.0 == "Unknown");
    for (index, (state, variant)) in states.into_iter().enumerate() {
        window
            .read_with(cx, |root, _| {
                let notice = root.notice_projection().unwrap();
                assert_eq!(notice.kind, NoticeKind::ExactStopFeedback);
                assert_eq!(notice.content.variant, variant, "{state:?}");
                assert_eq!(notice.content.dismissal, NoticeDismissal::Dismissible);
                assert_eq!(notice.content.commands().count(), 0);
                assert!(!notice.content.detail().as_str().contains("available again"));
                assert_eq!(root.test_exact_status_diagnostics().3, states.len() - index);
            })
            .unwrap();
        close_notice(window, cx);
    }
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
}

fn volatile_dismissal(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    worker: &ExactStopWorker,
    thread: SyndicThreadId,
    publication: &mut Option<Arc<()>>,
    session: beryl_state::SessionState,
) {
    let feedback = feedback(
        worker,
        thread,
        ExactStopFeedbackState::VolatileNondispatch,
        cx,
    );
    retain(window, cx, feedback.clone());
    support::wait(window, cx, |d| d.0 == "working" && !d.2);
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    force_anchor_loss(window, cx);
    window
        .read_with(cx, |root, _| {
            let notice = root.notice_projection().unwrap();
            assert_eq!(notice.content.variant, NoticeVariant::Error);
            assert_eq!(notice.content.commands().count(), 0);
        })
        .unwrap();
    close_notice(window, cx);
    support::wait(window, cx, |d| d.0 == "working" && d.3 == 0 && !d.2);
    let mut same_origin = support::join(
        support::worker({
            let worker = worker.clone();
            move || worker.selected_operation_snapshot(thread)
        }),
        cx,
    );
    assert!(same_origin.operation_active);
    same_origin.state = ExactParentState::Unknown;
    same_origin.operation_active = false;
    same_origin.stop =
        ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::AuthorityUnavailable);
    window
        .update(cx, |root, window, cx| {
            let stamp = root.test_exact_status_observation_stamp(cx);
            root.test_apply_exact_status_observation(stamp, same_origin, window, cx);
        })
        .unwrap();
    support::draw(window, cx);
    support::wait(window, cx, |d| d.0 == "working" && d.3 == 0 && !d.2);
    drop(publication.take());
    support::draw(window, cx);
    assert!(
        !window
            .read_with(cx, |root, _| root.test_exact_status_diagnostics().2)
            .unwrap()
    );
    *publication = Some(Arc::new(()));
    window
        .update(cx, |root, window, cx| {
            root.test_mount_exact_status_worker(
                worker.clone(),
                Arc::downgrade(publication.as_ref().unwrap()),
                session,
                window,
                cx,
            )
        })
        .unwrap();
    support::wait(window, cx, |d| d.0 == "working" && d.3 == 0 && !d.2);
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_none())
            .unwrap()
    );
    retain(window, cx, feedback);
    support::wait(window, cx, |d| d.0 == "working" && d.3 == 0 && !d.2);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let anchor = visual.debug_bounds("main-window-status-turn").unwrap();
    visual.simulate_click(anchor.center(), gpui::Modifiers::none());
    visual.simulate_keystrokes("enter space");
    assert_eq!(
        window
            .read_with(&visual, |root, _| root.test_exact_status_diagnostics().3)
            .unwrap(),
        0
    );
}
