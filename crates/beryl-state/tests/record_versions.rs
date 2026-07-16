mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainReader, DomainRegistrationError, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceSchemaVersion, ReadError, RecordCodec, RecordFamily, RecordVersion,
    StorageDomain,
};
use beryl_model::{RuntimeId, SyndicThreadId};
use tempfile::tempdir;

use support::{binding, create_host_runtime, create_metadata, open};

struct RuntimeV2Probe;
struct RuntimeRecordV2;
struct ExecutableIndexBytes;
struct RootRecordBytes;
struct RootIdIndexBytes;
struct RootPathIndexBytes;
struct HomeRootIndexBytes;
struct MetadataV2Probe;
struct MetadataRecordV2;

const RUNTIME_FAMILIES: &[RecordFamily<RuntimeV2Probe>] = &[
    RecordFamily::new::<RuntimeRecordV2>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<ExecutableIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootRecordBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootIdIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RootPathIndexBytes>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<HomeRootIndexBytes>(KeyspaceSchemaVersion::new(1)),
];
const METADATA_FAMILIES: &[RecordFamily<MetadataV2Probe>] =
    &[RecordFamily::new::<MetadataRecordV2>(
        KeyspaceSchemaVersion::new(1),
    )];

impl StorageDomain for RuntimeV2Probe {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = RUNTIME_FAMILIES;
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
    const FAMILIES: &'static [RecordFamily<Self>] = METADATA_FAMILIES;
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
    let DomainRegistrationError::ValidationAccess {
        source: DomainCallbackSource::Read(source),
        ..
    } = error
    else {
        panic!("expected typed validation-access error, got {error}");
    };
    assert!(matches!(
        source,
        ReadError::UnsupportedRecordVersion {
            supported,
            found: 1,
            ..
        } if supported == RecordVersion::new(2)
    ));
}
