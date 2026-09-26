use super::*;
use crate::cas_projection::runtime_work::RuntimeWorkError;

#[test]
fn runtime_work_session_contention_and_poison_do_not_close_or_revise_source() {
    let (_provider, sessions) = ProcessScheduledExecutionProvider::new();
    assert_eq!(sessions.try_work_revision(), Err(RuntimeWorkError::Closed));
    let held = sessions.state.lock().unwrap();
    assert_eq!(sessions.try_work_revision(), Err(RuntimeWorkError::Busy));
    drop(held);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = sessions.state.lock().unwrap();
            panic!("inject session observation poison");
        }))
        .is_err()
    );
    assert_eq!(
        sessions.try_work_revision(),
        Err(RuntimeWorkError::Unavailable)
    );
    let state = sessions.state.lock().err().unwrap().into_inner();
    assert!(!state.closed);
    assert_eq!(state.work_revision, Some(1));
}
