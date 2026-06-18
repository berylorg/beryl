use std::{collections::HashMap, path::Path, str::FromStr};

use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    ByteRange, CanonicalItemRecord, CasProjectionBindingRecord, CasProjectionBindingSummary,
    CommitSummary, ConversationRecord, ConversationTitleCandidate,
    ConversationTitleCandidateSource, ConversationViewBranchSummary, ConversationViewSummary,
    CursorId, CursorRecord, ProjectionRecord, ProviderRevision, RecoveryMarkerRecord, ResourceId,
    ResourceMetadataRecord, ResourceRangeResponse, ResourceRecord, ResourceState, Result,
    SourceEventPayload, SourceEventRecord, StorageError, SyndicWriteBatch, SyndicWriteOperation,
    ThreadViewId, TranscriptPage, TranscriptPageAnchor, TranscriptPageDirection,
    TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
    TranscriptViewRecordSummary, TurnId, TurnRecord, keys,
};

pub const MAX_TRANSCRIPT_PAGE_LIMIT: usize = 1_024;
pub const MAX_SOURCE_EVENT_READ_LIMIT: usize = 4_096;
pub const MAX_CONVERSATION_SUMMARY_READ_LIMIT: usize = 1_024;
pub const MAX_SOURCE_EVENT_PAYLOAD_BYTES: usize = 64 * 1_024;
pub const MAX_RESOURCE_RANGE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreOpenOptions {
    pub sync_after_commit: bool,
}

impl Default for StoreOpenOptions {
    fn default() -> Self {
        Self {
            sync_after_commit: true,
        }
    }
}

pub struct SyndicStore {
    db: Database,
    sync_after_commit: bool,
    conversations: Keyspace,
    conversation_views: Keyspace,
    conversation_source_threads: Keyspace,
    turns: Keyspace,
    source_events: Keyspace,
    source_event_ids: Keyspace,
    source_event_heads: Keyspace,
    items: Keyspace,
    projections: Keyspace,
    view_records: Keyspace,
    resources: Keyspace,
    resource_bytes: Keyspace,
    revisions: Keyspace,
    cursors: Keyspace,
    recovery_markers: Keyspace,
    cas_projection_bindings: Keyspace,
    cas_projection_binding_views: Keyspace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceEventPage {
    pub turn_id: TurnId,
    pub records: Vec<SourceEventRecord>,
    pub next_sequence: Option<u64>,
    pub at_end: bool,
}

impl SyndicStore {
    pub fn open(path: impl AsRef<Path>, options: StoreOpenOptions) -> Result<Self> {
        let db = Database::builder(path).open()?;

        Ok(Self {
            conversations: open_keyspace(&db, "conversations")?,
            conversation_views: open_keyspace(&db, "conversation_views")?,
            conversation_source_threads: open_keyspace(&db, "conversation_source_threads")?,
            turns: open_keyspace(&db, "turns")?,
            source_events: open_keyspace(&db, "source_events")?,
            source_event_ids: open_keyspace(&db, "source_event_ids")?,
            source_event_heads: open_keyspace(&db, "source_event_heads")?,
            items: open_keyspace(&db, "items")?,
            projections: open_keyspace(&db, "projections")?,
            view_records: open_keyspace(&db, "view_records")?,
            resources: open_keyspace(&db, "resources")?,
            resource_bytes: open_keyspace(&db, "resource_bytes")?,
            revisions: open_keyspace(&db, "revisions")?,
            cursors: open_keyspace(&db, "cursors")?,
            recovery_markers: open_keyspace(&db, "recovery_markers")?,
            cas_projection_bindings: open_keyspace(&db, "cas_projection_bindings")?,
            cas_projection_binding_views: open_keyspace(&db, "cas_projection_binding_views")?,
            db,
            sync_after_commit: options.sync_after_commit,
        })
    }

    pub fn commit(&self, batch: SyndicWriteBatch) -> Result<CommitSummary> {
        let mut raw = self.db.batch();
        let mut summary = CommitSummary {
            operations: batch.len(),
            idempotent_source_events: 0,
        };
        let mut source_event_next_sequences = HashMap::new();
        let mut pending_source_event_ids = HashMap::new();
        let mut pending_source_event_sequences = HashMap::new();
        let mut pending_binding_view_revisions = HashMap::new();

        for operation in batch.operations {
            match operation {
                SyndicWriteOperation::PutConversation(record) => {
                    validate_conversation(&record)?;
                    put_json(
                        &mut raw,
                        &self.conversations,
                        keys::conversation_key(&record.id),
                        &record,
                    )?;
                    put_json(
                        &mut raw,
                        &self.conversation_views,
                        keys::conversation_view_key(&record.view_id),
                        &record.id,
                    )?;
                    if let Some(source) = &record.source
                        && let Some(external_thread_id) = source.external_thread_id.as_deref()
                    {
                        put_json(
                            &mut raw,
                            &self.conversation_source_threads,
                            keys::conversation_source_thread_key(
                                &source.provider,
                                source.runtime_target.as_deref(),
                                external_thread_id,
                            ),
                            &record.id,
                        )?;
                    }
                    put_revision(
                        &mut raw,
                        &self.revisions,
                        &record.view_id,
                        record.current_revision,
                    )?;
                }
                SyndicWriteOperation::PutTurn(record) => {
                    validate_turn(&record)?;
                    put_json(&mut raw, &self.turns, keys::turn_key(&record.id), &record)?;
                }
                SyndicWriteOperation::PutSourceEvent(record) => {
                    validate_source_event(&record)?;
                    if self.put_source_event(
                        &mut raw,
                        &record,
                        &mut source_event_next_sequences,
                        &mut pending_source_event_ids,
                        &mut pending_source_event_sequences,
                    )? {
                        summary.idempotent_source_events += 1;
                    }
                }
                SyndicWriteOperation::PutItem(record) => {
                    validate_item(&record)?;
                    reject_secret_like_json(&record.payload)?;
                    put_json(&mut raw, &self.items, keys::item_key(&record.id), &record)?;
                }
                SyndicWriteOperation::PutProjection(record) => {
                    validate_projection(&record)?;
                    put_json(
                        &mut raw,
                        &self.projections,
                        keys::projection_key(&record.id),
                        &record,
                    )?;
                    put_revision(&mut raw, &self.revisions, &record.view_id, record.revision)?;
                }
                SyndicWriteOperation::PutViewRecord(record) => {
                    validate_view_record(&record)?;
                    put_json(
                        &mut raw,
                        &self.view_records,
                        keys::transcript_view_key(&record.view_id, record.position, &record.id),
                        &record,
                    )?;
                }
                SyndicWriteOperation::RemoveViewRecord {
                    view_id,
                    position,
                    id,
                } => {
                    validate_view_record_key(&view_id, &id)?;
                    raw.remove(
                        &self.view_records,
                        keys::transcript_view_key(&view_id, position, &id),
                    );
                }
                SyndicWriteOperation::PutResource(record) => {
                    validate_resource(&record)?;
                    let mut metadata = record.metadata;
                    metadata.byte_len = record.bytes.len() as u64;
                    put_json(
                        &mut raw,
                        &self.resources,
                        keys::resource_key(&metadata.id),
                        &metadata,
                    )?;
                    raw.insert(
                        &self.resource_bytes,
                        keys::resource_key(&metadata.id),
                        record.bytes,
                    );
                }
                SyndicWriteOperation::PutCursor(record) => {
                    validate_cursor(&record)?;
                    put_json(
                        &mut raw,
                        &self.cursors,
                        keys::cursor_key(&record.id),
                        &record,
                    )?;
                }
                SyndicWriteOperation::PutRecoveryMarker(record) => {
                    validate_recovery_marker(&record)?;
                    put_json(
                        &mut raw,
                        &self.recovery_markers,
                        keys::recovery_marker_key(&record.id),
                        &record,
                    )?;
                }
                SyndicWriteOperation::ClearRecoveryMarker(id) => {
                    id.validate()?;
                    raw.remove(&self.recovery_markers, keys::recovery_marker_key(&id));
                }
                SyndicWriteOperation::PutCasProjectionBinding(record) => {
                    validate_cas_projection_binding(&record)?;
                    put_json(
                        &mut raw,
                        &self.cas_projection_bindings,
                        keys::cas_projection_binding_key(&record.id),
                        &record,
                    )?;
                    put_revision(
                        &mut raw,
                        &self.revisions,
                        &record.view_id,
                        record.selected_path_revision,
                    )?;
                    if self.should_index_binding_for_view(
                        &record,
                        &mut pending_binding_view_revisions,
                    )? {
                        put_json(
                            &mut raw,
                            &self.cas_projection_binding_views,
                            keys::cas_projection_binding_view_key(&record.view_id),
                            &record.id,
                        )?;
                    }
                }
            }
        }

        raw.commit()?;
        if self.sync_after_commit {
            self.db.persist(PersistMode::SyncAll)?;
        }

        Ok(summary)
    }

    pub fn conversation(&self, id: &crate::ConversationId) -> Result<Option<ConversationRecord>> {
        id.validate()?;
        get_json(&self.conversations, keys::conversation_key(id))
    }

    pub fn conversation_by_view(
        &self,
        view_id: &ThreadViewId,
    ) -> Result<Option<ConversationRecord>> {
        view_id.validate()?;
        let id: Option<crate::ConversationId> = get_json(
            &self.conversation_views,
            keys::conversation_view_key(view_id),
        )?;
        id.map(|id| self.conversation(&id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn conversation_by_external_thread(
        &self,
        provider: &str,
        runtime_target: Option<&str>,
        external_thread_id: &str,
    ) -> Result<Option<ConversationRecord>> {
        validate_source_thread_lookup(provider, external_thread_id)?;
        let id: Option<crate::ConversationId> = get_json(
            &self.conversation_source_threads,
            keys::conversation_source_thread_key(provider, runtime_target, external_thread_id),
        )?;
        id.map(|id| self.conversation(&id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn turn(&self, id: &TurnId) -> Result<Option<TurnRecord>> {
        id.validate()?;
        get_json(&self.turns, keys::turn_key(id))
    }

    pub fn item(&self, id: &crate::ItemId) -> Result<Option<CanonicalItemRecord>> {
        id.validate()?;
        get_json(&self.items, keys::item_key(id))
    }

    pub fn projection(&self, id: &crate::ProjectionRecordId) -> Result<Option<ProjectionRecord>> {
        id.validate()?;
        get_json(&self.projections, keys::projection_key(id))
    }

    pub fn resource_metadata(&self, id: &ResourceId) -> Result<Option<ResourceMetadataRecord>> {
        id.validate()?;
        get_json(&self.resources, keys::resource_key(id))
    }

    pub fn cursor(&self, id: &CursorId) -> Result<Option<CursorRecord>> {
        id.validate()?;
        get_json(&self.cursors, keys::cursor_key(id))
    }

    pub fn cas_projection_binding(
        &self,
        id: &crate::CasProjectionBindingId,
    ) -> Result<Option<CasProjectionBindingRecord>> {
        id.validate()?;
        get_json(
            &self.cas_projection_bindings,
            keys::cas_projection_binding_key(id),
        )
    }

    pub fn cas_projection_binding_by_view(
        &self,
        view_id: &ThreadViewId,
    ) -> Result<Option<CasProjectionBindingRecord>> {
        view_id.validate()?;
        let id: Option<crate::CasProjectionBindingId> = get_json(
            &self.cas_projection_binding_views,
            keys::cas_projection_binding_view_key(view_id),
        )?;
        id.map(|id| self.cas_projection_binding(&id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn conversation_view_summary(
        &self,
        view_id: &ThreadViewId,
    ) -> Result<Option<ConversationViewSummary>> {
        view_id.validate()?;
        self.conversation_by_view(view_id)?
            .map(|conversation| self.summary_for_conversation(conversation))
            .transpose()
    }

    pub fn conversation_view_summaries(
        &self,
        view_ids: &[ThreadViewId],
        limit: usize,
    ) -> Result<Vec<ConversationViewSummary>> {
        ensure_limit(limit, MAX_CONVERSATION_SUMMARY_READ_LIMIT)?;
        ensure_limit(view_ids.len(), limit)?;

        let mut summaries = Vec::with_capacity(view_ids.len());
        for view_id in view_ids {
            if let Some(summary) = self.conversation_view_summary(view_id)? {
                summaries.push(summary);
            }
        }
        Ok(summaries)
    }

    pub fn current_revision(&self, view_id: &ThreadViewId) -> Result<ProviderRevision> {
        view_id.validate()?;
        Ok(get_json(&self.revisions, keys::revision_key(view_id))?.unwrap_or_default())
    }

    pub fn next_source_event_sequence(&self, turn_id: &TurnId) -> Result<u64> {
        turn_id.validate()?;
        next_sequence_for_turn(&self.source_event_heads, turn_id)
    }

    pub fn read_source_events(
        &self,
        turn_id: &TurnId,
        start_sequence: u64,
        limit: usize,
    ) -> Result<SourceEventPage> {
        turn_id.validate()?;
        ensure_limit(limit, MAX_SOURCE_EVENT_READ_LIMIT)?;

        let start_key = keys::source_event_sequence_key(turn_id, start_sequence);
        let prefix = keys::source_event_prefix(turn_id);
        let upper = keys::prefix_range(&prefix).end;
        let mut records = Vec::new();
        let mut next_sequence = None;

        for guard in self
            .source_events
            .range(start_key..upper)
            .take(limit.saturating_add(1))
        {
            let record: SourceEventRecord = decode_guard(guard)?;
            if records.len() == limit {
                next_sequence = Some(record.sequence);
                break;
            }
            records.push(record);
        }

        Ok(SourceEventPage {
            turn_id: turn_id.clone(),
            at_end: next_sequence.is_none(),
            records,
            next_sequence,
        })
    }

    pub fn list_recovery_markers(&self, limit: usize) -> Result<Vec<RecoveryMarkerRecord>> {
        ensure_limit(limit, MAX_SOURCE_EVENT_READ_LIMIT)?;
        self.recovery_markers
            .iter()
            .take(limit)
            .map(decode_guard)
            .collect()
    }

    pub fn read_transcript_page(
        &self,
        view_id: &ThreadViewId,
        anchor: TranscriptPageAnchor,
        direction: TranscriptPageDirection,
        limit: usize,
        observed_revision: Option<ProviderRevision>,
    ) -> Result<TranscriptPage> {
        view_id.validate()?;
        ensure_limit(limit, MAX_TRANSCRIPT_PAGE_LIMIT)?;
        let revision = self.current_revision(view_id)?;
        ensure_revision(view_id, observed_revision, revision)?;

        let mut records = match direction {
            TranscriptPageDirection::Forward => {
                self.read_view_forward(view_id, &anchor, limit.saturating_add(1))?
            }
            TranscriptPageDirection::Backward => {
                self.read_view_backward(view_id, &anchor, limit.saturating_add(1))?
            }
        };

        let has_extra = records.len() > limit;
        if has_extra {
            records.truncate(limit);
        }

        if matches!(direction, TranscriptPageDirection::Backward) {
            records.reverse();
        }

        let first = records.first();
        let last = records.last();
        let at_start = match direction {
            TranscriptPageDirection::Forward => {
                self.anchor_starts_at_beginning(view_id, &anchor, first)?
            }
            TranscriptPageDirection::Backward => !has_extra,
        };
        let at_end = match direction {
            TranscriptPageDirection::Forward => !has_extra,
            TranscriptPageDirection::Backward => {
                self.anchor_ends_at_tail(view_id, &anchor, last)?
            }
        };

        let previous_cursor = if at_start {
            None
        } else {
            first
                .map(|record| cursor_for_record(CursorSide::Before, record))
                .transpose()?
        };
        let next_cursor = if at_end {
            None
        } else {
            last.map(|record| cursor_for_record(CursorSide::After, record))
                .transpose()?
        };

        Ok(TranscriptPage {
            view_id: view_id.clone(),
            revision,
            previous_cursor,
            next_cursor,
            records,
            at_start,
            at_end,
        })
    }

    pub fn read_projection_records(
        &self,
        view_id: &ThreadViewId,
        projection_ids: &[crate::ProjectionRecordId],
        observed_revision: Option<ProviderRevision>,
    ) -> Result<Vec<ProjectionRecord>> {
        view_id.validate()?;
        ensure_revision(view_id, observed_revision, self.current_revision(view_id)?)?;

        let mut records = Vec::with_capacity(projection_ids.len());
        for projection_id in projection_ids {
            projection_id.validate()?;
            let Some(record) = self.projection(projection_id)? else {
                return Err(StorageError::Missing {
                    kind: "projection record",
                    id: projection_id.to_string(),
                });
            };
            records.push(record);
        }
        Ok(records)
    }

    pub fn read_resource_range(
        &self,
        resource_id: &ResourceId,
        range: ByteRange,
        observed_revision: Option<ProviderRevision>,
    ) -> Result<ResourceRangeResponse> {
        resource_id.validate()?;
        let Some(metadata) = self.resource_metadata(resource_id)? else {
            return Err(StorageError::Missing {
                kind: "resource",
                id: resource_id.to_string(),
            });
        };
        ensure_revision_for_record(observed_revision, metadata.revision)?;

        if !matches!(metadata.state, ResourceState::Ready) {
            return Err(StorageError::Missing {
                kind: "ready resource",
                id: resource_id.to_string(),
            });
        }

        if range.start > range.end || range.end > metadata.byte_len {
            return Err(StorageError::ResourceRangeOutOfBounds {
                resource_id: resource_id.clone(),
                range: range.to_range(),
                byte_len: metadata.byte_len,
            });
        }
        if range.len() > MAX_RESOURCE_RANGE_BYTES {
            return Err(StorageError::ResourceRangeTooLarge {
                requested: range.len(),
                max: MAX_RESOURCE_RANGE_BYTES,
            });
        }

        let Some(bytes) = self.resource_bytes.get(keys::resource_key(resource_id))? else {
            return Err(StorageError::Missing {
                kind: "resource bytes",
                id: resource_id.to_string(),
            });
        };
        let bytes = bytes.as_ref();
        let start = range.start as usize;
        let end = range.end as usize;
        let complete = range.end == metadata.byte_len;
        Ok(ResourceRangeResponse {
            resource_id: resource_id.clone(),
            revision: metadata.revision,
            kind: metadata.kind.clone(),
            range,
            bytes: bytes[start..end].to_vec(),
            complete,
        })
    }

    fn put_source_event(
        &self,
        raw: &mut fjall::OwnedWriteBatch,
        record: &SourceEventRecord,
        next_sequences: &mut HashMap<TurnId, u64>,
        pending_ids: &mut HashMap<crate::SourceEventId, Vec<u8>>,
        pending_sequences: &mut HashMap<(TurnId, u64), Vec<u8>>,
    ) -> Result<bool> {
        let event_id_key = keys::source_event_id_key(&record.id);
        let sequence_key = keys::source_event_sequence_key(&record.turn_id, record.sequence);
        let encoded = encode(record)?;

        if let Some(existing) = pending_ids.get(&record.id) {
            if existing.as_slice() == encoded.as_slice() {
                return Ok(true);
            }
            return Err(StorageError::SourceEventConflict {
                event_id: record.id.to_string(),
            });
        }

        if let Some(existing) = self.source_event_ids.get(&event_id_key)? {
            if existing.as_ref() == encoded.as_slice() {
                return Ok(true);
            }
            return Err(StorageError::SourceEventConflict {
                event_id: record.id.to_string(),
            });
        }

        let pending_sequence_key = (record.turn_id.clone(), record.sequence);
        if let Some(existing) = pending_sequences.get(&pending_sequence_key) {
            if existing.as_slice() == encoded.as_slice() {
                return Ok(true);
            }
            let expected =
                *next_sequences
                    .entry(record.turn_id.clone())
                    .or_insert(next_sequence_for_turn(
                        &self.source_event_heads,
                        &record.turn_id,
                    )?);
            return Err(StorageError::SourceEventSequence {
                turn_id: record.turn_id.clone(),
                expected,
                received: record.sequence,
            });
        }

        if let Some(existing) = self.source_events.get(&sequence_key)? {
            if existing.as_ref() == encoded.as_slice() {
                return Ok(true);
            }
            return Err(StorageError::SourceEventSequence {
                turn_id: record.turn_id.clone(),
                expected: next_sequence_for_turn(&self.source_event_heads, &record.turn_id)?,
                received: record.sequence,
            });
        }

        let expected =
            *next_sequences
                .entry(record.turn_id.clone())
                .or_insert(next_sequence_for_turn(
                    &self.source_event_heads,
                    &record.turn_id,
                )?);
        if record.sequence != expected {
            return Err(StorageError::SourceEventSequence {
                turn_id: record.turn_id.clone(),
                expected,
                received: record.sequence,
            });
        }

        raw.insert(&self.source_events, sequence_key, encoded.clone());
        raw.insert(&self.source_event_ids, event_id_key, encoded.clone());
        put_json(
            raw,
            &self.source_event_heads,
            keys::source_event_head_key(&record.turn_id),
            &record.sequence,
        )?;
        pending_ids.insert(record.id.clone(), encoded.clone());
        pending_sequences.insert(pending_sequence_key, encoded);
        next_sequences.insert(record.turn_id.clone(), record.sequence.saturating_add(1));
        Ok(false)
    }

    fn read_view_forward(
        &self,
        view_id: &ThreadViewId,
        anchor: &TranscriptPageAnchor,
        limit: usize,
    ) -> Result<Vec<TranscriptViewRecord>> {
        let range = match anchor {
            TranscriptPageAnchor::Start => {
                keys::prefix_range(&keys::transcript_view_prefix(view_id))
            }
            TranscriptPageAnchor::End => {
                let prefix = keys::transcript_view_prefix(view_id);
                let end = keys::prefix_range(&prefix).end;
                end.clone()..end
            }
            TranscriptPageAnchor::Position(position) => {
                let start = keys::transcript_view_position_prefix(view_id, *position);
                start..keys::prefix_range(&keys::transcript_view_prefix(view_id)).end
            }
            TranscriptPageAnchor::Cursor(cursor) => {
                let token = decode_cursor(cursor, view_id)?;
                let record_id = TranscriptViewRecordId::from(token.record_id);
                let start = match token.side {
                    CursorSide::After | CursorSide::Before => keys::exclusive_after(
                        keys::transcript_view_key(view_id, token.position, &record_id),
                    ),
                };
                start..keys::prefix_range(&keys::transcript_view_prefix(view_id)).end
            }
        };

        self.view_records
            .range(range)
            .take(limit)
            .map(decode_guard)
            .collect()
    }

    fn read_view_backward(
        &self,
        view_id: &ThreadViewId,
        anchor: &TranscriptPageAnchor,
        limit: usize,
    ) -> Result<Vec<TranscriptViewRecord>> {
        let range = match anchor {
            TranscriptPageAnchor::Start => {
                let prefix = keys::transcript_view_prefix(view_id);
                prefix.clone()..prefix
            }
            TranscriptPageAnchor::End => keys::prefix_range(&keys::transcript_view_prefix(view_id)),
            TranscriptPageAnchor::Position(position) => {
                let end = keys::transcript_view_position_prefix(view_id, position.next());
                keys::transcript_view_prefix(view_id)..end
            }
            TranscriptPageAnchor::Cursor(cursor) => {
                let token = decode_cursor(cursor, view_id)?;
                let record_id = TranscriptViewRecordId::from(token.record_id);
                let end = keys::transcript_view_key(view_id, token.position, &record_id);
                keys::transcript_view_prefix(view_id)..end
            }
        };

        self.view_records
            .range(range)
            .rev()
            .take(limit)
            .map(decode_guard)
            .collect()
    }

    fn anchor_starts_at_beginning(
        &self,
        view_id: &ThreadViewId,
        anchor: &TranscriptPageAnchor,
        first: Option<&TranscriptViewRecord>,
    ) -> Result<bool> {
        match anchor {
            TranscriptPageAnchor::Start => Ok(true),
            TranscriptPageAnchor::End => Ok(first.is_none() && self.view_is_empty(view_id)?),
            TranscriptPageAnchor::Cursor(_) | TranscriptPageAnchor::Position(_) => {
                let Some(first) = first else {
                    return Ok(false);
                };
                let key = keys::transcript_view_key(view_id, first.position, &first.id);
                Ok(self
                    .view_records
                    .range(keys::transcript_view_prefix(view_id)..key)
                    .next_back()
                    .is_none())
            }
        }
    }

    fn anchor_ends_at_tail(
        &self,
        view_id: &ThreadViewId,
        anchor: &TranscriptPageAnchor,
        last: Option<&TranscriptViewRecord>,
    ) -> Result<bool> {
        match anchor {
            TranscriptPageAnchor::End => Ok(true),
            TranscriptPageAnchor::Start => Ok(last.is_none() && self.view_is_empty(view_id)?),
            TranscriptPageAnchor::Cursor(_) | TranscriptPageAnchor::Position(_) => {
                let Some(last) = last else {
                    return Ok(false);
                };
                let start = keys::exclusive_after(keys::transcript_view_key(
                    view_id,
                    last.position,
                    &last.id,
                ));
                let end = keys::prefix_range(&keys::transcript_view_prefix(view_id)).end;
                Ok(self.view_records.range(start..end).next().is_none())
            }
        }
    }

    fn view_is_empty(&self, view_id: &ThreadViewId) -> Result<bool> {
        let prefix = keys::transcript_view_prefix(view_id);
        Ok(self.view_records.prefix(prefix).next().is_none())
    }

    fn summary_for_conversation(
        &self,
        conversation: ConversationRecord,
    ) -> Result<ConversationViewSummary> {
        let latest_transcript_record = self
            .read_view_backward(&conversation.view_id, &TranscriptPageAnchor::End, 1)?
            .into_iter()
            .next()
            .map(transcript_record_summary);
        let cas_projection_binding = self
            .cas_projection_binding_by_view(&conversation.view_id)?
            .map(cas_projection_binding_summary);
        let title_candidates = conversation
            .title
            .as_deref()
            .and_then(|title| non_empty_trimmed(Some(title)))
            .map(|title| {
                vec![ConversationTitleCandidate {
                    title: title.to_string(),
                    source: ConversationTitleCandidateSource::ConversationRecord,
                }]
            })
            .unwrap_or_default();
        let branch = conversation.parent_view_id.clone().map(|parent_view_id| {
            ConversationViewBranchSummary {
                parent_view_id,
                source_turn_id: conversation.branch_source_turn_id.clone(),
            }
        });

        Ok(ConversationViewSummary {
            conversation_id: conversation.id,
            view_id: conversation.view_id,
            created_at_ms: conversation.created_at_ms,
            updated_at_ms: conversation.updated_at_ms,
            current_revision: conversation.current_revision,
            source: conversation.source,
            history_state: conversation.history_state,
            title_candidates,
            branch,
            latest_transcript_record,
            cas_projection_binding,
        })
    }

    fn should_index_binding_for_view(
        &self,
        record: &CasProjectionBindingRecord,
        pending_binding_view_revisions: &mut HashMap<ThreadViewId, u64>,
    ) -> Result<bool> {
        if let Some(pending_revision) = pending_binding_view_revisions.get(&record.view_id)
            && record.binding_revision < *pending_revision
        {
            return Ok(false);
        }

        let existing_revision =
            if let Some(pending_revision) = pending_binding_view_revisions.get(&record.view_id) {
                *pending_revision
            } else {
                self.cas_projection_binding_by_view(&record.view_id)?
                    .map_or(0, |binding| binding.binding_revision)
            };

        if record.binding_revision < existing_revision {
            return Ok(false);
        }

        pending_binding_view_revisions.insert(record.view_id.clone(), record.binding_revision);
        Ok(true)
    }
}

fn open_keyspace(db: &Database, name: &str) -> Result<Keyspace> {
    Ok(db.keyspace(name, KeyspaceCreateOptions::default)?)
}

fn encode<T: Serialize>(record: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(record)?)
}

fn put_json<T: Serialize>(
    raw: &mut fjall::OwnedWriteBatch,
    keyspace: &Keyspace,
    key: Vec<u8>,
    record: &T,
) -> Result<()> {
    raw.insert(keyspace, key, encode(record)?);
    Ok(())
}

fn get_json<T: DeserializeOwned>(keyspace: &Keyspace, key: Vec<u8>) -> Result<Option<T>> {
    keyspace
        .get(key)?
        .map(|value| serde_json::from_slice(value.as_ref()).map_err(StorageError::from))
        .transpose()
}

fn decode_guard<T: DeserializeOwned>(guard: fjall::Guard) -> Result<T> {
    let value = guard.value()?;
    Ok(serde_json::from_slice(value.as_ref())?)
}

fn transcript_record_summary(record: TranscriptViewRecord) -> TranscriptViewRecordSummary {
    TranscriptViewRecordSummary {
        id: record.id,
        position: record.position,
        narrative_kind: record.narrative_kind,
        turn_id: record.provenance.turn_id,
        item_id: record.provenance.item_id,
        projection_id: record.projection_id,
    }
}

fn cas_projection_binding_summary(
    record: CasProjectionBindingRecord,
) -> CasProjectionBindingSummary {
    CasProjectionBindingSummary {
        id: record.id,
        binding_revision: record.binding_revision,
        selected_path_revision: record.selected_path_revision,
        selected_path_digest: record.selected_path_digest,
        established_at_ms: record.established_at_ms,
        status: record.status,
    }
}

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

fn put_revision(
    raw: &mut fjall::OwnedWriteBatch,
    revisions: &Keyspace,
    view_id: &ThreadViewId,
    revision: ProviderRevision,
) -> Result<()> {
    put_json(raw, revisions, keys::revision_key(view_id), &revision)
}

fn next_sequence_for_turn(heads: &Keyspace, turn_id: &TurnId) -> Result<u64> {
    let last: Option<u64> = get_json(heads, keys::source_event_head_key(turn_id))?;
    Ok(last.map_or(0, |sequence| sequence.saturating_add(1)))
}

fn ensure_limit(requested: usize, max: usize) -> Result<()> {
    if requested > max {
        return Err(StorageError::LimitExceeded { requested, max });
    }
    Ok(())
}

fn ensure_revision(
    view_id: &ThreadViewId,
    observed: Option<ProviderRevision>,
    current: ProviderRevision,
) -> Result<()> {
    if observed.is_some_and(|revision| revision != current) {
        return Err(StorageError::StaleRevision {
            view_id: view_id.clone(),
            observed,
            current,
        });
    }
    Ok(())
}

fn ensure_revision_for_record(
    observed: Option<ProviderRevision>,
    current: ProviderRevision,
) -> Result<()> {
    if observed.is_some_and(|revision| revision != current) {
        return Err(StorageError::StaleRecordRevision {
            target: "resource".to_string(),
            observed,
            current,
        });
    }
    Ok(())
}

fn validate_conversation(record: &ConversationRecord) -> Result<()> {
    record.id.validate()?;
    record.view_id.validate()?;
    validate_source_metadata(record.source.as_ref())?;
    Ok(())
}

fn validate_turn(record: &TurnRecord) -> Result<()> {
    record.id.validate()?;
    record.conversation_id.validate()?;
    record.view_id.validate()?;
    if let Some(parent_turn_id) = &record.parent_turn_id {
        parent_turn_id.validate()?;
    }
    validate_source_metadata(record.source.as_ref())?;
    Ok(())
}

fn validate_source_event(record: &SourceEventRecord) -> Result<()> {
    record.id.validate()?;
    record.turn_id.validate()?;
    validate_source_metadata(Some(&record.source))?;
    validate_source_payload(&record.payload)
}

fn validate_source_payload(payload: &SourceEventPayload) -> Result<()> {
    reject_secret_like_json(&payload.body)?;
    let size = serde_json::to_vec(payload)?.len();
    if size > MAX_SOURCE_EVENT_PAYLOAD_BYTES {
        return Err(StorageError::LimitExceeded {
            requested: size,
            max: MAX_SOURCE_EVENT_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

fn validate_item(record: &CanonicalItemRecord) -> Result<()> {
    record.id.validate()?;
    record.turn_id.validate()?;
    record.source_event_id.validate()?;
    validate_source_metadata(record.source.as_ref())?;
    Ok(())
}

fn validate_projection(record: &ProjectionRecord) -> Result<()> {
    record.id.validate()?;
    record.view_id.validate()?;
    record.turn_id.validate()?;
    record.item_id.validate()?;
    validate_provenance(&record.provenance)
}

fn validate_view_record(record: &TranscriptViewRecord) -> Result<()> {
    record.id.validate()?;
    record.view_id.validate()?;
    record.projection_id.validate()?;
    validate_provenance(&record.provenance)
}

fn validate_view_record_key(
    view_id: &crate::ThreadViewId,
    id: &crate::TranscriptViewRecordId,
) -> Result<()> {
    view_id.validate()?;
    id.validate()
}

fn validate_resource(record: &ResourceRecord) -> Result<()> {
    record.metadata.id.validate()?;
    if let Some(range) = record.metadata.preview_range {
        if range.start > range.end || range.end > record.bytes.len() as u64 {
            return Err(StorageError::ResourceRangeOutOfBounds {
                resource_id: record.metadata.id.clone(),
                range: range.to_range(),
                byte_len: record.bytes.len() as u64,
            });
        }
    }
    Ok(())
}

fn validate_cursor(record: &CursorRecord) -> Result<()> {
    record.id.validate()?;
    record.view_id.validate()
}

fn validate_recovery_marker(record: &RecoveryMarkerRecord) -> Result<()> {
    record.id.validate()?;
    if let Some(view_id) = &record.view_id {
        view_id.validate()?;
    }
    if let Some(turn_id) = &record.turn_id {
        turn_id.validate()?;
    }
    Ok(())
}

fn validate_cas_projection_binding(record: &CasProjectionBindingRecord) -> Result<()> {
    record.id.validate()?;
    record.view_id.validate()
}

fn validate_provenance(provenance: &crate::SyndicSourceProvenance) -> Result<()> {
    provenance.view_id.validate()?;
    if let Some(turn_id) = &provenance.turn_id {
        turn_id.validate()?;
    }
    if let Some(item_id) = &provenance.item_id {
        item_id.validate()?;
    }
    if let Some(source_event_id) = &provenance.source_event_id {
        source_event_id.validate()?;
    }
    if let Some(projection_id) = &provenance.projection_id {
        projection_id.validate()?;
    }
    if let Some(resource_id) = &provenance.resource_id {
        resource_id.validate()?;
    }
    Ok(())
}

fn validate_source_metadata(source: Option<&crate::ExternalSourceMetadata>) -> Result<()> {
    if let Some(source) = source {
        if source.provider.is_empty() || source.provider.as_bytes().contains(&0) {
            return Err(StorageError::InvalidId {
                kind: "source provider",
                value: source.provider.clone(),
            });
        }
    }
    Ok(())
}

fn validate_source_thread_lookup(provider: &str, external_thread_id: &str) -> Result<()> {
    if provider.is_empty() || provider.as_bytes().contains(&0) {
        return Err(StorageError::InvalidId {
            kind: "source provider",
            value: provider.to_string(),
        });
    }
    if external_thread_id.is_empty() || external_thread_id.as_bytes().contains(&0) {
        return Err(StorageError::InvalidId {
            kind: "external thread",
            value: external_thread_id.to_string(),
        });
    }
    Ok(())
}

pub(crate) fn reject_secret_like_json(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
                if matches!(
                    normalized.as_str(),
                    "authorization"
                        | "cookie"
                        | "setcookie"
                        | "apikey"
                        | "accesstoken"
                        | "refreshtoken"
                        | "bearer"
                        | "capabilitytoken"
                ) {
                    return Err(StorageError::SecretLikeField { field: key.clone() });
                }
                reject_secret_like_json(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_secret_like_json(value)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum CursorSide {
    Before,
    After,
}

impl FromStr for CursorSide {
    type Err = ();

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "before" => Ok(Self::Before),
            "after" => Ok(Self::After),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CursorToken {
    side: CursorSide,
    view_id: ThreadViewId,
    position: TranscriptViewPosition,
    record_id: String,
}

fn cursor_for_record(side: CursorSide, record: &TranscriptViewRecord) -> Result<CursorId> {
    let token = CursorToken {
        side,
        view_id: record.view_id.clone(),
        position: record.position,
        record_id: record.id.to_string(),
    };
    Ok(CursorId(serde_json::to_string(&token)?))
}

fn decode_cursor(cursor: &CursorId, expected_view_id: &ThreadViewId) -> Result<CursorToken> {
    let token = serde_json::from_str::<CursorToken>(cursor.as_str()).map_err(|_| {
        StorageError::MissingCursor {
            cursor: cursor.to_string(),
        }
    })?;
    if token.view_id != *expected_view_id {
        return Err(StorageError::MissingCursor {
            cursor: cursor.to_string(),
        });
    }
    TranscriptViewRecordId::new(token.record_id.clone()).map_err(|_| {
        StorageError::MissingCursor {
            cursor: cursor.to_string(),
        }
    })?;
    Ok(token)
}
