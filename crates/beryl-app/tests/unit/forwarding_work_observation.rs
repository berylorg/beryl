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
