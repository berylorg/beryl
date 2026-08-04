use beryl_backend::{
    THREAD_INJECTION_MAX_ITEMS, THREAD_INJECTION_MAX_PAGE_BYTES, THREAD_INJECTION_MAX_TEXT_BYTES,
    ThreadInjectionPreflight, ThreadInjectionPreflightError, ThreadInjectionRole,
    ThreadInjectionSourceError, ThreadInjectionSourceIdentity, ThreadInjectionSourcePage,
    ThreadInjectionSourceRevision,
};
use beryl_model::RecoveryItemSequenceDigest;

#[path = "support/recovery_page.rs"]
mod recovery_page;

use recovery_page::{lease_from_pool, recovery_page_pool, recovery_page_pool_with_capacity};

#[test]
fn preflight_enforces_nonzero_and_exact_declared_bounds() {
    let identity = ThreadInjectionSourceIdentity::new([0x11; 32]);
    let revision = ThreadInjectionSourceRevision::new(9);
    let digest = RecoveryItemSequenceDigest::from_bytes([0x22; 32]);

    assert_eq!(
        ThreadInjectionPreflight::new(identity, revision, 0, 1, digest).unwrap_err(),
        ThreadInjectionPreflightError::Empty
    );
    assert_eq!(
        ThreadInjectionPreflight::new(identity, revision, 1, 0, digest).unwrap_err(),
        ThreadInjectionPreflightError::EmptyCanonicalUtf8
    );

    let exact = ThreadInjectionPreflight::new(
        identity,
        revision,
        THREAD_INJECTION_MAX_ITEMS,
        THREAD_INJECTION_MAX_TEXT_BYTES,
        digest,
    )
    .unwrap();
    assert_eq!(exact.item_count(), THREAD_INJECTION_MAX_ITEMS);
    assert_eq!(
        exact.canonical_utf8_bytes(),
        THREAD_INJECTION_MAX_TEXT_BYTES
    );

    assert_eq!(
        ThreadInjectionPreflight::new(
            identity,
            revision,
            THREAD_INJECTION_MAX_ITEMS + 1,
            1,
            digest,
        )
        .unwrap_err(),
        ThreadInjectionPreflightError::TooManyItems {
            actual: THREAD_INJECTION_MAX_ITEMS + 1,
            maximum: THREAD_INJECTION_MAX_ITEMS,
        }
    );
    assert_eq!(
        ThreadInjectionPreflight::new(
            identity,
            revision,
            1,
            THREAD_INJECTION_MAX_TEXT_BYTES + 1,
            digest,
        )
        .unwrap_err(),
        ThreadInjectionPreflightError::TooManyCanonicalUtf8Bytes {
            actual: THREAD_INJECTION_MAX_TEXT_BYTES + 1,
            maximum: THREAD_INJECTION_MAX_TEXT_BYTES,
        }
    );
}

#[test]
fn source_page_borrows_valid_utf8_from_one_noncloneable_lease_and_releases_it() {
    let pool = recovery_page_pool();
    let identity = ThreadInjectionSourceIdentity::new([0x33; 32]);
    let revision = ThreadInjectionSourceRevision::new(17);
    let text = "recovered 🙂";
    let lease = lease_from_pool(&pool, text.as_bytes());
    let leased_text_address = lease.as_slice().as_ptr();
    assert_eq!(pool.diagnostics().leased, 1);

    let page = ThreadInjectionSourcePage::new(
        identity,
        revision,
        3,
        ThreadInjectionRole::AssistantOutputText,
        text.len() as u64,
        0,
        lease,
        true,
        false,
    )
    .unwrap();
    assert_eq!(page.source_identity(), identity);
    assert_eq!(page.source_revision(), revision);
    assert_eq!(page.item_ordinal(), 3);
    assert_eq!(page.role(), ThreadInjectionRole::AssistantOutputText);
    assert_eq!(page.declared_item_utf8_bytes(), text.len() as u64);
    assert_eq!(page.item_offset(), 0);
    assert_eq!(page.text(), text);
    assert_eq!(page.text().as_ptr(), leased_text_address);
    assert!(page.item_terminal());
    assert!(!page.sequence_terminal());

    drop(page);
    let diagnostics = pool.diagnostics();
    assert_eq!(diagnostics.leased, 0);
    assert_eq!(diagnostics.high_water, 1);
}

#[test]
fn source_page_rejects_empty_oversized_and_invalid_utf8_leases() {
    let identity = ThreadInjectionSourceIdentity::new([0x44; 32]);
    let revision = ThreadInjectionSourceRevision::new(18);

    let empty_pool = recovery_page_pool();
    let empty = empty_pool.try_lease().unwrap();
    assert_eq!(
        source_page(identity, revision, empty).unwrap_err(),
        ThreadInjectionSourceError::EmptyPage
    );
    assert_eq!(empty_pool.diagnostics().leased, 0);

    let oversized_pool = recovery_page_pool_with_capacity(THREAD_INJECTION_MAX_PAGE_BYTES + 1);
    let mut oversized = oversized_pool.try_lease().unwrap();
    oversized.buffer_mut().fill(b'x');
    oversized
        .set_len(THREAD_INJECTION_MAX_PAGE_BYTES + 1)
        .unwrap();
    assert_eq!(
        source_page(identity, revision, oversized).unwrap_err(),
        ThreadInjectionSourceError::PageTooLarge {
            maximum: THREAD_INJECTION_MAX_PAGE_BYTES,
            actual: THREAD_INJECTION_MAX_PAGE_BYTES + 1,
        }
    );
    assert_eq!(oversized_pool.diagnostics().leased, 0);

    let invalid_pool = recovery_page_pool();
    let invalid = lease_from_pool(&invalid_pool, &[0xff]);
    assert_eq!(
        source_page(identity, revision, invalid).unwrap_err(),
        ThreadInjectionSourceError::InvalidSource
    );
    assert_eq!(invalid_pool.diagnostics().leased, 0);
}

fn source_page(
    identity: ThreadInjectionSourceIdentity,
    revision: ThreadInjectionSourceRevision,
    lease: beryl_stream::PageLease,
) -> Result<ThreadInjectionSourcePage, ThreadInjectionSourceError> {
    ThreadInjectionSourcePage::new(
        identity,
        revision,
        1,
        ThreadInjectionRole::UserInputText,
        1,
        0,
        lease,
        true,
        true,
    )
}
