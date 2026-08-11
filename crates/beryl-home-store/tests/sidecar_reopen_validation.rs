#![cfg(feature = "test-faults")]

use std::{error::Error, fmt, io, num::NonZeroU64};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    DomainRegistrationError, DomainSchemaVersion, DomainValidationError, HealthVerificationError,
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeRecoveryError, HomeSchemaVersion, HomeStore,
    KeyspaceSchemaVersion, MutationBuildError, MutationBuilder, PointReadLimit, RecordCodec,
    RecordFamily, RecordVersion, SidecarAddress, SidecarByteLimit, SidecarDigest, SidecarError,
    SidecarNamespace, SidecarVerifier, StorageDomain,
};
use tempfile::tempdir;

struct ReferenceDomain;
struct ReferenceRecord;

#[derive(Debug)]
enum ReferenceError {
    Read(beryl_home_store::ReadError),
    Sidecar(SidecarError),
    Build(MutationBuildError),
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Sidecar(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
        }
    }
}

impl Error for ReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Sidecar(source) => Some(source),
            Self::Build(source) => Some(source),
        }
    }
}

impl DomainCallbackError for ReferenceError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            Self::Sidecar(source) => Ok(DomainCallbackSource::Sidecar(source)),
            semantic => Err(semantic),
        }
    }
}

impl StorageDomain for ReferenceDomain {
    const NAME: &'static str = "sidecar-references";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<ReferenceRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = ReferenceError;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }

    fn validate_reopen(
        reader: &DomainReader<'_, Self>,
        sidecars: &SidecarVerifier<'_>,
    ) -> Result<(), Self::ValidationError> {
        let reference = reader
            .point::<ReferenceRecord>(&1, PointReadLimit::new(128).unwrap())
            .map_err(ReferenceError::Read)?;
        if let Some(reference) = reference {
            sidecars
                .verify(&reference, sidecar_limit())
                .map_err(ReferenceError::Sidecar)?;
        }
        Ok(())
    }
}

impl RecordCodec<ReferenceDomain> for ReferenceRecord {
    type Key = u8;
    type Value = SidecarAddress;
    type Error = io::Error;

    const FAMILY: &'static str = "references";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 96;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        encoded
            .first()
            .copied()
            .filter(|_| encoded.len() == 1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid key"))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let namespace = value.namespace().as_str().as_bytes();
        let mut encoded = Vec::with_capacity(1 + namespace.len() + 32 + 8);
        encoded.push(u8::try_from(namespace.len()).expect("bounded namespace"));
        encoded.extend_from_slice(namespace);
        encoded.extend_from_slice(&value.digest().as_bytes());
        encoded.extend_from_slice(&value.length().to_be_bytes());
        Ok(encoded)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let namespace_len = usize::from(*encoded.first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing namespace length")
        })?);
        let digest_start = 1 + namespace_len;
        let length_start = digest_start + 32;
        if encoded.len() != length_start + 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid reference length",
            ));
        }
        let namespace = std::str::from_utf8(&encoded[1..digest_start])
            .map_err(|source| io::Error::new(io::ErrorKind::InvalidData, source))?;
        let digest: [u8; 32] = encoded[digest_start..length_start]
            .try_into()
            .expect("validated digest length");
        let length = u64::from_be_bytes(
            encoded[length_start..]
                .try_into()
                .expect("validated byte length"),
        );
        Ok(SidecarAddress::new(
            SidecarNamespace::new(namespace).map_err(io::Error::other)?,
            SidecarDigest::from_bytes(digest),
            length,
        ))
    }
}

struct PutReference(SidecarAddress);

impl DomainMutation<ReferenceDomain> for PutReference {
    type Error = ReferenceError;

    fn validate(&self, _reader: &DomainReader<'_, ReferenceDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, ReferenceDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<ReferenceRecord>(1)
            .map_err(ReferenceError::Build)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, ReferenceDomain>,
        mutations: &mut MutationBuilder<'_, ReferenceDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<ReferenceRecord>(&1, &self.0)
            .map_err(ReferenceError::Build)
    }
}

fn sidecar_limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn put_reference(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<ReferenceDomain>,
    address: SidecarAddress,
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutReference(address),
        ))
        .unwrap();
    command
}

#[test]
fn reopen_validator_accepts_a_durable_referenced_sidecar() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain = store.register_domain::<ReferenceDomain>().unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"referenced",
            sidecar_limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    let mut command = put_reference(&store, domain, address);
    command.require_sidecar(sidecar).unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute(put_reference(
            &store,
            domain,
            SidecarAddress::new(
                SidecarNamespace::new("images").unwrap(),
                SidecarDigest::from_bytes([1; 32]),
                1,
            ),
        )),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    store.verify_health().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn missing_referenced_sidecar_fails_verification_and_same_home_reopen() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain = store.register_domain::<ReferenceDomain>().unwrap();
    let missing = SidecarAddress::new(
        SidecarNamespace::new("images").unwrap(),
        SidecarDigest::from_bytes([9; 32]),
        10,
    );
    assert!(matches!(
        store.execute(put_reference(&store, domain, missing.clone())),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute(put_reference(&store, domain, missing)),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        store.verify_health(),
        Err(HealthVerificationError::DomainValidation(
            DomainValidationError::Access {
                domain: "sidecar-references",
                source: DomainCallbackSource::Sidecar(SidecarError::Missing),
            }
        ))
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        store.recover_same_home(),
        Err(HomeRecoveryError::DomainValidation(
            DomainValidationError::Access {
                domain: "sidecar-references",
                source: DomainCallbackSource::Sidecar(SidecarError::Missing),
            }
        ))
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn existing_domain_registration_runs_its_sidecar_reopen_validator() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let domain = store.register_domain::<ReferenceDomain>().unwrap();
    let missing = SidecarAddress::new(
        SidecarNamespace::new("images").unwrap(),
        SidecarDigest::from_bytes([7; 32]),
        10,
    );
    assert!(matches!(
        store.execute(put_reference(&store, domain, missing)),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    store.close().unwrap();

    let mut reopened = open(directory.path(), faults);
    assert!(matches!(
        reopened.register_domain::<ReferenceDomain>(),
        Err(DomainRegistrationError::ValidationAccess {
            domain: "sidecar-references",
            source: DomainCallbackSource::Sidecar(SidecarError::Missing),
        })
    ));
}
