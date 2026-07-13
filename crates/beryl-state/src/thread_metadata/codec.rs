use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{ExecutionBinding, RootId, RuntimeId, SyndicThreadId};

use crate::{
    GeneratedTitle, ThreadActivitySummary, ThreadArchiveState, TokenUsageBreakdown,
    TokenUsageSnapshot, UnixMillis,
    encoding::{
        CodecError, Decoder, Encoder, decode_job_id, decode_thread_id, encode_job_id,
        encode_thread_id,
    },
};

use super::{THREAD_METADATA_RECORD_LIMIT, ThreadMetadataDomain, ThreadMetadataRecord};

pub(super) struct ThreadMetadataRecordCodec;

impl RecordCodec<ThreadMetadataDomain> for ThreadMetadataRecordCodec {
    type Key = SyndicThreadId;
    type Value = ThreadMetadataRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = THREAD_METADATA_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_thread_id(*key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_thread_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.thread_id.as_bytes());
        encoder.fixed(value.binding.runtime_id().as_bytes());
        encoder.fixed(value.binding.root_id().as_bytes());
        encoder.runtime_path(value.binding.root_path());
        encode_title(&mut encoder, value.generated_title.as_ref());
        encode_archive(&mut encoder, value.archive_state);
        encode_activity(&mut encoder, value.activity);
        encode_token_usage(&mut encoder, value.token_usage);
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let thread_id = SyndicThreadId::from_bytes(decoder.fixed()?);
        let runtime_id = RuntimeId::from_bytes(decoder.fixed()?);
        let root_id = RootId::from_bytes(decoder.fixed()?);
        let root_path = decoder.runtime_path()?;
        let binding = ExecutionBinding::new(runtime_id, root_id, root_path);
        let generated_title = decode_title(&mut decoder)?;
        let archive_state = decode_archive(&mut decoder)?;
        let activity = decode_activity(&mut decoder)?;
        let token_usage = decode_token_usage(&mut decoder)?;
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(ThreadMetadataRecord {
            thread_id,
            binding,
            generated_title,
            archive_state,
            activity,
            token_usage,
            revision,
        })
    }
}

fn encode_title(encoder: &mut Encoder, title: Option<&GeneratedTitle>) {
    match title {
        Some(title) => {
            encoder.u8(1);
            encoder.text(title.text());
            encoder.u64(title.source_thread_revision().get());
            encoder.u64(title.generated_at().get());
        }
        None => encoder.u8(0),
    }
}

fn decode_title(decoder: &mut Decoder<'_>) -> Result<Option<GeneratedTitle>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => GeneratedTitle::new(
            decoder.text("generated title")?,
            decoder.thread_revision()?,
            UnixMillis::new(decoder.u64()?),
        )
        .map(Some)
        .map_err(|source| invalid("generated title", source)),
        tag => Err(CodecError::InvalidTag {
            kind: "generated title option",
            tag,
        }),
    }
}

fn encode_archive(encoder: &mut Encoder, archive: ThreadArchiveState) {
    match archive {
        ThreadArchiveState::Ordinary => encoder.u8(0),
        ThreadArchiveState::BranchDiscussionOpen => encoder.u8(1),
        ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id,
            archived_at,
        } => {
            encoder.u8(2);
            encode_job_id(encoder, handoff_job_id);
            encoder.u64(archived_at.get());
        }
    }
}

fn decode_archive(decoder: &mut Decoder<'_>) -> Result<ThreadArchiveState, CodecError> {
    match decoder.u8()? {
        0 => Ok(ThreadArchiveState::Ordinary),
        1 => Ok(ThreadArchiveState::BranchDiscussionOpen),
        2 => Ok(ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id: decode_job_id(decoder)?,
            archived_at: UnixMillis::new(decoder.u64()?),
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "thread archive state",
            tag,
        }),
    }
}

fn encode_activity(encoder: &mut Encoder, activity: Option<ThreadActivitySummary>) {
    match activity {
        Some(activity) => {
            encoder.u8(1);
            encoder.u64(activity.source_thread_revision().get());
            encoder.u64(activity.last_activity_at().get());
        }
        None => encoder.u8(0),
    }
}

fn decode_activity(decoder: &mut Decoder<'_>) -> Result<Option<ThreadActivitySummary>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => Ok(Some(ThreadActivitySummary::new(
            decoder.thread_revision()?,
            UnixMillis::new(decoder.u64()?),
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "thread activity option",
            tag,
        }),
    }
}

fn encode_token_usage(encoder: &mut Encoder, usage: Option<TokenUsageSnapshot>) {
    match usage {
        Some(usage) => {
            encoder.u8(1);
            encode_breakdown(encoder, usage.last());
            encode_breakdown(encoder, usage.total());
            match usage.model_context_window() {
                Some(window) => {
                    encoder.u8(1);
                    encoder.u64(window);
                }
                None => encoder.u8(0),
            }
            encoder.u64(usage.source_thread_revision().get());
            encoder.u64(usage.observed_at().get());
        }
        None => encoder.u8(0),
    }
}

fn decode_token_usage(decoder: &mut Decoder<'_>) -> Result<Option<TokenUsageSnapshot>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => {
            let last = decode_breakdown(decoder)?;
            let total = decode_breakdown(decoder)?;
            let context_window = match decoder.u8()? {
                0 => None,
                1 => Some(decoder.u64()?),
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "model context window option",
                        tag,
                    });
                }
            };
            TokenUsageSnapshot::new(
                last,
                total,
                context_window,
                decoder.thread_revision()?,
                UnixMillis::new(decoder.u64()?),
            )
            .map(Some)
            .map_err(|source| invalid("token usage snapshot", source))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "token usage option",
            tag,
        }),
    }
}

fn encode_breakdown(encoder: &mut Encoder, value: TokenUsageBreakdown) {
    encoder.u64(value.cached_input_tokens());
    encoder.u64(value.input_tokens());
    encoder.u64(value.output_tokens());
    encoder.u64(value.reasoning_output_tokens());
    encoder.u64(value.total_tokens());
}

fn decode_breakdown(decoder: &mut Decoder<'_>) -> Result<TokenUsageBreakdown, CodecError> {
    Ok(TokenUsageBreakdown::new(
        decoder.u64()?,
        decoder.u64()?,
        decoder.u64()?,
        decoder.u64()?,
        decoder.u64()?,
    ))
}

fn invalid(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}
