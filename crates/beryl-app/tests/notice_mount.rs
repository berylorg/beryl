#![cfg(feature = "test-faults")]

#[path = "notice_mount/composer_feedback.rs"]
mod composer_feedback;
#[path = "notice_mount/mutation_completion.rs"]
mod mutation_completion;
#[path = "pending_composer_activation/support.rs"]
mod composer_support;
#[path = "main_window_creation/support.rs"]
mod creation_support;
#[path = "main_window_shell/support.rs"]
mod home_support;
#[path = "initial_composer/support.rs"]
mod initial_support;
#[path = "syndic_composer_history/support.rs"]
mod mutation_support;
#[path = "notice_mount/support.rs"]
mod support;

use beryl_app::main_window::*;
use beryl_app::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use initial_support::{Fixture, config, native_path, placement};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

fn record(window_id: WindowId, kind: NoticeKind, title: &str) -> NoticeRecord {
    NoticeRecord {
        window_id,
        condition: NoticeConditionId::new(),
        revision: 1,
        kind,
        content: NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            title,
            title,
        ),
    }
}

fn command_record(window_id: WindowId, title: &str, command: u64) -> NoticeRecord {
    let mut record = record(window_id, NoticeKind::Warning, title);
    record.content = record
        .content
        .clone()
        .with_commands(&[NoticeCommand::enabled(
            NoticeCommandId::new(command),
            "Recover",
        )])
        .expect("one bounded command");
    record
}

fn admitted(
    ingress: &MainWindowNoticeIngress,
    record: NoticeRecord,
    cx: &mut gpui::TestAppContext,
) -> NoticeRecordToken {
    cx.update(|app| match ingress.admit(record, app) {
        NoticeAdmission::Admitted(token) => token,
        outcome => panic!("notice admission failed: {outcome:?}"),
    })
}

fn projection(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &gpui::TestAppContext,
) -> Option<(NoticeVisibleToken, NoticeKind)> {
    window
        .read_with(cx, |root, _| {
            root.notice_projection()
                .map(|projection| (projection.token.clone(), projection.kind))
        })
        .expect("mounted root")
}

#[gpui::test]
fn mounted_projection_is_zero_or_one_and_preserves_shell_geometry(cx: &mut gpui::TestAppContext) {
    let mounted = support::mount(cx, 31);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let before = support::shell_geometry(mounted.window, cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);

    let warning = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "warning"),
        cx,
    );
    let warning_visible = support::visible_token(mounted.window, cx);
    support::draw(cx);
    let after_admission = support::shell_geometry(mounted.window, cx);
    let notice = support::notice_bounds(mounted.window, cx).expect("visible notice");
    assert_eq!(before, after_admission);
    assert!(notice.top() >= gpui::px(56.));
    assert_eq!(
        projection(mounted.window, cx),
        Some((
            support::visible_token(mounted.window, cx),
            NoticeKind::Warning
        ))
    );

    let error = admitted(&ingress, record(window_id, NoticeKind::Error, "error"), cx);
    support::draw(cx);
    assert_eq!(before, support::shell_geometry(mounted.window, cx));
    assert_eq!(
        projection(mounted.window, cx),
        Some((
            support::visible_token(mounted.window, cx),
            NoticeKind::Error
        ))
    );
    let stale = warning_visible;
    assert!(matches!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(stale), app)),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::StaleVisibility
        ))
    ));
    assert!(matches!(
        {
            let current = support::visible_token(mounted.window, cx);
            cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(current), app))
        },
        Ok(())
    ));
    support::draw(cx);
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Warning
    );
    assert!(matches!(
        cx.update(|app| ingress.remove(&warning, app)),
        Ok(())
    ));
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);
    assert_eq!(before, support::shell_geometry(mounted.window, cx));
    assert!(matches!(
        cx.update(|app| ingress.remove(&error, app)),
        Err(NoticeRejection::StaleRecord)
    ));
    support::finish(mounted, cx);
}

#[gpui::test]
fn queued_widget_dismissal_cannot_block_a_newer_same_record_close(cx: &mut gpui::TestAppContext) {
    let mounted = support::mount(cx, 46);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let token = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "first"),
        cx,
    );
    let first_visible = support::visible_token(mounted.window, cx);
    let widget = mounted
        .window
        .read_with(cx, |root, _| root.notice_widget())
        .expect("mounted notice widget");
    let updated = cx
        .update(|app| {
            widget.update(app, |widget, cx| {
                widget.request_dismissal_for_test(first_visible, cx);
            });
            ingress.update(
                &token,
                2,
                NoticeContent::new(
                    NoticeVariant::Warning,
                    NoticeDismissal::Dismissible,
                    "updated",
                    "updated",
                ),
                app,
            )
        })
        .expect("newer same-record revision");
    support::draw(cx);
    assert_eq!(
        support::visible_token(mounted.window, cx).record(),
        &updated
    );
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let latest_close = visual
        .debug_bounds("main-window-notice-close")
        .expect("newer close remains interactive");
    visual.simulate_click(latest_close.center(), gpui::Modifiers::none());
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);
    assert!(
        mounted
            .window
            .update(cx, |root, window, app| root
                .notice_safe_focus(app)
                .is_focused(window))
            .expect("safe focus state")
    );
    drop(visual);
    support::finish(mounted, cx);
}

#[gpui::test]
fn inert_mounted_notice_rejects_events_without_losing_its_projection(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 38);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let token = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "inert"),
        cx,
    );
    let visible = support::visible_token(mounted.window, cx);
    mounted
        .window
        .update(cx, |root, window, root_cx| {
            root.set_notices_inert(true, window, root_cx);
        })
        .expect("make mounted notice inert");
    assert!(matches!(
        cx.update(
            |app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible.clone()), app)
        ),
        Err(MainWindowNoticeRouteRejection::Inert)
    ));
    assert_eq!(projection(mounted.window, cx).unwrap().0, visible);

    mounted
        .window
        .update(cx, |root, window, root_cx| {
            root.set_notices_inert(false, window, root_cx);
        })
        .expect("restore mounted notice interaction");
    assert!(matches!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible), app)),
        Ok(())
    ));
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(matches!(
        cx.update(|app| ingress.remove(&token, app)),
        Err(NoticeRejection::StaleRecord)
    ));
    support::finish(mounted, cx);
}

#[gpui::test]
fn mounted_notice_reflows_with_the_window_without_shifting_shell_regions(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 39);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let viewport = gpui::size(gpui::px(320.), gpui::px(180.));
    cx.simulate_window_resize(mounted.window.into(), viewport);
    support::draw(cx);
    let before = support::shell_geometry(mounted.window, cx);
    let token = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "resized"),
        cx,
    );
    support::draw(cx);
    let notice = support::notice_bounds(mounted.window, cx).expect("resized notice");
    assert_eq!(support::shell_geometry(mounted.window, cx), before);
    assert!(notice.top() >= gpui::px(56.));
    assert!(notice.left() >= gpui::px(0.));
    assert!(notice.right() <= viewport.width);
    assert!(notice.bottom() <= viewport.height);

    assert!(matches!(
        cx.update(|app| ingress.remove(&token, app)),
        Ok(())
    ));
    support::draw(cx);
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);
    assert_eq!(support::shell_geometry(mounted.window, cx), before);
    support::finish(mounted, cx);
}

#[gpui::test]
fn appearance_target_retirement_disposes_the_mounted_notice_route(cx: &mut gpui::TestAppContext) {
    let mounted = support::mount(cx, 40);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let token = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "retire target"),
        cx,
    );
    assert!(projection(mounted.window, cx).is_some());

    mounted.appearance.update(cx, |owner, _| owner.retire());
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);
    assert!(matches!(
        cx.update(|app| ingress.remove(&token, app)),
        Err(NoticeRejection::Disposed)
    ));
    assert!(matches!(
        cx.update(|app| ingress.admit(
            record(window_id, NoticeKind::Error, "after retirement"),
            app
        )),
        NoticeAdmission::Rejected(NoticeRejection::Disposed)
    ));
    support::finish(mounted, cx);
}

#[gpui::test]
fn appearance_publication_updates_the_mounted_notice_with_the_shell_and_composer(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = support::mount(cx, 42);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let _token = admitted(
        &ingress,
        record(window_id, NoticeKind::Warning, "appearance publication"),
        cx,
    );
    let before = mounted
        .window
        .read_with(cx, |root, app| {
            let controller = root.controller().expect("controller");
            (
                controller.appearance().clone(),
                root.notice_widget().read(app).appearance().clone(),
            )
        })
        .expect("mounted appearance");
    assert!(Arc::ptr_eq(&before.0, &before.1));

    let current = support::publish_preview(&mut mounted, 42, cx);
    support::draw(cx);
    mounted
        .window
        .read_with(cx, |root, app| {
            let controller = root.controller().expect("controller");
            assert!(Arc::ptr_eq(controller.appearance(), &current));
            assert!(Arc::ptr_eq(
                root.notice_widget().read(app).appearance(),
                &current
            ));
            let composer = controller
                .composer_mount()
                .expect("mounted composer")
                .read(app)
                .contribution()
                .expect("live composer");
            assert!(composer.read(app).surface_snapshot(app).is_some());
        })
        .expect("updated mounted appearance");
    assert!(!Arc::ptr_eq(&before.0, &current));
    assert!(projection(mounted.window, cx).is_some());
    support::finish(mounted, cx);
}

#[gpui::test]
fn current_command_forwards_once_without_dismissal_and_stale_routes_are_rejected(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 41);
    let ingress = support::ingress(mounted.window, cx);
    let window_id = support::window_id(mounted.window, cx);
    let root = mounted.window.entity(cx).expect("mounted root entity");
    let commands = Rc::new(RefCell::new(Vec::new()));
    let observed = commands.clone();
    let _subscription = mounted
        .window
        .update(cx, |_, window, app| {
            window.subscribe(
                &root,
                app,
                move |_, event: &MainWindowNoticeOwnerCommand, _, _| {
                    observed.borrow_mut().push(event.clone());
                },
            )
        })
        .expect("root subscription");

    let token = admitted(&ingress, command_record(window_id, "current", 77), cx);
    let visible = support::visible_token(mounted.window, cx);
    assert_eq!(visible.record(), &token);
    let command_selector = support::command_selector(mounted.window, 0, cx);
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    let command = visual
        .debug_bounds(&command_selector)
        .expect("mounted enabled command");
    visual.simulate_click(command.center(), gpui::Modifiers::none());
    support::draw(cx);
    assert_eq!(
        commands.borrow().as_slice(),
        &[MainWindowNoticeOwnerCommand {
            token: visible.clone(),
            command: NoticeCommandId::new(77),
        }]
    );
    assert_eq!(projection(mounted.window, cx).unwrap().0, visible);
    drop(visual);

    let replacement = admitted(
        &ingress,
        record(window_id, NoticeKind::Error, "replacement"),
        cx,
    );
    assert!(matches!(
        cx.update(|app| ingress.dispatch(
            MainWindowNoticeWidgetEvent::Command {
                token: visible,
                command: NoticeCommandId::new(77),
            },
            app
        )),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::StaleVisibility
        ))
    ));
    assert_eq!(commands.borrow().len(), 1);
    assert!(matches!(
        cx.update(|app| ingress.remove(&replacement, app)),
        Ok(())
    ));
    assert!(matches!(
        cx.update(|app| ingress.remove(&token, app)),
        Ok(())
    ));
    drop((_subscription, root));
    support::finish(mounted, cx);
}

#[gpui::test]
fn two_windows_and_retired_owner_keep_exact_notice_routes_isolated(cx: &mut gpui::TestAppContext) {
    let mounted = support::mount(cx, 51);
    let second = support::mount_second(&mounted, cx);
    let first_ingress = support::ingress(mounted.window, cx);
    let second_ingress = support::ingress(second, cx);
    let first_window = support::window_id(mounted.window, cx);
    let second_window = support::window_id(second, cx);
    let first = admitted(
        &first_ingress,
        record(first_window, NoticeKind::Warning, "first"),
        cx,
    );
    let second_token = admitted(
        &second_ingress,
        record(second_window, NoticeKind::Error, "second"),
        cx,
    );
    assert_eq!(
        projection(mounted.window, cx).unwrap().1,
        NoticeKind::Warning
    );
    assert_eq!(projection(second, cx).unwrap().1, NoticeKind::Error);
    assert!(matches!(
        cx.update(|app| first_ingress.remove(&second_token, app)),
        Err(NoticeRejection::WrongWindow)
    ));
    assert_eq!(projection(second, cx).unwrap().0.record(), &second_token);

    mounted
        .window
        .update(cx, |root, window, root_cx| {
            root.retire_notices(window, root_cx)
        })
        .expect("retire first notice owner");
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert!(!support::widget_diagnostics(mounted.window, cx).visible);
    assert!(matches!(
        cx.update(|app| first_ingress.remove(&first, app)),
        Err(NoticeRejection::Disposed)
    ));
    assert_eq!(projection(second, cx).unwrap().0.record(), &second_token);
    support::finish(mounted, cx);
}
