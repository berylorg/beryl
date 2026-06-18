use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use beryl_backend::{
    AgentMessageItem, ImageGenerationItem, ProtocolPhase, ThreadItem, TurnError, TurnInfo,
    TurnStreamEvent, UserInput,
};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use serde_json::{Value, json};
use syndic_storage::{
    ByteRange, CanonicalItemKind, CanonicalItemRecord, CanonicalItemVisibility,
    CasProjectionBindingId, CasProjectionBindingRecord, CasProjectionBindingStatus, ConversationId,
    ConversationRecord, ExternalSourceMetadata, HistoryIncompleteReason, HistoryState, ItemId,
    ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordKind,
    ProjectionStatus, ProviderRevision, RecoveryMarkerId, RecoveryMarkerKind, RecoveryMarkerRecord,
    ResourceKind, ResourceMetadataRecord, ResourceRecord, ResourceState, SourceEventId,
    SourceEventPayload, SourceEventRecord, SourceEventVisibility, StoreOpenOptions,
    SyndicSourceProvenance, SyndicStore, SyndicWriteBatch, TerminalError, ThreadViewId,
    TranscriptNarrativeKind, TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
    TurnId, TurnKind, TurnRecord, TurnStatus,
};

use crate::{BerylWorkspacePersistence, WorkspacePersistenceError};

use super::{token_usage_snapshot, turn_input::UserInputFragment};

const CAS_PROVIDER: &str = "codex-app-server";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyndicTurnAdmission {
    storage_dir: PathBuf,
    conversation_id: ConversationId,
    view_id: ThreadViewId,
    turn_id: TurnId,
    binding_id: CasProjectionBindingId,
    recovery_marker_id: RecoveryMarkerId,
    runtime_target: String,
    admitted_at_ms: u64,
    next_sequence: u64,
    next_position: u64,
    first_user_item_id: Option<ItemId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyndicTurnIdentity {
    storage_dir: PathBuf,
    conversation_id: ConversationId,
    view_id: ThreadViewId,
    turn_id: TurnId,
    runtime_target: String,
}

#[derive(Debug)]
pub(super) enum SyndicIngestionError {
    CreateStorageDir {
        path: PathBuf,
        source: std::io::Error,
    },
    Storage(syndic_storage::StorageError),
    Workspace(WorkspacePersistenceError),
    MissingConversation {
        id: ConversationId,
    },
    MissingTurn {
        id: TurnId,
    },
}

impl std::fmt::Display for SyndicIngestionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateStorageDir { path, source } => {
                write!(
                    formatter,
                    "failed to create Syndic storage directory {}: {source}",
                    path.display()
                )
            }
            Self::Storage(error) => write!(formatter, "{error}"),
            Self::Workspace(error) => write!(formatter, "{error}"),
            Self::MissingConversation { id } => {
                write!(formatter, "missing admitted Syndic conversation {id}")
            }
            Self::MissingTurn { id } => write!(formatter, "missing admitted Syndic turn {id}"),
        }
    }
}

impl std::error::Error for SyndicIngestionError {}

impl From<syndic_storage::StorageError> for SyndicIngestionError {
    fn from(error: syndic_storage::StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<WorkspacePersistenceError> for SyndicIngestionError {
    fn from(error: WorkspacePersistenceError) -> Self {
        Self::Workspace(error)
    }
}

pub(super) fn admit_user_turn(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    selected_thread_id: Option<&str>,
    fragments: &[UserInputFragment],
) -> Result<SyndicTurnAdmission, SyndicIngestionError> {
    let storage_dir = persistence.workspace_syndic_storage_dir(workspace_id);
    ensure_storage_dir(&storage_dir)?;
    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default())?;
    let runtime_target = runtime_target_id(execution_target);
    let now = token_usage_snapshot::current_unix_millis();
    let resolved = resolve_conversation(
        &store,
        workspace_id,
        &runtime_target,
        selected_thread_id,
        now,
    )?;
    let turn_id = local_turn_id(workspace_id, now, fragments);
    let binding_id = CasProjectionBindingId::from(format!("binding:{}", resolved.view_id));
    let recovery_marker_id = RecoveryMarkerId::from(format!("recovery:{}", turn_id));
    let first_user_item_id = fragments
        .first()
        .map(|fragment| user_item_id(&turn_id, fragment.id));
    let mut revision = resolved.current_revision;
    let mut next_sequence = store.next_source_event_sequence(&turn_id)?;
    let mut next_position = next_transcript_position(&store, &resolved.view_id)?;
    let final_revision = ProviderRevision(
        resolved
            .current_revision
            .0
            .saturating_add(fragments.len() as u64),
    );
    let mut batch = SyndicWriteBatch::new()
        .put_conversation(ConversationRecord {
            id: resolved.conversation_id.clone(),
            view_id: resolved.view_id.clone(),
            title: resolved.title,
            created_at_ms: resolved.created_at_ms,
            updated_at_ms: now,
            current_revision: final_revision,
            source: selected_thread_id.map(|thread_id| {
                source_metadata(
                    &runtime_target,
                    Some(thread_id),
                    None,
                    None,
                    Some("admission"),
                )
            }),
            history_state: resolved.history_state,
        })
        .put_turn(TurnRecord {
            id: turn_id.clone(),
            conversation_id: resolved.conversation_id.clone(),
            view_id: resolved.view_id.clone(),
            parent_turn_id: None,
            kind: TurnKind::User,
            status: TurnStatus::Pending,
            source: selected_thread_id.map(|thread_id| {
                source_metadata(
                    &runtime_target,
                    Some(thread_id),
                    None,
                    None,
                    Some("admission"),
                )
            }),
            created_at_ms: now,
            started_at_ms: None,
            completed_at_ms: None,
            terminal_error: None,
            projection_revision: final_revision,
        })
        .put_recovery_marker(RecoveryMarkerRecord {
            id: recovery_marker_id.clone(),
            kind: RecoveryMarkerKind::SourceIngestionInterrupted,
            view_id: Some(resolved.view_id.clone()),
            turn_id: Some(turn_id.clone()),
            created_at_ms: now,
            detail: Some("turn admitted before CAS terminal event was captured".to_string()),
        });

    for fragment in fragments {
        revision = revision.next();
        let records = user_fragment_records(
            &resolved.view_id,
            &turn_id,
            &runtime_target,
            selected_thread_id,
            None,
            next_sequence,
            next_position,
            revision,
            now,
            fragment,
        );
        batch = batch
            .put_source_event(records.source_event)
            .put_item(records.item)
            .put_projection(records.projection)
            .put_view_record(records.view_record);
        next_sequence = next_sequence.saturating_add(1);
        next_position = next_position.saturating_add(1);
    }

    store.commit(batch)?;
    Ok(SyndicTurnAdmission {
        storage_dir,
        conversation_id: resolved.conversation_id,
        view_id: resolved.view_id,
        turn_id,
        binding_id,
        recovery_marker_id,
        runtime_target,
        admitted_at_ms: now,
        next_sequence,
        next_position,
        first_user_item_id,
    })
}

pub(super) fn admit_additional_user_fragment(
    identity: &SyndicTurnIdentity,
    fragment: &UserInputFragment,
    external_thread_id: Option<&str>,
    external_turn_id: Option<&str>,
) -> Result<(), SyndicIngestionError> {
    admit_additional_user_fragment_with_marker_clear(
        identity,
        fragment,
        external_thread_id,
        external_turn_id,
        None,
    )
}

pub(super) fn journal_steering_user_fragment(
    identity: &SyndicTurnIdentity,
    fragment: &UserInputFragment,
    external_thread_id: &str,
    external_turn_id: Option<&str>,
) -> Result<(), SyndicIngestionError> {
    let store = SyndicStore::open(&identity.storage_dir, StoreOpenOptions::default())?;
    let sequence = store.next_source_event_sequence(&identity.turn_id)?;
    let now = token_usage_snapshot::current_unix_millis();
    let event_id = live_source_event_id(&identity.turn_id, sequence, "steeringFragmentAccepted");
    let marker_id = steering_recovery_marker_id(&identity.turn_id, fragment.id);
    store.commit(
        SyndicWriteBatch::new()
            .put_source_event(SourceEventRecord {
                id: event_id.clone(),
                turn_id: identity.turn_id.clone(),
                sequence,
                captured_at_ms: now,
                source: source_metadata(
                    &identity.runtime_target,
                    Some(external_thread_id),
                    external_turn_id,
                    None,
                    Some(event_id.as_str()),
                ),
                visibility: SourceEventVisibility::Operational,
                payload: SourceEventPayload {
                    kind: "steeringFragmentAccepted".to_string(),
                    body: json!({
                        "fragment": user_fragment_payload(fragment),
                        "externalTurnId": external_turn_id,
                    }),
                },
            })
            .put_recovery_marker(RecoveryMarkerRecord {
                id: marker_id,
                kind: RecoveryMarkerKind::SourceIngestionInterrupted,
                view_id: Some(identity.view_id.clone()),
                turn_id: Some(identity.turn_id.clone()),
                created_at_ms: now,
                detail: Some(format!(
                    "active-turn steering fragment {} was admitted before delivery reached a terminal outcome",
                    fragment.id
                )),
            }),
    )?;
    Ok(())
}

pub(super) fn promote_steering_user_fragment(
    identity: &SyndicTurnIdentity,
    fragment: &UserInputFragment,
    external_thread_id: &str,
    external_turn_id: &str,
) -> Result<(), SyndicIngestionError> {
    admit_additional_user_fragment_with_marker_clear(
        identity,
        fragment,
        Some(external_thread_id),
        Some(external_turn_id),
        Some(steering_recovery_marker_id(&identity.turn_id, fragment.id)),
    )
}

pub(super) fn mark_steering_user_fragment_redirected(
    identity: &SyndicTurnIdentity,
    fragment_id: u64,
    external_thread_id: &str,
    external_turn_id: Option<&str>,
    detail: impl Into<String>,
) -> Result<(), SyndicIngestionError> {
    let store = SyndicStore::open(&identity.storage_dir, StoreOpenOptions::default())?;
    let sequence = store.next_source_event_sequence(&identity.turn_id)?;
    let now = token_usage_snapshot::current_unix_millis();
    let event_id = live_source_event_id(&identity.turn_id, sequence, "steeringFragmentRedirected");
    store.commit(
        SyndicWriteBatch::new()
            .put_source_event(SourceEventRecord {
                id: event_id.clone(),
                turn_id: identity.turn_id.clone(),
                sequence,
                captured_at_ms: now,
                source: source_metadata(
                    &identity.runtime_target,
                    Some(external_thread_id),
                    external_turn_id,
                    None,
                    Some(event_id.as_str()),
                ),
                visibility: SourceEventVisibility::Operational,
                payload: SourceEventPayload {
                    kind: "steeringFragmentRedirected".to_string(),
                    body: json!({
                        "fragmentId": fragment_id,
                        "detail": detail.into(),
                    }),
                },
            })
            .clear_recovery_marker(steering_recovery_marker_id(&identity.turn_id, fragment_id)),
    )?;
    Ok(())
}

fn admit_additional_user_fragment_with_marker_clear(
    identity: &SyndicTurnIdentity,
    fragment: &UserInputFragment,
    external_thread_id: Option<&str>,
    external_turn_id: Option<&str>,
    clear_marker: Option<RecoveryMarkerId>,
) -> Result<(), SyndicIngestionError> {
    let store = SyndicStore::open(&identity.storage_dir, StoreOpenOptions::default())?;
    let sequence = store.next_source_event_sequence(&identity.turn_id)?;
    let position = next_transcript_position(&store, &identity.view_id)?;
    let revision = store.current_revision(&identity.view_id)?.next();
    let now = token_usage_snapshot::current_unix_millis();
    let records = user_fragment_records(
        &identity.view_id,
        &identity.turn_id,
        &identity.runtime_target,
        external_thread_id,
        external_turn_id,
        sequence,
        position,
        revision,
        now,
        fragment,
    );
    let mut conversation =
        load_conversation(&store, &identity.conversation_id)?.ok_or_else(|| {
            SyndicIngestionError::MissingConversation {
                id: identity.conversation_id.clone(),
            }
        })?;
    conversation.current_revision = revision;
    conversation.updated_at_ms = now;

    let mut batch = SyndicWriteBatch::new()
        .put_conversation(conversation)
        .put_source_event(records.source_event)
        .put_item(records.item)
        .put_projection(records.projection)
        .put_view_record(records.view_record);
    if let Some(marker_id) = clear_marker {
        batch = batch.clear_recovery_marker(marker_id);
    }
    store.commit(batch)?;
    Ok(())
}

impl SyndicTurnAdmission {
    pub(super) fn identity(&self) -> SyndicTurnIdentity {
        SyndicTurnIdentity {
            storage_dir: self.storage_dir.clone(),
            conversation_id: self.conversation_id.clone(),
            view_id: self.view_id.clone(),
            turn_id: self.turn_id.clone(),
            runtime_target: self.runtime_target.clone(),
        }
    }
}

struct ResolvedConversation {
    conversation_id: ConversationId,
    view_id: ThreadViewId,
    title: Option<String>,
    created_at_ms: u64,
    current_revision: ProviderRevision,
    history_state: HistoryState,
}

struct UserFragmentRecords {
    source_event: SourceEventRecord,
    item: CanonicalItemRecord,
    projection: ProjectionRecord,
    view_record: TranscriptViewRecord,
}

fn resolve_conversation(
    store: &SyndicStore,
    workspace_id: &BerylWorkspaceId,
    runtime_target: &str,
    selected_thread_id: Option<&str>,
    now: u64,
) -> Result<ResolvedConversation, SyndicIngestionError> {
    if let Some(thread_id) = selected_thread_id
        && let Some(existing) =
            store.conversation_by_external_thread(CAS_PROVIDER, Some(runtime_target), thread_id)?
    {
        return Ok(ResolvedConversation {
            conversation_id: existing.id,
            view_id: existing.view_id,
            title: existing.title,
            created_at_ms: existing.created_at_ms,
            current_revision: existing.current_revision,
            history_state: existing.history_state,
        });
    }

    let suffix = selected_thread_id
        .map(|thread_id| format!("cas:{thread_id}"))
        .unwrap_or_else(|| format!("local:{now}"));
    let conversation_id =
        ConversationId::from(format!("conversation:{}:{suffix}", workspace_id.as_str()));
    let view_id = ThreadViewId::from(format!("view:{}:{suffix}", workspace_id.as_str()));
    let history_state = if selected_thread_id.is_some() {
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::NotCaptured,
            detail: Some(
                "older CAS thread history was not captured through the live Syndic stream"
                    .to_string(),
            ),
        }
    } else {
        HistoryState::Complete
    };
    Ok(ResolvedConversation {
        conversation_id,
        view_id,
        title: None,
        created_at_ms: now,
        current_revision: ProviderRevision(0),
        history_state,
    })
}

fn next_transcript_position(
    store: &SyndicStore,
    view_id: &ThreadViewId,
) -> Result<u64, SyndicIngestionError> {
    let page = store.read_transcript_page(
        view_id,
        syndic_storage::TranscriptPageAnchor::End,
        syndic_storage::TranscriptPageDirection::Backward,
        1,
        None,
    )?;
    Ok(page
        .records
        .last()
        .map(|record| record.position.next().0)
        .unwrap_or_default())
}

fn user_fragment_records(
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    runtime_target: &str,
    external_thread_id: Option<&str>,
    external_turn_id: Option<&str>,
    sequence: u64,
    position: u64,
    revision: ProviderRevision,
    captured_at_ms: u64,
    fragment: &UserInputFragment,
) -> UserFragmentRecords {
    let event_id = SourceEventId::from(format!("event:{turn_id}:user:{}", fragment.id));
    let item_id = user_item_id(turn_id, fragment.id);
    let projection_id = ProjectionRecordId::from(format!("projection:{item_id}"));
    let view_record_id = TranscriptViewRecordId::from(format!("view-record:{projection_id}"));
    let source = source_metadata(
        runtime_target,
        external_thread_id,
        external_turn_id,
        None,
        Some(event_id.as_str()),
    );
    let payload = user_fragment_payload(fragment);
    let provenance = text_provenance(
        view_id,
        Some(TranscriptViewPosition(position)),
        Some(turn_id.clone()),
        Some(item_id.clone()),
        Some(event_id.clone()),
        Some(projection_id.clone()),
        &fragment.text,
    );

    UserFragmentRecords {
        source_event: SourceEventRecord {
            id: event_id.clone(),
            turn_id: turn_id.clone(),
            sequence,
            captured_at_ms,
            source: source.clone(),
            visibility: SourceEventVisibility::TranscriptVisible,
            payload: SourceEventPayload {
                kind: "acceptedUserInput".to_string(),
                body: payload.clone(),
            },
        },
        item: CanonicalItemRecord {
            id: item_id.clone(),
            turn_id: turn_id.clone(),
            source_event_id: event_id,
            kind: CanonicalItemKind::UserInput,
            visibility: CanonicalItemVisibility::Transcript,
            source: Some(source),
            payload,
        },
        projection: ProjectionRecord {
            id: projection_id.clone(),
            view_id: view_id.clone(),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            revision,
            kind: ProjectionRecordKind::TextChunk,
            status: ProjectionStatus::Current,
            payload: ProjectionPayload::Text {
                text: fragment.text.clone(),
            },
            provenance: provenance.clone(),
        },
        view_record: TranscriptViewRecord {
            id: view_record_id,
            view_id: view_id.clone(),
            position: TranscriptViewPosition(position),
            projection_id,
            narrative_kind: TranscriptNarrativeKind::UserInput,
            provenance,
        },
    }
}

fn user_fragment_payload(fragment: &UserInputFragment) -> Value {
    json!({
        "fragmentId": fragment.id,
        "text": fragment.text.as_str(),
        "backendInput": fragment.backend_input(),
        "imageMarkerCount": fragment.image_marker_specs().len(),
    })
}

pub(super) struct SyndicLiveTurnIngestor {
    admission: SyndicTurnAdmission,
    store: SyndicStore,
    cas_thread_id: Option<String>,
    cas_turn_id: Option<String>,
    next_sequence: u64,
    next_position: u64,
    agent_messages: HashMap<String, AgentMessageProjectionState>,
    transcript_items: HashSet<String>,
    saw_turn_started: bool,
    saw_terminal_turn: bool,
}

struct AgentMessageProjectionState {
    position: TranscriptViewPosition,
    text: String,
    phase: Option<ProtocolPhase>,
}

impl SyndicLiveTurnIngestor {
    pub(super) fn new(admission: SyndicTurnAdmission) -> Result<Self, SyndicIngestionError> {
        let store = SyndicStore::open(&admission.storage_dir, StoreOpenOptions::default())?;
        Ok(Self {
            next_sequence: admission.next_sequence,
            next_position: admission.next_position,
            admission,
            store,
            cas_thread_id: None,
            cas_turn_id: None,
            agent_messages: HashMap::new(),
            transcript_items: HashSet::new(),
            saw_turn_started: false,
            saw_terminal_turn: false,
        })
    }

    pub(super) fn bind_cas_thread(
        &mut self,
        cas_thread_id: &str,
    ) -> Result<(), SyndicIngestionError> {
        self.cas_thread_id = Some(cas_thread_id.to_string());
        let now = token_usage_snapshot::current_unix_millis();
        let mut conversation = load_conversation(&self.store, &self.admission.conversation_id)?
            .ok_or_else(|| SyndicIngestionError::MissingConversation {
                id: self.admission.conversation_id.clone(),
            })?;
        conversation.source = Some(source_metadata(
            &self.admission.runtime_target,
            Some(cas_thread_id),
            None,
            None,
            Some("cas-thread-bound"),
        ));
        conversation.updated_at_ms = now;

        let mut turn = load_turn(&self.store, &self.admission.turn_id)?.ok_or_else(|| {
            SyndicIngestionError::MissingTurn {
                id: self.admission.turn_id.clone(),
            }
        })?;
        turn.source = Some(source_metadata(
            &self.admission.runtime_target,
            Some(cas_thread_id),
            None,
            None,
            Some("cas-thread-bound"),
        ));

        self.store.commit(
            SyndicWriteBatch::new()
                .put_conversation(conversation)
                .put_turn(turn)
                .put_cas_projection_binding(CasProjectionBindingRecord {
                    id: self.admission.binding_id.clone(),
                    view_id: self.admission.view_id.clone(),
                    binding_revision: 1,
                    selected_path_revision: self.store.current_revision(&self.admission.view_id)?,
                    selected_path_digest: Some(format!("admission:{}", self.admission.turn_id)),
                    established_at_ms: now,
                    status: CasProjectionBindingStatus::Valid {
                        runtime_target: self.admission.runtime_target.clone(),
                        cas_thread_id: cas_thread_id.to_string(),
                        lineage_proof: format!("live-admission:{}", self.admission.turn_id),
                    },
                }),
        )?;
        Ok(())
    }

    pub(super) fn ingest_event(
        &mut self,
        event: &TurnStreamEvent,
    ) -> Result<(), SyndicIngestionError> {
        match event {
            TurnStreamEvent::TurnStarted { thread_id, turn } => {
                self.ingest_turn_started(thread_id, turn)
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn } => {
                self.ingest_turn_completed(thread_id, turn)
            }
            TurnStreamEvent::ItemStarted {
                thread_id,
                turn_id,
                item,
            } => self.ingest_item_event("itemStarted", thread_id, turn_id, item, false),
            TurnStreamEvent::ItemCompleted {
                thread_id,
                turn_id,
                item,
            } => self.ingest_item_event("itemCompleted", thread_id, turn_id, item, true),
            TurnStreamEvent::AgentMessageDelta {
                thread_id,
                turn_id,
                item_id,
                delta,
            } => self.ingest_agent_message_delta(thread_id, turn_id, item_id, delta),
            TurnStreamEvent::TokenUsageUpdated {
                thread_id,
                turn_id,
                token_usage,
            } => self.ingest_metadata_event(
                "tokenUsageUpdated",
                thread_id,
                Some(turn_id),
                None,
                json!({ "tokenUsage": token_usage }),
            ),
            TurnStreamEvent::ThreadStatusChanged { thread_id, status } => self
                .ingest_metadata_event(
                    "threadStatusChanged",
                    thread_id,
                    None,
                    None,
                    json!({ "status": status }),
                ),
            TurnStreamEvent::ThreadNameUpdated {
                thread_id,
                thread_name,
            } => self.ingest_metadata_event(
                "threadNameUpdated",
                thread_id,
                None,
                None,
                json!({ "threadName": thread_name }),
            ),
            TurnStreamEvent::ReasoningSummaryPartAdded {
                thread_id,
                turn_id,
                item_id,
                summary_index,
            } => self.ingest_metadata_event(
                "reasoningSummaryPartAdded",
                thread_id,
                Some(turn_id),
                Some(item_id),
                json!({ "summaryIndex": summary_index }),
            ),
            TurnStreamEvent::ReasoningSummaryTextDelta {
                thread_id,
                turn_id,
                item_id,
                summary_index,
                delta,
            } => self.ingest_metadata_event(
                "reasoningSummaryTextDelta",
                thread_id,
                Some(turn_id),
                Some(item_id),
                json!({ "summaryIndex": summary_index, "delta": delta }),
            ),
            TurnStreamEvent::CommandExecutionOutputDelta {
                thread_id,
                turn_id,
                item_id,
                delta,
            }
            | TurnStreamEvent::FileChangeOutputDelta {
                thread_id,
                turn_id,
                item_id,
                delta,
            } => self.ingest_metadata_event(
                event_kind(event),
                thread_id,
                Some(turn_id),
                Some(item_id),
                json!({ "delta": delta }),
            ),
            TurnStreamEvent::ThreadStarted { thread } => self.ingest_metadata_event(
                "threadStarted",
                &thread.id,
                None,
                None,
                json!({ "thread": thread }),
            ),
            TurnStreamEvent::AgentLabelUpdated { thread_id, label } => self.ingest_metadata_event(
                "agentLabelUpdated",
                thread_id,
                None,
                None,
                json!({ "label": label }),
            ),
            TurnStreamEvent::ThreadClosed { thread_id }
            | TurnStreamEvent::ThreadArchived { thread_id }
            | TurnStreamEvent::ThreadUnarchived { thread_id } => {
                self.ingest_metadata_event(event_kind(event), thread_id, None, None, json!({}))
            }
            TurnStreamEvent::AccountRateLimitsUpdated { .. }
            | TurnStreamEvent::ApprovalRequested(_)
            | TurnStreamEvent::DynamicToolCallRequested(_)
            | TurnStreamEvent::ReasoningTextDelta { .. }
            | TurnStreamEvent::ProtocolError { .. } => Ok(()),
        }
    }
}

impl SyndicLiveTurnIngestor {
    fn agent_projection_record(
        &self,
        item_id: &ItemId,
        projection_id: &ProjectionRecordId,
        position: TranscriptViewPosition,
        source_event_id: SourceEventId,
        revision: ProviderRevision,
        text: String,
        _phase: Option<ProtocolPhase>,
    ) -> ProjectionRecord {
        let provenance = text_provenance(
            &self.admission.view_id,
            Some(position),
            Some(self.admission.turn_id.clone()),
            Some(item_id.clone()),
            Some(source_event_id),
            Some(projection_id.clone()),
            &text,
        );
        ProjectionRecord {
            id: projection_id.clone(),
            view_id: self.admission.view_id.clone(),
            turn_id: self.admission.turn_id.clone(),
            item_id: item_id.clone(),
            revision,
            kind: ProjectionRecordKind::TextChunk,
            status: ProjectionStatus::Current,
            payload: ProjectionPayload::Text { text },
            provenance,
        }
    }

    fn agent_view_record(
        &mut self,
        item_id: &ItemId,
        projection_id: &ProjectionRecordId,
        position: TranscriptViewPosition,
        source_event_id: SourceEventId,
        phase: Option<ProtocolPhase>,
        text: &str,
    ) -> TranscriptViewRecord {
        self.transcript_items.insert(item_id.to_string());
        TranscriptViewRecord {
            id: TranscriptViewRecordId::from(format!("view-record:{projection_id}")),
            view_id: self.admission.view_id.clone(),
            position,
            projection_id: projection_id.clone(),
            narrative_kind: match phase {
                Some(ProtocolPhase::FinalAnswer) => TranscriptNarrativeKind::AssistantFinalAnswer,
                _ => TranscriptNarrativeKind::AssistantCommentary,
            },
            provenance: text_provenance(
                &self.admission.view_id,
                Some(position),
                Some(self.admission.turn_id.clone()),
                Some(item_id.clone()),
                Some(source_event_id),
                Some(projection_id.clone()),
                text,
            ),
        }
    }
}

fn ensure_storage_dir(path: &Path) -> Result<(), SyndicIngestionError> {
    fs::create_dir_all(path).map_err(|source| SyndicIngestionError::CreateStorageDir {
        path: path.to_path_buf(),
        source,
    })
}

fn load_conversation(
    store: &SyndicStore,
    id: &ConversationId,
) -> Result<Option<ConversationRecord>, SyndicIngestionError> {
    Ok(store.conversation(id)?)
}

fn load_turn(store: &SyndicStore, id: &TurnId) -> Result<Option<TurnRecord>, SyndicIngestionError> {
    Ok(store.turn(id)?)
}

fn runtime_target_id(execution_target: &WorkspaceId) -> String {
    execution_target.runtime_mode().display_name().to_string()
}

fn local_turn_id(
    workspace_id: &BerylWorkspaceId,
    now: u64,
    fragments: &[UserInputFragment],
) -> TurnId {
    let first_fragment_id = fragments.first().map(|fragment| fragment.id).unwrap_or(0);
    TurnId::from(format!(
        "turn:{}:{now}:{first_fragment_id}",
        workspace_id.as_str()
    ))
}

fn user_item_id(turn_id: &TurnId, fragment_id: u64) -> ItemId {
    ItemId::from(format!("item:{turn_id}:user:{fragment_id}"))
}

fn steering_recovery_marker_id(turn_id: &TurnId, fragment_id: u64) -> RecoveryMarkerId {
    RecoveryMarkerId::from(format!("recovery:steering:{turn_id}:{fragment_id}"))
}

fn canonical_item_id(turn_id: &TurnId, external_item_id: &str) -> ItemId {
    ItemId::from(format!("item:{turn_id}:cas:{external_item_id}"))
}

fn live_source_event_id(turn_id: &TurnId, sequence: u64, _kind: &str) -> SourceEventId {
    SourceEventId::from(format!("event:{turn_id}:cas:{sequence}"))
}

fn latest_source_event_id(turn_id: &TurnId, sequence: u64) -> SourceEventId {
    live_source_event_id(turn_id, sequence, "event")
}

fn source_metadata(
    runtime_target: &str,
    external_thread_id: Option<&str>,
    external_turn_id: Option<&str>,
    external_item_id: Option<&str>,
    external_event_id: Option<&str>,
) -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        provider: CAS_PROVIDER.to_string(),
        runtime_target: Some(runtime_target.to_string()),
        external_thread_id: external_thread_id.map(str::to_string),
        external_turn_id: external_turn_id.map(str::to_string),
        external_item_id: external_item_id.map(str::to_string),
        external_event_id: external_event_id.map(str::to_string),
    }
}

fn text_provenance(
    view_id: &ThreadViewId,
    position: Option<TranscriptViewPosition>,
    turn_id: Option<TurnId>,
    item_id: Option<ItemId>,
    source_event_id: Option<SourceEventId>,
    projection_id: Option<ProjectionRecordId>,
    text: &str,
) -> SyndicSourceProvenance {
    let range = ByteRange::new(0, text.len() as u64);
    provenance_with_ranges(
        view_id,
        position,
        turn_id,
        item_id,
        source_event_id,
        projection_id,
        Some(range),
        Some(range),
    )
}

fn provenance_with_ranges(
    view_id: &ThreadViewId,
    position: Option<TranscriptViewPosition>,
    turn_id: Option<TurnId>,
    item_id: Option<ItemId>,
    source_event_id: Option<SourceEventId>,
    projection_id: Option<ProjectionRecordId>,
    source_range: Option<ByteRange>,
    copy_source_range: Option<ByteRange>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position,
        turn_id,
        item_id,
        source_event_id,
        projection_id,
        resource_id: None,
        source_range,
        resource_range: None,
        copy_source_range,
    }
}

fn sanitized_turn_payload(turn: &TurnInfo) -> Value {
    json!({
        "id": turn.id,
        "status": turn.status,
        "itemsView": turn.items_view,
        "itemCount": turn.items.len(),
        "error": turn.error.as_ref().map(|error| {
            json!({
                "message": error.message,
                "additionalDetails": error.additional_details,
            })
        }),
        "items": turn.items.iter().map(sanitized_thread_item_payload).collect::<Vec<_>>(),
    })
}

fn sanitized_thread_item_payload(item: &ThreadItem) -> Value {
    match item {
        ThreadItem::UserMessage(item) => json!({
            "type": "userMessage",
            "id": item.id,
            "contentCount": item.content.len(),
            "content": item.content.iter().map(sanitized_user_input).collect::<Vec<_>>(),
        }),
        ThreadItem::AgentMessage(item) => json!({
            "type": "agentMessage",
            "id": item.id,
            "phase": item.phase,
            "textBytes": item.text.len(),
        }),
        ThreadItem::Reasoning(item) => json!({
            "type": "reasoning",
            "id": item.id,
            "summaryCount": item.summary.len(),
            "contentCount": item.content.len(),
        }),
        ThreadItem::CommandExecution(item) => json!({
            "type": "commandExecution",
            "id": item.id,
            "status": item.status,
            "processId": item.process_id,
            "exitCode": item.exit_code,
            "durationMs": item.duration_ms,
            "commandBytes": item.command.len(),
            "cwdBytes": item.cwd.len(),
            "aggregatedOutputBytes": item.aggregated_output.as_ref().map(String::len),
        }),
        ThreadItem::FileChange(item) => json!({
            "type": "fileChange",
            "id": item.id,
            "status": item.status,
            "changeCount": item.changes.len(),
        }),
        ThreadItem::ImageGeneration(item) => json!({
            "type": "imageGeneration",
            "id": item.id,
            "status": item.status,
            "revisedPromptBytes": item.revised_prompt.as_ref().map(String::len),
            "savedPath": item.saved_path,
            "resultPresent": item.result.as_ref().is_some_and(|value| !value.is_empty()),
        }),
        ThreadItem::Generic(item) => json!({
            "type": item.item_type,
            "id": item.id,
            "tool": item.tool,
            "server": item.server,
            "namespace": item.namespace,
            "status": item.status,
            "model": item.model,
            "reasoningEffort": item.reasoning_effort,
            "receiverThreadIds": item.receiver_thread_ids,
            "agentNickname": item.agent_nickname,
        }),
    }
}

fn sanitized_user_input(input: &UserInput) -> Value {
    match input {
        UserInput::Text { text } => json!({ "type": "text", "text": text }),
        UserInput::Image { url } => json!({ "type": "image", "url": url }),
        UserInput::LocalImage { path } => json!({ "type": "localImage", "path": path }),
        UserInput::Skill { name, path } => {
            json!({ "type": "skill", "name": name, "path": path })
        }
        UserInput::Mention { name, path } => {
            json!({ "type": "mention", "name": name, "path": path })
        }
    }
}

fn item_visibility(item: &ThreadItem) -> SourceEventVisibility {
    match item {
        ThreadItem::AgentMessage(_) => SourceEventVisibility::TranscriptVisible,
        ThreadItem::UserMessage(_) => SourceEventVisibility::CanonicalOnly,
        _ => SourceEventVisibility::Operational,
    }
}

fn turn_status_from_backend(status: beryl_backend::TurnStatus) -> TurnStatus {
    match status {
        beryl_backend::TurnStatus::Completed => TurnStatus::Completed,
        beryl_backend::TurnStatus::Interrupted => TurnStatus::Interrupted,
        beryl_backend::TurnStatus::Failed => TurnStatus::Failed {
            reason: "cas_turn_failed".to_string(),
        },
        beryl_backend::TurnStatus::InProgress => TurnStatus::Running,
    }
}

fn terminal_error(error: &TurnError) -> TerminalError {
    TerminalError {
        code: None,
        message: match error.additional_details.as_deref() {
            Some(details) if !details.trim().is_empty() => {
                format!("{} ({details})", error.message)
            }
            _ => error.message.clone(),
        },
    }
}

fn event_kind(event: &TurnStreamEvent) -> &'static str {
    match event {
        TurnStreamEvent::ThreadClosed { .. } => "threadClosed",
        TurnStreamEvent::ThreadArchived { .. } => "threadArchived",
        TurnStreamEvent::ThreadUnarchived { .. } => "threadUnarchived",
        TurnStreamEvent::CommandExecutionOutputDelta { .. } => "commandExecutionOutputDelta",
        TurnStreamEvent::FileChangeOutputDelta { .. } => "fileChangeOutputDelta",
        _ => "event",
    }
}

impl SyndicLiveTurnIngestor {
    pub(super) fn mark_local_failure(
        &mut self,
        detail: impl Into<String>,
    ) -> Result<(), SyndicIngestionError> {
        let detail = detail.into();
        let now = token_usage_snapshot::current_unix_millis();
        let event = self.source_event(
            "localFailure",
            self.cas_thread_id.as_deref().unwrap_or("unbound"),
            self.cas_turn_id.as_deref(),
            None,
            SourceEventVisibility::Operational,
            json!({ "detail": detail }),
            now,
        );
        let revision = self.store.current_revision(&self.admission.view_id)?;
        self.store.commit(
            SyndicWriteBatch::new()
                .put_source_event(event)
                .put_turn(self.turn_record(
                    TurnStatus::Failed {
                        reason: "local_turn_delivery_failure".to_string(),
                    },
                    self.cas_thread_id.as_deref(),
                    self.cas_turn_id.as_deref(),
                    None,
                    Some(now),
                    Some(TerminalError {
                        code: Some("local_failure".to_string()),
                        message: detail.clone(),
                    }),
                    revision,
                )?)
                .clear_recovery_marker(self.admission.recovery_marker_id.clone()),
        )?;
        Ok(())
    }

    pub(super) fn mark_stream_lost(
        &mut self,
        detail: impl Into<String>,
    ) -> Result<(), SyndicIngestionError> {
        let detail = detail.into();
        let now = token_usage_snapshot::current_unix_millis();
        let event = self.source_event(
            "streamLost",
            self.cas_thread_id.as_deref().unwrap_or("unknown"),
            self.cas_turn_id.as_deref(),
            None,
            SourceEventVisibility::Operational,
            json!({ "detail": detail }),
            now,
        );
        let revision = self.store.current_revision(&self.admission.view_id)?;
        let mut conversation = load_conversation(&self.store, &self.admission.conversation_id)?
            .ok_or_else(|| SyndicIngestionError::MissingConversation {
                id: self.admission.conversation_id.clone(),
            })?;
        conversation.history_state = HistoryState::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            detail: Some(detail.clone()),
        };
        conversation.updated_at_ms = now;
        self.store.commit(
            SyndicWriteBatch::new()
                .put_conversation(conversation)
                .put_source_event(event)
                .put_turn(self.turn_record(
                    TurnStatus::Incomplete {
                        reason: HistoryIncompleteReason::StreamLost,
                        detail: Some(detail.clone()),
                    },
                    self.cas_thread_id.as_deref(),
                    self.cas_turn_id.as_deref(),
                    None,
                    Some(now),
                    None,
                    revision,
                )?)
                .put_recovery_marker(RecoveryMarkerRecord {
                    id: self.admission.recovery_marker_id.clone(),
                    kind: RecoveryMarkerKind::SourceIngestionInterrupted,
                    view_id: Some(self.admission.view_id.clone()),
                    turn_id: Some(self.admission.turn_id.clone()),
                    created_at_ms: now,
                    detail: Some(detail),
                }),
        )?;
        Ok(())
    }

    fn commit_event_batch(&mut self, batch: SyndicWriteBatch) -> Result<(), SyndicIngestionError> {
        self.store.commit(batch)?;
        self.next_sequence = self
            .store
            .next_source_event_sequence(&self.admission.turn_id)?;
        Ok(())
    }

    fn source_event(
        &self,
        kind: &str,
        thread_id: &str,
        turn_id: Option<&str>,
        item_id: Option<&str>,
        visibility: SourceEventVisibility,
        body: Value,
        captured_at_ms: u64,
    ) -> SourceEventRecord {
        let sequence = self.next_sequence;
        let event_id = live_source_event_id(&self.admission.turn_id, sequence, kind);
        SourceEventRecord {
            id: event_id.clone(),
            turn_id: self.admission.turn_id.clone(),
            sequence,
            captured_at_ms,
            source: source_metadata(
                &self.admission.runtime_target,
                Some(thread_id),
                turn_id,
                item_id,
                Some(event_id.as_str()),
            ),
            visibility,
            payload: SourceEventPayload {
                kind: kind.to_string(),
                body,
            },
        }
    }

    fn turn_record(
        &self,
        status: TurnStatus,
        thread_id: Option<&str>,
        turn_id: Option<&str>,
        started_at_ms: Option<u64>,
        completed_at_ms: Option<u64>,
        terminal_error: Option<TerminalError>,
        projection_revision: ProviderRevision,
    ) -> Result<TurnRecord, SyndicIngestionError> {
        let existing = load_turn(&self.store, &self.admission.turn_id)?.ok_or_else(|| {
            SyndicIngestionError::MissingTurn {
                id: self.admission.turn_id.clone(),
            }
        })?;
        Ok(TurnRecord {
            id: existing.id,
            conversation_id: existing.conversation_id,
            view_id: existing.view_id,
            parent_turn_id: existing.parent_turn_id,
            kind: existing.kind,
            status,
            source: Some(source_metadata(
                &self.admission.runtime_target,
                thread_id,
                turn_id,
                None,
                Some("turn-state"),
            )),
            created_at_ms: existing.created_at_ms,
            started_at_ms: started_at_ms.or(existing.started_at_ms),
            completed_at_ms: completed_at_ms.or(existing.completed_at_ms),
            terminal_error,
            projection_revision,
        })
    }
}

impl SyndicLiveTurnIngestor {
    fn ingest_metadata_event(
        &mut self,
        kind: &str,
        thread_id: &str,
        turn_id: Option<&str>,
        item_id: Option<&str>,
        body: Value,
    ) -> Result<(), SyndicIngestionError> {
        let now = token_usage_snapshot::current_unix_millis();
        let source_event = self.source_event(
            kind,
            thread_id,
            turn_id,
            item_id,
            SourceEventVisibility::Operational,
            body.clone(),
            now,
        );
        let item = CanonicalItemRecord {
            id: ItemId::from(format!(
                "item:{}:metadata:{}",
                self.admission.turn_id, source_event.sequence
            )),
            turn_id: self.admission.turn_id.clone(),
            source_event_id: source_event.id.clone(),
            kind: CanonicalItemKind::ProviderMetadata,
            visibility: CanonicalItemVisibility::CanonicalOnly,
            source: Some(source_event.source.clone()),
            payload: json!({ "kind": kind, "body": body }),
        };
        self.commit_event_batch(
            SyndicWriteBatch::new()
                .put_source_event(source_event)
                .put_item(item),
        )
    }

    fn add_turn_items_to_batch(
        &mut self,
        mut batch: SyndicWriteBatch,
        thread_id: &str,
        turn_id: &str,
        items: &[ThreadItem],
        complete: bool,
    ) -> Result<SyndicWriteBatch, SyndicIngestionError> {
        for item in items {
            batch = self.add_item_to_batch(batch, thread_id, turn_id, item, complete)?;
        }
        Ok(batch)
    }

    fn add_item_to_batch(
        &mut self,
        batch: SyndicWriteBatch,
        thread_id: &str,
        turn_id: &str,
        item: &ThreadItem,
        complete: bool,
    ) -> Result<SyndicWriteBatch, SyndicIngestionError> {
        match item {
            ThreadItem::AgentMessage(message) => {
                self.add_agent_message_to_batch(batch, thread_id, turn_id, message, complete)
            }
            ThreadItem::ImageGeneration(image) => {
                self.add_image_generation_to_batch(batch, thread_id, turn_id, image)
            }
            _ => Ok(batch.put_item(CanonicalItemRecord {
                id: canonical_item_id(&self.admission.turn_id, item.id()),
                turn_id: self.admission.turn_id.clone(),
                source_event_id: latest_source_event_id(
                    &self.admission.turn_id,
                    self.next_sequence,
                ),
                kind: CanonicalItemKind::Operational,
                visibility: CanonicalItemVisibility::CanonicalOnly,
                source: Some(source_metadata(
                    &self.admission.runtime_target,
                    Some(thread_id),
                    Some(turn_id),
                    Some(item.id()),
                    Some("item-canonical"),
                )),
                payload: sanitized_thread_item_payload(item),
            })),
        }
    }

    fn add_agent_message_to_batch(
        &mut self,
        mut batch: SyndicWriteBatch,
        thread_id: &str,
        turn_id: &str,
        message: &AgentMessageItem,
        complete: bool,
    ) -> Result<SyndicWriteBatch, SyndicIngestionError> {
        let revision = self.store.current_revision(&self.admission.view_id)?.next();
        let canonical_item_id = canonical_item_id(&self.admission.turn_id, &message.id);
        let projection_id = ProjectionRecordId::from(format!("projection:{canonical_item_id}"));
        let position = self
            .agent_messages
            .get(&message.id)
            .map(|state| state.position)
            .unwrap_or_else(|| TranscriptViewPosition(self.next_position));
        let source_event_id = latest_source_event_id(&self.admission.turn_id, self.next_sequence);
        let is_new_projection = !self.agent_messages.contains_key(&message.id);
        self.agent_messages.insert(
            message.id.clone(),
            AgentMessageProjectionState {
                position,
                text: message.text.clone(),
                phase: message.phase,
            },
        );
        batch = batch
            .put_item(CanonicalItemRecord {
                id: canonical_item_id.clone(),
                turn_id: self.admission.turn_id.clone(),
                source_event_id: source_event_id.clone(),
                kind: CanonicalItemKind::AssistantMessage,
                visibility: CanonicalItemVisibility::Transcript,
                source: Some(source_metadata(
                    &self.admission.runtime_target,
                    Some(thread_id),
                    Some(turn_id),
                    Some(&message.id),
                    Some("agent-message"),
                )),
                payload: json!({
                    "id": message.id,
                    "text": message.text,
                    "phase": message.phase,
                    "complete": complete,
                }),
            })
            .put_projection(self.agent_projection_record(
                &canonical_item_id,
                &projection_id,
                position,
                source_event_id.clone(),
                revision,
                message.text.clone(),
                message.phase,
            ));
        if is_new_projection {
            batch = batch.put_view_record(self.agent_view_record(
                &canonical_item_id,
                &projection_id,
                position,
                source_event_id,
                message.phase,
                &message.text,
            ));
            self.next_position = self.next_position.saturating_add(1);
        }
        Ok(batch)
    }

    fn add_image_generation_to_batch(
        &mut self,
        batch: SyndicWriteBatch,
        thread_id: &str,
        turn_id: &str,
        image: &ImageGenerationItem,
    ) -> Result<SyndicWriteBatch, SyndicIngestionError> {
        let source_event_id = latest_source_event_id(&self.admission.turn_id, self.next_sequence);
        let item_id = canonical_item_id(&self.admission.turn_id, &image.id);
        let resource_id = syndic_storage::ResourceId::from(format!("resource:{item_id}"));
        Ok(batch
            .put_item(CanonicalItemRecord {
                id: item_id,
                turn_id: self.admission.turn_id.clone(),
                source_event_id,
                kind: CanonicalItemKind::GeneratedMedia,
                visibility: CanonicalItemVisibility::CanonicalOnly,
                source: Some(source_metadata(
                    &self.admission.runtime_target,
                    Some(thread_id),
                    Some(turn_id),
                    Some(&image.id),
                    Some("image-generation"),
                )),
                payload: json!({
                    "id": image.id,
                    "status": image.status,
                    "revisedPrompt": image.revised_prompt,
                    "savedPath": image.saved_path,
                    "resultPresent": image.result.as_ref().is_some_and(|value| !value.is_empty()),
                }),
            })
            .put_resource(ResourceRecord {
                metadata: ResourceMetadataRecord {
                    id: resource_id,
                    revision: self.store.current_revision(&self.admission.view_id)?.next(),
                    kind: ResourceKind::GeneratedImage,
                    state: ResourceState::Missing {
                        reason: HistoryIncompleteReason::ResourceMissing,
                        detail: Some(
                            "generated image payload was not ingested through a bounded resource API"
                                .to_string(),
                        ),
                    },
                    media_type: None,
                    byte_len: 0,
                    digest: None,
                    line_count: None,
                    row_count: None,
                    column_count: None,
                    preview_range: None,
                },
                bytes: Vec::new(),
            }))
    }
}

impl SyndicLiveTurnIngestor {
    fn ingest_turn_started(
        &mut self,
        thread_id: &str,
        turn: &TurnInfo,
    ) -> Result<(), SyndicIngestionError> {
        if self.saw_turn_started && self.cas_turn_id.as_deref() == Some(turn.id.as_str()) {
            return Ok(());
        }
        self.cas_thread_id = Some(thread_id.to_string());
        self.cas_turn_id = Some(turn.id.clone());
        self.saw_turn_started = true;
        let now = token_usage_snapshot::current_unix_millis();
        let event = self.source_event(
            "turnStarted",
            thread_id,
            Some(&turn.id),
            None,
            SourceEventVisibility::CanonicalOnly,
            sanitized_turn_payload(turn),
            now,
        );
        let mut batch = SyndicWriteBatch::new().put_source_event(event);
        let revision = self.store.current_revision(&self.admission.view_id)?;
        batch = batch
            .put_turn(self.turn_record(
                TurnStatus::Running,
                Some(thread_id),
                Some(&turn.id),
                Some(now),
                None,
                None,
                revision,
            )?)
            .put_cas_projection_binding(CasProjectionBindingRecord {
                id: self.admission.binding_id.clone(),
                view_id: self.admission.view_id.clone(),
                binding_revision: 2,
                selected_path_revision: revision,
                selected_path_digest: Some(format!("active:{}", self.admission.turn_id)),
                established_at_ms: now,
                status: CasProjectionBindingStatus::Active {
                    runtime_target: self.admission.runtime_target.clone(),
                    cas_thread_id: thread_id.to_string(),
                    cas_turn_id: Some(turn.id.clone()),
                    execution_snapshot_id: format!("snapshot:{}", self.admission.turn_id),
                    accepted_input_id: self.admission.first_user_item_id.clone().unwrap_or_else(
                        || ItemId::from(format!("item:{}", self.admission.turn_id)),
                    ),
                    started_at_ms: now,
                    lineage_proof: format!("live-turn-start:{}", self.admission.turn_id),
                },
            });
        batch = self.add_turn_items_to_batch(batch, thread_id, &turn.id, &turn.items, false)?;
        self.commit_event_batch(batch)
    }

    fn ingest_turn_completed(
        &mut self,
        thread_id: &str,
        turn: &TurnInfo,
    ) -> Result<(), SyndicIngestionError> {
        if self.saw_terminal_turn && self.cas_turn_id.as_deref() == Some(turn.id.as_str()) {
            return Ok(());
        }
        self.cas_thread_id = Some(thread_id.to_string());
        self.cas_turn_id = Some(turn.id.clone());
        self.saw_terminal_turn = true;
        let now = token_usage_snapshot::current_unix_millis();
        let event = self.source_event(
            "turnCompleted",
            thread_id,
            Some(&turn.id),
            None,
            SourceEventVisibility::CanonicalOnly,
            sanitized_turn_payload(turn),
            now,
        );
        let mut batch = SyndicWriteBatch::new().put_source_event(event);
        batch = self.add_turn_items_to_batch(batch, thread_id, &turn.id, &turn.items, true)?;
        let revision = self.store.current_revision(&self.admission.view_id)?;
        batch = batch
            .put_turn(self.turn_record(
                turn_status_from_backend(turn.status),
                Some(thread_id),
                Some(&turn.id),
                None,
                Some(now),
                turn.error.as_ref().map(terminal_error),
                revision,
            )?)
            .clear_recovery_marker(self.admission.recovery_marker_id.clone())
            .put_cas_projection_binding(CasProjectionBindingRecord {
                id: self.admission.binding_id.clone(),
                view_id: self.admission.view_id.clone(),
                binding_revision: 3,
                selected_path_revision: revision,
                selected_path_digest: Some(format!("terminal:{}", self.admission.turn_id)),
                established_at_ms: now,
                status: CasProjectionBindingStatus::Valid {
                    runtime_target: self.admission.runtime_target.clone(),
                    cas_thread_id: thread_id.to_string(),
                    lineage_proof: format!("terminal-live-turn:{}", self.admission.turn_id),
                },
            });
        self.commit_event_batch(batch)
    }

    fn ingest_item_event(
        &mut self,
        kind: &str,
        thread_id: &str,
        turn_id: &str,
        item: &ThreadItem,
        complete: bool,
    ) -> Result<(), SyndicIngestionError> {
        let now = token_usage_snapshot::current_unix_millis();
        let event = self.source_event(
            kind,
            thread_id,
            Some(turn_id),
            Some(item.id()),
            item_visibility(item),
            json!({ "item": sanitized_thread_item_payload(item) }),
            now,
        );
        let batch = SyndicWriteBatch::new().put_source_event(event);
        let batch = self.add_item_to_batch(batch, thread_id, turn_id, item, complete)?;
        self.commit_event_batch(batch)
    }

    fn ingest_agent_message_delta(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        delta: &str,
    ) -> Result<(), SyndicIngestionError> {
        let now = token_usage_snapshot::current_unix_millis();
        let source_event = self.source_event(
            "agentMessageDelta",
            thread_id,
            Some(turn_id),
            Some(item_id),
            SourceEventVisibility::TranscriptVisible,
            json!({ "itemId": item_id, "delta": delta }),
            now,
        );
        let revision = self.store.current_revision(&self.admission.view_id)?.next();
        let canonical_item_id = canonical_item_id(&self.admission.turn_id, item_id);
        let projection_id = ProjectionRecordId::from(format!("projection:{canonical_item_id}"));
        let position = self
            .agent_messages
            .get(item_id)
            .map(|state| state.position)
            .unwrap_or_else(|| TranscriptViewPosition(self.next_position));
        let text = self
            .agent_messages
            .get(item_id)
            .map(|state| state.text.clone())
            .unwrap_or_default()
            + delta;
        let state = AgentMessageProjectionState {
            position,
            text: text.clone(),
            phase: self
                .agent_messages
                .get(item_id)
                .and_then(|state| state.phase),
        };
        let is_new_projection = self
            .agent_messages
            .insert(item_id.to_string(), state)
            .is_none();
        let mut batch = SyndicWriteBatch::new()
            .put_source_event(source_event.clone())
            .put_item(CanonicalItemRecord {
                id: canonical_item_id.clone(),
                turn_id: self.admission.turn_id.clone(),
                source_event_id: source_event.id.clone(),
                kind: CanonicalItemKind::AssistantMessage,
                visibility: CanonicalItemVisibility::Transcript,
                source: Some(source_metadata(
                    &self.admission.runtime_target,
                    Some(thread_id),
                    Some(turn_id),
                    Some(item_id),
                    Some(source_event.id.as_str()),
                )),
                payload: json!({
                    "id": item_id,
                    "text": text.as_str(),
                    "complete": false,
                }),
            })
            .put_projection(self.agent_projection_record(
                &canonical_item_id,
                &projection_id,
                position,
                source_event.id.clone(),
                revision,
                text.clone(),
                None,
            ));
        if is_new_projection {
            batch = batch.put_view_record(self.agent_view_record(
                &canonical_item_id,
                &projection_id,
                position,
                source_event.id.clone(),
                None,
                text.as_str(),
            ));
            self.next_position = self.next_position.saturating_add(1);
        }
        self.commit_event_batch(batch)
    }
}
