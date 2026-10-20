use super::code_long_line::{retire_code_fixture, wait_until, with_native};
use super::narrative_native::{
    copy_both, drag, key, point_at, ready, record, require_fixture_ledger, writer,
};
use super::*;
use crate::support::exact_cas;
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId, SyndicThreadId};
use syndic_storage::*;

#[path = "branch_discussion/capacity.rs"]
mod capacity;
#[path = "branch_discussion/code.rs"]
mod code;
#[path = "branch_discussion/code_interaction.rs"]
mod code_interaction;
#[path = "branch_discussion/code_later_fragment.rs"]
mod code_later_fragment;
#[path = "branch_discussion/code_multiline.rs"]
mod code_multiline;
#[path = "branch_discussion/eligibility.rs"]
mod eligibility;
#[path = "branch_discussion/fixture.rs"]
pub(super) mod fixture;
#[path = "branch_discussion/late_retry.rs"]
mod late_retry;
#[path = "branch_discussion/lifecycle.rs"]
mod lifecycle;
#[path = "branch_discussion/outcomes.rs"]
mod outcomes;
#[path = "branch_discussion/scope.rs"]
mod scope;
#[path = "branch_discussion/status.rs"]
mod status;
#[path = "branch_discussion/status_initial.rs"]
mod status_initial;
#[path = "branch_discussion/status_retry.rs"]
mod status_retry;
#[path = "branch_discussion/status_support.rs"]
mod status_support;

pub(super) const SOURCE: &str = "branch prefix **café 🌸 words** suffix\r\n";
pub(super) const SELECTED: &str = "**café 🌸 words**";

async fn code_delivery_ready(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &gpui::Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    head: &TranscriptViewHeadRecord,
    summary: &HistorySummaryRecord,
    cx: &mut AsyncApp,
) {
    wait_until(window, cx, |cx| {
        window.read_with(cx, |root, _| !root.test_transcript_delivery_pending() && root.test_transcript_published_source_matches(head, summary)).unwrap()
            && panel.read_with(cx, |panel, _| panel.code_commands_current_for_test() && panel.resource_demand().is_none() && panel.narrative_demand().is_none() && panel.diagnostic_snapshot().code_panels.first().is_some_and(|code| code.layout_state == "ready")).unwrap()
    }, "the exact published Code source must settle all queued and staged deliveries before a new invocation").await;
}

fn open(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &gpui::Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    with_native(window, cx, |native, app| {
        let (id, _, display) = record(panel.read(app), "branch prefix");
        let start = display.find("café 🌸 words").unwrap();
        drag(
            native,
            app,
            point_at(panel.read(app), &id, start),
            point_at(panel.read(app), &id, start + "café 🌸 words".len()),
        );
        key(native, app, "shift-f10");
    })
    .unwrap();
}

pub(super) async fn invoke(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &gpui::Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    open(window, panel, cx);
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    panel
                        .transcript_menu_rows_for_test()
                        .iter()
                        .any(|row| row.0 == "Discuss in new branch" && row.1)
                })
                .unwrap()
        },
        "the exact terminal assistant source must qualify off the GUI thread",
    )
    .await;
    with_native(window, cx, |native, app| {
        assert_eq!(
            panel.read(app).transcript_menu_rows_for_test(),
            vec![
                ("Quote".into(), true, None),
                ("Discuss in new branch".into(), true, None)
            ]
        );
        key(native, app, "down");
        key(native, app, "enter");
    })
    .unwrap();
}

#[test]
fn native_captured_branch_creates_exact_context_and_activates_through_ordinary_selection() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app)
                                    .is_some_and(|current| current != parent)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "the exact created discussion must complete ordinary activation",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let owner = owner.borrow();
                let graph = owner.test_services().graph().unwrap();
                let thread = graph
                    .syndic()
                    .thread(graph.home(), child, fixture::limit())
                    .unwrap()
                    .unwrap();
                assert_eq!(thread.parent_thread_id(), Some(parent));
                let context = graph
                    .syndic()
                    .context_envelope(
                        graph.home(),
                        thread.context_owner_id().unwrap(),
                        fixture::limit(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(context.envelope().text().as_str(), SELECTED);
                assert_eq!(context.envelope().descriptor().source().thread_id(), parent);
                assert_eq!(
                    context.envelope().descriptor().source().item_id(),
                    fixture::assistant()
                );
                assert_eq!(
                    context.envelope().descriptor().source().range().len(),
                    SELECTED.len() as u64
                );
                assert!(
                    graph
                        .state()
                        .catalog()
                        .row(
                            graph.home(),
                            child,
                            beryl_state::CatalogPointReadLimit::schema_maximum()
                        )
                        .unwrap()
                        .is_some()
                );
                assert_eq!(
                    graph
                        .syndic()
                        .thread(graph.home(), parent, fixture::limit())
                        .unwrap()
                        .unwrap()
                        .committed_tail(),
                    thread.committed_tail()
                );
                drop(owner);
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_noncommit_preserves_selection_and_fresh_menu_attempt() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(|cancellation| cancellation.cancel())),
                            None,
                        )
                    })
                    .unwrap();
                let (copied, _) = writer(&panel, cx);
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, _| {
                                root.test_captured_branch_state()
                                    .is_some_and(|state| state.0 == "NotCommitted")
                            })
                            .unwrap()
                    },
                    "proven branch noncommit must settle",
                )
                .await;
                let (_, child, captured) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(captured.text.as_str(), SELECTED);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    copy_both(native, app, &copied, SELECTED, "café 🌸 words");
                })
                .unwrap();
                let owner = owner.borrow();
                let graph = owner.test_services().graph().unwrap();
                assert!(
                    graph
                        .syndic()
                        .thread(graph.home(), child, fixture::limit())
                        .unwrap()
                        .is_none()
                );
                assert!(
                    graph
                        .state()
                        .catalog()
                        .row(
                            graph.home(),
                            child,
                            beryl_state::CatalogPointReadLimit::schema_maximum()
                        )
                        .unwrap()
                        .is_none()
                );
                drop(owner);
                assert!(
                    !panel
                        .read_with(cx, |panel, _| panel.quote_menu_open_for_test())
                        .unwrap()
                );
                invoke(window, &panel, cx).await;
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| root.coherent_viewed_thread(app).is_some_and(|thread| thread != parent && thread != child) && !root.test_running_thread_activation_pending()).unwrap(), "a deliberate fresh menu attempt after proven noncommit must own a fresh creation request").await;
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_rejects_nonterminal_assistant_before_durable_creation() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| seed_history_text(path, 1, Some("parent conversation")),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["parent conversation"]).await;
                fixture::append_live(&owner, SOURCE);
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                open(window, &panel, cx);
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                !panel.transcript_menu_rows_for_test().iter().any(|row| {
                                    row.0 == "Discuss in new branch"
                                        && (row.1
                                            || row.2.as_deref().is_none_or(|reason| {
                                                !reason.contains("terminal reply")
                                            }))
                                })
                            })
                            .unwrap()
                    },
                    "nonterminal assistant selection must stay unavailable",
                )
                .await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
