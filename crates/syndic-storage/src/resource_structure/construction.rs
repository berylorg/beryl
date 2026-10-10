use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, MutationBuildError, MutationBuilder, ReadError,
    ReconciliationReservation,
};

use super::query::QueryRecords;
use super::{codec::*, ledger::Ledger, *};
use crate::{
    ContentEncoding, ContentLifecycle, ProjectionTextSource, ResourceBacking,
    ResourceMetadataRecord, ResourceStructure,
    codec::{
        CodecError, ContentManifestsFamily, Family, ProviderNarrativeSpanKey,
        ProviderNarrativeSpansCodec, ResourcesFamily,
    },
    domain::SyndicDomain,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConstructionError {
    #[error("resource structure read failed: {0}")]
    Read(#[from] ReadError),
    #[error("resource structure contribution failed: {0}")]
    Write(#[from] MutationBuildError),
    #[error("resource structure canonical encoding failed: {0}")]
    Codec(#[from] CodecError),
    #[error("resource structure derivation failed: {0}")]
    Derivation(#[from] DerivationError),
    #[error("resource structure command exceeds its bounded allowance")]
    Limit,
    #[error("resource structure is unavailable for this exact resource")]
    Unavailable,
    #[error("resource structure operation or selected companion conflicts")]
    Collision,
    #[error("resource structure companion is already selected")]
    AlreadyCurrent,
    #[error("resource structure selected closure is inconsistent")]
    Corrupt,
}

impl DomainCallbackError for ConstructionError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

pub(crate) struct AdmitBuild {
    pub(crate) key: BuildKey,
}

pub(super) fn initial_build(
    key: BuildKey,
    metadata: ResourceMetadataRecord,
) -> Result<Build, ConstructionError> {
    if metadata.id() != key.resource.id || metadata.revision() != key.resource.revision {
        return Err(ConstructionError::Collision);
    }
    if ResourcesFamily::encode_value(&metadata)?.len() > 4_096 {
        return Err(ConstructionError::Unavailable);
    }
    let ResourceBacking::TextRange { source, range } = metadata.backing() else {
        return Err(ConstructionError::Unavailable);
    };
    if range.end() > source.logical_utf8_bytes() {
        return Err(ConstructionError::Unavailable);
    }
    let pass = match metadata.structure() {
        ResourceStructure::Code { .. } => Pass::CodeLines,
        ResourceStructure::Table { header_range, .. }
            if header_range.start() == 0 && header_range.end() < range.len() =>
        {
            Pass::TableRows
        }
        _ => return Err(ConstructionError::Unavailable),
    };
    let binding = Binding {
        resource: key.resource,
        metadata,
        source,
        range: ByteRange {
            start: range.start(),
            end: range.end(),
        },
    };
    if binding_bytes(&binding)?.len() > 4_608 {
        return Err(ConstructionError::Unavailable);
    }
    let digest = binding_digest(&binding)?;
    let build = Build {
        key,
        binding,
        binding_digest: digest,
        revision: 1,
        lifecycle: Lifecycle::Building,
        transition: 0,
        progress: None,
        next_page: 1,
        chain: empty_chain(key, &digest),
        counts: [0; 4],
        checkpoint: Checkpoint {
            pass,
            next_byte: 0,
            line_start: 0,
            role: RowRole::Header,
            next_row: 0,
            next_column: 0,
            phase: ParsePhase::Consuming,
            pending_cr: false,
            utf8_carry: Vec::new(),
            syntax_start: 0,
            syntax: Vec::new(),
            output_position: 0,
            escaped: false,
            in_code: false,
        },
    };
    encode_build(&build)?;
    Ok(build)
}

pub(super) trait AuthenticationRecords: QueryRecords {
    fn narrative_frontier(
        &self,
        narrative: crate::ProviderNarrativeReference,
    ) -> Result<
        Option<(ProviderNarrativeSpanKey, crate::ProviderNarrativeSpanRecord)>,
        ConstructionError,
    >;
}

impl AuthenticationRecords for DomainReader<'_, SyndicDomain> {
    fn narrative_frontier(
        &self,
        narrative: crate::ProviderNarrativeReference,
    ) -> Result<
        Option<(ProviderNarrativeSpanKey, crate::ProviderNarrativeSpanRecord)>,
        ConstructionError,
    > {
        let frontier = self.cursor::<ProviderNarrativeSpansCodec>(
            &CursorRange::closed(
                ProviderNarrativeSpanKey::first_for_generation(
                    narrative.content_id(),
                    narrative.generation(),
                ),
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    narrative.logical_utf8_bytes() - 1,
                ),
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).map_err(|_| ConstructionError::Limit)?,
        )?;
        Ok(frontier
            .into_records()
            .into_iter()
            .next()
            .map(|record| record.into_parts()))
    }
}

pub(super) fn authenticate_source(
    reader: &impl AuthenticationRecords,
    ledger: &mut Ledger,
    binding: &Binding,
) -> Result<u64, ConstructionError> {
    let content_id = match binding.source {
        ProjectionTextSource::Composer(content) => content.id(),
        ProjectionTextSource::ProviderNarrative(narrative) => narrative.content_id(),
    };
    ledger.reserve_point::<ContentManifestsFamily>(4_096)?;
    let manifest = reader
        .point::<ContentManifestsFamily>(&content_id, 4_096)?
        .ok_or(ConstructionError::Unavailable)?;
    validate_source_manifest(binding, &manifest)?;
    if let ProjectionTextSource::ProviderNarrative(narrative) = binding.source {
        ledger.reserve_read(1, 512, 512)?;
        let (key, span) = reader
            .narrative_frontier(narrative)?
            .ok_or(ConstructionError::Unavailable)?;
        validate_narrative_frontier(narrative, manifest.encoded_bytes(), &key, &span)?;
    }
    Ok(manifest.encoded_bytes())
}

pub(super) fn validate_source_manifest(
    binding: &Binding,
    manifest: &crate::ContentManifestRecord,
) -> Result<(), ConstructionError> {
    let content_id = match binding.source {
        ProjectionTextSource::Composer(content) => content.id(),
        ProjectionTextSource::ProviderNarrative(narrative) => narrative.content_id(),
    };
    if manifest.id() != content_id {
        return Err(ConstructionError::Corrupt);
    }
    match binding.source {
        ProjectionTextSource::Composer(content) => {
            if content.encoding() != ContentEncoding::ComposerV1
                || manifest.sealed_reference() != Some(content)
            {
                return Err(ConstructionError::Unavailable);
            }
        }
        ProjectionTextSource::ProviderNarrative(narrative) => {
            if manifest.encoding() != ContentEncoding::ProviderItemV1
                || manifest.owner() != Some(binding.metadata.item_id())
                || !matches!(
                    manifest.lifecycle(),
                    ContentLifecycle::Live | ContentLifecycle::Finalized
                )
                || manifest.current_reference().is_none()
                || narrative.logical_utf8_bytes() == 0
            {
                return Err(ConstructionError::Unavailable);
            }
        }
    }
    Ok(())
}

pub(super) fn validate_narrative_frontier(
    narrative: crate::ProviderNarrativeReference,
    encoded_bytes: u64,
    key: &ProviderNarrativeSpanKey,
    span: &crate::ProviderNarrativeSpanRecord,
) -> Result<(), ConstructionError> {
    if key.content_id() != narrative.content_id()
        || key.generation() != narrative.generation()
        || key.logical_start() != span.logical_start()
        || span.content_id() != narrative.content_id()
        || span.generation() != narrative.generation()
        || span.logical_end() != narrative.logical_utf8_bytes()
        || span.resulting_chain_digest() != narrative.chain_digest()
        || span.source_end() > encoded_bytes
    {
        return Err(ConstructionError::Unavailable);
    }
    Ok(())
}

pub(super) fn authenticate_selected(
    reader: &DomainReader<'_, SyndicDomain>,
    ledger: &mut Ledger,
    binding: &Binding,
    head: &Head,
) -> Result<(), ConstructionError> {
    if head.key != binding.resource || head.binding_digest != binding_digest(binding)? {
        return Err(ConstructionError::Collision);
    }
    let root = ledger
        .point::<RootsFamily>(reader, &head.build, 4_608)?
        .ok_or(ConstructionError::Corrupt)?;
    let build = ledger
        .point::<BuildsFamily>(reader, &head.build, 24_576)?
        .ok_or(ConstructionError::Corrupt)?;
    let selected = build.progress.ok_or(ConstructionError::Corrupt)?;
    let receipt = ledger
        .point::<ProgressFamily>(reader, &selected.key, 512)?
        .ok_or(ConstructionError::Corrupt)?;
    validate_selected(binding, head, &root, &build, &receipt)
}

pub(super) fn validate_selected(
    binding: &Binding,
    head: &Head,
    root: &Root,
    build: &Build,
    receipt: &Progress,
) -> Result<(), ConstructionError> {
    if head.key != binding.resource || head.binding_digest != binding_digest(binding)? {
        return Err(ConstructionError::Collision);
    }
    if root.key != head.build
        || root.digest != head.root
        || root.binding_digest != head.binding_digest
        || build.key != head.build
        || build.binding != *binding
        || build.binding_digest != head.binding_digest
        || build.lifecycle != Lifecycle::Completed
        || build.checkpoint.pass != Pass::Sealed
        || root.counts != build.counts
        || root.pages.checked_add(1) != Some(build.next_page)
        || root.chain != build.chain
        || build.progress.map(|p| p.key) != Some(root.progress)
    {
        return Err(ConstructionError::Corrupt);
    }
    match binding.metadata.structure() {
        ResourceStructure::Code {
            language,
            logical_lines,
        } if root.code_lines == *logical_lines
            && root.language == *language
            && root.header.is_none()
            && root.body_rows == 0
            && root.columns == 0 => {}
        ResourceStructure::Table {
            body_rows,
            columns,
            header_range,
        } if root.body_rows == *body_rows
            && root.columns == *columns
            && root.header
                == Some(ByteRange {
                    start: header_range.start(),
                    end: header_range.end(),
                })
            && root.code_lines == 0
            && root.language.is_none() => {}
        _ => return Err(ConstructionError::Corrupt),
    }
    let selected = build.progress.ok_or(ConstructionError::Corrupt)?;
    if receipt.key != selected.key
        || receipt.digest != selected.digest
        || receipt.command != Command::Complete
        || receipt.root != Some(root.digest)
        || receipt.after_revision != build.revision
        || receipt.after_digest != build_digest(&build)?
    {
        return Err(ConstructionError::Corrupt);
    }
    Ok(())
}

impl DomainMutation<SyndicDomain> for AdmitBuild {
    type Error = ConstructionError;
    type Prepared = Build;

    fn prepare(self, reader: &DomainReader<'_, SyndicDomain>) -> Result<Build, ConstructionError> {
        let mut ledger = Ledger::default();
        let metadata = ledger
            .point::<ResourcesFamily>(reader, &self.key.resource.id, 4_096)
            .map_err(|error| match error {
                ConstructionError::Limit => ConstructionError::Unavailable,
                error => error,
            })?
            .ok_or(ConstructionError::Unavailable)?;
        let build = initial_build(self.key, metadata)?;
        authenticate_source(reader, &mut ledger, &build.binding)?;
        if ledger
            .point::<BuildsFamily>(reader, &self.key, 24_576)?
            .is_some()
        {
            return Err(ConstructionError::Collision);
        }
        if let Some(head) = ledger.point::<HeadsFamily>(reader, &self.key.resource, 168)? {
            authenticate_selected(reader, &mut ledger, &build.binding, &head)?;
            return Err(ConstructionError::AlreadyCurrent);
        }
        ledger.reserve_write(40, encode_build(&build)?.len(), false)?;
        Ok(build)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), ConstructionError> {
        reservation.reserve_records::<BuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        build: Build,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), ConstructionError> {
        mutations.put::<BuildsCodec>(&build.key, &build)?;
        Ok(())
    }
}
