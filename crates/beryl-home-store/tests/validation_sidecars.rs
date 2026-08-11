mod support;

use std::{error::Error, fmt, io, num::NonZeroU64};

use beryl_home_store::{
    CommandError, ContributorCallbackStage, DomainCallbackError, DomainCallbackSource,
    DomainReader, DomainValidator, HomeCommand, HomeHealthState, PointReadLimit, ReadError,
    ReadStage, SidecarByteLimit, SidecarNamespace,
};
use tempfile::tempdir;

use support::{
    committed, not_committed, open_home, AlphaDomain, BetaDomain, BytesRecord, PutBytes,
};

#[derive(Clone, Copy)]
enum Failure {
    Semantic,
    Access,
}

#[derive(Debug)]
enum GuardError {
    Semantic,
    Access(ReadError),
}

impl fmt::Display for GuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Semantic => formatter.write_str("sidecar guard rejected semantically"),
            Self::Access(source) => source.fmt(formatter),
        }
    }
}

impl Error for GuardError {}

impl DomainCallbackError for GuardError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Access(source) => Ok(DomainCallbackSource::Read(source)),
            semantic @ Self::Semantic => Err(semantic),
        }
    }
}

struct RejectingValidator {
    failure: Failure,
}

impl DomainValidator<BetaDomain> for RejectingValidator {
    type Error = GuardError;

    fn validate(&self, _reader: &DomainReader<'_, BetaDomain>) -> Result<(), Self::Error> {
        match self.failure {
            Failure::Semantic => Err(GuardError::Semantic),
            Failure::Access => Err(GuardError::Access(ReadError::Storage {
                stage: ReadStage::PointValue,
                source: Box::new(io::Error::other("synthetic validator access failure")),
            })),
        }
    }
}

fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

#[test]
fn validator_failure_drops_retained_sidecar_command_and_allows_later_reference() {
    for failure in [Failure::Semantic, Failure::Access] {
        let directory = tempdir().unwrap();
        let mut store = open_home(directory.path());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();
        let beta = store.register_domain::<BetaDomain>().unwrap();
        let bytes = b"validation-sidecar-bytes";
        let sidecar = store
            .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
            .unwrap();
        let address = sidecar.address().clone();
        let home_before = store.home_revision().unwrap();
        let alpha_before = store.domain_revision(alpha).unwrap();
        let beta_before = store.domain_revision(beta).unwrap();
        let mut rejected = HomeCommand::new(home_before);
        rejected.require_sidecar(sidecar).unwrap();
        rejected
            .add(alpha.contribution(
                alpha_before,
                PutBytes::<AlphaDomain>::new(1, address.digest().as_bytes().to_vec()),
            ))
            .unwrap()
            .add_validation(beta.validation(beta_before, RejectingValidator { failure }))
            .unwrap();

        let error = not_committed(store.execute(rejected));
        match failure {
            Failure::Semantic => {
                assert!(matches!(
                    error,
                    CommandError::ContributorValidation { domain: "beta", .. }
                ));
                assert_eq!(store.health().state(), HomeHealthState::Healthy);
            }
            Failure::Access => {
                assert!(matches!(
                    error,
                    CommandError::ContributorAccess {
                        domain: "beta",
                        stage: ContributorCallbackStage::Validation,
                        source: DomainCallbackSource::Read(ReadError::Storage { .. }),
                    }
                ));
                assert_eq!(store.health().state(), HomeHealthState::Verifying);
                store.verify_health().unwrap();
            }
        }
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(store.domain_revision(alpha).unwrap(), alpha_before);
        assert_eq!(store.domain_revision(beta).unwrap(), beta_before);
        assert_eq!(
            store
                .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                    alpha,
                    &1,
                    PointReadLimit::new(1_028).unwrap(),
                )
                .unwrap(),
            None
        );

        let readmitted = store
            .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
            .unwrap();
        assert_eq!(readmitted.address(), &address);
        let mut accepted = HomeCommand::new(home_before);
        accepted.require_sidecar(readmitted).unwrap();
        accepted
            .add(alpha.contribution(
                alpha_before,
                PutBytes::<AlphaDomain>::new(1, address.digest().as_bytes().to_vec()),
            ))
            .unwrap();
        let receipt = committed(store.execute(accepted));
        assert!(store
            .receipt_domain_revision(&receipt, alpha)
            .unwrap()
            .is_some());
        assert_eq!(store.receipt_domain_revision(&receipt, beta).unwrap(), None);
        assert_eq!(
            store
                .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                    alpha,
                    &1,
                    PointReadLimit::new(1_028).unwrap(),
                )
                .unwrap(),
            Some(address.digest().as_bytes().to_vec())
        );
        assert!(store.verify_sidecar(&address, limit()).is_ok());
    }
}
