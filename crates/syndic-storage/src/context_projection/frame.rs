use crate::{ContextEnvelopeRecord, RecoveryProjectionError, SyndicStorage};
use beryl_home_store::HomeStore;
use beryl_model::{DiscussionContextOwnerId, SyndicThreadId};
use sha2::{Digest, Sha256};

pub(crate) const FRAME_OVERHEAD: u64 = 461;
const TRAILER: &[u8] = b"\n[/Beryl discussion context v1]";

pub(crate) fn update_sequence_text(
    record: &ContextEnvelopeRecord,
    accumulator: &mut beryl_model::RecoveryItemSequenceAccumulator,
) -> Result<(), RecoveryProjectionError> {
    let header = header(record);
    for bytes in [
        header.as_bytes(),
        record.envelope().text().as_str().as_bytes(),
        TRAILER,
    ] {
        accumulator
            .update_text(bytes)
            .map_err(|_| RecoveryProjectionError::Invariant("context frame sequence text"))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextFrameSource {
    pub(crate) thread: SyndicThreadId,
    pub(crate) owner: DiscussionContextOwnerId,
    pub(crate) entry: [u8; 32],
    pub(crate) utf8_bytes: u64,
    pub(crate) sequence_digest: beryl_model::RecoveryItemSequenceDigest,
}

fn header(record: &ContextEnvelopeRecord) -> String {
    let descriptor = record.envelope().descriptor();
    let source = descriptor.source();
    let hex = |bytes: &[u8]| -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() };
    format!(
        "[Beryl discussion context v1]\nsource_thread={}\nsource_turn={}\nsource_item={}\nsource_projection={}\nprojection_revision={:016x}\nrange_start={:016x}\nrange_end={:016x}\nselected_utf8_bytes={:016x}\nselected_sha256={}\n\n",
        hex(source.thread_id().as_bytes()),
        hex(source.turn_id().as_bytes()),
        hex(source.item_id().as_bytes()),
        hex(source.projection_id().as_bytes()),
        source.projection_revision().get(),
        source.range().start(),
        source.range().end(),
        record.envelope().text().len(),
        hex(descriptor.digest().as_bytes())
    )
}

pub(crate) fn describe(
    store: &HomeStore,
    thread: SyndicThreadId,
    record: &ContextEnvelopeRecord,
) -> Result<ContextFrameSource, RecoveryProjectionError> {
    describe_for_home(store.home_id(), thread, record)
}

pub(crate) fn describe_for_home(
    home: beryl_model::BerylHomeId,
    thread: SyndicThreadId,
    record: &ContextEnvelopeRecord,
) -> Result<ContextFrameSource, RecoveryProjectionError> {
    let DiscussionContextOwnerId::SubmittedTurn(owner) = record.owner() else {
        return Err(RecoveryProjectionError::UnsupportedHistory {
            reason: "draft context is not execution authority",
        });
    };
    let envelope = record.envelope();
    let descriptor = envelope.descriptor();
    let source = descriptor.source();
    let header = header(record);
    if header.len() != 430 {
        return Err(RecoveryProjectionError::Invariant(
            "context header length disagrees",
        ));
    }
    let utf8_bytes = envelope.text().len() as u64 + FRAME_OVERHEAD;
    let mut sequence = beryl_model::RecoveryItemSequenceAccumulator::new(1, utf8_bytes);
    sequence
        .begin_item(
            1,
            beryl_model::RecoveryItemSequenceRole::AssistantOutputText,
            utf8_bytes,
        )
        .map_err(|_| RecoveryProjectionError::Invariant("context frame sequence header"))?;
    update_sequence_text(record, &mut sequence)?;
    sequence
        .finish_item()
        .map_err(|_| RecoveryProjectionError::Invariant("context frame sequence end"))?;
    let sequence_digest = sequence
        .finish()
        .map_err(|_| RecoveryProjectionError::Invariant("context frame sequence EOF"))?;
    let mut frame_hash = Sha256::new();
    frame_hash.update(b"syndic/context-frame/v1\0");
    frame_hash.update(header.as_bytes());
    frame_hash.update(envelope.text().as_str().as_bytes());
    frame_hash.update(TRAILER);
    let frame_digest: [u8; 32] = frame_hash.finalize().into();
    let mut hash = Sha256::new();
    hash.update(b"syndic/context-entry/v1\0");
    hash.update(home.as_bytes());
    hash.update(thread.as_bytes());
    hash.update(owner.as_bytes());
    hash.update(record.revision().get().to_be_bytes());
    hash.update([1]);
    hash.update(source.thread_id().as_bytes());
    hash.update(source.turn_id().as_bytes());
    hash.update(source.item_id().as_bytes());
    hash.update(source.projection_id().as_bytes());
    hash.update(source.projection_revision().get().to_be_bytes());
    hash.update(source.range().start().to_be_bytes());
    hash.update(source.range().end().to_be_bytes());
    hash.update(descriptor.digest().as_bytes());
    hash.update(descriptor.created_at().unix_millis().to_be_bytes());
    hash.update(utf8_bytes.to_be_bytes());
    hash.update(frame_digest);
    Ok(ContextFrameSource {
        thread,
        owner: record.owner(),
        entry: hash.finalize().into(),
        utf8_bytes,
        sequence_digest,
    })
}

pub(crate) fn read_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: ContextFrameSource,
    offset: u64,
    maximum: usize,
    output: &mut [u8],
) -> Result<(usize, crate::read::ReadByteTotals), RecoveryProjectionError> {
    if maximum == 0 {
        return Err(RecoveryProjectionError::InvalidCursorPageLimit { actual: 0 });
    }
    let (record, totals) = storage.context_record_for_execution_counted(store, source.thread)?;
    if describe(store, source.thread, &record)? != source {
        return Err(RecoveryProjectionError::CursorMismatch {
            reason: "context frame source changed",
        });
    }
    let header = header(&record);
    let selected = record.envelope().text().as_str().as_bytes();
    let parts = [header.as_bytes(), selected, TRAILER];
    let mut skip = usize::try_from(offset)
        .map_err(|_| RecoveryProjectionError::Invariant("context offset overflow"))?;
    let limit = maximum.min(output.len()).min(65_536);
    let mut len = 0;
    for part in parts {
        if skip >= part.len() {
            skip -= part.len();
            continue;
        }
        let take = (part.len() - skip).min(limit - len);
        output[len..len + take].copy_from_slice(&part[skip..skip + take]);
        len += take;
        skip = 0;
        if len == limit {
            break;
        }
    }
    while len > 0 && std::str::from_utf8(&output[..len]).is_err() {
        len -= 1;
    }
    if len == 0 {
        return Err(RecoveryProjectionError::CursorPageLimitTooSmall {
            offset,
            actual: maximum,
        });
    }
    Ok((len, totals))
}
