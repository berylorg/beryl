use super::*;

#[test]
fn allocation_exhaustion_never_wraps_or_reuses_the_last_period() {
    assert_eq!(
        allocate_period(HomeRevision::new(u64::MAX - 1).unwrap())
            .unwrap()
            .get(),
        u64::MAX
    );
    assert!(allocate_period(HomeRevision::new(u64::MAX).unwrap()).is_err());
}
