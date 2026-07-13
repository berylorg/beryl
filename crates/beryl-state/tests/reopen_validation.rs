use std::{convert::Infallible, error::Error, fmt};

use beryl_home_store::{
    DomainMutation, DomainReader, DomainRegistrationError, DomainSchemaVersion, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceFamily, KeyspaceSchemaVersion,
    MutationBuildError, MutationBuilder, RecordCodec, RecordVersion, StorageDomain,
};
use beryl_model::RuntimeId;
use beryl_state::{BerylStateBootstrap, BerylStateRegistrationError};
use tempfile::tempdir;

const RUNTIME_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("runtimes", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-executable-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("roots", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-id-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-path-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-home-root-index", KeyspaceSchemaVersion::new(1)),
];

struct IncompleteRuntimeDomain;
struct RuntimeBytes;
struct ExecutableIndexBytes;

impl StorageDomain for IncompleteRuntimeDomain {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = RUNTIME_FAMILIES;
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

#[derive(Debug)]
struct FixtureCodecError;

impl fmt::Display for FixtureCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("fixture bytes exceed their schema contract")
    }
}

impl Error for FixtureCodecError {}

macro_rules! byte_codec {
    ($codec:ident, $family:literal, $max_key:expr, $max_value:expr) => {
        impl RecordCodec<IncompleteRuntimeDomain> for $codec {
            type Key = Vec<u8>;
            type Value = Vec<u8>;
            type Error = FixtureCodecError;

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

byte_codec!(RuntimeBytes, "runtimes", 16, 132 * 1024);
byte_codec!(
    ExecutableIndexBytes,
    "runtime-executable-index",
    u16::MAX as usize,
    16
);

struct SeedRuntimeWithoutHomeRoot {
    runtime_key: Vec<u8>,
    runtime_value: Vec<u8>,
    executable_key: Vec<u8>,
}

#[derive(Debug)]
struct FixtureMutationError(MutationBuildError);

impl fmt::Display for FixtureMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for FixtureMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl From<MutationBuildError> for FixtureMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self(source)
    }
}

impl DomainMutation<IncompleteRuntimeDomain> for SeedRuntimeWithoutHomeRoot {
    type Error = FixtureMutationError;

    fn validate(
        &self,
        _reader: &DomainReader<'_, IncompleteRuntimeDomain>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, IncompleteRuntimeDomain>,
        mutations: &mut MutationBuilder<'_, IncompleteRuntimeDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<RuntimeBytes>(&self.runtime_key, &self.runtime_value)?;
        mutations.put::<ExecutableIndexBytes>(&self.executable_key, &self.runtime_key)?;
        Ok(())
    }
}

#[test]
fn minimal_session_bootstrap_defers_unrelated_runtime_validation() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let domain = store.register_domain::<IncompleteRuntimeDomain>().unwrap();
    let runtime_id = RuntimeId::from_bytes([1; 16]);
    let executable = r"C:\Codex\codex.exe";
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            SeedRuntimeWithoutHomeRoot {
                runtime_key: runtime_id.as_bytes().to_vec(),
                runtime_value: runtime_record(runtime_id, executable),
                executable_key: executable_index_key(executable),
            },
        ))
        .unwrap();
    store.execute(command).unwrap();
    store.close().unwrap();

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let bootstrap = BerylStateBootstrap::register(&mut reopened)
        .expect("minimal bootstrap must register only the session domain");
    assert!(
        bootstrap
            .session()
            .minimal_bootstrap(&reopened)
            .unwrap()
            .is_none()
    );
    let error = match bootstrap.complete(&mut reopened) {
        Ok(_) => panic!("incomplete runtime unexpectedly passed validation"),
        Err(error) => error,
    };
    assert_missing_home_root(error);
}

fn runtime_record(runtime_id: RuntimeId, executable: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(runtime_id.as_bytes());
    push_host_path(&mut encoded, executable);
    encoded.push(0); // Host runtime mode.
    encoded.push(0); // Host runtime mode in the runtime-native path.
    encoded.push(0); // Windows path flavor.
    push_text(&mut encoded, executable);
    push_text(&mut encoded, "Host");
    encoded.extend_from_slice(&10_u64.to_be_bytes());
    encoded.push(0); // Unknown availability.
    encoded.push(0); // No availability observation time.
    encoded.extend_from_slice(&1_u64.to_be_bytes());
    encoded
}

fn executable_index_key(executable: &str) -> Vec<u8> {
    let mut encoded = vec![1];
    push_host_path(&mut encoded, executable);
    encoded
}

fn push_host_path(encoded: &mut Vec<u8>, value: &str) {
    encoded.push(0); // Windows path flavor.
    push_text(encoded, value);
}

fn push_text(encoded: &mut Vec<u8>, value: &str) {
    encoded.extend_from_slice(&(value.len() as u32).to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
}

fn assert_missing_home_root(error: BerylStateRegistrationError) {
    let BerylStateRegistrationError::Domain { domain, source } = error else {
        panic!("expected domain registration failure, got {error}");
    };
    assert_eq!(domain, "beryl-runtime-root");
    let DomainRegistrationError::Validation { source, .. } = source else {
        panic!("expected domain validation failure, got {source}");
    };
    assert_eq!(
        source.to_string(),
        "runtime has no non-removable home-root index"
    );
}
