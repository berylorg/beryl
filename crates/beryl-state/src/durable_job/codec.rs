use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, JobRevision, ResolutionIntentId,
    SyndicDraftId, SyndicThreadId, SyndicTurnId,
};

use crate::encoding::{CodecError, Decoder, Encoder};

use super::{
    BRANCH_HANDOFF_JOB_RECORD_LIMIT, BranchHandoffJobRecord, DiscussionContextDigest,
    DiscussionContextOwnerId, DurableJobDomain, LatestBranchHandoffAttempt, ParentQueueOrdinal,
    ResolutionAttemptOrdinal, ResolutionRequestAdmission, ResolutionRequestIdentity,
    ResolutionText, branch_handoff_job_id,
};

mod state;

use state::{decode_state, encode_state};

pub(super) struct JobRecordCodec;
pub(super) struct LiveJobIndexCodec;
pub(super) struct RequestIdempotencyIndexCodec;
pub(super) struct DiscussionAttemptIndexCodec;
pub(super) struct LatestAttemptIndexCodec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RequestIndexKey {
    Lower,
    Value(ResolutionRequestIdentity),
    Upper,
}

impl RequestIndexKey {
    pub(super) const fn new(request: ResolutionRequestIdentity) -> Self {
        Self::Value(request)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DiscussionAttemptKey {
    discussion_thread_id: SyndicThreadId,
    attempt_ordinal: ResolutionAttemptOrdinal,
}

impl DiscussionAttemptKey {
    pub(super) const fn new(
        discussion_thread_id: SyndicThreadId,
        attempt_ordinal: ResolutionAttemptOrdinal,
    ) -> Self {
        Self {
            discussion_thread_id,
            attempt_ordinal,
        }
    }

    pub(super) const fn discussion_thread_id(self) -> SyndicThreadId {
        self.discussion_thread_id
    }

    pub(super) const fn attempt_ordinal(self) -> ResolutionAttemptOrdinal {
        self.attempt_ordinal
    }
}

impl RecordCodec<DurableJobDomain> for JobRecordCodec {
    type Key = JobId;
    type Value = BranchHandoffJobRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = BRANCH_HANDOFF_JOB_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_job_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_job_record(value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_job_record(encoded)
    }
}

impl RecordCodec<DurableJobDomain> for LiveJobIndexCodec {
    type Key = JobId;
    type Value = BranchHandoffJobRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "live-jobs";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = BRANCH_HANDOFF_JOB_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_job_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_job_record(value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_job_record(encoded)
    }
}

impl RecordCodec<DurableJobDomain> for RequestIdempotencyIndexCodec {
    type Key = RequestIndexKey;
    type Value = ResolutionRequestAdmission;
    type Error = CodecError;

    const FAMILY: &'static str = "request-idempotency";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1024;
    const MAX_VALUE_BYTES: usize = 64;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        match key {
            RequestIndexKey::Lower => encoder.u8(0),
            RequestIndexKey::Value(request) => {
                encoder.u8(1);
                encode_request(&mut encoder, request);
            }
            RequestIndexKey::Upper => encoder.u8(u8::MAX),
        }
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = match decoder.u8()? {
            0 => RequestIndexKey::Lower,
            1 => RequestIndexKey::new(decode_request(&mut decoder)?),
            u8::MAX => RequestIndexKey::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "request-idempotency key",
                    tag,
                });
            }
        };
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.job_id.as_bytes());
        encoder.fixed(value.intent_id.as_bytes());
        encoder.fixed(value.discussion_thread_id.as_bytes());
        encoder.u64(value.attempt_ordinal.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let value = ResolutionRequestAdmission {
            job_id: JobId::from_bytes(decoder.fixed()?),
            intent_id: ResolutionIntentId::from_bytes(decoder.fixed()?),
            discussion_thread_id: SyndicThreadId::from_bytes(decoder.fixed()?),
            attempt_ordinal: decode_attempt_ordinal(&mut decoder)?,
        };
        decoder.finish()?;
        if value.job_id != branch_handoff_job_id(value.intent_id) {
            return Err(invalid(
                "request admission job does not derive from its intent",
            ));
        }
        Ok(value)
    }
}

impl RecordCodec<DurableJobDomain> for DiscussionAttemptIndexCodec {
    type Key = DiscussionAttemptKey;
    type Value = JobId;
    type Error = CodecError;

    const FAMILY: &'static str = "discussion-attempts";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = 16;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(key.discussion_thread_id.as_bytes());
        encoder.u64(key.attempt_ordinal.get());
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = DiscussionAttemptKey::new(
            SyndicThreadId::from_bytes(decoder.fixed()?),
            decode_attempt_ordinal(&mut decoder)?,
        );
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.as_bytes().to_vec())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_job_id(encoded)
    }
}

impl RecordCodec<DurableJobDomain> for LatestAttemptIndexCodec {
    type Key = SyndicThreadId;
    type Value = LatestBranchHandoffAttempt;
    type Error = CodecError;

    const FAMILY: &'static str = "latest-attempts";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 24;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_thread_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.job_id.as_bytes());
        encoder.u64(value.attempt_ordinal.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let value = LatestBranchHandoffAttempt {
            job_id: JobId::from_bytes(decoder.fixed()?),
            attempt_ordinal: decode_attempt_ordinal(&mut decoder)?,
        };
        decoder.finish()?;
        Ok(value)
    }
}

fn encode_job_record(value: &BranchHandoffJobRecord) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.fixed(value.job_id.as_bytes());
    encoder.fixed(value.intent_id.as_bytes());
    encoder.u64(value.attempt_ordinal.get());
    encoder.fixed(value.discussion_thread_id.as_bytes());
    encoder.fixed(value.parent_thread_id.as_bytes());
    encode_context_owner(&mut encoder, value.context_owner_id);
    encoder.fixed_32(value.context_digest.as_bytes());
    encoder.fixed(value.resolving_turn_id.as_bytes());
    encode_request(&mut encoder, &value.request);
    encoder.u64(value.parent_queue_ordinal.get());
    encoder.text(value.resolution.as_str());
    encode_state(&mut encoder, &value.state);
    encoder.u64(value.revision.get());
    encoder.finish()
}

fn decode_job_record(encoded: &[u8]) -> Result<BranchHandoffJobRecord, CodecError> {
    let mut decoder = Decoder::new(encoded);
    let job_id = JobId::from_bytes(decoder.fixed()?);
    let intent_id = ResolutionIntentId::from_bytes(decoder.fixed()?);
    let attempt_ordinal = decode_attempt_ordinal(&mut decoder)?;
    let discussion_thread_id = SyndicThreadId::from_bytes(decoder.fixed()?);
    let parent_thread_id = SyndicThreadId::from_bytes(decoder.fixed()?);
    let context_owner_id = decode_context_owner(&mut decoder)?;
    let context_digest = DiscussionContextDigest::from_bytes(decoder.fixed_32()?);
    let resolving_turn_id = SyndicTurnId::from_bytes(decoder.fixed()?);
    let request = decode_request(&mut decoder)?;
    let parent_queue_ordinal = ParentQueueOrdinal::new(decoder.u64()?);
    let resolution = ResolutionText::new(decoder.text("branch resolution text")?)
        .map_err(|source| invalid_value("branch resolution text", source))?;
    let state = decode_state(&mut decoder)?;
    let revision =
        JobRevision::new(decoder.u64()?).map_err(|source| invalid_value("job revision", source))?;
    decoder.finish()?;
    if job_id != branch_handoff_job_id(intent_id) {
        return Err(invalid(
            "job identity does not derive from its resolution intent",
        ));
    }
    Ok(BranchHandoffJobRecord {
        job_id,
        intent_id,
        attempt_ordinal,
        discussion_thread_id,
        parent_thread_id,
        context_owner_id,
        context_digest,
        resolving_turn_id,
        request,
        parent_queue_ordinal,
        resolution,
        state,
        revision,
    })
}

fn encode_context_owner(encoder: &mut Encoder, owner: DiscussionContextOwnerId) {
    match owner {
        DiscussionContextOwnerId::Draft(id) => {
            encoder.u8(0);
            encoder.fixed(id.as_bytes());
        }
        DiscussionContextOwnerId::SubmittedTurn(id) => {
            encoder.u8(1);
            encoder.fixed(id.as_bytes());
        }
    }
}

fn decode_context_owner(decoder: &mut Decoder<'_>) -> Result<DiscussionContextOwnerId, CodecError> {
    match decoder.u8()? {
        0 => Ok(DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes(
            decoder.fixed()?,
        ))),
        1 => Ok(DiscussionContextOwnerId::SubmittedTurn(
            SyndicTurnId::from_bytes(decoder.fixed()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "discussion context owner",
            tag,
        }),
    }
}

fn encode_request(encoder: &mut Encoder, request: &ResolutionRequestIdentity) {
    encoder.text(request.cas_thread_id().as_str());
    encoder.text(request.cas_turn_id().as_str());
    encoder.text(request.tool_call_id().as_str());
}

fn decode_request(decoder: &mut Decoder<'_>) -> Result<ResolutionRequestIdentity, CodecError> {
    let cas_thread_id = CasThreadId::new(decoder.text("resolving CAS thread identity")?)
        .map_err(|source| invalid_value("resolving CAS thread identity", source))?;
    let cas_turn_id = CasTurnId::new(decoder.text("resolving CAS turn identity")?)
        .map_err(|source| invalid_value("resolving CAS turn identity", source))?;
    let tool_call_id = DynamicToolCallId::new(decoder.text("resolution tool call identity")?)
        .map_err(|source| invalid_value("resolution tool call identity", source))?;
    Ok(ResolutionRequestIdentity::new(
        cas_thread_id,
        cas_turn_id,
        tool_call_id,
    ))
}

fn decode_attempt_ordinal(
    decoder: &mut Decoder<'_>,
) -> Result<ResolutionAttemptOrdinal, CodecError> {
    ResolutionAttemptOrdinal::new(decoder.u64()?)
        .map_err(|source| invalid_value("resolution attempt ordinal", source))
}

fn decode_job_id(encoded: &[u8]) -> Result<JobId, CodecError> {
    decode_identity(encoded, "job identity").map(JobId::from_bytes)
}

fn decode_thread_id(encoded: &[u8]) -> Result<SyndicThreadId, CodecError> {
    decode_identity(encoded, "Syndic thread identity").map(SyndicThreadId::from_bytes)
}

fn decode_identity(encoded: &[u8], kind: &'static str) -> Result<[u8; 16], CodecError> {
    encoded
        .try_into()
        .map_err(|_| CodecError::InvalidLength { kind })
}

fn invalid(message: &'static str) -> CodecError {
    invalid_value(
        "branch handoff job record",
        std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    )
}

fn invalid_value(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}
