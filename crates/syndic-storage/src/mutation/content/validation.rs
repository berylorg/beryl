use super::*;

pub(super) fn validate_prepared_manifest(
    manifest: &ContentManifestRecord,
    content: &PreparedContent,
) -> Result<(), SyndicMutationError> {
    if manifest.id() != content.id()
        || manifest.encoding() != content.encoding()
        || manifest.expected() != content.summary()
        || SyndicContentId::from_digest(*content.summary().digest().as_bytes()) != content.id()
    {
        return Err(SyndicMutationError::ContentIdentityCollision);
    }
    if manifest.lifecycle() == ContentLifecycle::Sealed
        && (manifest.chunk_count() != manifest.expected().chunk_count()
            || manifest.encoded_bytes() != manifest.expected().encoded_bytes()
            || manifest.chain_digest() != manifest.expected().digest())
    {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    Ok(())
}

pub(super) fn validate_publishable(
    manifest: &ContentManifestRecord,
    expected: ContentSummary,
) -> Result<(), SyndicMutationError> {
    if manifest.expected() != expected
        || manifest.id() != SyndicContentId::from_digest(*expected.digest().as_bytes())
        || manifest.chunk_count() != expected.chunk_count()
        || manifest.encoded_bytes() != expected.encoded_bytes()
        || manifest.chain_digest() != expected.digest()
    {
        return Err(SyndicMutationError::ContentNotComplete);
    }
    Ok(())
}
