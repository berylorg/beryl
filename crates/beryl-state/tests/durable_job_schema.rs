mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, DomainRegistrationError,
    DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion, HomeStore, KeyspaceFamily,
    KeyspaceSchemaVersion, ReadError, RecordCodec, RecordVersion, StorageDomain,
};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, ResolutionIntentId, SyndicDraftId,
    SyndicThreadId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffJobAdmission, DiscussionContextDigest,
    DiscussionContextOwnerId, ParentQueueOrdinal, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText,
};
use tempfile::tempdir;

use support::{execute, open};

const JOB_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("records", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("live-jobs", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("request-idempotency", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("discussion-attempts", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("latest-attempts", KeyspaceSchemaVersion::new(1)),
];

struct DurableJobV2Probe;
struct JobRecordV2;

impl StorageDomain for DurableJobV2Probe {
    const NAME: &'static str = "beryl-durable-job";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = JOB_FAMILIES;
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
        formatter.write_str("probe job identity is not 16 bytes")
    }
}

impl Error for ProbeCodecError {}

#[test]
fn durable_job_records_reject_an_unsupported_record_version_on_reopen() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let admission = BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([1; 16]),
        ResolutionAttemptOrdinal::FIRST,
        SyndicThreadId::from_bytes([2; 16]),
        SyndicThreadId::from_bytes([3; 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([4; 16])),
        DiscussionContextDigest::from_bytes([5; 32]),
        SyndicTurnId::from_bytes([6; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new("child-cas-thread").unwrap(),
            CasTurnId::new("child-cas-turn").unwrap(),
            DynamicToolCallId::new("resolution-tool-call").unwrap(),
        ),
        ParentQueueOrdinal::new(0),
        ResolutionText::new("Persist this exact resolution.").unwrap(),
    );
    execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ),
    )
    .unwrap();
    store.close().unwrap();

    let mut probe = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = probe.register_domain::<DurableJobV2Probe>().unwrap_err();
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
