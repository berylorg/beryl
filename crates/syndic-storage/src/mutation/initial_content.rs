use super::*;

pub(super) fn validate_initial_content(
    reader: &DomainReader<'_, SyndicDomain>,
    records: &InitialThreadRecords,
) -> Result<(), SyndicMutationError> {
    match point::<ContentManifestsFamily>(reader, &records.content_manifest.id())? {
        Some(manifest) if manifest != records.content_manifest => {
            Err(SyndicMutationError::IdentityCollision)
        }
        Some(_) => {
            for chunk in &records.content_chunks {
                let stored = point::<ContentChunksFamily>(
                    reader,
                    &ContentChunkKey {
                        owner: chunk.content_id(),
                        ordinal: chunk.ordinal(),
                    },
                )?;
                if stored.as_ref() != Some(chunk) {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            for span in &records.content_spans {
                let stored = point::<ContentByteSpansFamily>(
                    reader,
                    &ContentByteSpanKey {
                        owner: span.content_id(),
                        start: span.start(),
                    },
                )?;
                if stored.as_ref() != Some(span) {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            Ok(())
        }
        None => {
            for chunk in &records.content_chunks {
                if point::<ContentChunksFamily>(
                    reader,
                    &ContentChunkKey {
                        owner: chunk.content_id(),
                        ordinal: chunk.ordinal(),
                    },
                )?
                .is_some()
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            for span in &records.content_spans {
                if point::<ContentByteSpansFamily>(
                    reader,
                    &ContentByteSpanKey {
                        owner: span.content_id(),
                        start: span.start(),
                    },
                )?
                .is_some()
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            Ok(())
        }
    }
}
