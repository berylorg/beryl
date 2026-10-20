use super::*;
use crate::discussion_creation::{
    DiscussionCreationOperations, DiscussionCreationOutcome, DiscussionCreationService,
};
use crate::discussion_presentation::{
    DiscussionComposerContribution as Composer, DiscussionStatus as Status,
};
use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand};

fn seed_selected_discussion(path: &std::path::Path) {
    seed_selected_discussion_with_archive(path, false);
}

fn seed_archived_discussion(path: &std::path::Path) {
    seed_selected_discussion_with_archive(path, true);
}

fn seed_selected_discussion_with_archive(path: &std::path::Path, archived: bool) {
    fixture::seed(path, SOURCE, true);
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("discussion startup fixture")
    };
    let home = candidate.publish().unwrap();
    let session = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let window = &session.windows()[0];
    let parent = window.selected_thread().unwrap().thread_id();
    let thread = syndic
        .thread(&home, parent, fixture::limit())
        .unwrap()
        .unwrap();
    let head = syndic
        .transcript_view_head(&home, parent, fixture::limit())
        .unwrap()
        .unwrap();
    let entries = syndic
        .transcript_entries(
            &home,
            parent,
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == fixture::assistant())
        .unwrap();
    let source = syndic
        .prepare_discussion_source(
            &home,
            DiscussionContextSource::new(
                parent,
                thread.committed_tail().unwrap(),
                fixture::assistant(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, SOURCE.len() as u64).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new(SOURCE).unwrap(),
        )
        .unwrap();
    let child = SyndicThreadId::from_bytes([36; 16]);
    let created_at = syndic
        .thread_tail(&home, parent, fixture::limit())
        .unwrap()
        .unwrap()
        .last_activity_at();
    let service = DiscussionCreationService::new(
        DiscussionCreationOperations::new(
            crate::process_admission::ProcessAdmissionGate::new(),
            std::num::NonZeroUsize::new(1).unwrap(),
        ),
        home.service_reference(),
        state.clone(),
        syndic.clone(),
    );
    assert!(matches!(
        service
            .prepare(
                source,
                CreateDiscussion::new(
                    child,
                    SyndicDraftId::from_bytes([37; 16]),
                    created_at,
                    DraftEditHistoryPolicyV1::new(65536, 1).unwrap()
                ),
                beryl_home_store::CommandCancellation::new()
            )
            .unwrap()
            .execute(),
        DiscussionCreationOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let job = super::status_support::admit_retryable(&home, &state, &syndic, child);
    crate::support::discussion_handoff::complete_resolving_turn_for(
        &home,
        &syndic,
        child,
        SyndicItemId::from_bytes([225; 16]),
    );
    if archived {
        archive_discussion(&home, &state, &syndic, job);
    }
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.session().replace_claim(
            state.session().revision(&home).unwrap(),
            beryl_state::ReplaceWindowClaim::new(
                session.header().revision(),
                window.window_id(),
                window.revision(),
                window.selected_thread(),
                window.remembered_target().unwrap(),
                child,
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    drop((service, state, syndic));
    home.close().unwrap();
}

fn archive_discussion(
    home: &beryl_home_store::HomeStore,
    state: &beryl_state::BerylState,
    storage: &SyndicStorage,
    job: beryl_state::BranchHandoffJobRecord,
) {
    use crate::discussion_settlement::{
        DiscussionParentInputRequest, DiscussionSettlementOperations, DiscussionSettlementOutcome,
        DiscussionSettlementResult, DiscussionSettlementService,
    };
    let service = DiscussionSettlementService::new(
        DiscussionSettlementOperations::new(
            crate::process_admission::ProcessAdmissionGate::new(),
            std::num::NonZeroUsize::new(1).unwrap(),
        ),
        home.service_reference(),
        state.clone(),
        storage.clone(),
    );
    let committed = |outcome: DiscussionSettlementOutcome| match outcome {
        DiscussionSettlementOutcome::Committed {
            result,
            later_failure: None,
            ..
        } => result,
        _ => panic!("archive fixture settlement did not commit"),
    };
    assert!(matches!(
        committed(
            service
                .prepare_retry(
                    job.job_id(),
                    job.revision(),
                    beryl_home_store::CommandCancellation::new()
                )
                .unwrap()
                .execute()
        ),
        DiscussionSettlementResult::RetryResumed
    ));
    assert_eq!(
        committed(
            service
                .prepare(job.job_id(), beryl_home_store::CommandCancellation::new())
                .unwrap()
                .unwrap()
                .execute()
        ),
        DiscussionSettlementResult::ReadyForParent
    );
    let base = storage
        .thread_tail(home, job.parent_thread_id(), fixture::limit())
        .unwrap()
        .unwrap()
        .last_activity_at()
        .unix_millis()
        .max(
            storage
                .current_draft(home, job.discussion_thread_id(), fixture::limit())
                .unwrap()
                .unwrap()
                .draft()
                .updated_at()
                .unix_millis(),
        )
        .checked_add(100)
        .unwrap();
    let at = |offset| SyndicTimestamp::from_unix_millis(base + offset);
    let turn = beryl_model::SyndicTurnId::from_bytes([230; 16]);
    let item = SyndicItemId::from_bytes([231; 16]);
    let parent = match committed(
        service
            .prepare_parent_input(
                job.job_id(),
                DiscussionParentInputRequest {
                    turn_id: turn,
                    item_id: item,
                    admitted_at: at(0),
                },
                beryl_home_store::CommandCancellation::new(),
            )
            .unwrap()
            .unwrap()
            .execute(),
    ) {
        DiscussionSettlementResult::StartingParent(parent) => parent,
        other => panic!("archive fixture parent admission: {other:?}"),
    };
    let source = crate::support::exact_cas::establish_turn(
        home,
        storage.clone(),
        job.parent_thread_id(),
        turn,
        at(1),
    );
    assert_eq!(
        committed(
            service
                .prepare_parent_execution(
                    job.job_id(),
                    at(2),
                    beryl_home_store::CommandCancellation::new()
                )
                .unwrap()
                .unwrap()
                .execute()
        ),
        DiscussionSettlementResult::ParentActive(parent)
    );
    crate::support::exact_cas::admit_event(
        home,
        storage.clone(),
        job.parent_thread_id(),
        turn,
        &source,
        syndic_storage::SourceEventPayload::TurnActivated,
        at(3),
    );
    crate::support::exact_cas::correlate_user_item(
        home,
        storage.clone(),
        job.parent_thread_id(),
        turn,
        item,
        &source,
        at(4),
    );
    crate::support::exact_cas::admit_event(
        home,
        storage.clone(),
        job.parent_thread_id(),
        turn,
        &source,
        syndic_storage::SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        at(5),
    );
    crate::support::exact_cas::converge_and_release_terminal_history(
        home,
        storage.clone(),
        job.parent_thread_id(),
        turn,
    );
    assert_eq!(
        committed(
            service
                .prepare_parent_execution(
                    job.job_id(),
                    at(6),
                    beryl_home_store::CommandCancellation::new()
                )
                .unwrap()
                .unwrap()
                .execute()
        ),
        DiscussionSettlementResult::ParentSucceeded(parent)
    );
}

#[test]
fn native_archived_discussion_first_frame_is_readonly_and_has_no_retry_or_navigation_change() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(1, 0, seed_archived_discussion, |owner, _, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            let initial = window
                .read_with(cx, |root, _| {
                    root.test_initial_discussion_presentation().unwrap()
                })
                .unwrap();
            assert_eq!(initial.status(), Some(Status::Archived));
            assert_eq!(initial.composer(), Composer::ReadOnly);
            assert!(initial.retry().is_none());
            window
                .read_with(cx, |root, app| {
                    let current = root.test_discussion_presentation(app).unwrap();
                    assert_eq!((current.1, current.2, current.4), (true, true, true));
                    assert_eq!(root.coherent_viewed_thread(app), Some(initial.thread_id()));
                    assert_eq!(initial.thread_id(), SyndicThreadId::from_bytes([36; 16]));
                })
                .unwrap();
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            retire_code_fixture(window, &panel, cx).await;
        })
    });
}

#[test]
fn native_restored_discussion_first_frame_already_has_failure_strip_exact_retry_and_inert_composer()
{
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(1, 0, seed_selected_discussion, |owner, _, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            let initial = window
                .read_with(cx, |root, _| {
                    root.test_initial_discussion_presentation().unwrap()
                })
                .unwrap();
            assert_eq!(initial.status(), Some(Status::HandoffFailed));
            assert_eq!(initial.composer(), Composer::Inert);
            assert!(initial.retry().is_some());
            let (snapshot, gated, enabled, _, _) = window
                .read_with(cx, |root, app| {
                    root.test_discussion_presentation(app).unwrap()
                })
                .unwrap();
            assert_eq!(snapshot.status(), Some(Status::HandoffFailed));
            assert!(gated && !enabled);
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            retire_code_fixture(window, &panel, cx).await;
        })
    });
}

#[test]
fn native_failed_home_discussion_recovery_publishes_exact_status_gate_and_retry_on_same_editor() {
    use crate::running_owner::InterruptedExitRecoveryOutcome;
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(1, 0, seed_selected_discussion, |owner, faults, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            let prior = window
                .read_with(cx, |root, app| {
                    root.test_discussion_presentation(app).unwrap()
                })
                .unwrap();
            let mut original = None;
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
                    let job = graph
                        .state()
                        .durable_jobs()
                        .job(graph.home(), prior.0.retry().unwrap().job_id())
                        .unwrap()
                        .unwrap();
                    original = Some((graph.home().service_reference(), job));
                    true
                },
                "initial discussion job observation must retain GUI service custody",
            )
            .await;
            let (home, original_job) = original.unwrap();
            let selected = prior.0.thread_id();
            owner.borrow_mut().test_enable_resident_frame_capture();
            faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
            assert!(home.home_revision().is_err());
            cx.update(|app| RunningProcessOwner::test_observe_running_home_failure(&owner, app))
                .unwrap();
            let mut recovered = false;
            let mut recovered_job = None;
            for _ in 0..750 {
                with_native(window, cx, |native, app| {
                    native.draw_and_present_for_test(app)
                })
                .unwrap();
                cx.update(|app| {
                    RunningProcessOwner::test_dispatch_scheduled_resident_frame(&owner, app);
                    RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                })
                .unwrap();
                let retained = owner.borrow();
                recovered = match retained.automatic_recovery_outcome().as_deref() {
                    Some(InterruptedExitRecoveryOutcome::Completed)
                        if retained.test_services_ready() =>
                    {
                        if let Some(graph) = retained.test_services().graph() {
                            recovered_job = Some((
                                graph
                                    .state()
                                    .durable_jobs()
                                    .job(graph.home(), original_job.job_id())
                                    .unwrap()
                                    .unwrap(),
                                graph.home().health().generation().unwrap(),
                            ));
                            true
                        } else {
                            false
                        }
                    }
                    Some(InterruptedExitRecoveryOutcome::Unavailable(error)) => {
                        panic!("discussion recovery unavailable: {error}")
                    }
                    Some(InterruptedExitRecoveryOutcome::Cancelled) => {
                        panic!("discussion recovery cancelled")
                    }
                    _ => false,
                };
                drop(retained);
                if recovered {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
            assert!(
                recovered,
                "actual discussion failed-Home recovery must complete"
            );
            let published = window
                .read_with(cx, |root, _| {
                    root.test_recovered_discussion_presentation()
                        .expect("exact prepared release must capture discussion tuple")
                })
                .unwrap();
            assert_eq!(published.0.status(), Some(Status::HandoffFailed));
            assert_eq!(published.0.composer(), Composer::Inert);
            assert_eq!(
                (published.1, published.2, published.3, published.4),
                (true, false, prior.3, true)
            );
            assert_eq!(published.0.thread_id(), selected);
            assert_eq!(published.0.home_id(), prior.0.home_id());
            assert_ne!(published.0.home_generation(), prior.0.home_generation());
            assert_ne!(
                published.0.service_generation(),
                prior.0.service_generation()
            );
            assert_eq!(published.0.retry().unwrap().job_id(), original_job.job_id());
            assert_eq!(
                published.0.retry().unwrap().job_revision(),
                original_job.revision()
            );
            assert!(!published.0.retry_pending());
            assert_eq!(windows(&owner), vec![window]);
            window
                .read_with(cx, |root, app| {
                    assert_eq!(root.coherent_viewed_thread(app), Some(selected));
                    let current = root.test_discussion_presentation(app).unwrap();
                    assert_eq!((current.1, current.2, current.3), (true, false, prior.3));
                    assert!(!root.test_discussion_retry_pending());
                })
                .unwrap();
            {
                let (job, generation) = recovered_job.unwrap();
                assert_eq!(generation, published.0.home_generation());
                assert_eq!(job.revision(), original_job.revision());
                assert_eq!(job.intent_id(), original_job.intent_id());
                assert_eq!(job.request(), original_job.request());
                assert_eq!(job.resolution(), original_job.resolution());
                assert_eq!(job.parent_thread_id(), original_job.parent_thread_id());
                assert_eq!(job.attempt_ordinal(), original_job.attempt_ordinal());
            }
            drop(home);
            retire_code_fixture(window, &panel, cx).await;
        })
    });
}
