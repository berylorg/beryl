use std::{
    convert::Infallible,
    error::Error,
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader, DomainSchemaVersion,
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    MutationBuildError, MutationBuilder, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
    WholeHomeScrubTrigger,
};
use tempfile::tempdir;

static DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static VALIDATOR_CALLS: AtomicUsize = AtomicUsize::new(0);

struct CountedDomain;
struct CountedRecord;

impl StorageDomain for CountedDomain {
    const NAME: &'static str = "counted_work";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<CountedRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        VALIDATOR_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

impl RecordCodec<CountedDomain> for CountedRecord {
    type Key = u64;
    type Value = u8;
    type Error = CodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.to_be_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        DECODE_CALLS.fetch_add(1, Ordering::Relaxed);
        let bytes = encoded.try_into().map_err(|_| CodecError)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*value])
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        DECODE_CALLS.fetch_add(1, Ordering::Relaxed);
        encoded.first().copied().ok_or(CodecError)
    }
}

#[derive(Debug)]
struct CodecError;

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid counted record")
    }
}

impl Error for CodecError {}

#[derive(Debug)]
struct PutError(MutationBuildError);

impl fmt::Display for PutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for PutError {}

impl DomainCallbackError for PutError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

struct Put(u64);

impl DomainMutation<CountedDomain> for Put {
    type Error = PutError;

    fn validate(&self, _reader: &DomainReader<'_, CountedDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, CountedDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<CountedRecord>(1)
            .map_err(PutError)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, CountedDomain>,
        mutations: &mut MutationBuilder<'_, CountedDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<CountedRecord>(&self.0, &1)
            .map_err(PutError)
    }
}

#[test]
fn one_command_never_scans_the_existing_domain() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let domain = store.register_domain::<CountedDomain>().unwrap();

    for key in 0..64 {
        execute(&store, domain, key);
    }
    DECODE_CALLS.store(0, Ordering::Relaxed);
    VALIDATOR_CALLS.store(0, Ordering::Relaxed);

    execute(&store, domain, 64);
    assert_eq!(DECODE_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(VALIDATOR_CALLS.load(Ordering::Relaxed), 0);

    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(DECODE_CALLS.load(Ordering::Relaxed), 130);
    assert_eq!(VALIDATOR_CALLS.load(Ordering::Relaxed), 1);
}

fn execute(store: &HomeStore, domain: beryl_home_store::DomainHandle<CountedDomain>, key: u64) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(store.domain_revision(domain).unwrap(), Put(key)))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}
