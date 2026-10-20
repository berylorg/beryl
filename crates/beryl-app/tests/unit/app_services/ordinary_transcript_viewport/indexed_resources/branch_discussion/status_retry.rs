use super::status_support::admit_retryable;
use super::*;
use crate::discussion_presentation::{
    DiscussionComposerContribution as Composer, DiscussionStatus as Status,
};
use beryl_home_store::test_faults::FaultBlock;

struct Held(FaultBlock);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[test]
fn native_discussion_retry_observes_durable_failure_suppresses_duplicates_and_resumes_same_job() {
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
                    "the real discussion must be selected",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let (home, state, storage, process) = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    (
                        graph.home().service_reference(),
                        graph.state().clone(),
                        graph.syndic().clone(),
                        owner.test_services().process.clone(),
                    )
                };
                let fence = process.test_fence().unwrap();
                wait_until(
                    window,
                    cx,
                    |_| fence.fence().validate_settled_for(&process).is_ok(),
                    "fixture seeding must wait for original admitted background work to drain",
                )
                .await;
                let worker_home = home.service_reference();
                let worker_state = state.clone();
                let worker_storage = storage.clone();
                let paused = cx
                    .background_executor()
                    .spawn(async move {
                        admit_retryable(&worker_home, &worker_state, &worker_storage, child)
                    })
                    .await;
                owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .cas()
                    .try_reopen_shutdown_admission(fence.fence())
                    .unwrap();
                wait_until(window,cx,|cx|window.read_with(cx,|root,app|root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,_,_)|snapshot.status()==Some(Status::HandoffFailed)&&snapshot.retry().is_some()&&gated&&!enabled)).unwrap(),"the existing selected poll must coherently join the durable retryable job and composer gate").await;
                let held = Held(faults.block_next(FaultPoint::BeforeCommit));
                window
                    .update(cx, |root, native, app| {
                        let mut stale = root.test_discussion_observation_target(app).unwrap();
                        stale.2 = std::sync::Arc::new(());
                        root.test_retry_discussion_target(stale, native, app);
                        assert!(
                            !root.test_discussion_retry_pending(),
                            "stale publication callback must not admit a retry"
                        );
                        root.test_retry_current_discussion(native, app);
                        assert!(root.test_discussion_retry_pending());
                        root.test_retry_current_discussion(native, app);
                        assert!(root.test_discussion_retry_pending());
                    })
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |_| held.0.wait_until_reached(Duration::ZERO),
                    "the original retry command must own the blocked writer flight",
                )
                .await;
                window
                    .read_with(cx, |root, app| {
                        let (snapshot, gated, enabled, _, _) =
                            root.test_discussion_presentation(app).unwrap();
                        assert_eq!(snapshot.status(), Some(Status::HandoffFailed));
                        assert_eq!(snapshot.composer(), Composer::Inert);
                        assert!(
                            snapshot.retry().is_some()
                                && gated
                                && !enabled
                                && root.test_discussion_retry_pending()
                        );
                    })
                    .unwrap();
                held.0.release();
                wait_until(window,cx,|cx|window.read_with(cx,|root,app|root.test_discussion_presentation(app).is_some_and(|(snapshot,gated,enabled,_,_)|snapshot.status()==Some(Status::ResolutionPending)&&snapshot.retry().is_none()&&gated&&!enabled)&&!root.test_discussion_retry_pending()).unwrap(),"retry commitment must refresh the same selected tuple before enabling another action").await;
                let current = state
                    .durable_jobs()
                    .job(&home, paused.job_id())
                    .unwrap()
                    .unwrap();
                assert_eq!(current.job_id(), paused.job_id());
                assert_eq!(current.intent_id(), paused.intent_id());
                assert_eq!(current.resolution(), paused.resolution());
                assert_eq!(current.request(), paused.request());
                assert_eq!(current.parent_thread_id(), parent);
                assert_eq!(current.attempt_ordinal(), paused.attempt_ordinal());
                assert_eq!(current.revision().get(), paused.revision().get() + 1);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(child)
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
