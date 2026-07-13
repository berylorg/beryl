mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, DomainRegistrationError,
    DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceFamily,
    KeyspaceSchemaVersion, ReadError, RecordCodec, RecordVersion, StorageDomain,
};
use beryl_model::{RuntimeId, SyndicThreadId};
use tempfile::tempdir;

use support::{binding, create_host_runtime, create_metadata, open};

const RUNTIME_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("runtimes", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-executable-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("roots", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-id-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-path-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-home-root-index", KeyspaceSchemaVersion::new(1)),
];
const METADATA_FAMILIES: &[KeyspaceFamily] = &[KeyspaceFamily::new(
    "records",
    KeyspaceSchemaVersion::new(1),
)];

struct RuntimeV2Probe;
struct RuntimeRecordV2;
struct MetadataV2Probe;
struct MetadataRecordV2;

impl StorageDomain for RuntimeV2Probe {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = RUNTIME_FAMILIES;
    type ValidationError = ProbeError;

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

impl StorageDomain for MetadataV2Probe {
    const NAME: &'static str = "beryl-thread-metadata";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = METADATA_FAMILIES;
    type ValidationError = ProbeError;

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        reader
            .cursor::<MetadataRecordV2>(
                &CursorRange::closed(
                    SyndicThreadId::from_bytes([0; 16]),
                    SyndicThreadId::from_bytes([u8::MAX; 16]),
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
v2_codec!(
    MetadataRecordV2,
    MetadataV2Probe,
    SyndicThreadId,
    "records",
    |key: &SyndicThreadId| key.as_bytes().to_vec(),
    |encoded: &[u8]| decode_id(encoded).map(SyndicThreadId::from_bytes)
);

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
fn both_product_domains_reject_an_unsupported_record_version_on_reopen() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(
        &store,
        state,
        1,
        2,
        r"C:\Codex\codex.exe",
        r"C:\Users\operator",
    );
    create_metadata(
        &store,
        state,
        3,
        binding(1, 2, r"C:\Users\operator"),
        beryl_state::ThreadMetadataKind::Ordinary,
    );
    store.close().unwrap();

    let mut runtime_probe = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    assert_version_error(
        runtime_probe
            .register_domain::<RuntimeV2Probe>()
            .unwrap_err(),
    );
    runtime_probe.close().unwrap();

    let mut metadata_probe = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    assert_version_error(
        metadata_probe
            .register_domain::<MetadataV2Probe>()
            .unwrap_err(),
    );
}

fn assert_version_error(error: DomainRegistrationError) {
    let DomainRegistrationError::Validation { source, .. } = error else {
        panic!("expected validation error, got {error}");
    };
    let probe = source.downcast_ref::<ProbeError>().unwrap();
    assert!(matches!(
        probe.0,
        ReadError::UnsupportedRecordVersion {
            supported,
            found: 1,
            ..
        } if supported == RecordVersion::new(2)
    ));
}
