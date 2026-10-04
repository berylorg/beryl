use super::*;
use beryl_app::{
    LifecycleYieldOutcome,
    lifecycle_attention::{
        LifecycleAttentionAttempt, LifecycleAttentionRecord, ProcessLifecycleAttentionPool,
    },
};
use beryl_model::{BerylHomeId, SyndicTurnId};

fn attention(
    pool: &ProcessLifecycleAttentionPool,
    turn: u8,
) -> (LifecycleAttentionAttempt, LifecycleAttentionRecord) {
    let attempt = pool
        .track_accepted_yield(
            BerylHomeId::from_bytes([1; 16]),
            SyndicThreadId::from_bytes([9; 16]),
            SyndicTurnId::from_bytes([turn; 16]),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    pool.report_terminal(&attempt);
    let record = pool
        .snapshot()
        .into_iter()
        .find(|record| record.turn_id() == SyndicTurnId::from_bytes([turn; 16]))
        .unwrap();
    (attempt, record)
}

fn offer(
    ingress: &MainWindowNoticeIngress,
    record: &LifecycleAttentionRecord,
    pool: &Arc<ProcessLifecycleAttentionPool>,
    cx: &mut gpui::TestAppContext,
) -> bool {
    let result = cx.update(|app| ingress.test_offer_lifecycle_attention(record, pool, app));
    support::draw(cx);
    result
}

#[gpui::test]
fn stale_displayed_lifecycle_notice_cannot_acknowledge_an_updated_or_successor_record(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 154);
    let ingress = support::ingress(mounted.window, cx);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let (attempt, first) = attention(&pool, 1);
    assert!(offer(&ingress, &first, &pool, cx));
    let stale = support::visible_token(mounted.window, cx);
    pool.report_terminal(&attempt);
    let updated = pool.snapshot()[0].clone();
    assert!(offer(&ingress, &updated, &pool, cx));
    assert!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(stale.clone()), app))
            .is_err()
    );
    assert_eq!(pool.snapshot(), vec![updated]);
    let displayed = support::visible_token(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(displayed.clone()), app))
        .unwrap();
    assert!(pool.snapshot().is_empty());
    let (_, successor) = attention(&pool, 2);
    assert!(offer(&ingress, &successor, &pool, cx));
    assert!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(displayed), app))
            .is_err()
    );
    assert_eq!(pool.snapshot(), vec![successor]);
    support::finish(mounted, cx);
}

#[gpui::test]
fn preemption_and_routing_removal_never_acknowledge_process_attention(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 158);
    let ingress = support::ingress(mounted.window, cx);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let (_, attention) = attention(&pool, 1);
    assert!(offer(&ingress, &attention, &pool, cx));
    let stale = support::visible_token(mounted.window, cx);
    let protected = admitted(
        &ingress,
        NoticeRecord {
            window_id: support::window_id(mounted.window, cx),
            condition: NoticeConditionId::new(),
            revision: 1,
            kind: NoticeKind::HomeFailure,
            content: NoticeContent::new(
                NoticeVariant::Error,
                NoticeDismissal::Persistent,
                "Home unavailable",
                "Recovery is pending.",
            ),
        },
        cx,
    );
    assert_ne!(support::visible_token(mounted.window, cx), stale);
    assert!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(stale), app))
            .is_err()
    );
    assert_eq!(pool.snapshot(), vec![attention.clone()]);
    cx.update(|app| ingress.remove(&protected, app)).unwrap();
    cx.update(|app| ingress.test_remove_lifecycle_attention(attention.token(), app));
    assert_eq!(pool.snapshot(), vec![attention.clone()]);
    assert!(offer(&ingress, &attention, &pool, cx));
    let displayed = support::visible_token(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(displayed), app))
        .unwrap();
    assert!(pool.snapshot().is_empty());
    support::finish(mounted, cx);
}

#[gpui::test]
fn lifecycle_notice_reroutes_to_second_window_without_focus_or_selection_effects(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 162);
    let second = support::mount_second(&mounted, cx);
    let first_ingress = support::ingress(mounted.window, cx);
    let second_ingress = support::ingress(second, cx);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let (_, record) = attention(&pool, 1);
    let first_selection = mounted
        .window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap();
    let second_selection = second
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap();
    let first_focus = mounted
        .window
        .read_with(cx, |root, app| root.notice_safe_focus(app))
        .unwrap();
    mounted
        .window
        .update(cx, |_, window, _| first_focus.focus(window))
        .unwrap();
    assert!(offer(&first_ingress, &record, &pool, cx));
    let stale = support::visible_token(mounted.window, cx);
    cx.update(|app| first_ingress.test_remove_lifecycle_attention(record.token(), app));
    assert!(offer(&second_ingress, &record, &pool, cx));
    assert!(
        cx.update(|app| first_ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(stale), app))
            .is_err()
    );
    assert_eq!(pool.snapshot(), vec![record]);
    mounted
        .window
        .update(cx, |_, window, _| assert!(first_focus.is_focused(window)))
        .unwrap();
    assert_eq!(
        mounted
            .window
            .read_with(cx, |root, app| root
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity())
            .unwrap(),
        first_selection
    );
    assert_eq!(
        second
            .read_with(cx, |root, app| root
                .controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity())
            .unwrap(),
        second_selection
    );
    assert_eq!(cx.windows().len(), 2);
    pool.close();
    support::finish(mounted, cx);
}

#[gpui::test]
fn closed_pool_and_retired_window_lifetimes_reject_late_offers_and_acknowledgements(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = support::mount(cx, 166);
    let ingress = support::ingress(mounted.window, cx);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let (_, record) = attention(&pool, 1);
    assert!(offer(&ingress, &record, &pool, cx));
    let displayed = support::visible_token(mounted.window, cx);
    mounted
        .window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    assert!(!offer(&ingress, &record, &pool, cx));
    assert!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(displayed), app))
            .is_err()
    );
    assert_eq!(pool.snapshot(), vec![record.clone()]);
    pool.close();
    assert!(!offer(&ingress, &record, &pool, cx));
    support::finish(mounted, cx);
}

#[gpui::test]
fn unchanged_omitted_attention_is_counted_once_until_a_new_report(cx: &mut gpui::TestAppContext) {
    let mounted = support::mount(cx, 170);
    let ingress = support::ingress(mounted.window, cx);
    let mut occupants = Vec::new();
    for _ in 0..beryl_app::main_window::NOTICE_GENERAL_CAPACITY {
        occupants.push(admitted(
            &ingress,
            NoticeRecord {
                window_id: support::window_id(mounted.window, cx),
                condition: NoticeConditionId::new(),
                revision: 1,
                kind: NoticeKind::Lifecycle,
                content: NoticeContent::new(
                    NoticeVariant::Info,
                    NoticeDismissal::Dismissible,
                    "Review ready",
                    "A thread is ready for review.",
                ),
            },
            cx,
        ));
    }
    let before = mounted
        .window
        .read_with(cx, |root, _| root.notice_diagnostics())
        .unwrap();
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let (attempt, record) = attention(&pool, 1);
    for _ in 0..4 {
        assert!(!offer(&ingress, &record, &pool, cx));
    }
    let omitted = mounted
        .window
        .read_with(cx, |root, _| root.notice_diagnostics())
        .unwrap();
    assert_eq!(omitted.omitted, before.omitted + 1);
    assert_eq!(omitted.retained_records, before.retained_records);
    assert_eq!(pool.snapshot(), vec![record.clone()]);
    cx.update(|app| ingress.remove(&occupants[0], app)).unwrap();
    assert!(!offer(&ingress, &record, &pool, cx));
    pool.report_terminal(&attempt);
    assert!(offer(&ingress, &pool.snapshot()[0], &pool, cx));
    assert_eq!(
        mounted
            .window
            .read_with(cx, |root, _| root.notice_diagnostics().omitted)
            .unwrap(),
        omitted.omitted
    );
    support::finish(mounted, cx);
}
