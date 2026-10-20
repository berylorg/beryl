use super::flow_support as runtime;
use super::*;
use crate::{
    discussion_presentation::DiscussionStatus, running_owner::InterruptedExitRecoveryOutcome,
};
use beryl_state::BranchHandoffJobState;
use std::sync::{Arc, Mutex};

#[test]
fn native_discussion_tool_uncertainty_retains_original_admission_until_graph_recovery_proves_exact_new()
 {
    require_fixture_ledger();
    let configured = Arc::new(Mutex::new(None));
    let preparation = configured.clone();
    run_mounted_prepared_runtime_home_retained(
        move |path| *preparation.lock().unwrap() = Some(runtime::seed(path)),
        move |owner, faults, cx| {
            Box::pin(async move {
                let root = configured.lock().unwrap().take().unwrap();
                let original_windows = windows(&owner);
                let window = original_windows[0];
                runtime::prepare_source(&owner, window, &root, cx).await;
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                invoke(window, &panel, cx).await;
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| {
                root.coherent_viewed_thread(app).is_some_and(|thread| thread != parent) && !root.test_running_thread_activation_pending()
            }).unwrap(), "captured discussion must finish ordinary activation before the original uncertainty cut").await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                runtime::submit(
                    window,
                    "resolve once while retaining any uncertain admission",
                    cx,
                )
                .await;
                let resolving = runtime::started(
                    window,
                    &root,
                    "resolve once while retaining any uncertain admission",
                    cx,
                )
                .await;
                let unviewed = SyndicThreadId::from_bytes([237; 16]);
                runtime::select(window, unviewed, cx).await;
                // The sequential poll's new positive proof follows every old child read.
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| {
                    root.coherent_viewed_thread(app) == Some(unviewed)
                        && root.test_submission_observation(app).is_ok()
                        && root.test_discussion_presentation(app).is_some_and(|tuple| tuple.0.status().is_none())
                }).unwrap(), "a completed new-selection status poll must retire the old child observation before the tool call").await;
                let prior = window
                    .read_with(cx, |root, app| {
                        root.test_discussion_presentation(app).unwrap()
                    })
                    .unwrap();
                let history = window
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                let resolution = "One exact proposal survives the unknown command outcome without another tool request.";
                faults
                    .fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
                let response = runtime::response(window, &root, &resolving, resolution, cx).await;
                assert_eq!(
                    response["status"], "outcome_unresolved",
                    "the fault must hit the actual tool admission rather than an unrelated background command"
                );
                assert!(
                    response.get("job_id").is_none(),
                    "unconfirmed admission must not claim success to the model"
                );
                let mut original = None;
                wait_until(window, cx, |_| {
                let retained = owner.borrow();
                if !retained.test_services_ready() { return false; }
                let Some(graph) = retained.test_services().graph() else { return false; };
                let Some(latest) = graph.state().durable_jobs().latest_attempt(graph.home(), child).unwrap() else { return false; };
                let job = graph.state().durable_jobs().job(graph.home(), latest.job_id()).unwrap().unwrap();
                assert_eq!(job.request().cas_thread_id().as_str(), resolving["thread"].as_str().unwrap());
                assert_eq!(job.request().cas_turn_id().as_str(), resolving["turn"].as_str().unwrap());
                assert_eq!(job.request().tool_call_id().as_str(), format!("resolution-{}", resolving["turn"].as_str().unwrap()));
                assert_eq!(job.resolution().as_str(), resolution);
                assert!(matches!(job.state(), BranchHandoffJobState::WaitingResolvingTurn));
                let operations = retained.test_services().settlements.clone();
                assert!(operations.retained_audit(job.job_id()).is_some(), "the original process, independent of the delivered response, must retain the exact operation audit");
                original = Some((graph.home().service_reference(), operations, job));
                true
            }, "the actual uncertain command must expose its exact retained operation identity").await;
                let (home, operations, original_job) = original.unwrap();
                assert!(prior.0.status().is_none());
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(unviewed)
                );
                assert!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .all(|turn| !turn["input"][0]["text"]
                            .as_str()
                            .unwrap()
                            .starts_with("Discussion resolution:\n\n"))
                );
                runtime::complete(&root, &resolving);
                wait_until(window, cx, |_| {
                let retained = owner.borrow();
                if !retained.test_services_ready() { return false; }
                let Some(graph) = retained.test_services().graph() else { return false; };
                graph.syndic().turn_state(graph.home(), original_job.resolving_turn_id(), fixture::limit()).unwrap().unwrap().end_status() == Some(TurnEndStatus::complete())
            }, "the original resolving capture may settle its own terminal history while admission remains unconfirmed").await;
                assert!(operations.retained_audit(original_job.job_id()).is_some());
                owner.borrow_mut().test_enable_resident_frame_capture();
                faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
                assert!(home.home_revision().is_err());
                cx.update(|app| {
                    RunningProcessOwner::test_observe_running_home_failure(&owner, app)
                })
                .unwrap();
                let mut recovered = None;
                for _ in 0..750 {
                    for window in &original_windows {
                        with_native(*window, cx, |native, app| {
                            native.draw_and_present_for_test(app)
                        })
                        .unwrap();
                    }
                    cx.update(|app| {
                        RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app);
                        RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                    })
                    .unwrap();
                    let retained = owner.borrow();
                    match retained.automatic_recovery_outcome().as_deref() {
                        Some(InterruptedExitRecoveryOutcome::Completed)
                            if retained.test_services_ready() =>
                        {
                            if let Some(graph) = retained.test_services().graph() {
                                let job = graph
                                    .state()
                                    .durable_jobs()
                                    .job(graph.home(), original_job.job_id())
                                    .unwrap()
                                    .unwrap();
                                recovered = Some(job);
                            }
                        }
                        Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                            panic!("exact original admission recovery unavailable: {error}")
                        }
                        Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                            panic!("exact original admission recovery cancelled")
                        }
                        _ => {}
                    }
                    drop(retained);
                    if recovered.is_some() {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                let recovered = recovered.unwrap_or_else(|| {
                    let owner = owner.borrow();
                    panic!("the original process audit must settle through actual candidate recovery: stage={}, shutdown={:?}, drafts={}", owner.test_thread_creation_recovery_stage(), owner.shutdown_status(), owner.test_shutdown_draft_diagnostics());
                });
                assert_eq!(recovered.job_id(), original_job.job_id());
                assert_eq!(recovered.intent_id(), original_job.intent_id());
                assert_eq!(recovered.request(), original_job.request());
                assert_eq!(recovered.resolution(), original_job.resolution());
                assert_eq!(recovered.attempt_ordinal(), original_job.attempt_ordinal());
                assert!(
                    operations.retained_audit(original_job.job_id()).is_none(),
                    "exact New qualification must release original process command custody without a test reconciliation call"
                );
                let publication = window
                    .read_with(cx, |root, _| {
                        root.test_recovered_discussion_presentation().unwrap()
                    })
                    .unwrap();
                assert_eq!(publication.0.home_id(), prior.0.home_id());
                assert_ne!(publication.0.home_generation(), prior.0.home_generation());
                assert_ne!(
                    publication.0.service_generation(),
                    prior.0.service_generation()
                );
                assert_eq!(publication.3, prior.3);
                assert_eq!(windows(&owner), original_windows);
                let generated = runtime::started(
                    window,
                    &root,
                    &format!("Discussion resolution:\n\n{resolution}"),
                    cx,
                )
                .await;
                runtime::complete(&root, &generated);
                let mut final_job = None;
                wait_until(window, cx, |cx| {
                let retained = owner.borrow();
                if !retained.test_services_ready() { return false; }
                let Some(graph) = retained.test_services().graph() else { return false; };
                let job = graph.state().durable_jobs().job(graph.home(), original_job.job_id()).unwrap().unwrap();
                if !matches!(job.state(), BranchHandoffJobState::Succeeded { .. }) { return false; }
                final_job = Some(job);
                window.read_with(cx, |root, app| root.test_discussion_presentation(app).is_some_and(|(snapshot,_,_,editor,_)| {
                    snapshot.status().is_none() && editor == prior.3
                })).unwrap()
            }, "the one original recovered admission must reach one parent success and archive").await;
                let final_job = final_job.unwrap();
                assert_eq!(final_job.job_id(), original_job.job_id());
                assert_eq!(final_job.intent_id(), original_job.intent_id());
                assert_eq!(final_job.request(), original_job.request());
                assert_eq!(
                    final_job.state().parent_cas().unwrap().turn_id().as_str(),
                    generated["turn"].as_str().unwrap()
                );
                assert_eq!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .filter(|turn| turn["input"][0]["text"]
                            == format!("Discussion resolution:\n\n{resolution}"))
                        .count(),
                    1
                );
                assert_eq!(
                    runtime::records(&root, "discussion-response-").len(),
                    1,
                    "graph replacement must not send another resolution-tool request or response"
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(unviewed)
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_thread_navigation_history())
                        .unwrap(),
                    history
                );
                runtime::select(window, child, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.test_discussion_presentation(app).is_some_and(
                                    |(snapshot, gated, enabled, _, fresh)| {
                                        snapshot.status() == Some(DiscussionStatus::Archived)
                                            && snapshot.retry().is_none()
                                            && gated
                                            && enabled
                                            && fresh
                                    },
                                )
                            })
                            .unwrap()
                    },
                    "only explicit navigation may mount the archived background discussion",
                )
                .await;
                drop((home, operations));
                activate_exit(window, cx);
            })
        },
    );
}
