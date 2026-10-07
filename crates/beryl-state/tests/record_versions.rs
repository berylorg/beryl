mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainReader, DomainSchemaVersion, HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, KeyspaceSchemaVersion, PointReadLimit, ReadError, RecordCodec, RecordFamily,
    RecordVersion, StorageDomain, WholeHomeScrubTrigger,
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

    fn create_runtime_attachment(
        _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Self::RuntimeAttachmentError> {
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
        passthrough_codec!(RuntimeV2Probe, $codec, $family, $max_key, $max_value);
    };
    ($domain:ident, $codec:ident, $family:literal, $max_key:expr, $max_value:expr) => {
        impl RecordCodec<$domain> for $codec {
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
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = candidate.register_domain::<RuntimeV2Probe>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<RuntimeV2Probe>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    store
        .inject_persisted_corrupt_record::<RuntimeV2Probe, RuntimeRecordV2>(
            &probe,
            &[1; 16],
            &1_u32.to_be_bytes(),
        )
        .unwrap();
    store.close().unwrap();

    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = candidate.register_domain::<RuntimeV2Probe>().unwrap();
    let reopened = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<RuntimeV2Probe>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
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

struct MalformedRuntimeProbe;
struct MalformedRuntimeRecord;

impl StorageDomain for MalformedRuntimeProbe {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[
        RecordFamily::new::<MalformedRuntimeRecord>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<ExecutableIndexBytes>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<RootRecordBytes>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<RootIdIndexBytes>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<RootPathIndexBytes>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<HomeRootIndexBytes>(KeyspaceSchemaVersion::new(1)),
    ];
    type ValidationError = std::convert::Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment(
        _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl RecordCodec<MalformedRuntimeProbe> for MalformedRuntimeRecord {
    type Key = RuntimeId;
    type Value = Vec<u8>;
    type Error = ProbeCodecError;
    const FAMILY: &'static str = "runtimes";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 128 * 1024;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_id(encoded).map(RuntimeId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }

    fn decode_value(_encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Err(ProbeCodecError)
    }
}

passthrough_codec!(
    MalformedRuntimeProbe,
    ExecutableIndexBytes,
    "runtime-executable-index",
    u16::MAX as usize,
    16
);
passthrough_codec!(
    MalformedRuntimeProbe,
    RootRecordBytes,
    "roots",
    32,
    132 * 1024
);
passthrough_codec!(
    MalformedRuntimeProbe,
    RootIdIndexBytes,
    "root-id-index",
    16,
    16
);
passthrough_codec!(
    MalformedRuntimeProbe,
    RootPathIndexBytes,
    "root-path-index",
    u16::MAX as usize,
    16
);
passthrough_codec!(
    MalformedRuntimeProbe,
    HomeRootIndexBytes,
    "runtime-home-root-index",
    16,
    16
);

#[test]
fn runtime_launch_form_rejects_unknown_missing_and_legacy_record_tags() {
    for (version, tag) in [(2_u32, Some(2_u8)), (2, Some(255)), (2, None), (1, Some(1))] {
        let directory = tempdir().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let probe = candidate
            .register_domain::<MalformedRuntimeProbe>()
            .unwrap();
        let store = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<MalformedRuntimeProbe>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let executable = r"C:\runtime\selected.exe";
        let mut encoded = version.to_be_bytes().to_vec();
        encoded.extend_from_slice(&[1; 16]);
        encoded.push(0);
        encoded.extend_from_slice(&(executable.len() as u32).to_be_bytes());
        encoded.extend_from_slice(executable.as_bytes());
        encoded.push(0);
        if let Some(tag) = tag {
            encoded.push(tag);
            encoded.extend_from_slice(&[0, 0]);
            encoded.extend_from_slice(&(executable.len() as u32).to_be_bytes());
            encoded.extend_from_slice(executable.as_bytes());
            encoded.extend_from_slice(&4_u32.to_be_bytes());
            encoded.extend_from_slice(b"Host");
            encoded.extend_from_slice(&1_u64.to_be_bytes());
            encoded.extend_from_slice(&[0, 0]);
            encoded.extend_from_slice(&1_u64.to_be_bytes());
        }
        store
            .inject_persisted_corrupt_record::<MalformedRuntimeProbe, MalformedRuntimeRecord>(
                &probe, &[1; 16], &encoded,
            )
            .unwrap();
        store.close().unwrap();

        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = beryl_state::BerylState::register(&mut candidate).unwrap();
        let reopened = candidate
            .prepare_publication(beryl_state::BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let error = state
            .runtime_roots()
            .runtime(&reopened, RuntimeId::from_bytes([1; 16]))
            .unwrap_err();
        if version == 1 {
            assert_version_error(error);
        } else {
            assert!(
                matches!(error, ReadError::Codec { .. }),
                "unexpected malformed-tag error: {error}"
            );
        }
        reopened.close().unwrap();

        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        assert!(beryl_state::BerylState::register_with_schema_validation(&mut candidate).is_err());
        candidate.close().unwrap();
    }
}
