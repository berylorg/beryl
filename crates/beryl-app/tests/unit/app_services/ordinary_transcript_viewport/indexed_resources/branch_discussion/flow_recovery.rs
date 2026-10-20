use super::flow_support as runtime;
use super::*;
use crate::{
    discussion_presentation::{
        DiscussionComposerContribution as Composer, DiscussionStatus as Status,
    },
    running_owner::InterruptedExitRecoveryOutcome,
};
use beryl_state::{BranchHandoffJobRecord, BranchHandoffJobState};
use std::sync::{Arc, Mutex};

fn current_job(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    child: SyndicThreadId,
) -> Option<BranchHandoffJobRecord> {
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
    ready: impl Fn(&BranchHandoffJobRecord) -> bool,
) -> BranchHandoffJobRecord {
    let mut captured = None;
    wait_until(
        window,
        cx,
        |_| {
            captured = current_job(owner, child).filter(&ready);
            captured.is_some()
        },
        "the original exact job must remain in GUI service custody for its observation",
    )
    .await;
    captured.unwrap()
}

#[test]
fn native_discussion_actual_nondispatch_recovery_requires_mounted_retry_of_the_same_parent_input() {
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
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app)
                                    .is_some_and(|thread| thread != parent)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "the actual captured discussion must finish ordinary activation",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                runtime::submit(
                    window,
                    "resolve this discussion after a safe runtime refusal",
                    cx,
                )
                .await;
                let resolving = runtime::started(
                    window,
                    &root,
                    "resolve this discussion after a safe runtime refusal",
                    cx,
                )
                .await;
                wait_until(window, cx, |_| {
                    let retained = owner.borrow();
                    if !retained.test_services_ready() { return false; }
                    let Some(graph) = retained.test_services().graph() else { return false; };
                    let storage = graph.syndic();
                    let binding = match storage.current_binding(graph.home(), child, fixture::limit()) {
                        Ok(Some(binding)) => binding,
                        Ok(None) | Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                        Err(error) => panic!("the exact resolving binding observation failed: {error:?}"),
                    };
                    let syndic_storage::BindingState::Active(active) = binding.binding().state() else { return false; };
                    let Some(cas) = storage.active_cas_turn(graph.home(), active.snapshot_id(), fixture::limit()).unwrap() else { return false; };
                    let source = syndic_storage::CasItemSource::new(
                        syndic_storage::CasTurnSource::new(cas.cas_thread_id().clone(), cas.cas_turn_id().clone()),
                        beryl_model::CasItemId::new(format!("user-{}", resolving["turn"].as_str().unwrap())).unwrap(),
                    );
                    let echo = match storage.capture_item(graph.home(), &source, fixture::limit()) {
                        Ok(Some(echo)) => echo,
                        Ok(None) | Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                        Err(error) => panic!("the exact checked user echo observation failed: {error:?}"),
                    };
                    cas.cas_thread_id().as_str() == resolving["thread"].as_str().unwrap()
                        && cas.cas_turn_id().as_str() == resolving["turn"].as_str().unwrap()
                        && echo.item().turn_id() == active.turn_id()
                        && echo.item().provider_lifecycle() == syndic_storage::ProviderItemLifecycle::Completed
                        && storage.input_gate(graph.home(), child, fixture::limit()).unwrap().is_some_and(|gate| gate.state() == &syndic_storage::InputGateState::Steerable(active.turn_id()))
                }, "the exact resolving turn and echoed input must finish canonical activation before its single resolution request").await;
                runtime::publish(&root, "discussion-reject-generated-once", b"");
                let resolution = "Retain this exact resolution and original generated parent input across recovery.";
                let response = runtime::response(window, &root, &resolving, resolution, cx).await;
                assert_eq!(response["status"], "admitted");
                runtime::complete(&root, &resolving);
                wait_until(window, cx, |cx| {
                current_job(&owner, child).is_some_and(|job| matches!(job.state(), BranchHandoffJobState::RetryableFailed { .. }))
                    && window.read_with(cx, |root, app| root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,_,fresh)| {
                        snapshot.status() == Some(Status::HandoffFailed) && snapshot.retry().is_some() && gated && !enabled && fresh
                    })).unwrap()
            }, "the actual generated turn refusal must become a durable exact retryable failure").await;
                let original_job = wait_job(window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::RetryableFailed { .. })
                })
                .await;
                let parent_identity = original_job
                    .state()
                    .parent()
                    .expect("safe refusal must retain admitted parent input and turn");
                assert!(original_job.state().parent_cas().is_none());
                assert_eq!(
                    original_job.job_id().to_string(),
                    response["job_id"].as_str().unwrap()
                );
                assert_eq!(original_job.resolution().as_str(), resolution);
                let rejected = runtime::records(&root, "discussion-rejected-");
                assert_eq!(rejected.len(), 1);
                assert_eq!(
                    rejected[0]["params"]["input"][0]["text"],
                    format!("Discussion resolution:\n\n{resolution}")
                );
                let prior = window
                    .read_with(cx, |root, app| {
                        root.test_discussion_presentation(app).unwrap()
                    })
                    .unwrap();
                let history = window
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                let mut captured = None;
                wait_until(
                    window,
                    cx,
                    |_| {
                        let retained = owner.borrow();
                        if !retained.test_services_ready() {
                            return false;
                        }
                        let Some(graph) = retained.test_services().graph() else {
                            return false;
                        };
                        let input = graph
                            .syndic()
                            .accepted_input(
                                graph.home(),
                                parent_identity.accepted_input_id(),
                                fixture::limit(),
                            )
                            .unwrap()
                            .unwrap();
                        let turn = graph
                            .syndic()
                            .turn(graph.home(), parent_identity.turn_id(), fixture::limit())
                            .unwrap()
                            .unwrap();
                        captured = Some((graph.home().service_reference(), input, turn));
                        true
                    },
                    "failure capture must retain the original GUI service owner",
                )
                .await;
                let (home, original_input, original_turn) = captured.unwrap();
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
                                recovered = Some((
                                    graph
                                        .state()
                                        .durable_jobs()
                                        .job(graph.home(), original_job.job_id())
                                        .unwrap()
                                        .unwrap(),
                                    graph
                                        .syndic()
                                        .accepted_input(
                                            graph.home(),
                                            parent_identity.accepted_input_id(),
                                            fixture::limit(),
                                        )
                                        .unwrap()
                                        .unwrap(),
                                    graph
                                        .syndic()
                                        .turn(
                                            graph.home(),
                                            parent_identity.turn_id(),
                                            fixture::limit(),
                                        )
                                        .unwrap()
                                        .unwrap(),
                                ));
                            }
                        }
                        Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                            panic!("actual nondispatch recovery unavailable: {error}")
                        }
                        Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                            panic!("actual nondispatch recovery cancelled")
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
                let (recovered_job, recovered_input, recovered_turn) = recovered.expect(
                    "actual graph replacement must complete within its original recovery bound",
                );
                assert_eq!(recovered_job, original_job);
                assert_eq!(recovered_input, original_input);
                assert_eq!(recovered_turn, original_turn);
                assert_eq!(windows(&owner), original_windows);
                let published = window
                    .read_with(cx, |root, _| {
                        root.test_recovered_discussion_presentation().unwrap()
                    })
                    .unwrap();
                assert_eq!(published.0.home_id(), prior.0.home_id());
                assert_ne!(published.0.home_generation(), prior.0.home_generation());
                assert_ne!(
                    published.0.service_generation(),
                    prior.0.service_generation()
                );
                assert_eq!(published.0.status(), Some(Status::HandoffFailed));
                assert_eq!(published.0.composer(), Composer::Inert);
                assert_eq!(
                    (published.1, published.2, published.3, published.4),
                    (true, false, prior.3, true)
                );
                assert_eq!(published.0.retry().unwrap().job_id(), original_job.job_id());
                assert_eq!(
                    published.0.retry().unwrap().job_revision(),
                    original_job.revision()
                );
                assert!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .all(|turn| !turn["input"][0]["text"]
                            .as_str()
                            .unwrap()
                            .starts_with("Discussion resolution:\n\n")),
                    "graph replacement must not retry the original generated input automatically"
                );
                assert_eq!(runtime::records(&root, "discussion-rejected-").len(), 1);
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .update(cx, |root, native, app| {
                                assert!(!root.test_discussion_retry_pending());
                                let ready = root.test_discussion_presentation(app).is_some_and(
                                    |(snapshot, gated, enabled, _, fresh)| {
                                        snapshot.retry().is_some_and(|descriptor| {
                                            descriptor.thread_id() == child
                                                && descriptor.job_id() == original_job.job_id()
                                                && descriptor.job_revision()
                                                    == original_job.revision()
                                        }) && snapshot.attempt() == published.0.attempt()
                                            && snapshot.home_id() == published.0.home_id()
                                            && snapshot.home_generation()
                                                == published.0.home_generation()
                                            && snapshot.service_generation()
                                                == published.0.service_generation()
                                            && snapshot.status() == Some(Status::HandoffFailed)
                                            && gated
                                            && !enabled
                                            && fresh
                                            && snapshot.current()
                                    },
                                ) && root
                                    .test_discussion_observation_target(app)
                                    .is_some_and(|target| target.3.current());
                                if !ready {
                                    return false;
                                }
                                root.test_retry_current_discussion(native, app);
                                if !root.test_discussion_retry_pending() {
                                    return false;
                                }
                                root.test_retry_current_discussion(native, app);
                                assert!(root.test_discussion_retry_pending());
                                true
                            })
                            .unwrap()
                    },
                    "the current mounted Retry must admit one flight before its duplicate click",
                )
                .await;
                let parent = runtime::started(
                    window,
                    &root,
                    &format!("Discussion resolution:\n\n{resolution}"),
                    cx,
                )
                .await;
                wait_until(window, cx, |_| current_job(&owner, child).is_some_and(|job| matches!(job.state(), BranchHandoffJobState::ParentActive { .. })),
                "explicit mounted Retry must resume ordinary execution of the same admitted parent").await;
                let active = wait_job(window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::ParentActive { .. })
                })
                .await;
                assert_eq!(active.job_id(), original_job.job_id());
                assert_eq!(active.intent_id(), original_job.intent_id());
                assert_eq!(active.request(), original_job.request());
                assert_eq!(active.resolution(), original_job.resolution());
                assert_eq!(active.attempt_ordinal(), original_job.attempt_ordinal());
                assert_eq!(active.state().parent(), Some(parent_identity));
                assert_eq!(
                    active.state().parent_cas().unwrap().turn_id().as_str(),
                    parent["turn"].as_str().unwrap()
                );
                runtime::complete(&root, &parent);
                wait_until(window, cx, |cx| {
                current_job(&owner, child).is_some_and(|job| matches!(job.state(), BranchHandoffJobState::Succeeded { .. }))
                    && window.read_with(cx, |root, app| root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,editor,fresh)| {
                        snapshot.status() == Some(Status::Archived) && snapshot.retry().is_none() && gated && enabled && fresh && editor == prior.3
                    })).unwrap()
            }, "the same retried parent terminal success must archive the preserved selected discussion").await;
                let finished = wait_job(window, &owner, child, cx, |job| {
                    matches!(job.state(), BranchHandoffJobState::Succeeded { .. })
                })
                .await;
                assert_eq!(finished.state().parent(), Some(parent_identity));
                assert_eq!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .filter(|turn| turn["input"][0]["text"]
                            == format!("Discussion resolution:\n\n{resolution}"))
                        .count(),
                    1
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(child)
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_thread_navigation_history())
                        .unwrap(),
                    history
                );
                drop(home);
                activate_exit(window, cx);
            })
        },
    );
}
