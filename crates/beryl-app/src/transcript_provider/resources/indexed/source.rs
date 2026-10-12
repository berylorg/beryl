use super::*;

impl TranscriptProviderReader {
    pub(super) fn read_source_response(
        &self,
        request: &TranscriptIndexedResourceRequest,
        binding: &index::Binding,
        root: index::Root,
        root_bytes: usize,
    ) -> Result<IndexedResourceResponse, TranscriptAttachmentError> {
        let IndexedResourceQuery::Source { range, bytes } = request.query else {
            return Err(TranscriptAttachmentError::Identity);
        };
        if request.root != Some(root.digest)
            || !matches!(
                binding.metadata.kind(),
                syndic_storage::ResourceKind::Code | syndic_storage::ResourceKind::Table
            )
            || range.start >= range.end
            || range.start < binding.range.start
            || range.end > binding.range.end
            || range.end - range.start > 8192
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let mut raw = request.resource.clone();
        raw.demand.indexed = None;
        raw.demand.metadata = Some(resource_descriptor(&binding.metadata)?);
        raw.demand.range = Some(range.start - binding.range.start..range.end - binding.range.start);
        let TranscriptProviderResponseKind::ResourceRange(page) =
            self.read_resource_response(&raw, Some(&binding.metadata))?
        else {
            return Err(TranscriptAttachmentError::Unavailable);
        };
        if page.range != raw.demand.range.unwrap()
            || page.bytes.len() as u64 != range.end - range.start
            || !page.complete
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let encoded_bytes = root_bytes
            .checked_add(page.bytes.len())
            .ok_or(TranscriptAttachmentError::Identity)?;
        if encoded_bytes > bytes {
            return Err(TranscriptAttachmentError::Unavailable);
        }
        Ok(IndexedResourceResponse {
            root: Some(root),
            value: IndexedResourceValue::Source(IndexedSourceResponse {
                binding: binding.clone(),
                range,
                bytes: page.bytes,
            }),
            encoded_bytes,
        })
    }
}
