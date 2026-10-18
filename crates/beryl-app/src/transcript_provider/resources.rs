use super::*;
use crate::syndic_transcript::{
    ProviderRevision, ResourceKind, ResourceMetadata, ResourceRangeResponse,
    ResourceTranscriptDemand, TranscriptProviderRejection, TranscriptProviderRejectionReason,
    TranscriptProviderResponseKind, TranscriptProviderTarget,
};
use syndic_storage::{
    ResourceBacking, ResourceMetadataRecord, ResourceStructure, SyndicPointReadLimit,
};

mod indexed;
pub(crate) use crate::syndic_transcript::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    IndexedSourceResponse, ResourceDeliveryResponse,
};
pub(crate) use indexed::{PreparedIndexedResource, TranscriptIndexedResourceRequest};

pub(crate) enum PreparedResourceDelivery {
    Raw(PreparedTranscriptResource),
    Indexed(PreparedIndexedResource),
}

impl PreparedResourceDelivery {
    pub(crate) fn response(&self) -> ResourceDeliveryResponse {
        match self {
            Self::Raw(prepared) => ResourceDeliveryResponse::Raw(prepared.response().clone()),
            Self::Indexed(prepared) => {
                ResourceDeliveryResponse::Indexed(prepared.shared_response())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptResourceRequest {
    pub(crate) attachment: TranscriptAttachmentRequest,
    pub(crate) source: TranscriptAttachmentSourceIdentity,
    pub(crate) demand: ResourceTranscriptDemand,
}

pub(crate) struct PreparedTranscriptResource {
    request: TranscriptResourceRequest,
    metadata: Option<ResourceMetadataRecord>,
    response: TranscriptProviderResponseKind,
    observation: HomeMutationObservation,
    owner: Weak<TranscriptProviderState>,
    _reservation: PendingRead,
}

impl PreparedTranscriptResource {
    pub(crate) fn response(&self) -> &TranscriptProviderResponseKind {
        &self.response
    }
}

impl TranscriptProviderReader {
    pub(crate) fn prepare_resource_delivery(
        &self,
        request: TranscriptResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedResourceDelivery, TranscriptAttachmentError> {
        match request.demand.indexed.clone() {
            Some(inner) => self
                .prepare_indexed_resource(
                    TranscriptIndexedResourceRequest {
                        resource: request,
                        revision: inner.revision,
                        root: inner.root,
                        query: inner.query,
                    },
                    cancelled,
                )
                .map(PreparedResourceDelivery::Indexed),
            None => self
                .prepare_resource(request, cancelled)
                .map(PreparedResourceDelivery::Raw),
        }
    }

    pub(crate) fn publish_resource_delivery_if_current<T>(
        &self,
        prepared: &PreparedResourceDelivery,
        expected: &TranscriptResourceRequest,
        cancelled: &AtomicBool,
        publish: impl FnOnce() -> T,
    ) -> Result<T, TranscriptAttachmentError> {
        match (prepared, expected.demand.indexed.as_ref()) {
            (PreparedResourceDelivery::Raw(prepared), None) => {
                self.publish_resource_if_current(prepared, expected, cancelled, |_| publish())
            }
            (PreparedResourceDelivery::Indexed(prepared), Some(inner)) => self
                .publish_indexed_resource_if_current(
                    prepared,
                    &TranscriptIndexedResourceRequest {
                        resource: expected.clone(),
                        revision: inner.revision,
                        root: inner.root,
                        query: inner.query.clone(),
                    },
                    cancelled,
                    |_| publish(),
                ),
            _ => Err(TranscriptAttachmentError::Identity),
        }
    }

    pub(crate) fn prepare_resource(
        &self,
        request: TranscriptResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedTranscriptResource, TranscriptAttachmentError> {
        self.check_resource_request(&request, cancelled)?;
        self.state
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| TranscriptAttachmentError::Busy)?;
        let reservation = PendingRead {
            state: Arc::clone(&self.state),
        };
        let observation = self
            .observer
            .observe()
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        let metadata = self.read_resource_backing(&request, cancelled)?;
        let response = self.read_resource_response(&request, metadata.as_ref())?;
        self.check_resource_request(&request, cancelled)?;
        if self.read_resource_backing(&request, cancelled)? != metadata {
            return Err(TranscriptAttachmentError::Stale);
        }
        self.home
            .try_elect_observed_coherent(&observation, self.home_generation, || ())
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        Ok(PreparedTranscriptResource {
            request,
            metadata,
            response,
            observation,
            owner: Arc::downgrade(&self.state),
            _reservation: reservation,
        })
    }

    pub(crate) fn publish_resource_if_current<T>(
        &self,
        prepared: &PreparedTranscriptResource,
        expected: &TranscriptResourceRequest,
        cancelled: &AtomicBool,
        publish: impl FnOnce(&TranscriptProviderResponseKind) -> T,
    ) -> Result<T, TranscriptAttachmentError> {
        self.check_resource_request(expected, cancelled)?;
        if !prepared.owner.ptr_eq(&Arc::downgrade(&self.state))
            || !same_request(&prepared.request.attachment, &expected.attachment)
            || prepared.request.source != expected.source
            || prepared.request.demand != expected.demand
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        if let Some(metadata) = &expected.demand.metadata
            && prepared
                .metadata
                .as_ref()
                .and_then(|backing| resource_descriptor(backing).ok())
                .as_ref()
                != Some(metadata)
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.home
            .try_elect_observed_coherent(&prepared.observation, self.home_generation, || {
                // The worker rechecked this exact backing under the retained observation.
                // Election excludes later mutation; no storage read belongs on the GUI thread.
                self.check_request(&expected.attachment, cancelled)?;
                Ok(publish(&prepared.response))
            })
            .map_err(|error| match error {
                beryl_home_store::HomeObservedCoherenceError::Observation(
                    beryl_home_store::HomeMutationObservationError::Busy,
                )
                | beryl_home_store::HomeObservedCoherenceError::Coherence(
                    beryl_home_store::HomeCoherenceError::Busy,
                ) => TranscriptAttachmentError::Busy,
                _ => TranscriptAttachmentError::Stale,
            })?
    }

    fn check_resource_request(
        &self,
        request: &TranscriptResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<(), TranscriptAttachmentError> {
        self.check_request(&request.attachment, cancelled)?;
        if self.home.home_id() != self.home_id
            || self.home.generation_identity().ok() != Some(self.home_identity)
        {
            return Err(TranscriptAttachmentError::Retired);
        }
        let demand = &request.demand;
        let TranscriptAttachmentAuthority::Published { head, .. } = &request.source.authority
        else {
            return Err(TranscriptAttachmentError::Identity);
        };
        if request.attachment.purpose != TranscriptAttachmentPurpose::Resource
            || head.thread_id() != request.attachment.thread_id
            || demand.view_id.0 != request.attachment.thread_id.to_string()
            || demand.provenance.view_id != demand.view_id
            || demand.revision != ProviderRevision(head.revision().get())
            || demand.metadata.is_none() != demand.range.is_none()
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        Ok(())
    }

    fn read_resource_backing(
        &self,
        request: &TranscriptResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<Option<ResourceMetadataRecord>, TranscriptAttachmentError> {
        self.check_resource_request(request, cancelled)?;
        let point = SyndicPointReadLimit::new(ATTACHMENT_MAX_BYTES).unwrap();
        let TranscriptAttachmentAuthority::Published { head, summary } = &request.source.authority
        else {
            return Err(TranscriptAttachmentError::Identity);
        };
        if self
            .syndic
            .transcript_view_head(&self.home, head.thread_id(), point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .as_ref()
            != Some(head)
            || self
                .syndic
                .history_summary(&self.home, head.thread_id(), point)
                .map_err(|_| TranscriptAttachmentError::Unavailable)?
                .as_ref()
                != Some(summary)
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        let demand = &request.demand;
        let projection_id = demand
            .provenance
            .projection_id
            .as_ref()
            .ok_or(TranscriptAttachmentError::Identity)?
            .0
            .parse()
            .map_err(|_| TranscriptAttachmentError::Identity)?;
        let projection = self
            .syndic
            .projection(&self.home, projection_id, point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let resource_id = demand
            .resource_id
            .0
            .parse()
            .map_err(|_| TranscriptAttachmentError::Identity)?;
        if !matches!(projection.payload(), syndic_storage::ProjectionPayload::ResourceReference { resource_id: id, .. } if *id == resource_id)
            || demand.provenance.item_id.as_ref().map(|id| &id.0)
                != Some(&projection.item_id().to_string())
            || demand.provenance.turn_id.as_ref().map(|id| &id.0)
                != Some(&projection.turn_id().to_string())
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let position = demand
            .provenance
            .position
            .ok_or(TranscriptAttachmentError::Identity)?
            .0;
        if position == 0 {
            return Err(TranscriptAttachmentError::Identity);
        }
        let entries = self
            .syndic
            .transcript_entries(
                &self.home,
                head.thread_id(),
                head.generation(),
                (position > 1)
                    .then(|| syndic_storage::TranscriptPosition::new(position - 1).unwrap()),
                beryl_home_store::CursorReadLimits::new(1, ATTACHMENT_MAX_BYTES).unwrap(),
            )
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        let Some(entry) = entries.records().first() else {
            return Err(TranscriptAttachmentError::Stale);
        };
        let item = self
            .syndic
            .canonical_item(&self.home, projection.item_id(), point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if entry.position().get() != position
            || entry.projection_id() != projection.id()
            || entry.projection_revision() != projection.revision()
            || entry.item_id() != item.id()
            || item.id() != projection.item_id()
            || item.turn_id() != projection.turn_id()
            || entry.item_revision() != item.revision()
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        let metadata = self
            .syndic
            .resource(
                &self.home,
                resource_id,
                SyndicPointReadLimit::new(4096).unwrap(),
            )
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        if let Some(metadata) = &metadata
            && (metadata.id() != resource_id
                || metadata.item_id() != projection.item_id()
                || metadata.projection_id() != Some(projection.id())
                || resource_kind(metadata.kind()) != demand.kind)
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        if let Some(metadata) = &metadata
            && let ResourceBacking::TextRange { source, range } = metadata.backing()
            && (projection.payload().source_range() != Some(range)
                || demand.provenance.source_range.as_ref() != Some(&(range.start()..range.end()))
                || !item
                    .projection_source()
                    .is_some_and(|current| source.can_extend(current)))
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.check_resource_request(request, cancelled)?;
        Ok(metadata)
    }

    fn read_resource_response(
        &self,
        request: &TranscriptResourceRequest,
        metadata: Option<&ResourceMetadataRecord>,
    ) -> Result<TranscriptProviderResponseKind, TranscriptAttachmentError> {
        let demand = &request.demand;
        let rejection = |reason| {
            TranscriptProviderResponseKind::Rejected(TranscriptProviderRejection {
                target: demand.range.as_ref().map_or_else(
                    || TranscriptProviderTarget::Resource(demand.resource_id.clone()),
                    |range| TranscriptProviderTarget::ResourceRange {
                        resource_id: demand.resource_id.clone(),
                        range: range.clone(),
                    },
                ),
                reason,
                revision: metadata.map(|metadata| ProviderRevision(metadata.revision().get())),
                message: None,
            })
        };
        if !matches!(demand.kind, ResourceKind::Code | ResourceKind::Table) {
            return Ok(rejection(
                TranscriptProviderRejectionReason::UnsupportedResourceKind,
            ));
        }
        let Some(metadata) = metadata else {
            return Ok(rejection(
                TranscriptProviderRejectionReason::MissingResource,
            ));
        };
        if !matches!(metadata.backing(), ResourceBacking::TextRange { .. }) {
            return Ok(rejection(
                TranscriptProviderRejectionReason::UnsupportedResourceKind,
            ));
        }
        let descriptor = resource_descriptor(metadata)?;
        let Some(range) = &demand.range else {
            return Ok(TranscriptProviderResponseKind::ResourceMetadata(descriptor));
        };
        if demand.metadata.as_ref() != Some(&descriptor) {
            return Err(TranscriptAttachmentError::Stale);
        }
        if range.start >= range.end || range.end > descriptor.byte_len {
            return Ok(rejection(
                TranscriptProviderRejectionReason::RangeOutOfBounds,
            ));
        }
        let page = self
            .syndic
            .resource_text_range(
                &self.home,
                metadata.id(),
                range.start,
                range.end,
                ATTACHMENT_MAX_BYTES,
            )
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let end = range
            .start
            .checked_add(page.bytes().len() as u64)
            .ok_or(TranscriptAttachmentError::Identity)?;
        let expected_end = range.start + (range.end - range.start).min(ATTACHMENT_MAX_BYTES as u64);
        if page.resource_id() != metadata.id()
            || page.start() != range.start
            || end != expected_end
            || page.next_offset() != (end < range.end).then_some(end)
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        Ok(TranscriptProviderResponseKind::ResourceRange(
            ResourceRangeResponse {
                resource_id: demand.resource_id.clone(),
                revision: descriptor.revision,
                kind: descriptor.kind,
                range: range.start..end,
                bytes: page.bytes().to_vec(),
                complete: page.next_offset().is_none(),
            },
        ))
    }
}

fn resource_kind(kind: syndic_storage::ResourceKind) -> ResourceKind {
    match kind {
        syndic_storage::ResourceKind::Code => ResourceKind::Code,
        syndic_storage::ResourceKind::Table => ResourceKind::Table,
        syndic_storage::ResourceKind::Image => ResourceKind::Image,
        syndic_storage::ResourceKind::Attachment => ResourceKind::Attachment,
        _ => ResourceKind::Other("other".into()),
    }
}

fn resource_descriptor(
    metadata: &ResourceMetadataRecord,
) -> Result<ResourceMetadata, TranscriptAttachmentError> {
    let (line_count, row_count, column_count) = match metadata.structure() {
        ResourceStructure::Code { logical_lines, .. } => (Some(*logical_lines), None, None),
        ResourceStructure::Table {
            body_rows, columns, ..
        } => (None, Some(*body_rows), Some(*columns)),
        ResourceStructure::Opaque => (None, None, None),
    };
    Ok(ResourceMetadata {
        resource_id: crate::syndic_transcript::ResourceId(metadata.id().to_string()),
        revision: ProviderRevision(metadata.revision().get()),
        kind: resource_kind(metadata.kind()),
        media_type: metadata.media_type().map(str::to_owned),
        byte_len: metadata
            .byte_length()
            .ok_or(TranscriptAttachmentError::Identity)?,
        digest: metadata
            .digest()
            .map(|digest| digest.iter().map(|byte| format!("{byte:02x}")).collect()),
        line_count,
        row_count,
        column_count,
        preview_range: metadata
            .preview_range()
            .map(|range| range.start()..range.end()),
    })
}
