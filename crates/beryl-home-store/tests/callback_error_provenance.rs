mod support;

use std::{error::Error, fmt, io};

use beryl_home_store::{
    CommandError, ContributorCallbackStage, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, DomainRegistrationError, DomainSchemaVersion, DomainValidator,
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    KeyspaceSchemaVersion, MutationBuildError, MutationBuilder, ReadError, ReadStage, RecordCodec,
    RecordFamily, RecordVersion, StorageDomain,
};
use tempfile::tempdir;

use support::{committed, not_committed, AlphaDomain, PutBytes};

struct AccessDomain;
struct AccessRecord;

impl StorageDomain for AccessDomain {
    const NAME: &'static str = "callback_access";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<AccessRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = CallbackError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        let marker = reader
            .point::<AccessRecord>(&0, beryl_home_store::PointReadLimit::new(1_028).unwrap())
            .map_err(CallbackError::Read)?;
        match marker.as_deref() {
            Some(b"registration-storage") => Err(CallbackError::Read(storage_read())),
            Some(b"registration-reject") => Err(CallbackError::Semantic("registration")),
            _ => Ok(()),
        }
    }
}

impl RecordCodec<AccessDomain> for AccessRecord {
    type Key = u64;
    type Value = Vec<u8>;
    type Error = CodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 1_024;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.to_be_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let bytes = encoded.try_into().map_err(|_| CodecError)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(encoded.to_vec())
    }
}

#[derive(Debug)]
struct CodecError;

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid callback fixture record")
    }
}

impl Error for CodecError {}

#[derive(Debug)]
enum CallbackError {
    Read(ReadError),
    Build(MutationBuildError),
    Semantic(&'static str),
}

impl fmt::Display for CallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Semantic(stage) => write!(formatter, "semantic rejection during {stage}"),
        }
    }
}

impl Error for CallbackError {}

impl DomainCallbackError for CallbackError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source @ (Self::Build(_) | Self::Semantic(_)) => Err(source),
        }
    }
}

impl From<MutationBuildError> for CallbackError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

#[derive(Clone, Copy)]
enum Failure {
    None,
    Storage,
    Structural,
    Semantic,
}

struct Put {
    key: u64,
    value: Vec<u8>,
    validation: Failure,
    contribution: Failure,
}

impl DomainMutation<AccessDomain> for Put {
    type Error = CallbackError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, AccessDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        fail(self.validation, "validation")?;
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, AccessDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AccessRecord>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AccessDomain>,
    ) -> Result<(), Self::Error> {
        fail(prepared.contribution, "contribution")?;
        mutations.put::<AccessRecord>(&prepared.key, &prepared.value)?;
        Ok(())
    }
}

struct AccessValidator {
    failure: Failure,
}

impl DomainValidator<AccessDomain> for AccessValidator {
    type Error = CallbackError;

    fn validate(&self, _reader: &DomainReader<'_, AccessDomain>) -> Result<(), Self::Error> {
        fail(self.failure, "validation-only participant")
    }
}

fn fail(failure: Failure, stage: &'static str) -> Result<(), CallbackError> {
    match failure {
        Failure::None => Ok(()),
        Failure::Storage => Err(CallbackError::Read(storage_read())),
        Failure::Structural => Err(CallbackError::Read(ReadError::MalformedRecord {
            domain: AccessDomain::NAME,
            family: AccessRecord::FAMILY,
        })),
        Failure::Semantic => Err(CallbackError::Semantic(stage)),
    }
}

fn storage_read() -> ReadError {
    ReadError::Storage {
        stage: ReadStage::PointValue,
        source: Box::new(io::Error::other("synthetic callback storage failure")),
    }
}

fn open(path: &std::path::Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn execute(
    store: &HomeStore,
    domain: &beryl_home_store::DomainHandle<AccessDomain>,
    mutation: Put,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(store.domain_revision(domain).unwrap(), mutation))
        .unwrap();
    store.execute(command)
}

fn execute_with_validator(
    store: &HomeStore,
    mutation_domain: &beryl_home_store::DomainHandle<AlphaDomain>,
    validator_domain: &beryl_home_store::DomainHandle<AccessDomain>,
    failure: Failure,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(mutation_domain.contribution(
            store.domain_revision(mutation_domain).unwrap(),
            PutBytes::<AlphaDomain>::new(17, b"must stay atomic".to_vec()),
        ))
        .unwrap()
        .add_validation(validator_domain.validation(
            store.domain_revision(validator_domain).unwrap(),
            AccessValidator { failure },
        ))
        .unwrap();
    store.execute(command)
}

fn put(value: impl Into<Vec<u8>>) -> Put {
    Put {
        key: 1,
        value: value.into(),
        validation: Failure::None,
        contribution: Failure::None,
    }
}

#[test]
fn storage_access_from_either_callback_stage_fails_closed_with_provenance() {
    for (validation, contribution, expected_stage) in [
        (
            Failure::Storage,
            Failure::None,
            ContributorCallbackStage::Validation,
        ),
        (
            Failure::None,
            Failure::Storage,
            ContributorCallbackStage::Contribution,
        ),
    ] {
        let directory = tempdir().unwrap();
        let mut store = open(directory.path());
        let domain = store.register_domain::<AccessDomain>().unwrap();
        let home_before = store.home_revision().unwrap();
        let domain_before = store.domain_revision(&domain).unwrap();

        let error = execute(
            &store,
            &domain,
            Put {
                key: 1,
                value: b"ignored".to_vec(),
                validation,
                contribution,
            },
        );
        let error = not_committed(error);
        assert!(matches!(
            error,
            CommandError::ContributorAccess {
                stage,
                source: DomainCallbackSource::Read(ReadError::Storage { .. }),
                ..
            } if stage == expected_stage
        ));
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        let candidate = store.recover_same_home().unwrap();
        let domain = candidate.domain_handle::<AccessDomain>().unwrap();
        let store = candidate.publish();
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(store.domain_revision(&domain).unwrap(), domain_before);
        committed(execute(&store, &domain, put(b"committed")));
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        store.close().unwrap();
    }
}

#[test]
fn structural_and_semantic_callback_failures_have_distinct_health_effects() {
    let directory = tempdir().unwrap();
    let mut store = open(directory.path());
    let domain = store.register_domain::<AccessDomain>().unwrap();

    for (validation, contribution) in [
        (Failure::Semantic, Failure::None),
        (Failure::None, Failure::Semantic),
    ] {
        let home_before = store.home_revision().unwrap();
        assert!(matches!(
            execute(
                &store,
                &domain,
                Put {
                    key: 1,
                    value: b"ignored".to_vec(),
                    validation,
                    contribution,
                },
            ),
            beryl_home_store::CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        assert_eq!(store.home_revision().unwrap(), home_before);
    }
    committed(execute(&store, &domain, put(b"committed")));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    store.close().unwrap();

    let directory = tempdir().unwrap();
    let mut store = open(directory.path());
    let domain = store.register_domain::<AccessDomain>().unwrap();
    let error = execute(
        &store,
        &domain,
        Put {
            key: 1,
            value: b"ignored".to_vec(),
            validation: Failure::Structural,
            contribution: Failure::None,
        },
    );
    let error = not_committed(error);
    assert!(matches!(
        error,
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Read(ReadError::MalformedRecord { .. }),
            ..
        }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn validation_only_participant_preserves_semantic_and_access_provenance() {
    for failure in [Failure::Storage, Failure::Structural, Failure::Semantic] {
        let directory = tempdir().unwrap();
        let mut store = open(directory.path());
        let mutation_domain = store.register_domain::<AlphaDomain>().unwrap();
        let validator_domain = store.register_domain::<AccessDomain>().unwrap();
        let home_before = store.home_revision().unwrap();
        let mutation_before = store.domain_revision(&mutation_domain).unwrap();
        let validator_before = store.domain_revision(&validator_domain).unwrap();

        let error = not_committed(execute_with_validator(
            &store,
            &mutation_domain,
            &validator_domain,
            failure,
        ));
        match failure {
            Failure::Storage => {
                assert!(matches!(
                    error,
                    CommandError::ContributorAccess {
                        domain: "callback_access",
                        stage: ContributorCallbackStage::Validation,
                        source: DomainCallbackSource::Read(ReadError::Storage { .. }),
                    }
                ));
                assert_eq!(store.health().state(), HomeHealthState::Failed);
                continue;
            }
            Failure::Structural => {
                assert!(matches!(
                    error,
                    CommandError::ContributorAccess {
                        domain: "callback_access",
                        stage: ContributorCallbackStage::Validation,
                        source: DomainCallbackSource::Read(ReadError::MalformedRecord { .. }),
                    }
                ));
                assert_eq!(store.health().state(), HomeHealthState::Failed);
                continue;
            }
            Failure::Semantic => {
                assert!(matches!(
                    error,
                    CommandError::ContributorValidation {
                        domain: "callback_access",
                        ..
                    }
                ));
                assert_eq!(store.health().state(), HomeHealthState::Healthy);
            }
            Failure::None => unreachable!(),
        }
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(
            store.domain_revision(&mutation_domain).unwrap(),
            mutation_before
        );
        assert_eq!(
            store.domain_revision(&validator_domain).unwrap(),
            validator_before
        );
    }
}

#[test]
fn registration_preserves_access_provenance_and_semantic_rejection() {
    for (marker, access) in [
        (b"registration-storage".as_slice(), true),
        (b"registration-reject".as_slice(), false),
    ] {
        let directory = tempdir().unwrap();
        let mut store = open(directory.path());
        let domain = store.register_domain::<AccessDomain>().unwrap();
        committed(execute(
            &store,
            &domain,
            Put {
                key: 0,
                value: marker.to_vec(),
                validation: Failure::None,
                contribution: Failure::None,
            },
        ));
        store.close().unwrap();

        let mut reopened = open(directory.path());
        let error = reopened
            .register_domain_with_schema_validation::<AccessDomain>()
            .unwrap_err();
        if access {
            assert!(matches!(
                error,
                DomainRegistrationError::ValidationAccess {
                    source: DomainCallbackSource::Read(ReadError::Storage { .. }),
                    ..
                }
            ));
            assert_eq!(reopened.health().state(), HomeHealthState::Failed);
        } else {
            assert!(matches!(error, DomainRegistrationError::Validation { .. }));
            assert_eq!(reopened.health().state(), HomeHealthState::Failed);
        }
    }
}
