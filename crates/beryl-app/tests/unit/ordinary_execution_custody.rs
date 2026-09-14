use super::*;
use crate::{
    cas_projection::{persistent_failure::MasterCommandGate, service_config::ProjectionWorkerPool},
    process_admission::{ProcessAdmissionError, ProcessAdmissionGate},
};

#[test]
fn ordinary_unwind_releases_worker_and_counted_admission() {
    let process = ProcessAdmissionGate::new();
    let gate = MasterCommandGate::new(
        process.clone(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let commands = gate.authorizer();
    let pool = ProjectionWorkerPool::new(std::num::NonZeroUsize::new(4).unwrap());
    let command = commands.authorize().unwrap();
    let custody =
        OrdinaryExecutionCustody::admit(pool.try_acquire_ordinary().unwrap(), &commands, &command)
            .unwrap();
    let fence = process.fence().unwrap();
    assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Unsettled));
    assert_eq!(pool.diagnostics().active(), 1);
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _custody = custody;
        panic!("ordinary owner unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(pool.diagnostics().active(), 0);
    fence.reopen_if(true).unwrap();
    assert!(pool.try_acquire_ordinary().is_ok());
}
