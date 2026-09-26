use super::*;
use crate::cas_projection::runtime_work::RuntimeWorkError;

#[test]
fn runtime_work_authority_busy_and_poison_do_not_retire_the_connection() {
    let authority = ConnectionRegistryAuthority::new(
        RuntimeId::from_bytes([57; 16]),
        CasProcessGeneration::new(1).unwrap(),
    )
    .unwrap();
    let before = authority.work_fact().unwrap();
    let held = authority.gate.lock().unwrap();
    assert_eq!(authority.try_work_fact(), Err(RuntimeWorkError::Busy));
    drop(held);
    assert_eq!(authority.try_work_fact().unwrap(), before);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = authority.gate.lock().unwrap();
            panic!("inject connection authority poison");
        }))
        .is_err()
    );
    assert_eq!(
        authority.try_work_fact(),
        Err(RuntimeWorkError::Unavailable)
    );
    assert!(!authority.is_retired());
    let state = authority.gate.lock().err().unwrap().into_inner();
    assert_eq!(state.session_owner_live, before.session_owner_live);
    assert_eq!(state.next_cleanup_id, before.next_cleanup_id);
    assert!(!state.retirement_complete);
}
