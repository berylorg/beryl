mod support;

use beryl_home_store::{
    HomeCandidateError, HomeDomainRequirements, HomeDomainRequirementsError, HomeHealthState,
    HomeOpenCandidate, HomeOpenError, HomeOpenOptions, HomeSchemaVersion,
};
use tempfile::tempdir;

use support::{AlphaDomain, AlphaDomainSchema2, BetaDomain, DuplicateFamilyDomain};

struct RetiredDomain;

struct Retirement(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl beryl_home_store::DomainRuntimeAttachment for Retirement {
    fn retire(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl beryl_home_store::StorageDomain for RetiredDomain {
    const NAME: &'static str = "retirement";
    const SCHEMA_VERSION: beryl_home_store::DomainSchemaVersion =
        beryl_home_store::DomainSchemaVersion::new(1);
    const FAMILIES: &'static [beryl_home_store::RecordFamily<Self>] =
        &[beryl_home_store::RecordFamily::new::<
            support::BytesRecord<Self>,
        >(beryl_home_store::KeyspaceSchemaVersion::new(1))];
    type ValidationError = std::convert::Infallible;
    type RuntimeAttachment = Retirement;
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<Retirement, Self::RuntimeAttachmentError> {
        Ok(Retirement(std::sync::Arc::new(
            std::sync::atomic::AtomicBool::new(false),
        )))
    }

    fn validate(_: &beryl_home_store::DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

#[test]
fn failed_close_and_unpublished_drop_retire_attachments_before_same_home_reopens() {
    for fail_before_close in [false, true] {
        let directory = tempdir().unwrap();
        let mut candidate = support::open_home(directory.path());
        let handle = candidate.register_domain::<RetiredDomain>().unwrap();
        let retired = candidate
            .with_domain_attachment(&handle.attachment_capability(), |attachment| {
                attachment.0.clone()
            })
            .unwrap();
        assert!(!retired.load(std::sync::atomic::Ordering::SeqCst));
        if fail_before_close {
            let failure = candidate
                .prepare_publication(HomeDomainRequirements::new())
                .unwrap_err();
            assert!(!retired.load(std::sync::atomic::Ordering::SeqCst));
            failure.into_parts().1.close().unwrap();
        } else {
            let publication = candidate
                .prepare_publication(
                    HomeDomainRequirements::new()
                        .with_domain::<RetiredDomain>()
                        .unwrap(),
                )
                .unwrap();
            drop(publication);
        }
        assert!(retired.load(std::sync::atomic::Ordering::SeqCst));
        support::open_home(directory.path()).close().unwrap();
    }
}

#[test]
fn exact_registration_stays_opening_until_publication_and_preserves_home_identity() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let home_id = candidate.home_id();
    let generation = candidate.generation();
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(candidate.health().state(), HomeHealthState::Opening);
    candidate
        .with_domain_attachment(&alpha.attachment_capability(), |_| ())
        .unwrap();
    let publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(publication.health().state(), HomeHealthState::Opening);
    assert_eq!(publication.home_id(), home_id);
    assert_eq!(publication.generation(), generation);
    let store = publication.publish().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 1);
    store.close().unwrap();

    let mut reopened = support::open_home(directory.path());
    assert_eq!(reopened.home_id(), home_id);
    assert_eq!(reopened.health().state(), HomeHealthState::Opening);
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    let store = reopened
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 1);
    store.close().unwrap();
}

#[test]
fn missing_declaration_retains_failed_candidate_and_lock_until_close_then_exact_retry() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let home_id = candidate.home_id();
    candidate.register_domain::<AlphaDomain>().unwrap();
    let failure = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap()
                .with_domain::<BetaDomain>()
                .unwrap(),
        )
        .unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::MissingDomain { domain: "beta" }
    ));
    assert_eq!(
        failure.candidate().health().state(),
        HomeHealthState::Failed
    );
    assert!(matches!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        )),
        Err(HomeOpenError::Busy { .. })
    ));
    failure.into_parts().1.close().unwrap();

    let mut retry = support::open_home(directory.path());
    assert_eq!(retry.home_id(), home_id);
    let alpha = retry.register_domain::<AlphaDomain>().unwrap();
    let beta = retry.register_domain::<BetaDomain>().unwrap();
    let store = retry
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<BetaDomain>()
                .unwrap()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 1);
    assert_eq!(store.domain_revision(&beta).unwrap().get(), 1);
    store.close().unwrap();
}

#[test]
fn unexpected_or_foreign_typed_declarations_cannot_publish() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    candidate.register_domain::<AlphaDomain>().unwrap();
    let failure = candidate
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::UnexpectedDomain { domain: "alpha" }
    ));
    failure.into_parts().1.close().unwrap();

    let mut candidate = support::open_home(directory.path());
    candidate.register_domain::<AlphaDomain>().unwrap();
    let failure = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomainSchema2>()
                .unwrap(),
        )
        .unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::DomainMismatch { domain: "alpha" }
    ));
    failure.into_parts().1.close().unwrap();
}

#[test]
fn duplicate_and_invalid_requirements_are_rejected_before_candidate_use() {
    assert!(matches!(
        HomeDomainRequirements::new()
            .with_domain::<AlphaDomain>()
            .unwrap()
            .with_domain::<AlphaDomain>(),
        Err(HomeDomainRequirementsError::DuplicateDomain { domain: "alpha" })
    ));
    assert!(matches!(
        HomeDomainRequirements::new()
            .with_domain::<AlphaDomain>()
            .unwrap()
            .merge(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomainSchema2>()
                    .unwrap()
            ),
        Err(HomeDomainRequirementsError::DuplicateDomain { domain: "alpha" })
    ));
    assert!(matches!(
        HomeDomainRequirements::new().with_domain::<DuplicateFamilyDomain>(),
        Err(HomeDomainRequirementsError::Definition(_))
    ));
}

#[test]
fn failed_registration_closes_partial_candidate_and_cannot_be_omitted_for_publication() {
    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    candidate.register_domain::<AlphaDomain>().unwrap();
    candidate
        .register_domain::<DuplicateFamilyDomain>()
        .unwrap_err();
    assert_eq!(candidate.health().state(), HomeHealthState::Failed);
    assert!(candidate.register_domain::<BetaDomain>().is_err());
    let failure = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(failure.error(), HomeCandidateError::HealthGate(error) if error.state() == HomeHealthState::Failed)
    );
    failure.into_parts().1.close().unwrap();
    let mut retry = support::open_home(directory.path());
    let alpha = retry.register_domain::<AlphaDomain>().unwrap();
    let store = retry
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 1);
    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_reads_commands_sidecars_and_receipts_remain_closed_before_publication() {
    use beryl_home_store::{
        CommandError, CommitReceiptError, HomeCommand, PointReadLimit, ReadError, SidecarByteLimit,
        SidecarError, SidecarNamespace,
        test_faults::{with_initial_candidate_store, with_initial_publication_store},
    };
    use support::{BytesRecord, PutBytes, committed, not_committed};

    let origin_directory = tempdir().unwrap();
    let mut origin = support::open_home(origin_directory.path());
    let origin_alpha = origin.register_domain::<AlphaDomain>().unwrap();
    let origin = origin
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let mut command = HomeCommand::new(origin.home_revision().unwrap());
    command
        .add(origin_alpha.contribution(
            origin.domain_revision(&origin_alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"origin"),
        ))
        .unwrap();
    let receipt = committed(origin.execute(command));

    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
    let assert_denied = |store: &beryl_home_store::HomeStore| {
        assert!(
            matches!(store.home_revision(), Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Opening)
        );
        assert!(
            matches!(store.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(&alpha, &1, PointReadLimit::new(1028).unwrap()), Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Opening)
        );
        let mut command = HomeCommand::new(origin.home_revision().unwrap());
        command
            .add(alpha.contribution(
                origin.domain_revision(&origin_alpha).unwrap(),
                PutBytes::<AlphaDomain>::new(1, b"candidate"),
            ))
            .unwrap();
        assert!(
            matches!(not_committed(store.execute(command)), CommandError::HealthGate(error) if error.state() == HomeHealthState::Opening)
        );
        assert!(
            matches!(store.admit_sidecar(SidecarNamespace::new("images").unwrap(), b"closed", SidecarByteLimit::new(std::num::NonZeroU64::new(1024).unwrap())), Err(SidecarError::HealthGate(error)) if error.state() == HomeHealthState::Opening)
        );
        assert!(
            matches!(store.receipt_domain_revision(&receipt, &alpha), Err(CommitReceiptError::HealthGate(error)) if error.state() == HomeHealthState::Opening)
        );
    };
    with_initial_candidate_store(&candidate, assert_denied);
    let publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    with_initial_publication_store(&publication, assert_denied);
    let store = publication.publish().unwrap();
    assert!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(1028).unwrap()
            )
            .unwrap()
            .is_none()
    );
    store.close().unwrap();
    origin.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn publication_detects_unobserved_storage_failure_and_retains_cleanup_custody() {
    use beryl_home_store::test_faults::with_initial_publication_store;

    let directory = tempdir().unwrap();
    let mut candidate = support::open_home(directory.path());
    candidate.register_domain::<AlphaDomain>().unwrap();
    let publication = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap();
    with_initial_publication_store(&publication, |store| {
        store.inject_retained_maintenance_terminal()
    });
    assert_eq!(publication.health().state(), HomeHealthState::Opening);
    let failure = publication.publish().unwrap_err();
    assert!(matches!(
        failure.error(),
        HomeCandidateError::StorageHealth { .. }
    ));
    assert_eq!(
        failure.candidate().health().state(),
        HomeHealthState::Failed
    );
    assert!(matches!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        )),
        Err(HomeOpenError::Busy { .. })
    ));
    failure.into_parts().1.close().unwrap();
    support::open_home(directory.path()).close().unwrap();
}
