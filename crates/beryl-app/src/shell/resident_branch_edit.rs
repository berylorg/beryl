use std::{
    collections::HashSet,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_backend::UserInput;
use syndic_storage::{
    CasProjectionBindingId, CasProjectionBindingRecord, CasProjectionBindingStatus, ConversationId,
    ConversationRecord, HistoryState, ProjectionRecordId, SourceEventRecord, StoreOpenOptions,
    SyndicSourceProvenance, SyndicStore, SyndicWriteBatch, ThreadViewId, TranscriptPageAnchor,
    TranscriptPageDirection, TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
    TurnId, TurnKind,
};

use super::syndic_transcript::{
    ResidentActionTargetProvenance, ResidentBranchActionTarget, ResidentEditActionTarget,
    SyndicTurnId,
};

const TARGET_VIEW_PAGE_LIMIT: usize = 1_024;
const TARGET_MAX_VIEW_RECORDS: usize = 8_192;
const TARGET_SOURCE_EVENT_LIMIT: usize = 1_024;
const TARGET_MAX_SOURCE_EVENTS: usize = 8_192;
const CAS_PROVIDER: &str = "codex-app-server";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResidentBranchProof {
    pub(super) source_view_id: ThreadViewId,
    pub(super) target_turn_id: TurnId,
    pub(super) source_thread_id: String,
    pub(super) source_turn_id: String,
    pub(super) rollback_turns_after_target: u32,
    pub(super) title_seed: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResidentEditProof {
    pub(super) source_thread_id: String,
    pub(super) source_turn_id: String,
    pub(super) rollback_turns_including_target: u32,
    pub(super) display_text: String,
    pub(super) backend_input: Vec<UserInput>,
    pub(super) detached_view_records: Vec<ResidentDetachedViewRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResidentDetachedViewRecord {
    pub(super) view_id: ThreadViewId,
    pub(super) position: TranscriptViewPosition,
    pub(super) id: TranscriptViewRecordId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResidentBranchMaterialization {
    pub(super) conversation_id: ConversationId,
    pub(super) view_id: ThreadViewId,
    pub(super) copied_view_records: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ResidentBranchEditProofError {
    StorageUnavailable(String),
    MissingTargetTurn,
    MissingConversation,
    IncompleteHistory(String),
    MissingCasThread,
    MissingCasTurn,
    TargetNotInSelectedView,
    TooManyTurns,
    NoEditableUserInput,
    MalformedAcceptedInput(String),
}

pub(super) fn prove_resident_branch_target(
    storage_dir: &Path,
    target: &ResidentBranchActionTarget,
) -> Result<ResidentBranchProof, ResidentBranchEditProofError> {
    let context = resolve_target_context(storage_dir, &target.provenance)?;
    let user_input = accepted_user_input_for_turn(&context.store, &context.target_turn_id)?;
    let Some(target_index) = context
        .selected_user_turns
        .iter()
        .position(|turn_id| turn_id == &context.target_turn_id)
    else {
        return Err(ResidentBranchEditProofError::TargetNotInSelectedView);
    };
    let rollback_count = context
        .selected_user_turns
        .len()
        .saturating_sub(target_index.saturating_add(1));

    Ok(ResidentBranchProof {
        source_view_id: context.source_view_id,
        target_turn_id: context.target_turn_id,
        source_thread_id: context.source_thread_id,
        source_turn_id: context.source_turn_id,
        rollback_turns_after_target: u32::try_from(rollback_count)
            .map_err(|_| ResidentBranchEditProofError::TooManyTurns)?,
        title_seed: user_input.display_text,
    })
}

pub(super) fn prove_resident_edit_target(
    storage_dir: &Path,
    target: &ResidentEditActionTarget,
) -> Result<ResidentEditProof, ResidentBranchEditProofError> {
    let context = resolve_target_context(storage_dir, &target.provenance)?;
    let user_input = accepted_user_input_for_turn(&context.store, &context.target_turn_id)?;
    let Some(target_index) = context
        .selected_user_turns
        .iter()
        .position(|turn_id| turn_id == &context.target_turn_id)
    else {
        return Err(ResidentBranchEditProofError::TargetNotInSelectedView);
    };
    let rollback_count = context
        .selected_user_turns
        .len()
        .saturating_sub(target_index);
    let detached_view_records =
        detached_view_records_for_target(&context.selected_view_records, &context.target_turn_id)?;

    Ok(ResidentEditProof {
        source_thread_id: context.source_thread_id,
        source_turn_id: context.source_turn_id,
        rollback_turns_including_target: u32::try_from(rollback_count)
            .map_err(|_| ResidentBranchEditProofError::TooManyTurns)?,
        display_text: user_input.display_text,
        backend_input: user_input.backend_input,
        detached_view_records,
    })
}

pub(super) fn detach_resident_edit_tail(
    storage_dir: &Path,
    proof: &ResidentEditProof,
) -> Result<(), ResidentBranchEditProofError> {
    let first_record = proof
        .detached_view_records
        .first()
        .ok_or(ResidentBranchEditProofError::TargetNotInSelectedView)?;
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default())
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;
    let mut conversation = store
        .conversation_by_view(&first_record.view_id)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
        .ok_or(ResidentBranchEditProofError::MissingConversation)?;
    let next_revision = conversation.current_revision.next();
    let now = current_unix_millis();
    conversation.current_revision = next_revision;
    conversation.updated_at_ms = now;

    let mut batch = SyndicWriteBatch::new().put_conversation(conversation);
    for record in &proof.detached_view_records {
        if record.view_id != first_record.view_id {
            return Err(ResidentBranchEditProofError::StorageUnavailable(
                "resident edit proof contained view records from more than one view".to_string(),
            ));
        }
        batch =
            batch.remove_view_record(record.view_id.clone(), record.position, record.id.clone());
    }
    batch = batch.put_cas_projection_binding(CasProjectionBindingRecord {
        id: CasProjectionBindingId::from(format!("binding:{}", first_record.view_id)),
        view_id: first_record.view_id.clone(),
        binding_revision: next_revision.0,
        selected_path_revision: next_revision,
        selected_path_digest: Some(format!("edit-detached:{}", proof.source_turn_id)),
        established_at_ms: now,
        status: CasProjectionBindingStatus::Stale {
            old_cas_thread_id: Some(proof.source_thread_id.clone()),
            reason: "selected transcript tail detached by replacement edit".to_string(),
        },
    });
    store
        .commit(batch)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;
    Ok(())
}

pub(super) fn materialize_resident_branch_prefix(
    storage_dir: &Path,
    workspace_id: &str,
    proof: &ResidentBranchProof,
    runtime_target: &str,
    branch_thread_id: &str,
    title: Option<&str>,
) -> Result<ResidentBranchMaterialization, ResidentBranchEditProofError> {
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default())
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;
    if store
        .conversation_by_external_thread(CAS_PROVIDER, Some(runtime_target), branch_thread_id)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
        .is_some()
    {
        return Err(ResidentBranchEditProofError::StorageUnavailable(format!(
            "Syndic already has a conversation bound to CAS branch thread {branch_thread_id}"
        )));
    }

    let source_conversation = store
        .conversation_by_view(&proof.source_view_id)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
        .ok_or(ResidentBranchEditProofError::MissingConversation)?;
    match &source_conversation.history_state {
        HistoryState::Complete => {}
        HistoryState::Incomplete { reason, detail }
        | HistoryState::Unavailable { reason, detail } => {
            return Err(ResidentBranchEditProofError::IncompleteHistory(
                history_state_detail(reason, detail.as_deref()),
            ));
        }
    }

    let view_scan = selected_turns_for_complete_view(
        &store,
        &proof.source_view_id,
        source_conversation.current_revision,
    )?;
    let prefix_records = prefix_view_records_for_target(&view_scan.records, &proof.target_turn_id)?;
    let now = current_unix_millis();
    let conversation_id = ConversationId::from(format!(
        "conversation:{workspace_id}:cas:{branch_thread_id}"
    ));
    let branch_view_id = ThreadViewId::from(branch_thread_id.to_string());
    let revision = syndic_storage::ProviderRevision(1);
    let source = Some(syndic_storage::ExternalSourceMetadata {
        provider: CAS_PROVIDER.to_string(),
        runtime_target: Some(runtime_target.to_string()),
        external_thread_id: Some(branch_thread_id.to_string()),
        external_turn_id: None,
        external_item_id: None,
        external_event_id: Some("resident-branch-prefix".to_string()),
    });
    let mut batch = SyndicWriteBatch::new().put_conversation(ConversationRecord {
        id: conversation_id.clone(),
        view_id: branch_view_id.clone(),
        title: title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string),
        created_at_ms: now,
        updated_at_ms: now,
        current_revision: revision,
        source,
        history_state: HistoryState::Complete,
    });

    for (index, record) in prefix_records.iter().enumerate() {
        let position = TranscriptViewPosition(index as u64);
        let projection = store
            .projection(&record.projection_id)
            .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
            .ok_or_else(|| {
                ResidentBranchEditProofError::StorageUnavailable(format!(
                    "Syndic projection {} is missing for resident branch prefix",
                    record.projection_id
                ))
            })?;
        let projection_id = ProjectionRecordId::from(format!(
            "branch-projection:{branch_thread_id}:{}",
            projection.id
        ));
        let view_record_id = TranscriptViewRecordId::from(format!(
            "branch-view-record:{branch_thread_id}:{}",
            record.id
        ));
        let provenance = branch_provenance(
            projection.provenance,
            &branch_view_id,
            position,
            &projection_id,
        );
        batch = batch
            .put_projection(syndic_storage::ProjectionRecord {
                id: projection_id.clone(),
                view_id: branch_view_id.clone(),
                turn_id: projection.turn_id,
                item_id: projection.item_id,
                revision,
                kind: projection.kind,
                status: projection.status,
                payload: projection.payload,
                provenance: provenance.clone(),
            })
            .put_view_record(TranscriptViewRecord {
                id: view_record_id,
                view_id: branch_view_id.clone(),
                position,
                projection_id,
                narrative_kind: record.narrative_kind.clone(),
                provenance,
            });
    }

    batch = batch.put_cas_projection_binding(CasProjectionBindingRecord {
        id: CasProjectionBindingId::from(format!("binding:{branch_view_id}")),
        view_id: branch_view_id.clone(),
        binding_revision: 1,
        selected_path_revision: revision,
        selected_path_digest: Some(format!(
            "resident-branch-prefix:{}:{}",
            proof.source_turn_id,
            prefix_records.len()
        )),
        established_at_ms: now,
        status: CasProjectionBindingStatus::Valid {
            runtime_target: runtime_target.to_string(),
            cas_thread_id: branch_thread_id.to_string(),
            lineage_proof: format!(
                "resident-branch:{}:{}",
                proof.source_thread_id, proof.source_turn_id
            ),
        },
    });
    store
        .commit(batch)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;
    Ok(ResidentBranchMaterialization {
        conversation_id,
        view_id: branch_view_id,
        copied_view_records: prefix_records.len(),
    })
}

struct TargetContext {
    store: SyndicStore,
    target_turn_id: TurnId,
    source_view_id: ThreadViewId,
    source_thread_id: String,
    source_turn_id: String,
    selected_user_turns: Vec<TurnId>,
    selected_view_records: Vec<TranscriptViewRecord>,
}

fn resolve_target_context(
    storage_dir: &Path,
    provenance: &ResidentActionTargetProvenance,
) -> Result<TargetContext, ResidentBranchEditProofError> {
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default())
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;
    let view_id = ThreadViewId::from(provenance.source.view_id.0.clone());
    let target_turn_id = storage_turn_id(provenance.source.turn_id.as_ref())?;
    let conversation = store
        .conversation_by_view(&view_id)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
        .ok_or(ResidentBranchEditProofError::MissingConversation)?;
    match &conversation.history_state {
        HistoryState::Complete => {}
        HistoryState::Incomplete { reason, detail } => {
            return Err(ResidentBranchEditProofError::IncompleteHistory(
                history_state_detail(reason, detail.as_deref()),
            ));
        }
        HistoryState::Unavailable { reason, detail } => {
            return Err(ResidentBranchEditProofError::IncompleteHistory(
                history_state_detail(reason, detail.as_deref()),
            ));
        }
    }

    let target_turn = store
        .turn(&target_turn_id)
        .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
        .ok_or(ResidentBranchEditProofError::MissingTargetTurn)?;
    let source = target_turn.source.as_ref();
    let source_thread_id = source
        .and_then(|source| source.external_thread_id.clone())
        .or_else(|| {
            conversation
                .source
                .as_ref()
                .and_then(|source| source.external_thread_id.clone())
        })
        .ok_or(ResidentBranchEditProofError::MissingCasThread)?;
    let source_turn_id = source
        .and_then(|source| source.external_turn_id.clone())
        .ok_or(ResidentBranchEditProofError::MissingCasTurn)?;
    let view_scan =
        selected_turns_for_complete_view(&store, &view_id, conversation.current_revision)?;

    Ok(TargetContext {
        store,
        target_turn_id,
        source_view_id: view_id,
        source_thread_id,
        source_turn_id,
        selected_user_turns: view_scan.user_turns,
        selected_view_records: view_scan.records,
    })
}

struct CompleteViewScan {
    records: Vec<TranscriptViewRecord>,
    user_turns: Vec<TurnId>,
}

fn selected_turns_for_complete_view(
    store: &SyndicStore,
    view_id: &ThreadViewId,
    revision: syndic_storage::ProviderRevision,
) -> Result<CompleteViewScan, ResidentBranchEditProofError> {
    let mut anchor = TranscriptPageAnchor::Start;
    let mut scanned_records = 0usize;
    let mut view_records = Vec::new();
    let mut turn_order = Vec::new();
    let mut seen_turns = HashSet::new();

    loop {
        let page = store
            .read_transcript_page(
                view_id,
                anchor,
                TranscriptPageDirection::Forward,
                TARGET_VIEW_PAGE_LIMIT,
                Some(revision),
            )
            .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;

        scanned_records = scanned_records.saturating_add(page.records.len());
        if scanned_records > TARGET_MAX_VIEW_RECORDS {
            return Err(ResidentBranchEditProofError::TooManyTurns);
        }

        for record in page.records {
            let Some(turn_id) = record.provenance.turn_id.as_ref() else {
                view_records.push(record);
                continue;
            };
            if seen_turns.insert(turn_id.clone()) {
                turn_order.push(turn_id.clone());
            }
            view_records.push(record);
        }

        if page.at_end {
            break;
        }
        anchor = TranscriptPageAnchor::Cursor(page.next_cursor.ok_or_else(|| {
            ResidentBranchEditProofError::StorageUnavailable(
                "Syndic transcript page did not provide a continuation cursor.".to_string(),
            )
        })?);
    }

    let mut user_turns = Vec::new();
    for turn_id in turn_order {
        let turn = store
            .turn(&turn_id)
            .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?
            .ok_or(ResidentBranchEditProofError::MissingTargetTurn)?;
        if matches!(turn.kind, TurnKind::User) {
            user_turns.push(turn_id);
        }
    }

    Ok(CompleteViewScan {
        records: view_records,
        user_turns,
    })
}

fn detached_view_records_for_target(
    records: &[TranscriptViewRecord],
    target_turn_id: &TurnId,
) -> Result<Vec<ResidentDetachedViewRecord>, ResidentBranchEditProofError> {
    let Some(target_index) = records.iter().position(|record| {
        record
            .provenance
            .turn_id
            .as_ref()
            .is_some_and(|turn_id| turn_id == target_turn_id)
    }) else {
        return Err(ResidentBranchEditProofError::TargetNotInSelectedView);
    };

    Ok(records[target_index..]
        .iter()
        .map(|record| ResidentDetachedViewRecord {
            view_id: record.view_id.clone(),
            position: record.position,
            id: record.id.clone(),
        })
        .collect())
}

fn prefix_view_records_for_target<'a>(
    records: &'a [TranscriptViewRecord],
    target_turn_id: &TurnId,
) -> Result<&'a [TranscriptViewRecord], ResidentBranchEditProofError> {
    let Some(target_index) = records.iter().rposition(|record| {
        record
            .provenance
            .turn_id
            .as_ref()
            .is_some_and(|turn_id| turn_id == target_turn_id)
    }) else {
        return Err(ResidentBranchEditProofError::TargetNotInSelectedView);
    };
    Ok(&records[..=target_index])
}

fn branch_provenance(
    mut provenance: SyndicSourceProvenance,
    view_id: &ThreadViewId,
    position: TranscriptViewPosition,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    provenance.view_id = view_id.clone();
    provenance.position = Some(position);
    provenance.projection_id = Some(projection_id.clone());
    provenance
}

struct AcceptedUserInput {
    display_text: String,
    backend_input: Vec<UserInput>,
}

fn accepted_user_input_for_turn(
    store: &SyndicStore,
    turn_id: &TurnId,
) -> Result<AcceptedUserInput, ResidentBranchEditProofError> {
    let mut start_sequence = 0u64;
    let mut scanned_events = 0usize;
    let mut display_text = String::new();
    let mut backend_input = Vec::new();

    loop {
        let page = store
            .read_source_events(turn_id, start_sequence, TARGET_SOURCE_EVENT_LIMIT)
            .map_err(|error| ResidentBranchEditProofError::StorageUnavailable(error.to_string()))?;

        scanned_events = scanned_events.saturating_add(page.records.len());
        if scanned_events > TARGET_MAX_SOURCE_EVENTS {
            return Err(ResidentBranchEditProofError::TooManyTurns);
        }

        for record in page.records {
            observe_accepted_user_input(&record, &mut display_text, &mut backend_input)?;
        }

        if page.at_end {
            break;
        }
        start_sequence = page.next_sequence.ok_or_else(|| {
            ResidentBranchEditProofError::StorageUnavailable(
                "Syndic source-event page did not provide a continuation sequence.".to_string(),
            )
        })?;
    }

    if backend_input.is_empty() || display_text.trim().is_empty() {
        return Err(ResidentBranchEditProofError::NoEditableUserInput);
    }

    Ok(AcceptedUserInput {
        display_text,
        backend_input,
    })
}

fn observe_accepted_user_input(
    record: &SourceEventRecord,
    display_text: &mut String,
    backend_input: &mut Vec<UserInput>,
) -> Result<(), ResidentBranchEditProofError> {
    if record.payload.kind != "acceptedUserInput" {
        return Ok(());
    }

    let Some(input_value) = record.payload.body.get("backendInput") else {
        return Ok(());
    };
    let records = serde_json::from_value::<Vec<UserInput>>(input_value.clone())
        .map_err(|error| ResidentBranchEditProofError::MalformedAcceptedInput(error.to_string()))?;
    if let Some(text) = record
        .payload
        .body
        .get("text")
        .and_then(|value| value.as_str())
    {
        display_text.push_str(text);
    } else {
        append_user_input_display_text(&records, display_text);
    }
    backend_input.extend(records);
    Ok(())
}

fn append_user_input_display_text(records: &[UserInput], display_text: &mut String) {
    for record in records {
        let UserInput::Text { text } = record else {
            continue;
        };
        display_text.push_str(text);
    }
}

fn storage_turn_id(id: Option<&SyndicTurnId>) -> Result<TurnId, ResidentBranchEditProofError> {
    id.map(|id| TurnId::from(id.0.clone()))
        .ok_or(ResidentBranchEditProofError::MissingTargetTurn)
}

fn history_state_detail(
    reason: &syndic_storage::HistoryIncompleteReason,
    detail: Option<&str>,
) -> String {
    detail
        .filter(|detail| !detail.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{reason:?}"))
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or_default()
}
