use std::num::NonZeroUsize;

use beryl_state::ThemeManifestReadLimits;

#[test]
fn manifest_read_limits_reject_unbounded_requests() {
    assert!(
        ThemeManifestReadLimits::new(
            NonZeroUsize::new(4097).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
            NonZeroUsize::new(256 * 1024).unwrap(),
        )
        .is_err()
    );
}
