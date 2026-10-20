use super::flow_support as runtime;
use super::*;
use crate::{
    discussion_presentation::{
        DiscussionComposerContribution as Composer, DiscussionStatus as Status,
    },
    discussion_settlement::{
        DiscussionSettlementAuditOutcome, DiscussionSettlementError, presentation::LocalDisposition,
    },
    running_owner::{InterruptedExitRecoveryOutcome, RecoveryPreparationFailure},
};
use beryl_home_store::{CommandOutcome, HomeCommand};
use std::sync::{Arc, Mutex};

#[test]
fn native_discussion_original_admission_collision_survives_refused_graph_recovery_without_resend() {
    require_fixture_ledger();
    let configured = Arc::new(Mutex::new(None));
    let preparation = configured.clone();
    run_mounted_prepared_runtime_home_unavailable(
        move |path| *preparation.lock().unwrap() = Some(runtime::seed(path)),
        move |owner, faults, cx| {
            Box::pin(async move {
                let root = configured.lock().unwrap().take().unwrap();
                let original_windows = windows(&owner);
                let window = original_windows[0];
                let unrelated = original_windows[1];
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
                    "the captured child must finish its original ordinary activation",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                runtime::submit(
                    window,
                    "retain this exact proposal if its participants disagree",
                    cx,
                )
                .await;
                let resolving = runtime::started(
                    window,
                    &root,
                    "retain this exact proposal if its participants disagree",
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
                }).unwrap(), "the sequential original poll must finish a new-selection observation before uncertain background resolution").await;
                window
                    .update(cx, |root, _, _| root.test_suspend_discussion_refresh(true))
                    .unwrap();
                runtime::select(window, child, cx).await;
                faults
                    .fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
                let resolution =
                    "The exact conflicting admission remains unavailable and must never be resent.";
                assert_eq!(
                    runtime::response(window, &root, &resolving, resolution, cx).await["status"],
                    "outcome_unresolved"
                );
                let mut capture = None;
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
                        let Some(latest) = graph
                            .state()
                            .durable_jobs()
                            .latest_attempt(graph.home(), child)
                            .unwrap()
                        else {
                            return false;
                        };
                        let job = graph
                            .state()
                            .durable_jobs()
                            .job(graph.home(), latest.job_id())
                            .unwrap()
                            .unwrap();
                        let operations = retained.test_services().settlements.clone();
                        assert!(operations.retained_audit(job.job_id()).is_some());
                        capture = Some((
                            graph.home().service_reference(),
                            graph.state().clone(),
                            graph.syndic().clone(),
                            operations,
                            job,
                        ));
                        true
                    },
                    "the actual uncertain admission must retain its original process-owned audit",
                )
                .await;
                let (home, state, storage, operations, original_job) = capture.unwrap();
                runtime::complete(&root, &resolving);
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
                        graph.syndic().turn_state(
                            graph.home(), original_job.resolving_turn_id(), fixture::limit(),
                        ).unwrap().unwrap().end_status() == Some(TurnEndStatus::complete())
                            && graph.cas().accepted_input_scheduler_diagnostics().workers_active() == 0
                            && operations.retained_audit(original_job.job_id()).is_some()
                    },
                    "the original child must finish terminal capture and join while its exact admission audit retains the job flight",
                )
                .await;
                let worker_home = home.service_reference();
                let worker_storage = storage.clone();
                cx.background_executor()
                    .spawn(async move {
                        for _ in 0..8 {
                            let gate = worker_storage
                                .discussion_handoff_gate(&worker_home, child, fixture::limit())
                                .unwrap()
                                .unwrap();
                            let release = worker_storage
                                .prepare_discussion_handoff(
                                    &worker_home,
                                    DiscussionHandoffMutation::Release { expected: gate },
                                )
                                .unwrap();
                            let mut command = HomeCommand::new(worker_home.home_revision().unwrap());
                            command.add(release.contribution()).unwrap();
                            match worker_home.execute(command) {
                                CommandOutcome::Committed {
                                    later_failure: None,
                                    ..
                                } => return,
                                CommandOutcome::NotCommitted {
                                    evidence: beryl_home_store::CommandError::Conflict { conflicts, .. },
                                } if !conflicts.is_empty() => continue,
                                other => panic!("contradictory fixture setup must definitively commit: {other:?}"),
                            }
                        }
                        panic!("contradictory fixture setup exhausted its bounded revision-conflict attempts");
                    })
                    .await;
                let original_audit = operations.retained_audit(original_job.job_id()).unwrap();
                assert_eq!(
                    original_audit.reconcile(&home, &storage, &state).unwrap(),
                    DiscussionSettlementAuditOutcome::Collision
                );
                drop(original_audit);
                let collision_fact = operations.presentation_fact(child).unwrap().unwrap();
                assert!(collision_fact.admission);
                assert_eq!(collision_fact.job, original_job.job_id());
                assert_eq!(collision_fact.intent, original_job.intent_id());
                assert_eq!(collision_fact.attempt, original_job.attempt_ordinal());
                assert_eq!(collision_fact.disposition, LocalDisposition::Collision);
                window
                    .update(cx, |root, _, _| root.test_suspend_discussion_refresh(false))
                    .unwrap();
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,_,fresh)| {
                snapshot.status() == Some(Status::Unavailable) && snapshot.composer() == Composer::Inert
                    && snapshot.retry().is_none() && gated && !enabled && fresh
            })).unwrap(), "original Collision must publish its exact Unavailable status and deny retry while Home remains healthy").await;
                let prior = window
                    .read_with(cx, |root, app| {
                        root.test_discussion_presentation(app).unwrap()
                    })
                    .unwrap();
                let history = window
                    .read_with(cx, |root, _| root.test_thread_navigation_history())
                    .unwrap();
                runtime::submit(unrelated, "unrelated ordinary work remains available", cx).await;
                let independent = runtime::started(
                    unrelated,
                    &root,
                    "unrelated ordinary work remains available",
                    cx,
                )
                .await;
                runtime::complete(&root, &independent);
                wait_until(
                    unrelated,
                    cx,
                    |cx| {
                        unrelated
                            .read_with(cx, |root, app| {
                                root.test_submission_observation(app).is_ok()
                            })
                            .unwrap()
                    },
                    "an exact local collision must not block unrelated work on the healthy graph",
                )
                .await;
                assert!(
                    !storage
                        .thread_attributes(&home, child, fixture::limit())
                        .unwrap()
                        .unwrap()
                        .archive()
                        .is_archived()
                );
                assert!(operations.retained_audit(original_job.job_id()).is_some());
                owner.borrow_mut().test_enable_resident_frame_capture();
                faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
                assert!(home.home_revision().is_err());
                cx.update(|app| {
                    RunningProcessOwner::test_observe_running_home_failure(&owner, app)
                })
                .unwrap();
                let mut refused = false;
                let expected_refusal = DiscussionSettlementError::CustodyUnavailable.to_string();
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
                    assert!(
                        !matches!(
                            retained.automatic_recovery_outcome().as_deref(),
                            Some(InterruptedExitRecoveryOutcome::Completed)
                        ),
                        "contradictory original outcome must never publish a replacement graph"
                    );
                    let failure = retained.automatic_recovery_failure();
                    let original_prefix_refusal = matches!(
                        failure.as_deref().and_then(Option::as_ref),
                        Some(RecoveryPreparationFailure::Candidate(error))
                            if error.to_string() == expected_refusal
                    );
                    refused = original_prefix_refusal
                        && retained.test_services_ready()
                        && !retained.test_services_on_worker()
                        && retained.test_services().graph().is_none();
                    drop(failure);
                    drop(retained);
                    if refused {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(
                    refused,
                    "the exact original Collision must refuse the dependent candidate prefix"
                );
                assert_eq!(windows(&owner), original_windows);
                assert!(operations.retained_audit(original_job.job_id()).is_some());
                assert_eq!(
                    operations.presentation_fact(child).unwrap(),
                    Some(collision_fact)
                );
                window
                    .read_with(cx, |root, app| {
                        let current = root.test_discussion_presentation(app).unwrap();
                        assert_eq!(current.0.status(), Some(Status::Unavailable));
                        assert_eq!(current.0.composer(), Composer::Inert);
                        assert_eq!(current.3, prior.3);
                        assert!(current.1 && !current.2 && current.0.retry().is_none());
                        assert_eq!(root.test_thread_navigation_history(), history);
                        assert_eq!(
                            root.test_submission_composer(app)
                                .unwrap()
                                .read(app)
                                .selection_identity()
                                .claim()
                                .thread_id(),
                            child
                        );
                        assert!(root.test_recovered_discussion_presentation().is_none());
                    })
                    .unwrap();
                assert!(
                    runtime::records(&root, "discussion-started-")
                        .iter()
                        .all(|turn| !turn["input"][0]["text"]
                            .as_str()
                            .unwrap()
                            .starts_with("Discussion resolution:\n\n"))
                );
                assert_eq!(runtime::records(&root, "discussion-response-").len(), 1);
                owner.borrow().cancel_automatic_recovery();
                wait_until(window, cx, |_| {
                let retained = owner.borrow();
                !retained.test_services_on_worker() && matches!(retained.automatic_recovery_outcome().as_deref(),
                    Some(InterruptedExitRecoveryOutcome::Cancelled | InterruptedExitRecoveryOutcome::Unavailable(_)))
            }, "the expected unpublishable recovery fixture must cancel its original worker before termination").await;
                assert!(operations.retained_audit(original_job.job_id()).is_some());
                eprintln!(
                    "discussion Collision fixture: expected recovery refusal retained; fixture termination does not qualify successful Exit"
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}
