#![cfg(feature = "test-faults")]

mod support;

use std::{convert::Infallible, error::Error, fmt, sync::Arc};

use beryl_home_store::{
    test_faults::{FaultController, PersistedCorruptionError},
    CodecOperation, DomainCallbackSource, DomainHandle, DomainMutation, DomainReader,
    DomainSchemaVersion, DomainValidationError, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuilder, PointReadLimit,
    ReadError, RecordCodec, RecordFamily, RecordVersion, StorageDomain, WholeHomeScrubTrigger,
};
use tempfile::tempdir;

use support::{committed, AlphaDomain, BytesRecord, BytesRecordV2, FixtureMutationError};

const MAX_STORED_VALUE_BYTES: usize = 1_028;
const MAX_CORRUPTION_FIXTURE_BYTES: usize = 1_048_576;

struct StrictPayloadDomain;
struct StrictPayloadRecord;

#[derive(Debug)]
struct StrictPayloadError;

impl fmt::Display for StrictPayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("strict fixture payload must be zero or one")
    }
}

impl Error for StrictPayloadError {}

impl RecordCodec<StrictPayloadDomain> for StrictPayloadRecord {
    type Key = u8;
    type Value = u8;
    type Error = StrictPayloadError;

    const FAMILY: &'static str = "strict-records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        encoded.first().copied().ok_or(StrictPayloadError)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        if *value <= 1 {
            Ok(vec![*value])
        } else {
            Err(StrictPayloadError)
        }
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        match encoded {
            [value @ 0..=1] => Ok(*value),
            _ => Err(StrictPayloadError),
        }
    }
}

impl StorageDomain for StrictPayloadDomain {
    const NAME: &'static str = "strict-corruption-fixture";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<StrictPayloadRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

struct CorruptionReentrantProbe {
    store: Arc<HomeStore>,
    domain: DomainHandle<AlphaDomain>,
}

impl DomainMutation<AlphaDomain> for CorruptionReentrantProbe {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, AlphaDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !matches!(
            self.store
                .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
                    &self.domain,
                    &[7; 7],
                    &encoded_value(1, b"valid"),
                ),
            Err(PersistedCorruptionError::ReentrantWriter)
        ) {
            return Err(FixtureMutationError::Rejected(
                "persisted corruption seam did not reject writer reentry",
            ));
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<AlphaDomain>>(1)?;
        Ok(())
    }

    fn contribute(
        _prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&9, &b"outer".to_vec())?;
        Ok(())
    }
}

fn encoded_value(version: u32, payload: &[u8]) -> Vec<u8> {
    let mut encoded = version.to_be_bytes().to_vec();
    encoded.extend_from_slice(payload);
    encoded
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn codec_valid_envelope_is_rejected_without_mutating_the_family() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &1_u64.to_be_bytes(),
            &encoded_value(1, b"valid"),
        ),
        Err(PersistedCorruptionError::CodecAcceptedEnvelope {
            domain: "alpha",
            family: "records",
        })
    ));
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(MAX_STORED_VALUE_BYTES).unwrap(),
            )
            .unwrap(),
        None
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn same_thread_writer_reentry_is_rejected_without_deadlock_or_corruption() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(store);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            CorruptionReentrantProbe {
                store: Arc::clone(&store),
                domain: alpha.clone(),
            },
        ))
        .unwrap();

    committed(store.execute(command));
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &9,
                PointReadLimit::new(MAX_STORED_VALUE_BYTES).unwrap(),
            )
            .unwrap(),
        Some(b"outer".to_vec())
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn malformed_in_bound_key_is_persisted_only_for_structural_validation() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &[7; 7],
            &encoded_value(1, b"valid"),
        )
        .unwrap();
    assert!(matches!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            domain: "alpha",
            source: DomainCallbackSource::Read(ReadError::Codec {
                operation: CodecOperation::DecodeKey,
                ..
            }),
        }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

#[test]
fn malformed_in_bound_payload_is_persisted_only_when_the_exact_codec_rejects_it() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let domain = store.register_domain::<StrictPayloadDomain>().unwrap();

    store
        .inject_persisted_corrupt_record::<StrictPayloadDomain, StrictPayloadRecord>(
            &domain,
            &[1],
            &encoded_value(1, &[2]),
        )
        .unwrap();
    assert!(matches!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            domain: "strict-corruption-fixture",
            source: DomainCallbackSource::Read(ReadError::Codec {
                operation: CodecOperation::DecodeValue,
                ..
            }),
        }
    ));
}

#[test]
fn truncated_and_unsupported_version_values_are_admitted_as_corruption() {
    let directory = tempdir().unwrap();
    let mut truncated_store = support::open_home(directory.path());
    let truncated_alpha = truncated_store.register_domain::<AlphaDomain>().unwrap();
    truncated_store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &truncated_alpha,
            &1_u64.to_be_bytes(),
            &[0, 0, 1],
        )
        .unwrap();
    assert!(matches!(
        truncated_store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            source: DomainCallbackSource::Read(ReadError::MalformedRecord { .. }),
            ..
        }
    ));
    drop(truncated_store);

    let directory = tempdir().unwrap();
    let mut version_store = support::open_home(directory.path());
    let version_alpha = version_store.register_domain::<AlphaDomain>().unwrap();
    version_store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &version_alpha,
            &1_u64.to_be_bytes(),
            &encoded_value(2, b"unsupported"),
        )
        .unwrap();
    assert!(matches!(
        version_store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            source: DomainCallbackSource::Read(ReadError::UnsupportedRecordVersion {
                supported,
                found: 2,
                ..
            }),
            ..
        } if *supported == RecordVersion::new(1)
    ));
}

#[test]
fn oversized_envelope_remains_an_accepted_corruption_fixture() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &[0; 9],
            &encoded_value(1, b"valid"),
        )
        .unwrap();
    assert!(matches!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
            source: DomainCallbackSource::Read(ReadError::InvalidStoredKeySize {
                maximum: 8,
                actual: 9,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn foreign_domain_handle_and_shadow_codec_are_rejected_before_fixture_validation() {
    let first_directory = tempdir().unwrap();
    let second_directory = tempdir().unwrap();
    let mut first = support::open_home(first_directory.path());
    let mut second = support::open_home(second_directory.path());
    let first_alpha = first.register_domain::<AlphaDomain>().unwrap();
    let second_alpha = second.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        first.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &second_alpha,
            &1_u64.to_be_bytes(),
            &encoded_value(1, b"valid"),
        ),
        Err(PersistedCorruptionError::ForeignDomain { domain: "alpha" })
    ));
    assert!(matches!(
        first.inject_persisted_corrupt_record::<AlphaDomain, BytesRecordV2<AlphaDomain>>(
            &first_alpha,
            &[7; 7],
            &[0, 0, 1],
        ),
        Err(PersistedCorruptionError::CodecTypeMismatch {
            domain: "alpha",
            family: "records",
        })
    ));
    assert_eq!(first.health().state(), HomeHealthState::Healthy);
}

#[test]
fn empty_engine_oversized_and_fixture_oversized_requests_are_hard_rejected() {
    let directory = tempdir().unwrap();
    let mut store = support::open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &[],
            &[0, 0, 1],
        ),
        Err(PersistedCorruptionError::EmptyKey)
    ));

    let engine_oversized_key = vec![0; usize::from(u16::MAX) + 1];
    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &engine_oversized_key,
            &[],
        ),
        Err(PersistedCorruptionError::FixtureKeyBoundExceeded {
            maximum,
            actual,
        }) if maximum == usize::from(u16::MAX)
            && actual == usize::from(u16::MAX) + 1
    ));

    let fixture_oversized_value = vec![0; MAX_CORRUPTION_FIXTURE_BYTES - size_of::<u64>() + 1];
    assert!(matches!(
        store.inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &1_u64.to_be_bytes(),
            &fixture_oversized_value,
        ),
        Err(PersistedCorruptionError::FixtureBoundExceeded {
            maximum: MAX_CORRUPTION_FIXTURE_BYTES,
            actual,
        }) if actual == MAX_CORRUPTION_FIXTURE_BYTES + 1
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(
        store
            .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &alpha,
                &1,
                PointReadLimit::new(MAX_STORED_VALUE_BYTES).unwrap(),
            )
            .unwrap(),
        None
    );
}

#[test]
fn scrub_rejects_but_routine_recovery_ignores_a_dormant_malformed_envelope() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open_with_faults(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    store
        .inject_persisted_corrupt_record::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &1_u64.to_be_bytes(),
            &encoded_value(2, b"unsupported"),
        )
        .unwrap();

    assert!(matches!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::CorruptionEvidence)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Access {
                domain: "alpha",
                source: DomainCallbackSource::Read(ReadError::UnsupportedRecordVersion {
                    supported,
                    found: 2,
                    ..
                }),
        } if *supported == RecordVersion::new(1)
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let candidate = store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let recovered = candidate.publish();
    assert!(matches!(
        recovered.read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &1,
            PointReadLimit::new(MAX_STORED_VALUE_BYTES).unwrap(),
        ),
        Err(ReadError::UnsupportedRecordVersion { found: 2, .. })
    ));
    assert_eq!(recovered.health().state(), HomeHealthState::Failed);
}
