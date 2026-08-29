#[cfg(feature = "test-faults")]
use beryl_home_store::{ProofCommandSealError, test_faults::ProofCommandIdentityTestHarness};

#[cfg(feature = "test-faults")]
#[test]
fn command_identity_exhaustion_is_permanent_without_using_the_production_allocator() {
    let allocator = ProofCommandIdentityTestHarness::at_exhaustion_boundary();
    assert_eq!(allocator.allocate(), Ok(u64::MAX));
    assert_eq!(
        allocator.allocate(),
        Err(ProofCommandSealError::IdentityExhausted)
    );
    assert_eq!(
        allocator.allocate(),
        Err(ProofCommandSealError::IdentityExhausted)
    );
}
