mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    KeyspaceSchemaVersion, PointReadLimit, ReadError, RecordCodec, RecordFamily, RecordVersion,
    StorageDomain, WholeHomeScrubTrigger,
};
use beryl_model::RuntimeId;
use tempfile::tempdir;

struct RuntimeV2Probe;
struct RuntimeRecordV2;
struct ExecutableIndexBytes;
struct RootRecordBytes;
struct RootIdIndexBytes;
struct RootPathIndexBytes;
struct HomeRootIndexBytes;

const RUNTIME_FAMILIES: &[RecordFamily<RuntimeV2Probe>] = &[
    RecordFamily::new::<RuntimeRecordV2>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ExecutableIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootRecordBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootIdIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootPathIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<HomeRootIndexBytes>(KeyspaceSchemaVersion::new(1)),
];
impl StorageDomain for RuntimeV2Probe {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = RUNTIME_FAMILIES;
    type ValidationError = ProbeError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        reader
            .cursor::<RuntimeRecordV2>(
                &CursorRange::closed(
                    RuntimeId::from_bytes([0; 16]),
                    RuntimeId::from_bytes([u8::MAX; 16]),
                ),
                CursorDirection::Forward,
                CursorReadLimits::new(1, 1_000_000).unwrap(),
            )
            .map(|_| ())
            .map_err(ProbeError)
    }
}

macro_rules! v2_codec {
    ($codec:ident, $domain:ident, $key:ty, $family:literal, $encode:expr, $decode:expr) => {
        impl RecordCodec<$domain> for $codec {
            type Key = $key;
            type Value = Vec<u8>;
            type Error = ProbeCodecError;
            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(2);
            const MAX_KEY_BYTES: usize = 16;
            const MAX_VALUE_BYTES: usize = 128 * 1024;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok($encode(key))
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                $decode(encoded)
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

v2_codec!(
    RuntimeRecordV2,
    RuntimeV2Probe,
    RuntimeId,
    "runtimes",
    |key: &RuntimeId| key.as_bytes().to_vec(),
    |encoded: &[u8]| decode_id(encoded).map(RuntimeId::from_bytes)
);

macro_rules! passthrough_codec {
    ($codec:ident, $family:literal, $max_key:expr, $max_value:expr) => {
        impl RecordCodec<RuntimeV2Probe> for $codec {
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

passthrough_codec!(
    ExecutableIndexBytes,
    "runtime-executable-index",
    u16::MAX as usize,
    16
);
passthrough_codec!(RootRecordBytes, "roots", 32, 132 * 1024);
passthrough_codec!(RootIdIndexBytes, "root-id-index", 16, 16);
passthrough_codec!(RootPathIndexBytes, "root-path-index", u16::MAX as usize, 16);
passthrough_codec!(HomeRootIndexBytes, "runtime-home-root-index", 16, 16);
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
        formatter.write_str("probe identity is not 16 bytes")
    }
}

impl Error for ProbeCodecError {}

fn decode_id(encoded: &[u8]) -> Result<[u8; 16], ProbeCodecError> {
    encoded.try_into().map_err(|_| ProbeCodecError)
}

#[test]
fn routine_reopen_defers_an_unsupported_runtime_record_version_to_explicit_scrub() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = store.register_domain::<RuntimeV2Probe>().unwrap();
    store
        .inject_persisted_corrupt_record::<RuntimeV2Probe, RuntimeRecordV2>(
            &probe,
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
    let probe = reopened.register_domain::<RuntimeV2Probe>().unwrap();
    assert_version_error(
        reopened
            .read_point::<RuntimeV2Probe, RuntimeRecordV2>(
                &probe,
                &RuntimeId::from_bytes([1; 16]),
                PointReadLimit::new(128 * 1024 + 4).unwrap(),
            )
            .unwrap_err(),
    );
    assert!(
        reopened
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}

fn assert_version_error(source: ReadError) {
    assert!(matches!(
        source,
        ReadError::UnsupportedRecordVersion {
            supported,
            found: 1,
            ..
        } if supported == RecordVersion::new(2)
    ));
}
