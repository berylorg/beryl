use super::*;
use beryl_home_store::CommandCancellation;
use beryl_model::ProjectionRevision;
use syndic_storage::indexed_resource as index;

mod source;

#[derive(Clone, Debug)]
pub(crate) struct TranscriptIndexedResourceRequest {
    pub(crate) resource: TranscriptResourceRequest,
    pub(crate) revision: ProjectionRevision,
    pub(crate) root: Option<[u8; 32]>,
    pub(crate) query: IndexedResourceQuery,
}

pub(crate) struct PreparedIndexedResource {
    request: TranscriptIndexedResourceRequest,
    resource: PreparedTranscriptResource,
    response: Arc<IndexedResourceResponse>,
}

impl PreparedIndexedResource {
    pub(super) fn shared_response(&self) -> Arc<IndexedResourceResponse> {
        Arc::clone(&self.response)
    }
    pub(crate) fn response(&self) -> &IndexedResourceResponse {
        &self.response
    }
}

impl TranscriptProviderReader {
    pub(crate) fn prepare_indexed_resource(
        &self,
        request: TranscriptIndexedResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedIndexedResource, TranscriptAttachmentError> {
        let bytes = match &request.query {
            IndexedResourceQuery::Range {
                descriptors, bytes, ..
            } => {
                if *descriptors == 0 || *descriptors > 256 {
                    return Err(TranscriptAttachmentError::Identity);
                }
                *bytes
            }
            IndexedResourceQuery::Display { bytes, .. }
            | IndexedResourceQuery::Anchor { bytes, .. }
            | IndexedResourceQuery::Source { bytes, .. } => *bytes,
        };
        if bytes == 0
            || bytes > ATTACHMENT_MAX_BYTES
            || request.resource.demand.metadata.is_some()
            || request.resource.demand.range.is_some()
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let resource = self.prepare_resource(request.resource.clone(), cancelled)?;
        let metadata = resource
            .metadata
            .as_ref()
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if metadata.revision() != request.revision {
            return Err(TranscriptAttachmentError::Stale);
        }
        let ResourceBacking::TextRange { source, range } = metadata.backing() else {
            return Err(TranscriptAttachmentError::Unavailable);
        };
        let binding = index::Binding {
            resource: index::ResourceKey {
                id: metadata.id(),
                revision: metadata.revision(),
            },
            metadata: metadata.clone(),
            source,
            range: index::ByteRange {
                start: range.start(),
                end: range.end(),
            },
        };
        self.check_resource_request(&request.resource, cancelled)?;
        let frozen = self
            .home
            .capture_frozen_read(&CommandCancellation::new())
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        // Keep custody only for the synchronous worker read; all result paths release it.
        let response = match &request.query {
            IndexedResourceQuery::Range {
                request: range,
                cursor,
                descriptors,
                bytes,
            } => self
                .syndic
                .read_frozen_indexed_resource_range(
                    &self.home,
                    &frozen,
                    &binding,
                    request.root,
                    *range,
                    *cursor,
                    index::ResponseLimits {
                        descriptors: *descriptors,
                        bytes: *bytes,
                    },
                )
                .map(|value| transport(value, IndexedResourceValue::Range))
                .map_err(indexed_error),
            IndexedResourceQuery::Display {
                request: display,
                cursor,
                bytes,
            } => self
                .syndic
                .read_frozen_indexed_resource_display(
                    &self.home,
                    &frozen,
                    &binding,
                    request.root,
                    *display,
                    *cursor,
                    *bytes,
                )
                .map(|value| transport(value, IndexedResourceValue::Display))
                .map_err(indexed_error),
            IndexedResourceQuery::Anchor { anchor, bytes } => self
                .syndic
                .read_frozen_indexed_resource_anchor(
                    &self.home,
                    &frozen,
                    &binding,
                    request.root,
                    *anchor,
                    *bytes,
                )
                .map(|value| transport(value, IndexedResourceValue::Anchor))
                .map_err(indexed_error),
            IndexedResourceQuery::Source { bytes, .. } => (|| {
                let structure = match binding.metadata.kind() {
                    syndic_storage::ResourceKind::Code => index::Request::Code {
                        lines: index::OrdinalRange { start: 0, end: 0 },
                    },
                    syndic_storage::ResourceKind::Table => index::Request::Table {
                        role: index::RowRole::Body,
                        rows: index::OrdinalRange { start: 0, end: 0 },
                        columns: index::OrdinalRange { start: 0, end: 0 },
                    },
                    _ => return Err(TranscriptAttachmentError::Unavailable),
                };
                self.syndic
                    .read_frozen_indexed_resource_range(
                        &self.home,
                        &frozen,
                        &binding,
                        request.root,
                        structure,
                        None,
                        index::ResponseLimits {
                            descriptors: 1,
                            bytes: (*bytes).min(8192),
                        },
                    )
                    .map_err(indexed_error)
                    .and_then(|value| match value {
                        index::IndexedResourceResult::Pending => Ok(IndexedResourceResponse {
                            root: None,
                            value: IndexedResourceValue::Pending,
                            encoded_bytes: 1,
                        }),
                        index::IndexedResourceResult::Ready {
                            root,
                            encoded_bytes,
                            ..
                        } => self.read_source_response(&request, &binding, root, encoded_bytes),
                    })
            })(),
        };
        self.home
            .release_frozen_read(&frozen)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        let response = response?;
        #[cfg(test)]
        let held = self.indexed_read_gate.as_ref().filter(|gate| {
            gate.matches(&response.value) && gate.hold(request.resource.demand.revision)
        });
        let current = (|| {
            self.check_resource_request(&request.resource, cancelled)?;
            if self.read_resource_backing(&request.resource, cancelled)? != resource.metadata {
                return Err(TranscriptAttachmentError::Stale);
            }
            self.home
                .try_elect_observed_coherent(&resource.observation, self.home_generation, || ())
                .map_err(|_| TranscriptAttachmentError::Stale)
        })();
        #[cfg(test)]
        if let Some(gate) = held {
            gate.observe(&current);
        }
        current?;
        Ok(PreparedIndexedResource {
            request,
            resource,
            response: Arc::new(response),
        })
    }

    pub(crate) fn publish_indexed_resource_if_current<T>(
        &self,
        prepared: &PreparedIndexedResource,
        expected: &TranscriptIndexedResourceRequest,
        cancelled: &AtomicBool,
        publish: impl FnOnce(&Arc<IndexedResourceResponse>) -> T,
    ) -> Result<T, TranscriptAttachmentError> {
        if prepared.request.revision != expected.revision
            || prepared.request.root != expected.root
            || prepared.request.query != expected.query
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.publish_resource_if_current(&prepared.resource, &expected.resource, cancelled, |_| {
            publish(&prepared.response)
        })
    }
}

fn indexed_error(error: index::IndexedResourceError) -> TranscriptAttachmentError {
    match error {
        index::IndexedResourceError::Collision => TranscriptAttachmentError::Stale,
        _ => TranscriptAttachmentError::Unavailable,
    }
}

fn transport<T>(
    result: index::IndexedResourceResult<T>,
    value: impl FnOnce(T) -> IndexedResourceValue,
) -> IndexedResourceResponse {
    match result {
        index::IndexedResourceResult::Pending => IndexedResourceResponse {
            root: None,
            value: IndexedResourceValue::Pending,
            encoded_bytes: 1,
        },
        index::IndexedResourceResult::Ready {
            root,
            response,
            encoded_bytes,
        } => IndexedResourceResponse {
            root: Some(root),
            value: value(response),
            encoded_bytes,
        },
    }
}
