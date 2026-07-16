use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::RecoveryItemSequenceDigest;
use sha2::{Digest, Sha256};

use crate::{
    ContentReference, RecoveryItem, RecoveryItemRole, RecoveryProjectionError,
    SyndicPointReadLimit, SyndicStorage,
    codec::{ContentChunkKey, ContentChunksFamily},
};

use super::{INDEX_PAGE_MAX_BYTES, INDEX_PAGE_MAX_ITEMS, ItemFrontier};

const CHUNK_POINT_READ_MAX_BYTES: usize = crate::CONTENT_CHUNK_MAX_BYTES + 512;
const RECOVERY_DIGEST_DOMAIN: &[u8] = b"beryl.syndic.recovery-item-sequence.v1\0";

impl SyndicStorage {
    pub(super) fn materialize_items(
        &self,
        store: &HomeStore,
        frontier: &[ItemFrontier],
    ) -> Result<Vec<RecoveryItem>, RecoveryProjectionError> {
        let mut items = Vec::with_capacity(frontier.len());
        for item in frontier {
            let bytes = self.read_recovery_text(store, item.content)?;
            let text = String::from_utf8(bytes).map_err(|_| {
                RecoveryProjectionError::Invariant(
                    "canonical recovery text spans do not compose valid UTF-8",
                )
            })?;
            if text.is_empty() {
                return Err(RecoveryProjectionError::EmptyHistoryItem);
            }
            let text = text.into_boxed_str();
            items.push(match item.role {
                RecoveryItemRole::User => RecoveryItem::user(text),
                RecoveryItemRole::Assistant => RecoveryItem::assistant(text),
            });
        }
        Ok(items)
    }

    fn read_recovery_text(
        &self,
        store: &HomeStore,
        content: ContentReference,
    ) -> Result<Vec<u8>, RecoveryProjectionError> {
        let expected = content.summary().logical_utf8_bytes();
        let capacity = usize::try_from(expected).map_err(|_| {
            RecoveryProjectionError::Invariant("recovery text allocation length overflowed")
        })?;
        let mut bytes = Vec::with_capacity(capacity);
        let mut logical = 0_u64;
        let mut after = None;
        let mut cached_chunk = None;
        let limits = || {
            CursorReadLimits::new(INDEX_PAGE_MAX_ITEMS, INDEX_PAGE_MAX_BYTES)
                .expect("recovery text-span page bounds are nonzero")
        };

        while logical < expected {
            let page = self.content_text_spans(store, content.id(), after, limits())?;
            if page.records().is_empty() {
                return Err(RecoveryProjectionError::MissingHistory {
                    record: "content-text-span",
                });
            }
            for span in page.records() {
                if logical >= expected {
                    return Err(RecoveryProjectionError::Invariant(
                        "content text spans continue past the canonical logical frontier",
                    ));
                }
                if span.content_id() != content.id()
                    || span.logical_start() != logical
                    || span.logical_end() > expected
                {
                    return Err(RecoveryProjectionError::Invariant(
                        "content text spans do not exactly cover canonical logical bytes",
                    ));
                }
                if span.break_before() {
                    return Err(RecoveryProjectionError::MediaHistory {
                        reason: "canonical text is separated by an image marker",
                    });
                }
                if cached_chunk
                    .as_ref()
                    .is_none_or(|chunk: &crate::ContentChunkRecord| {
                        chunk.ordinal() != span.chunk_ordinal()
                    })
                {
                    let chunk = self
                        .point::<ContentChunksFamily>(
                            store,
                            ContentChunkKey {
                                owner: content.id(),
                                ordinal: span.chunk_ordinal(),
                            },
                            chunk_point_limit(),
                        )?
                        .ok_or(RecoveryProjectionError::MissingHistory {
                            record: "content-chunk",
                        })?;
                    cached_chunk = Some(chunk.record().clone());
                }
                let chunk = cached_chunk
                    .as_ref()
                    .expect("recovery text loads the span's exact chunk");
                if chunk.content_id() != content.id()
                    || chunk.ordinal() != span.chunk_ordinal()
                    || <[u8; 32]>::from(Sha256::digest(chunk.bytes())) != *chunk.digest()
                {
                    return Err(RecoveryProjectionError::Invariant(
                        "content chunk identity or digest disagrees with recovery authority",
                    ));
                }
                let local_start = span
                    .encoded_start()
                    .checked_sub(span.chunk_start())
                    .and_then(|offset| usize::try_from(offset).ok())
                    .ok_or(RecoveryProjectionError::Invariant(
                        "content text-span start does not lie in its chunk",
                    ))?;
                let local_end = span
                    .encoded_end()
                    .checked_sub(span.chunk_start())
                    .and_then(|offset| usize::try_from(offset).ok())
                    .ok_or(RecoveryProjectionError::Invariant(
                        "content text-span end does not lie in its chunk",
                    ))?;
                let selected = chunk.bytes().get(local_start..local_end).ok_or(
                    RecoveryProjectionError::Invariant(
                        "content text-span range lies outside its physical chunk",
                    ),
                )?;
                if <[u8; 32]>::from(Sha256::digest(selected)) != span.digest() {
                    return Err(RecoveryProjectionError::Invariant(
                        "content text-span digest disagrees with its physical bytes",
                    ));
                }
                bytes.extend_from_slice(selected);
                logical = span.logical_end();
                after = Some(span.logical_start());
            }
            if logical < expected && !page.has_more() {
                return Err(RecoveryProjectionError::MissingHistory {
                    record: "content-text-span",
                });
            }
            if logical == expected && page.has_more() {
                return Err(RecoveryProjectionError::Invariant(
                    "content text spans continue past the canonical logical frontier",
                ));
            }
        }
        if bytes.len() != capacity {
            return Err(RecoveryProjectionError::Invariant(
                "content text spans returned the wrong logical byte count",
            ));
        }
        Ok(bytes)
    }
}

fn chunk_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(CHUNK_POINT_READ_MAX_BYTES)
        .expect("recovery chunk point-read bound is nonzero")
}

pub(super) fn recovery_sequence_digest(
    items: &[RecoveryItem],
    utf8_bytes: u64,
) -> RecoveryItemSequenceDigest {
    let mut hash = Sha256::new();
    hash.update(RECOVERY_DIGEST_DOMAIN);
    hash.update(
        u64::try_from(items.len())
            .expect("bounded recovery item count fits u64")
            .to_be_bytes(),
    );
    hash.update(utf8_bytes.to_be_bytes());
    for (index, item) in items.iter().enumerate() {
        let role = match item {
            RecoveryItem::UserInputText(_) => 0_u8,
            RecoveryItem::AssistantOutputText(_) => 1_u8,
        };
        let text = item.text().as_bytes();
        hash.update(
            u64::try_from(index)
                .expect("bounded recovery item ordinal fits u64")
                .checked_add(1)
                .expect("bounded recovery item ordinal is not u64::MAX")
                .to_be_bytes(),
        );
        hash.update([role]);
        hash.update(
            u64::try_from(text.len())
                .expect("bounded recovery item text length fits u64")
                .to_be_bytes(),
        );
        hash.update(text);
    }
    RecoveryItemSequenceDigest::from_bytes(hash.finalize().into())
}
