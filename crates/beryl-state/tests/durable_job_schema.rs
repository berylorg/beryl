mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    KeyspaceSchemaVersion, PointReadLimit, ReadError, RecordCodec, RecordFamily, RecordVersion,
    StorageDomain, WholeHomeScrubTrigger,
};
use beryl_model::JobId;
use tempfile::tempdir;

struct DurableJobV2Probe;
struct JobRecordV2;
struct LiveJobBytes;
struct RequestIdempotencyBytes;
struct DiscussionAttemptBytes;
struct LatestAttemptBytes;

const JOB_FAMILIES: &[RecordFamily<DurableJobV2Probe>] = &[
    RecordFamily::new::<JobRecordV2>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<LiveJobBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RequestIdempotencyBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DiscussionAttemptBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<LatestAttemptBytes>(KeyspaceSchemaVersion::new(1)),
];

impl StorageDomain for DurableJobV2Probe {
    const NAME: &'static str = "beryl-durable-job";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = JOB_FAMILIES;
    type ValidationError = ProbeError;

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        reader
            .cursor::<JobRecordV2>(
                &CursorRange::closed(JobId::from_bytes([0; 16]), JobId::from_bytes([u8::MAX; 16])),
                CursorDirection::Forward,
                CursorReadLimits::new(1, 256 * 1024).unwrap(),
            )
            .map(|_| ())
            .map_err(ProbeError)
    }
}

impl RecordCodec<DurableJobV2Probe> for JobRecordV2 {
    type Key = JobId;
    type Value = Vec<u8>;
    type Error = ProbeCodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 128 * 1024;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        encoded
            .try_into()
            .map(JobId::from_bytes)
            .map_err(|_| ProbeCodecError)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(encoded.to_vec())
    }
}

macro_rules! passthrough_codec {
    ($codec:ident, $family:literal, $max_key:expr, $max_value:expr) => {
        impl RecordCodec<DurableJobV2Probe> for $codec {
            type Key = Vec<u8>;
            type Value = Vec<u8>;
            type Error = ProbeCodecError;

            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = $max_key;
            const MAX_VALUE_BYTES: usize = $max_value;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.clone())
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                Ok(encoded.to_vec())
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                Ok(value.clone())
            }

            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded.to_vec())
            }
        }
    };
}

passthrough_codec!(LiveJobBytes, "live-jobs", 16, 128 * 1024);
passthrough_codec!(RequestIdempotencyBytes, "request-idempotency", 1024, 64);
passthrough_codec!(DiscussionAttemptBytes, "discussion-attempts", 24, 16);
passthrough_codec!(LatestAttemptBytes, "latest-attempts", 16, 24);

#[derive(Debug)]
struct ProbeError(ReadError);

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ProbeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl DomainCallbackError for ProbeError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Ok(DomainCallbackSource::Read(self.0))
    }
}

#[derive(Debug)]
struct ProbeCodecError;

impl fmt::Display for ProbeCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("probe job identity is not 16 bytes")
    }
}

impl Error for ProbeCodecError {}

#[test]
fn routine_reopen_defers_an_unsupported_durable_job_record_version_to_explicit_scrub() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = store.register_domain::<DurableJobV2Probe>().unwrap();
    store
        .inject_persisted_corrupt_record::<DurableJobV2Probe, JobRecordV2>(
            probe,
            &[1; 16],
            &1_u32.to_be_bytes(),
        )
        .unwrap();
    store.close().unwrap();

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = reopened.register_domain::<DurableJobV2Probe>().unwrap();
    assert!(matches!(
        reopened.read_point::<DurableJobV2Probe, JobRecordV2>(
            probe,
            &JobId::from_bytes([1; 16]),
            PointReadLimit::new(128 * 1024 + 4).unwrap(),
        ),
        Err(ReadError::UnsupportedRecordVersion {
            supported,
            found: 1,
            ..
        }) if supported == RecordVersion::new(2)
    ));
    assert!(
        reopened
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}
