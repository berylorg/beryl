use std::{fs, path::PathBuf};

use beryl_backend::{ThreadSessionMetadata, ThreadStatus, ThreadSummary};
use beryl_model::workspace::WorkspaceId;

use super::super::{
    syndic_transcript::{
        PreparedTranscriptActivation, ProjectionRecordId, ProjectionRecordsRequest,
        ProviderRequestId, ProviderRevision, ResidentTranscriptPolicy, SyndicTranscriptProvider,
        TranscriptActivationPlacement, TranscriptPageAnchor, TranscriptPageDirection,
        TranscriptProviderHistoryReason, TranscriptProviderHistoryState, TranscriptProviderRequest,
        TranscriptProviderRequestKind, TranscriptProviderResponseKind, TranscriptViewId,
        TranscriptViewPage, TranscriptViewPageRequest,
    },
    syndic_transcript_storage_provider::StorageSyndicTranscriptProvider,
};
use super::SelectedThreadActivationSource;

pub(in crate::shell) struct ActivationPreparer;

#[derive(Clone)]
pub(in crate::shell) struct StagedSelectedThreadActivation {
    pub(in crate::shell) execution_target: WorkspaceId,
    pub(in crate::shell) summary: ThreadSummary,
    pub(in crate::shell) status: ThreadStatus,
    pub(in crate::shell) session_metadata: Option<ThreadSessionMetadata>,
    pub(in crate::shell) source: SelectedThreadActivationSource,
    pub(in crate::shell) prepared_transcript: PreparedTranscriptActivation,
}

impl ActivationPreparer {
    pub(in crate::shell) fn prepare(
        execution_target: WorkspaceId,
        summary: ThreadSummary,
        status: ThreadStatus,
        session_metadata: Option<ThreadSessionMetadata>,
        source: SelectedThreadActivationSource,
        prepared_transcript: PreparedTranscriptActivation,
    ) -> StagedSelectedThreadActivation {
        StagedSelectedThreadActivation {
            execution_target,
            summary,
            status,
            session_metadata,
            source,
            prepared_transcript,
        }
    }
}

pub(in crate::shell) fn prepare_storage_backed_transcript_activation(
    storage_dir: PathBuf,
    view_id: &str,
) -> PreparedTranscriptActivation {
    let view_id = TranscriptViewId(view_id.to_string());
    let placement = TranscriptActivationPlacement::Tail;
    if let Err(error) = fs::create_dir_all(&storage_dir) {
        return unavailable_activation(
            view_id,
            placement,
            format!(
                "Syndic transcript storage directory could not be prepared at {}: {error}",
                storage_dir.display()
            ),
        );
    }
    let mut provider = match StorageSyndicTranscriptProvider::open(&storage_dir) {
        Ok(provider) => provider,
        Err(error) => {
            return unavailable_activation(
                view_id,
                placement,
                format!("Syndic transcript storage unavailable: {error:?}"),
            );
        }
    };

    let policy = ResidentTranscriptPolicy::default();
    let view_request = TranscriptProviderRequest {
        id: ProviderRequestId(0),
        kind: TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
            view_id: view_id.clone(),
            anchor: TranscriptPageAnchor::End,
            direction: TranscriptPageDirection::Backward,
            limit: policy.view_page_limit.min(policy.max_resident_view_records),
            observed_revision: None,
        }),
    };

    let view_response = match provider.handle_request(view_request) {
        Ok(response) => response.kind,
        Err(error) => {
            return unavailable_activation(
                view_id,
                placement,
                format!("Syndic transcript provider failed: {error:?}"),
            );
        }
    };

    let projection_records_response = projection_request_for_view_response(&view_response)
        .and_then(|request| {
            provider
                .handle_request(request)
                .ok()
                .map(|response| response.kind)
        });

    PreparedTranscriptActivation::new(
        view_id,
        placement,
        view_response,
        projection_records_response,
    )
}

fn projection_request_for_view_response(
    response: &TranscriptProviderResponseKind,
) -> Option<TranscriptProviderRequest> {
    let TranscriptProviderResponseKind::ViewPage(page) = response else {
        return None;
    };
    let projection_ids = projection_ids_for_page(page);
    (!projection_ids.is_empty()).then(|| TranscriptProviderRequest {
        id: ProviderRequestId(1),
        kind: TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
            view_id: page.view_id.clone(),
            projection_ids,
            observed_revision: Some(page.revision),
        }),
    })
}

fn projection_ids_for_page(page: &TranscriptViewPage) -> Vec<ProjectionRecordId> {
    let mut projection_ids = Vec::new();
    for record in &page.records {
        if projection_ids
            .iter()
            .any(|projection_id| projection_id == &record.projection_id)
        {
            continue;
        }
        projection_ids.push(record.projection_id.clone());
    }
    projection_ids
}

fn unavailable_activation(
    view_id: TranscriptViewId,
    placement: TranscriptActivationPlacement,
    detail: String,
) -> PreparedTranscriptActivation {
    PreparedTranscriptActivation::new(
        view_id.clone(),
        placement,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id,
            revision: ProviderRevision::default(),
            history_state: TranscriptProviderHistoryState::Unavailable {
                reason: TranscriptProviderHistoryReason::StorageFailure,
                detail: Some(detail),
            },
            records: Vec::new(),
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        None,
    )
}

impl StagedSelectedThreadActivation {
    pub(in crate::shell) fn is_ready_for_publication(&self) -> bool {
        true
    }

    pub(in crate::shell) fn progress_cap(&self) -> f32 {
        super::PENDING_THREAD_ACTIVATION_PUBLICATION_PROGRESS_CAP
    }
}
