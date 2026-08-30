use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{SyndicPathDigest, SyndicThreadId, ThreadRevision};

use crate::{
    DraftImageLabelProtectionHeadV1, ImageLabelAuthorityHeadV1, ImageLabelOrdinal,
    ImageLabelOriginSpanRecord, SyndicReadError, ThreadLineageDepth, codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

mod activity;

pub use activity::*;

pub const QUERY_PAGE_MAX_RECORDS: usize = 256;
pub const QUERY_PAGE_MAX_STORED_BYTES: usize = crate::TRANSCRIPT_PAGE_MAX_BYTES;

/// Revision-bound immutable facts selecting one thread-lineage query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadLineageHead {
    leaf_thread_id: SyndicThreadId,
    thread_revision: ThreadRevision,
    lineage_depth: ThreadLineageDepth,
    lineage_digest: SyndicPathDigest,
    total_parent_count: u64,
}

impl ThreadLineageHead {
    #[must_use]
    pub const fn leaf_thread_id(&self) -> SyndicThreadId {
        self.leaf_thread_id
    }
    #[must_use]
    pub const fn thread_revision(&self) -> ThreadRevision {
        self.thread_revision
    }
    #[must_use]
    pub const fn lineage_depth(&self) -> ThreadLineageDepth {
        self.lineage_depth
    }
    #[must_use]
    pub const fn lineage_digest(&self) -> SyndicPathDigest {
        self.lineage_digest
    }
    #[must_use]
    pub const fn total_parent_count(&self) -> u64 {
        self.total_parent_count
    }

    #[must_use]
    pub fn cursor(&self) -> Option<ThreadLineageCursor> {
        (self.total_parent_count != 0).then_some(ThreadLineageCursor {
            leaf_thread_id: self.leaf_thread_id,
            thread_revision: self.thread_revision,
            lineage_digest: self.lineage_digest,
            next_depth: ThreadLineageDepth::FIRST,
        })
    }
}

/// Compact continuation for top-to-bottom ancestor lineage pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadLineageCursor {
    leaf_thread_id: SyndicThreadId,
    thread_revision: ThreadRevision,
    lineage_digest: SyndicPathDigest,
    next_depth: ThreadLineageDepth,
}

/// Lightweight immutable ancestor identity returned by a lineage page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadLineageEntry {
    thread_id: SyndicThreadId,
    depth: ThreadLineageDepth,
    lineage_digest: SyndicPathDigest,
}

impl ThreadLineageEntry {
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn depth(self) -> ThreadLineageDepth {
        self.depth
    }
    #[must_use]
    pub const fn lineage_digest(self) -> SyndicPathDigest {
        self.lineage_digest
    }
}

/// One fixed-cap lineage page and its optional exact continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadLineagePage {
    records: Vec<ThreadLineageEntry>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<ThreadLineageCursor>,
}

impl ThreadLineagePage {
    #[must_use]
    pub fn records(&self) -> &[ThreadLineageEntry] {
        &self.records
    }
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<ThreadLineageCursor> {
        self.next_cursor
    }
}

/// One bounded inherited image-label lookup result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicResolvedImageLabelOriginSpan {
    span: ImageLabelOriginSpanRecord,
    origin_depth: ThreadLineageDepth,
}

impl SyndicResolvedImageLabelOriginSpan {
    #[must_use]
    pub const fn span(&self) -> &ImageLabelOriginSpanRecord {
        &self.span
    }
    #[must_use]
    pub const fn origin_depth(&self) -> ThreadLineageDepth {
        self.origin_depth
    }
}

impl SyndicStorage {
    pub fn image_label_authority_head(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ImageLabelAuthorityHeadV1>, SyndicReadError> {
        let head = self.point::<ImageLabelAuthorityHeadsFamily>(store, thread_id, limit)?;
        if head.as_ref().is_some_and(|head| !head.is_exact()) {
            return Err(SyndicReadError::Invariant(
                "image-label authority head is invalid",
            ));
        }
        Ok(head)
    }

    pub fn draft_image_label_protection_head(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<DraftImageLabelProtectionHeadV1>, SyndicReadError> {
        let head = self.point::<DraftImageLabelProtectionHeadsFamily>(store, thread_id, limit)?;
        if head.as_ref().is_some_and(|head| !head.is_exact()) {
            return Err(SyndicReadError::Invariant(
                "draft image-label protection head is invalid",
            ));
        }
        Ok(head)
    }

    pub fn thread_lineage_head(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadLineageHead>, SyndicReadError> {
        let Some(thread) = self.point::<ThreadsFamily>(store, thread_id, limit)? else {
            return Ok(None);
        };
        Ok(Some(ThreadLineageHead {
            leaf_thread_id: thread.id(),
            thread_revision: thread.revision(),
            lineage_depth: thread.lineage_depth(),
            lineage_digest: thread.lineage_digest(),
            total_parent_count: thread.lineage_depth().get() - 1,
        }))
    }

    pub fn thread_lineage_page(
        &self,
        store: &HomeStore,
        head: &ThreadLineageHead,
        cursor: ThreadLineageCursor,
        limits: CursorReadLimits,
    ) -> Result<ThreadLineagePage, SyndicReadError> {
        if cursor.leaf_thread_id != head.leaf_thread_id
            || cursor.thread_revision != head.thread_revision
            || cursor.lineage_digest != head.lineage_digest
            || cursor.next_depth.get() > head.total_parent_count
        {
            return Err(SyndicReadError::InvalidThreadLineageCursor);
        }
        let point_limit = query_point_limit();
        let leaf = self
            .point::<ThreadsFamily>(store, head.leaf_thread_id, point_limit)?
            .ok_or(SyndicReadError::StaleThreadLineage)?;
        if leaf.revision() != head.thread_revision
            || leaf.lineage_depth() != head.lineage_depth
            || leaf.lineage_digest() != head.lineage_digest
        {
            return Err(SyndicReadError::StaleThreadLineage);
        }
        let max_items = limits.max_items().min(QUERY_PAGE_MAX_RECORDS);
        let max_bytes = limits.max_bytes().min(QUERY_PAGE_MAX_STORED_BYTES);
        let mut records = Vec::with_capacity(max_items);
        let mut stored_bytes = 0_usize;
        let mut decoded_bytes = 0_usize;
        let mut next_depth = Some(cursor.next_depth.get());
        while records.len() < max_items {
            let Some(depth) = next_depth else { break };
            let target = ThreadLineageDepth::new(depth)
                .map_err(|_| SyndicReadError::Invariant("thread-lineage depth is invalid"))?;
            let ancestor = crate::thread_lineage::lift_to_depth(
                leaf.clone(),
                target,
                |id| {
                    self.point::<ThreadsFamily>(store, id, point_limit)
                        .and_then(|record| {
                            record.ok_or(SyndicReadError::Invariant(
                                "thread-lineage ancestor is missing",
                            ))
                        })
                },
                SyndicReadError::Invariant,
            )?;
            let encoded_key = ThreadsFamily::encode_key(&ancestor.id())
                .map_err(|_| SyndicReadError::Invariant("valid thread key failed to encode"))?;
            let encoded_value = ThreadsFamily::encode_value(&ancestor)
                .map_err(|_| SyndicReadError::Invariant("valid thread failed to encode"))?;
            let encoded_bytes = encoded_key
                .len()
                .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
                .and_then(|bytes| bytes.checked_add(encoded_value.len()))
                .ok_or(SyndicReadError::Invariant(
                    "thread-lineage byte count overflowed",
                ))?;
            let decoded_record_bytes = encoded_key.len().checked_add(encoded_value.len()).ok_or(
                SyndicReadError::Invariant("thread-lineage decoded byte count overflowed"),
            )?;
            if stored_bytes
                .checked_add(encoded_bytes)
                .is_none_or(|total| total > max_bytes)
                || decoded_bytes
                    .checked_add(decoded_record_bytes)
                    .is_none_or(|total| total > max_bytes)
            {
                if records.is_empty() {
                    return Err(SyndicReadError::Invariant(
                        "thread-lineage byte limit cannot fit one record",
                    ));
                }
                break;
            }
            stored_bytes += encoded_bytes;
            decoded_bytes += decoded_record_bytes;
            records.push(ThreadLineageEntry {
                thread_id: ancestor.id(),
                depth: target,
                lineage_digest: ancestor.lineage_digest(),
            });
            next_depth = depth
                .checked_add(1)
                .filter(|depth| *depth <= head.total_parent_count);
        }
        let next_cursor = next_depth.map(|depth| ThreadLineageCursor {
            next_depth: ThreadLineageDepth::new(depth).expect("remaining depth is nonzero"),
            ..cursor
        });
        Ok(ThreadLineagePage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    pub fn resolve_image_label_origin_span(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        label: ImageLabelOrdinal,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicResolvedImageLabelOriginSpan>, SyndicReadError> {
        let Some(leaf) = self.point::<ThreadsFamily>(store, thread_id, limit)? else {
            return Ok(None);
        };
        let leaf_head = required_image_label_head(self, store, leaf.id(), limit)?;
        if !leaf_head.permanent().contains(label) {
            return Ok(None);
        }
        let origin_depth = if !leaf_head.inherited().contains(label) {
            leaf.lineage_depth()
        } else {
            let mut last_without_inherited_label = 1_u64;
            let mut first_with_inherited_label = leaf.lineage_depth().get();
            while first_with_inherited_label - last_without_inherited_label > 1 {
                let middle = last_without_inherited_label
                    + (first_with_inherited_label - last_without_inherited_label) / 2;
                let target = ThreadLineageDepth::new(middle).map_err(|_| {
                    SyndicReadError::Invariant("image-label lineage depth is invalid")
                })?;
                let ancestor = crate::thread_lineage::lift_to_depth(
                    leaf.clone(),
                    target,
                    |id| {
                        self.point::<ThreadsFamily>(store, id, limit)?.ok_or(
                            SyndicReadError::Invariant("image-label ancestor is missing"),
                        )
                    },
                    SyndicReadError::Invariant,
                )?;
                let ancestor_head = required_image_label_head(self, store, ancestor.id(), limit)?;
                if ancestor_head.inherited().contains(label) {
                    first_with_inherited_label = middle;
                } else {
                    last_without_inherited_label = middle;
                }
            }
            ThreadLineageDepth::new(last_without_inherited_label)
                .map_err(|_| SyndicReadError::Invariant("image-label origin depth is invalid"))?
        };
        let origin_thread = crate::thread_lineage::lift_to_depth(
            leaf.clone(),
            origin_depth,
            |id| {
                self.point::<ThreadsFamily>(store, id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "image-label origin ancestor is missing",
                    ))
            },
            SyndicReadError::Invariant,
        )?;
        let origin_head = required_image_label_head(self, store, origin_thread.id(), limit)?;
        if !origin_head.permanent().contains(label) || origin_head.inherited().contains(label) {
            return Err(SyndicReadError::Invariant(
                "image-label origin head disagrees with lineage",
            ));
        }
        let page = store.read_cursor::<crate::domain::SyndicDomain, ImageLabelOriginSpansCodec>(
            &self.handle,
            &CursorRange::closed(
                ImageLabelOriginSpanKey {
                    thread: origin_thread.id(),
                    end_label: label,
                },
                ImageLabelOriginSpanKey {
                    thread: origin_thread.id(),
                    end_label: ImageLabelOrdinal::new(u64::MAX)
                        .expect("maximum image label is nonzero"),
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(1, limit.max_bytes()).expect("point bound is nonzero"),
        )?;
        let Some((_, span)) = page
            .into_records()
            .into_iter()
            .next()
            .map(|row| row.into_parts())
        else {
            return Ok(None);
        };
        if !span.contains(label) {
            return Ok(None);
        }
        Ok(Some(SyndicResolvedImageLabelOriginSpan {
            span,
            origin_depth,
        }))
    }
}

fn required_image_label_head(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<ImageLabelAuthorityHeadV1, SyndicReadError> {
    let head = storage
        .point::<ImageLabelAuthorityHeadsFamily>(store, thread_id, limit)?
        .ok_or(SyndicReadError::Invariant(
            "image-label authority head is missing",
        ))?;
    if !head.is_exact() || head.thread_id() != thread_id {
        return Err(SyndicReadError::Invariant(
            "image-label authority head is corrupt",
        ));
    }
    Ok(head)
}

fn query_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(crate::codec::SMALL_MAX + 128).expect("query point bound is nonzero")
}
