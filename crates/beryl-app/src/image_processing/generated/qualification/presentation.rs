use std::sync::{Arc, atomic::AtomicBool};

use beryl_home_store::{CommandCancellation, HomeServiceReference};
use beryl_model::{
    AssetId, ProjectionRevision, SyndicProjectionId, SyndicResourceId, SyndicThreadId, WindowId,
};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

use crate::{
    cas_projection::{
        ProjectionConnectionService, SelectedTranscriptConstructionError,
        SelectedTranscriptConstructionProgress,
    },
    image_processing::{
        ImageSourceDemand, ImageSourceError, ImageSourceProvider, MediaResourceConfiguration,
        MediaResourceLimits,
    },
    syndic_transcript::{
        ProjectionPayload, TranscriptActivationPlacement, TranscriptProviderResponseKind,
    },
    transcript_provider::{
        TranscriptAttachmentError, TranscriptAttachmentPurpose, TranscriptAttachmentRequest,
        TranscriptProviderReader,
    },
};

#[derive(Debug)]
pub struct GeneratedResolution {
    pub view_thread: SyndicThreadId,
    pub source_thread: SyndicThreadId,
    pub original: AssetId,
    pub rendition: Result<AssetId, String>,
    pub metadata_dimensions: Option<[u64; 2]>,
    pub presentation: GeneratedPresentation,
}

#[derive(Debug)]
pub struct GeneratedPresentation {
    pub view_thread: SyndicThreadId,
    pub source_thread: SyndicThreadId,
    pub projection_id: SyndicProjectionId,
    pub resource_id: SyndicResourceId,
    pub lifecycle: syndic_storage::GeneratedMediaResourceDisposition,
    pub has_authored_source: bool,
    pub same_authority: Option<bool>,
    pub same_presentation: Option<bool>,
    identity: crate::transcript_provider::TranscriptAttachmentSourceIdentity,
}

struct PreparedGenerated {
    presentation: GeneratedPresentation,
    demand: ImageSourceDemand,
}

enum AttemptError {
    Retry,
    Failed(String),
}
impl From<String> for AttemptError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}
impl From<&str> for AttemptError {
    fn from(error: &str) -> Self {
        Self::Failed(error.into())
    }
}

fn attachment_error(error: TranscriptAttachmentError) -> AttemptError {
    match error {
        TranscriptAttachmentError::Busy | TranscriptAttachmentError::Stale => AttemptError::Retry,
        error => AttemptError::Failed(error.to_string()),
    }
}

fn source_error(error: syndic_storage::SyndicReadError) -> AttemptError {
    match error {
        syndic_storage::SyndicReadError::ConcurrentChange { .. } => AttemptError::Retry,
        error => AttemptError::Failed(error.to_string()),
    }
}

pub fn resolve_generated_presentation(
    home: HomeServiceReference,
    storage: SyndicStorage,
    state: &BerylState,
    service: &ProjectionConnectionService,
    selected_thread: SyndicThreadId,
    resource: SyndicResourceId,
) -> Result<Option<GeneratedResolution>, String> {
    match resolve_attempt(home, storage, state, service, selected_thread, resource) {
        Ok(resolution) => Ok(Some(resolution)),
        Err(AttemptError::Retry) => Ok(None),
        Err(AttemptError::Failed(error)) => Err(error),
    }
}

pub fn inspect_generated_presentation(
    home: HomeServiceReference,
    storage: SyndicStorage,
    service: &ProjectionConnectionService,
    selected_thread: SyndicThreadId,
    resource: SyndicResourceId,
    previous: Option<&GeneratedPresentation>,
) -> Result<Option<GeneratedPresentation>, String> {
    inspect_generated_presentation_after_preparation(
        home,
        storage,
        service,
        selected_thread,
        resource,
        previous,
        || {},
    )
}

pub fn inspect_generated_presentation_after_preparation(
    home: HomeServiceReference,
    storage: SyndicStorage,
    service: &ProjectionConnectionService,
    selected_thread: SyndicThreadId,
    resource: SyndicResourceId,
    previous: Option<&GeneratedPresentation>,
    after_preparation: impl FnOnce(),
) -> Result<Option<GeneratedPresentation>, String> {
    match prepare_attempt(
        home,
        storage,
        service,
        selected_thread,
        resource,
        previous,
        after_preparation,
    ) {
        Ok(prepared) => Ok(Some(prepared.presentation)),
        Err(AttemptError::Retry) => Ok(None),
        Err(AttemptError::Failed(error)) => Err(error),
    }
}

fn resolve_attempt(
    home: HomeServiceReference,
    storage: SyndicStorage,
    state: &BerylState,
    service: &ProjectionConnectionService,
    selected_thread: SyndicThreadId,
    resource: SyndicResourceId,
) -> Result<GeneratedResolution, AttemptError> {
    let prepared = prepare_attempt(
        home.clone(),
        storage.clone(),
        service,
        selected_thread,
        resource,
        None,
        || {},
    )?;
    let limits = MediaResourceLimits::new(MediaResourceConfiguration::default())
        .map_err(|e| e.to_string())?;
    let provider = ImageSourceProvider::new(home, storage, state.assets(), limits)
        .map_err(|e| e.to_string())?;
    let cancellation = CommandCancellation::new();
    let original = provider
        .resolve_original(&prepared.demand, &cancellation)
        .map_err(|error| match error {
            ImageSourceError::Stale => AttemptError::Retry,
            error => AttemptError::Failed(error.to_string()),
        })?;
    let rendition = match provider.resolve(&prepared.demand, &cancellation) {
        Ok(source) => Ok(source.asset_id()),
        Err(ImageSourceError::Stale) => return Err(AttemptError::Retry),
        Err(error) => Err(error.to_string()),
    };
    Ok(GeneratedResolution {
        view_thread: prepared.presentation.view_thread,
        source_thread: prepared.presentation.source_thread,
        original: original.asset_id(),
        rendition,
        metadata_dimensions: original
            .metadata()
            .dimensions()
            .map(|d| [d.width().get(), d.height().get()]),
        presentation: prepared.presentation,
    })
}

fn prepare_attempt(
    home: HomeServiceReference,
    storage: SyndicStorage,
    service: &ProjectionConnectionService,
    selected_thread: SyndicThreadId,
    resource: SyndicResourceId,
    previous: Option<&GeneratedPresentation>,
    after_preparation: impl FnOnce(),
) -> Result<PreparedGenerated, AttemptError> {
    let cancelled = AtomicBool::new(false);
    let progress = service
        .selected_transcript_construction_authority()
        .step(selected_thread, &cancelled)
        .map_err(|error| match error {
            SelectedTranscriptConstructionError::Busy => AttemptError::Retry,
            SelectedTranscriptConstructionError::Read(error)
                if matches!(
                    &*error,
                    syndic_storage::SyndicReadError::ConcurrentChange { .. }
                ) =>
            {
                AttemptError::Retry
            }
            error => AttemptError::Failed(error.to_string()),
        })?;
    if progress != SelectedTranscriptConstructionProgress::Current {
        return Err(AttemptError::Retry);
    }
    let lifetime = Arc::new(());
    let host = Arc::new(());
    let reader = TranscriptProviderReader::new(
        Arc::new(home.clone()),
        storage.clone(),
        Arc::downgrade(&lifetime),
        service.service_generation(),
        service.home_mutation_observer(),
    )
    .map_err(attachment_error)?;
    let request = TranscriptAttachmentRequest {
        window_id: WindowId::from_bytes([241; 16]),
        host: Arc::downgrade(&host),
        activation: 1,
        thread_id: selected_thread,
        request_id: 1,
        placement: TranscriptActivationPlacement::Tail,
        purpose: TranscriptAttachmentPurpose::Attach,
    };
    let attachment = reader
        .prepare_attachment(request.clone(), &cancelled)
        .map_err(attachment_error)?;
    let same_authority = previous.map(|previous| previous.identity.matches_authority(&attachment));
    let same_presentation =
        previous.map(|previous| previous.identity.matches_presentation(&attachment));
    let identity = attachment.source_identity();
    after_preparation();
    let activation = reader
        .publish_if_current(attachment, &request, &cancelled, |activation| activation)
        .map_err(attachment_error)?;
    let Some(TranscriptProviderResponseKind::ViewPage(page)) = activation.view_page_response else {
        return Err("missing prepared transcript page".into());
    };
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response
    else {
        return Err("missing prepared projection records".into());
    };
    let projection = projections.records.iter().find(|projection| matches!(&projection.payload, ProjectionPayload::GeneratedImage(image) if image.resource_id == resource)).ok_or_else(|| {
        let entries: Vec<_> = page.records.iter().take(8).map(|entry| (&entry.projection_id, entry.position.0, &entry.narrative_kind, &entry.provenance.item_id)).collect();
        let projection_kinds: Vec<_> = projections.records.iter().take(8).map(|projection| (&projection.id, &projection.kind, match &projection.payload {
            ProjectionPayload::GeneratedImage(image) => format!("generated {:?}", image.resource_id),
            ProjectionPayload::Text { text, .. } => format!("text {} bytes", text.len()),
            _ => "other".into(),
        })).collect();
        AttemptError::Failed(format!("generated occurrence {resource:?} missing from prepared view {selected_thread:?}; entries={entries:?}; projections={projection_kinds:?}"))
    })?;
    let ProjectionPayload::GeneratedImage(image) = projection.payload else {
        unreachable!()
    };
    let entry = page
        .records
        .iter()
        .find(|entry| entry.projection_id == projection.id)
        .ok_or("missing exact prepared transcript entry")?;
    let source = &entry.provenance;
    let view_thread = source
        .view_id
        .0
        .parse()
        .map_err(|_| "invalid selected-view identity")?;
    let projection_id = projection
        .id
        .0
        .parse()
        .map_err(|_| "invalid projection identity")?;
    let demand = ImageSourceDemand::TranscriptGeneratedResource {
        thread_id: view_thread,
        entry: source
            .transcript_entry
            .ok_or("missing current transcript-entry proof")?,
        projection_id,
        projection_revision: ProjectionRevision::new(projection.revision.0)
            .map_err(|e| e.to_string())?,
        image,
    };
    let proof = storage
        .generated_media_source(&home, resource)
        .map_err(source_error)?
        .ok_or("missing original generated source proof")?;
    let lifecycle = match image.lifecycle {
        crate::syndic_transcript::TranscriptGeneratedImageLifecycle::Pending => {
            syndic_storage::GeneratedMediaResourceDisposition::PendingAsset
        }
        crate::syndic_transcript::TranscriptGeneratedImageLifecycle::Admitted(asset) => {
            syndic_storage::GeneratedMediaResourceDisposition::Asset(asset)
        }
        crate::syndic_transcript::TranscriptGeneratedImageLifecycle::Unavailable(reason) => {
            syndic_storage::GeneratedMediaResourceDisposition::Unavailable(reason)
        }
    };
    Ok(PreparedGenerated {
        demand,
        presentation: GeneratedPresentation {
            view_thread,
            source_thread: proof.thread_id(),
            projection_id,
            resource_id: image.resource_id,
            lifecycle,
            has_authored_source: source.source_range.is_some()
                || source.resource_range.is_some()
                || source.copy_source_range.is_some(),
            same_authority,
            same_presentation,
            identity,
        },
    })
}
