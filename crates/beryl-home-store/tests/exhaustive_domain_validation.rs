#[path = "support/fjall.rs"]
mod fjall_support;

use std::{error::Error, fmt};

#[cfg(feature = "test-faults")]
use std::{thread, time::Duration};

#[cfg(feature = "test-faults")]
use beryl_home_store::{
    DomainCallbackError, DomainMutation, DomainValidationError, HomeCommand, MutationBuilder,
    test_faults::{FaultController, FaultPoint},
};
use beryl_home_store::{
    DomainCallbackSource, DomainReader, DomainRegistrationError, DomainSchemaVersion,
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    ReadError, RecordCodec, RecordFamily, RecordVersion, StorageDomain, WholeHomeScrubTrigger,
};
use fjall::{Database, PersistMode};
use tempfile::tempdir;

struct StrictDomain;
struct StrictRecord;
struct EmptyKeyDomain;
struct EmptyKeyRecord;

#[derive(Clone, Debug, Eq, PartialEq)]
enum StrictKey {
    Stored(u8),
    Lower,
    Upper,
}

#[derive(Debug)]
struct StrictCodecError(&'static str);

impl fmt::Display for StrictCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for StrictCodecError {}

impl RecordCodec<StrictDomain> for StrictRecord {
    type Key = StrictKey;
    type Value = u8;
    type Error = StrictCodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![match key {
            StrictKey::Stored(value) => *value,
            StrictKey::Lower => 0xfe,
            StrictKey::Upper => 0xff,
        }])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        match encoded {
            [value @ 0..=9] => Ok(StrictKey::Stored(*value)),
            [0xfe] => Ok(StrictKey::Lower),
            [0xff] => Ok(StrictKey::Upper),
            [_] => Err(StrictCodecError("unknown strict key")),
            _ => Err(StrictCodecError("strict key must have one byte")),
        }
    }

    fn validate_stored_key(key: &Self::Key) -> Result<(), Self::Error> {
        match key {
            StrictKey::Stored(_) => Ok(()),
            StrictKey::Lower | StrictKey::Upper => {
                Err(StrictCodecError("cursor sentinel is not a stored key"))
            }
        }
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*value])
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        match encoded {
            [value] => Ok(*value),
            _ => Err(StrictCodecError("strict value must have one byte")),
        }
    }
}

impl StorageDomain for StrictDomain {
    const NAME: &'static str = "strict_validation";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<StrictRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = std::convert::Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for EmptyKeyDomain {
    const NAME: &'static str = "empty_key_guard";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<EmptyKeyRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = std::convert::Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl RecordCodec<EmptyKeyDomain> for EmptyKeyRecord {
    type Key = ();
    type Value = ();
    type Error = std::convert::Infallible;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(_key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }

    fn decode_key(_encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        Ok(())
    }

    fn encode_value(_value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![0])
    }

    fn decode_value(_encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(())
    }
}

fn open(path: &std::path::Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn raw_insert(path: &std::path::Path, key: &[u8], value: &[u8]) {
    let database = Database::recover(fjall_support::config(&path.join("state"))).unwrap();
    let keyspace = database
        .open_keyspace("d.strict_validation.records")
        .unwrap();
    keyspace.insert(key, value).unwrap();
    database.persist(PersistMode::SyncAll).unwrap();
}

#[cfg(feature = "test-faults")]
fn valid_value(value: u8) -> Vec<u8> {
    let mut encoded = 1_u32.to_be_bytes().to_vec();
    encoded.push(value);
    encoded
}

#[test]
fn shared_physical_key_guard_rejects_empty_codec_output() {
    let directory = tempdir().unwrap();
    let mut store = open(directory.path());
    let domain = store.register_domain::<EmptyKeyDomain>().unwrap();
    assert!(matches!(
        store.read_point::<EmptyKeyDomain, EmptyKeyRecord>(
            &domain,
            &(),
            beryl_home_store::PointReadLimit::new(5).unwrap(),
        ),
        Err(ReadError::InvalidKeySize {
            maximum: 1,
            actual: 0,
            ..
        })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn explicit_schema_registration_scans_every_raw_key_and_value_envelope() {
    let cases: &[(&[u8], &[u8])] = &[
        (&[0, 1], &[0, 0, 0, 1, 7]),
        (&[42], &[0, 0, 0, 1, 7]),
        (&[0xfe], &[0, 0, 0, 1, 7]),
        (&[1], &[]),
        (&[1], &[0, 0, 0, 1, 7, 8]),
        (&[1], &[0, 0, 0, 2, 7]),
        (&[1], &[0, 0, 0, 1]),
    ];

    for (key, value) in cases {
        let directory = tempdir().unwrap();
        let mut store = open(directory.path());
        store.register_domain::<StrictDomain>().unwrap();
        store.close().unwrap();
        raw_insert(directory.path(), key, value);

        let mut reopened = open(directory.path());
        let error = reopened
            .register_domain_with_schema_validation::<StrictDomain>()
            .unwrap_err();
        assert!(matches!(
            error,
            DomainRegistrationError::ValidationAccess {
                source: DomainCallbackSource::Read(_),
                ..
            }
        ));
        assert_eq!(reopened.health().state(), HomeHealthState::Failed);
    }
}

#[derive(Debug)]
#[cfg(feature = "test-faults")]
struct ForcedFailure(ReadError);

#[cfg(feature = "test-faults")]
impl fmt::Display for ForcedFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(feature = "test-faults")]
impl Error for ForcedFailure {}

#[cfg(feature = "test-faults")]
impl DomainCallbackError for ForcedFailure {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Ok(DomainCallbackSource::Read(self.0))
    }
}

#[cfg(feature = "test-faults")]
struct FailStructurally;

#[cfg(feature = "test-faults")]
impl DomainMutation<StrictDomain> for FailStructurally {
    type Error = ForcedFailure;

    fn validate(&self, _reader: &DomainReader<'_, StrictDomain>) -> Result<(), Self::Error> {
        Err(ForcedFailure(ReadError::MalformedRecord {
            domain: StrictDomain::NAME,
            family: StrictRecord::FAMILY,
        }))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, StrictDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<StrictRecord>(1)
            .expect("fixture reservation must be structurally valid");
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, StrictDomain>,
        _mutations: &mut MutationBuilder<'_, StrictDomain>,
    ) -> Result<(), Self::Error> {
        unreachable!("validation fails before contribution")
    }
}

#[test]
#[cfg(feature = "test-faults")]
fn routine_recovery_ignores_raw_corruption_but_explicit_scrub_rejects_it() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let domain = store.register_domain::<StrictDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(store.domain_revision(&domain).unwrap(), FailStructurally))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let block = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    raw_insert(directory.path(), &[42], &valid_value(7));
    block.release();

    let recovered = worker.join().unwrap().unwrap().publish();
    assert!(matches!(
        recovered
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            source: DomainCallbackSource::Read(_),
            ..
        }
    ));
    assert_eq!(recovered.health().state(), HomeHealthState::Failed);
}
