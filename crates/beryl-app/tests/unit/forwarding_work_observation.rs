use super::*;
use crate::cas_projection::runtime_work::RuntimeWorkError;
use beryl_model::{CasProcessGeneration, RuntimeId};

#[test]
fn runtime_work_forwarding_contention_and_poison_preserve_authority() {
    let authority = Arc::new(
        ConnectionRegistryAuthority::new(
            RuntimeId::from_bytes([58; 16]),
            CasProcessGeneration::new(1).unwrap(),
        )
        .unwrap(),
    );
    let hub = ForwardingHub::new(Arc::clone(&authority));
    assert!(
        hub.try_with_work_attachment(|attachment| Ok(attachment.is_none()))
            .unwrap()
    );
    let held = hub.state.lock().unwrap();
    assert!(matches!(
        hub.try_with_work_attachment(|_| Ok(())),
        Err(RuntimeWorkError::Busy)
    ));
    drop(held);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = hub.state.lock().unwrap();
            panic!("inject forwarding observation poison");
        }))
        .is_err()
    );
    assert!(matches!(
        hub.try_with_work_attachment(|_| Ok(())),
        Err(RuntimeWorkError::Unavailable)
    ));
    assert!(!authority.is_retired());
    let state = hub.state.lock().err().unwrap().into_inner();
    assert!(!state.inert);
    assert!(state.endpoint.is_none());
}

#[test]
fn attachment_reads_preserve_observation_and_inert_changes_invalidate_it() {
    let authority = Arc::new(
        ConnectionRegistryAuthority::new(
            RuntimeId::from_bytes([59; 16]),
            CasProcessGeneration::new(1).unwrap(),
        )
        .unwrap(),
    );
    let hub = ForwardingHub::new(authority.clone());
    let boundary = &authority.work_boundary;
    let before = boundary.try_observe().unwrap();
    assert!(hub.work_attachment().unwrap().is_none());
    assert!(hub.is_detached());
    let mut held = hub.try_lock_attachment().unwrap().unwrap();
    assert!(held.attachment().is_none());
    assert!(!held.is_inert());
    assert!(hub.try_lock_attachment().unwrap().is_none());
    boundary.try_elect(&before, || ()).unwrap();
    held.mark_inert_in_place();
    assert!(matches!(
        boundary.try_observe(),
        Err(RuntimeWorkError::Busy)
    ));
    drop(held);
    assert!(matches!(
        boundary.try_elect(&before, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    let before_removal = boundary.try_observe().unwrap();
    let mut held = hub.lock_attachment().unwrap();
    assert!(held.mark_inert().is_none());
    drop(held);
    assert!(matches!(
        boundary.try_elect(&before_removal, || ()),
        Err(RuntimeWorkError::Stale)
    ));
    let final_observation = boundary.try_observe().unwrap();
    boundary.try_elect(&final_observation, || ()).unwrap();
    assert!(!authority.is_retired());
}

#[test]
fn attachment_unwind_invalidates_observation_and_poisoned_disposal_still_runs() {
    let authority = Arc::new(
        ConnectionRegistryAuthority::new(
            RuntimeId::from_bytes([60; 16]),
            CasProcessGeneration::new(1).unwrap(),
        )
        .unwrap(),
    );
    let hub = ForwardingHub::new(authority.clone());
    let before = authority.work_boundary.try_observe().unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = hub.lock_attachment().unwrap();
            panic!("interrupt attachment access");
        }))
        .is_err()
    );
    assert!(matches!(
        authority.work_boundary.try_elect(&before, || ()),
        Err(RuntimeWorkError::Unavailable)
    ));
    let (mut disposal, poisoned) = hub.lock_attachment_for_disposal();
    assert!(poisoned);
    assert!(disposal.mark_inert().is_none());
    assert!(disposal.is_inert());
    drop(disposal);
    assert!(matches!(
        authority.work_boundary.try_observe(),
        Err(RuntimeWorkError::Unavailable)
    ));
    assert!(!authority.is_retired());
}
