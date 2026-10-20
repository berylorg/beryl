use super::flow_support as runtime;
use super::*;
use crate::discussion_presentation::{DiscussionComposerContribution, DiscussionStatus};
use beryl_state::BranchHandoffJobState;
use std::sync::{Arc, Mutex};

fn job(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    child: SyndicThreadId,
) -> Option<beryl_state::BranchHandoffJobRecord> {
    let owner = owner.borrow();
    if !owner.test_services_ready() {
        return None;
    }
    let graph = owner.test_services().graph()?;
    let latest = graph
        .state()
        .durable_jobs()
        .latest_attempt(graph.home(), child)
        .unwrap()?;
    graph
        .state()
        .durable_jobs()
        .job(graph.home(), latest.job_id())
        .unwrap()
}

async fn wait_job(
    window: WindowHandle<MainWindowShellRoot>,
    owner: &Rc<RefCell<RunningProcessOwner>>,
    child: SyndicThreadId,
    cx: &mut AsyncApp,
    ready: impl Fn(&beryl_state::BranchHandoffJobRecord) -> bool,
) -> beryl_state::BranchHandoffJobRecord {
    let mut captured = None;
    wait_until(
        window,
        cx,
        |_| {
            captured = job(owner, child).filter(&ready);
            captured.is_some()
        },
        "the exact durable job must remain observable in its original GUI service custody",
    )
    .await;
    captured.unwrap()
}

#[test]
fn native_discussion_conversation_defers_queued_resolution_then_hands_off_in_parent_order_and_archives()
 {
    require_fixture_ledger();
    let configured = Arc::new(Mutex::new(None));
    let preparation = configured.clone();
    run_mounted_prepared_runtime_home_retained(
        move |path| *preparation.lock().unwrap() = Some(runtime::seed(path)),
        move |owner, _, cx| {
            Box::pin(async move {
                let root = configured.lock().unwrap().take().unwrap();
                let initial = windows(&owner);
                assert_eq!(initial.len(), 2);
                let child_window = initial[0];
                let parent_window = initial[1];
                runtime::prepare_source(&owner, child_window, &root, cx).await;
                let panel = child_window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(child_window, &panel, cx, &["branch prefix"]).await;
                let parent = child_window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                invoke(child_window, &panel, cx).await;
                wait_until(
                    child_window,
                    cx,
                    |cx| {
                        child_window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app)
                                    .is_some_and(|thread| thread != parent)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "captured branch must finish original ordinary activation",
                )
                .await;
                let child = child_window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                runtime::select(parent_window, parent, cx).await;
                runtime::submit(child_window, "first ordinary discussion turn", cx).await;
                let first =
                    runtime::started(child_window, &root, "first ordinary discussion turn", cx)
                        .await;
                runtime::submit(child_window, "second queued discussion turn", cx).await;
                wait_until(child_window, cx, |_| {
                let owner = owner.borrow();
                if !owner.test_services_ready() { return false; }
                let Some(graph) = owner.test_services().graph() else { return false; };
                graph.syndic().input_gate(graph.home(), child, fixture::limit()).unwrap().unwrap().live_next_turn_count() == 1
            }, "the original structured nonsteerable response must leave accepted queued input").await;
                let deferred = runtime::response(
                    child_window,
                    &root,
                    &first,
                    "deferred resolution must not be admitted",
                    cx,
                )
                .await;
                assert_eq!(deferred["status"], "deferred_queued_input");
                assert!(job(&owner, child).is_none());
                wait_until(
                    child_window,
                    cx,
                    |cx| {
                        child_window
                            .read_with(cx, |root, app| {
                                root.test_discussion_presentation(app).is_some_and(
                                    |(snapshot, gated, enabled, _, fresh)| {
                                        snapshot.status() == Some(DiscussionStatus::Open)
                                            && enabled
                                            && !gated
                                            && fresh
                                            && snapshot.retry().is_none()
                                    },
                                )
                            })
                            .unwrap()
                    },
                    "queued-input deferral must publish the unchanged Open discussion presentation",
                )
                .await;
                runtime::complete(&root, &first);
                let second =
                    runtime::started(child_window, &root, "second queued discussion turn", cx)
                        .await;
                assert_eq!(first["thread"], second["thread"]);
                assert_ne!(first["turn"], second["turn"]);
                let requests = runtime::records(&root, "discussion-request-");
                let canonical = serde_json::to_value(
                    crate::conversation_tools::ConversationToolRegistry::canonical().specs(),
                )
                .unwrap();
                let profiles: Vec<_> = requests
                    .iter()
                    .filter_map(|request| request["params"].get("dynamicTools"))
                    .filter(|profile| profile.is_array())
                    .collect();
                assert!(
                    !profiles.is_empty(),
                    "ordinary lineage must advertise its canonical dynamic tools"
                );
                for profile in profiles {
                    assert_eq!(profile, &canonical);
                }
                let injections: Vec<_> = requests
                    .iter()
                    .filter(|request| request["method"] == "thread/inject_items")
                    .collect();
                assert_eq!(
                    injections.len(),
                    1,
                    "the captured assistant context must be injected once for both child turns"
                );
                let items = injections[0]["params"]["items"].as_array().unwrap();
                assert_eq!(items.len(), 1);
                assert_eq!(items[0]["role"], "assistant");
                assert_eq!(items[0]["content"][0]["type"], "output_text");
                let injected = items[0]["content"][0]["text"].as_str().unwrap();
                assert!(injected.starts_with("[Beryl discussion context v1]\n"));
                assert!(
                    injected.ends_with(&format!("\n\n{SELECTED}\n[/Beryl discussion context v1]"))
                );
                assert!(!injected.contains("first ordinary discussion turn"));
                assert!(!injected.contains("second queued discussion turn"));
                runtime::submit(parent_window, "earlier parent input", cx).await;
                let parent_first =
                    runtime::started(parent_window, &root, "earlier parent input", cx).await;
                runtime::submit(parent_window, "queued parent input before handoff", cx).await;
                wait_until(
                    parent_window,
                    cx,
                    |_| {
                        let owner = owner.borrow();
                        if !owner.test_services_ready() {
                            return false;
                        }
                        let Some(graph) = owner.test_services().graph() else {
                            return false;
                        };
                        graph
                            .syndic()
                            .input_gate(graph.home(), parent, fixture::limit())
                            .unwrap()
                            .unwrap()
                            .live_next_turn_count()
                            == 1
                    },
                    "the exact parent's earlier accepted input must settle into its ordinary queue",
                )
                .await;
                let parent_editor = composer(parent_window, cx).await;
                let parent_editor_id = parent_editor.entity_id();
                runtime::type_text(parent_window, &parent_editor, "preserved parent draft", cx)
                    .await;
                wait_until(
                    parent_window,
                    cx,
                    |cx| {
                        parent_editor
                            .read_with(cx, |editor, app| {
                                editor
                                    .selection_identity()
                                    .binding()
                                    .logical_extent()
                                    .logical_utf8_bytes()
                                    == "preserved parent draft".len() as u64
                                    && editor.gpui_input().read(app).is_semantically_quiescent()
                                    && !editor.test_has_active_flight()
                            })
                            .unwrap()
                    },
                    "parent draft must settle independently of handoff admission",
                )
                .await;
                let parent_draft = parent_editor
                    .read_with(cx, |editor, _| editor.selection_identity().binding().root())
                    .unwrap();
                let child_history = child_window
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                let resolution = "Exact conversational resolution: café 🌸 and literal [image:A].";
                let admitted =
                    runtime::response(child_window, &root, &second, resolution, cx).await;
                assert_eq!(admitted["status"], "admitted");
                let accepted = wait_job(child_window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::WaitingResolvingTurn)
                })
                .await;
                assert_eq!(
                    accepted.job_id().to_string(),
                    admitted["job_id"].as_str().unwrap()
                );
                assert_eq!(accepted.resolution().as_str(), resolution);
                assert!(matches!(
                    accepted.state(),
                    BranchHandoffJobState::WaitingResolvingTurn
                ));
                wait_until(
                    child_window,
                    cx,
                    |cx| {
                        child_window
                            .read_with(cx, |root, app| {
                                root.test_discussion_presentation(app).is_some_and(
                                    |(snapshot, gated, enabled, _, fresh)| {
                                        snapshot.status()
                                            == Some(DiscussionStatus::ResolutionPending)
                                            && gated
                                            && !enabled
                                            && fresh
                                    },
                                )
                            })
                            .unwrap()
                    },
                    "accepted tool intent must disable the same child composer",
                )
                .await;
                runtime::complete(&root, &second);
                wait_until(child_window, cx, |_| job(&owner, child).is_some_and(|job| matches!(job.state(), BranchHandoffJobState::WaitingParent)),
                "resolving turn completion must leave the original handoff waiting for its busy parent").await;
                assert!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .all(|turn| !turn["input"][0]["text"]
                            .as_str()
                            .unwrap()
                            .starts_with("Discussion resolution:\n\n"))
                );
                runtime::complete(&root, &parent_first);
                let parent_queued = runtime::started(
                    parent_window,
                    &root,
                    "queued parent input before handoff",
                    cx,
                )
                .await;
                wait_job(child_window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::WaitingParent)
                })
                .await;
                runtime::complete(&root, &parent_queued);
                let generated = runtime::started(
                    parent_window,
                    &root,
                    &format!("Discussion resolution:\n\n{resolution}"),
                    cx,
                )
                .await;
                wait_until(
                    child_window,
                    cx,
                    |_| {
                        job(&owner, child).is_some_and(|job| {
                            matches!(job.state(), BranchHandoffJobState::ParentActive { .. })
                        })
                    },
                    "ordinary generated parent execution must publish its exact CAS binding",
                )
                .await;
                let active = wait_job(child_window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::ParentActive { .. })
                })
                .await;
                assert_eq!(active.job_id(), accepted.job_id());
                assert_eq!(active.intent_id(), accepted.intent_id());
                assert_eq!(active.request(), accepted.request());
                assert_eq!(active.parent_thread_id(), parent);
                assert_eq!(
                    active.state().parent_cas().unwrap().turn_id().as_str(),
                    generated["turn"].as_str().unwrap()
                );
                assert_eq!(
                    active.state().parent_cas().unwrap().thread_id().as_str(),
                    generated["thread"].as_str().unwrap()
                );
                let generated_parent = active.state().parent().unwrap();
                let mut parent_input = None;
                wait_until(
                    child_window,
                    cx,
                    |_| {
                        let owner = owner.borrow();
                        if !owner.test_services_ready() {
                            return false;
                        }
                        let Some(graph) = owner.test_services().graph() else {
                            return false;
                        };
                        let input = graph
                            .syndic()
                            .accepted_input(
                                graph.home(),
                                generated_parent.accepted_input_id(),
                                fixture::limit(),
                            )
                            .unwrap()
                            .unwrap();
                        assert_eq!(input.thread_id(), parent);
                        assert!(
                            !graph
                                .syndic()
                                .thread_attributes(graph.home(), child, fixture::limit())
                                .unwrap()
                                .unwrap()
                                .archive()
                                .is_archived()
                        );
                        parent_input = Some(input);
                        true
                    },
                    "generated input observation must retain the original GUI service owner",
                )
                .await;
                assert!(parent_input.is_some());
                runtime::complete(&root, &generated);
                wait_until(child_window, cx, |cx| {
                job(&owner, child).is_some_and(|job| matches!(job.state(), BranchHandoffJobState::Succeeded { .. }))
                    && child_window.read_with(cx, |root, app| root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,_,fresh)| {
                        snapshot.status() == Some(DiscussionStatus::Archived) && snapshot.composer() == DiscussionComposerContribution::ReadOnly
                            && snapshot.retry().is_none() && gated && enabled && fresh
                    })).unwrap()
            }, "exact parent terminal success must atomically archive and publish the readonly child").await;
                let finished = wait_job(child_window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::Succeeded { .. })
                })
                .await;
                assert_eq!(finished.state().parent(), Some(generated_parent));
                assert_eq!(finished.job_id(), accepted.job_id());
                assert_eq!(finished.intent_id(), accepted.intent_id());
                assert_eq!(
                    child_window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(child)
                );
                assert_eq!(
                    parent_window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                assert_eq!(
                    child_window
                        .read_with(cx, |root, _| root.test_thread_navigation_history())
                        .unwrap(),
                    child_history
                );
                parent_window
                    .read_with(cx, |root, app| {
                        let editor = root.test_submission_composer(app).unwrap();
                        assert_eq!(editor.entity_id(), parent_editor_id);
                        assert_eq!(
                            editor.read(app).selection_identity().binding().root(),
                            parent_draft
                        );
                        assert_eq!(
                            editor
                                .read(app)
                                .selection_identity()
                                .binding()
                                .logical_extent()
                                .logical_utf8_bytes(),
                            "preserved parent draft".len() as u64
                        );
                    })
                    .unwrap();
                wait_until(child_window, cx, |cx| child_window.update(cx, |root, native, app| {
                root.test_thread_lineage_view().is_some_and(|lineage| lineage.read(app).test_parent_input(parent, native).is_some_and(|(_,_,available)| !available))
            }).unwrap(), "the parent open elsewhere must remain represented by its unavailable breadcrumb").await;
                for window in windows(&owner) {
                    with_native(window, cx, |native, app| {
                        native.draw_and_present_for_test(app)
                    })
                    .unwrap();
                }
                activate_exit(child_window, cx);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                loop {
                    let current = windows(&owner);
                    if current.is_empty() {
                        break;
                    }
                    cx.update(|app| {
                        for window in current {
                            app.update_window(window.into(), |_, native, app| {
                                native.draw_and_present_for_test(app);
                            })
                            .unwrap();
                        }
                    })
                    .unwrap();
                    if std::time::Instant::now() >= deadline {
                        let owner = owner.borrow();
                        panic!(
                            "ordinary Exit did not close the actual native residents: status={:?}, shutdown={:?}, session={:?}, failure={:?}, drafts={}",
                            owner.test_ordinary_command_status(),
                            owner.shutdown_status(),
                            owner.shutdown_session(),
                            owner.test_last_ordinary_command_failure(),
                            owner.test_shutdown_draft_diagnostics(),
                        );
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(20))
                        .await;
                }
            })
        },
    );
}
