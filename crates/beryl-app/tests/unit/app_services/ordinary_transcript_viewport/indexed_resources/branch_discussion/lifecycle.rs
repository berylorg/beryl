use super::*;
use beryl_home_store::test_faults::FaultBlock;
use std::sync::{Arc, Mutex};

struct Held(FaultBlock);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[test]
fn native_captured_branch_worker_drains_and_releases_sources_before_retirement() {
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
                let block_slot = Arc::new(Mutex::new(None));
                let worker_slot = block_slot.clone();
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(move |_| {
                                *worker_slot.lock().unwrap() =
                                    Some(faults.block_next(FaultPoint::BeforeCommit));
                            })),
                            None,
                        )
                    })
                    .unwrap();
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |_| {
                        block_slot
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_some_and(|held: &FaultBlock| {
                                held.wait_until_reached(Duration::ZERO)
                            })
                    },
                    "the branch worker must retain its tracked lifetime at commit",
                )
                .await;
                let held = Held(block_slot.lock().unwrap().take().unwrap());
                let (_, child, captured) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                window
                    .update(cx, |root, native, app| {
                        root.test_suspend_code_saves_for_close(native, app);
                        assert!(!root.running_thread_reads_drained());
                        assert!(!root.release_suspended_running_thread_sources());
                    })
                    .unwrap();
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                held.0.release();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .update(cx, |root, _, _| {
                                root.release_suspended_running_thread_sources()
                            })
                            .unwrap()
                    },
                    "tracked branch work and source readers must release before retirement",
                )
                .await;
                let (_, same_child, original) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(same_child, child);
                assert_eq!(original, captured);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                window
                    .update(cx, |root, native, app| {
                        root.test_resume_code_saves_after_close(native, app)
                    })
                    .unwrap();
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| {
                root.test_captured_branch_state().is_some_and(|state| state.0 == "NotCommitted")
                    || (root.coherent_viewed_thread(app) == Some(child) && !root.test_running_thread_activation_pending())
            }).unwrap(), "the original creation outcome must resume after source retirement without another command").await;
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
