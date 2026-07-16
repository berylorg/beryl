use beryl_home_store::MutationBuilder;

use crate::{
    CanonicalItemRecord, CasItemIndexRecord, CasItemSource, ContentChunkRecord,
    ContentManifestRecord, ItemProjectionBuildRecord, ItemProjectionHeadRecord,
    ItemSourceEventIndexRecord, ItemSourceEventOrdinal, ResourceMetadataRecord,
    SourceEventSequence, SyndicMutationError, TurnItemIndexRecord, codec::*, domain::SyndicDomain,
};

pub(crate) struct ItemEffect {
    manifest: Option<ContentManifestRecord>,
    chunks: Vec<ContentChunkRecord>,
    spans: Vec<crate::ContentByteSpanRecord>,
    text_spans: Vec<crate::ContentTextSpanRecord>,
    pieces: Vec<crate::ContentPieceRecord>,
    resource: Option<ResourceMetadataRecord>,
    item: CanonicalItemRecord,
    item_index: TurnItemIndexRecord,
    source_index: ItemSourceEventIndexRecord,
    cas_index: CasItemIndexRecord,
    projection_build: Option<ItemProjectionBuildRecord>,
    projection_head: Option<ItemProjectionHeadRecord>,
}

impl ItemEffect {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        item: CanonicalItemRecord,
        source: CasItemSource,
        source_ordinal: ItemSourceEventOrdinal,
        source_event: SourceEventSequence,
        manifest: Option<ContentManifestRecord>,
        resource: Option<ResourceMetadataRecord>,
    ) -> Self {
        let revision = item.revision();
        Self {
            manifest,
            chunks: Vec::new(),
            spans: Vec::new(),
            text_spans: Vec::new(),
            pieces: Vec::new(),
            resource,
            item_index: TurnItemIndexRecord::new(
                item.turn_id(),
                item.ordinal(),
                item.id(),
                revision,
            ),
            source_index: ItemSourceEventIndexRecord::new(
                item.id(),
                source_ordinal,
                item.turn_id(),
                source_event,
            ),
            cas_index: CasItemIndexRecord::new(
                source.turn().thread_id().clone(),
                source.turn().turn_id().clone(),
                source.item_id().clone(),
                item.id(),
                revision,
            ),
            projection_build: None,
            projection_head: None,
            item,
        }
    }

    pub(super) fn set_content_parts(
        &mut self,
        chunks: Vec<ContentChunkRecord>,
        spans: Vec<crate::ContentByteSpanRecord>,
        text_spans: Vec<crate::ContentTextSpanRecord>,
        pieces: Vec<crate::ContentPieceRecord>,
    ) {
        self.chunks = chunks;
        self.spans = spans;
        self.text_spans = text_spans;
        self.pieces = pieces;
    }

    pub(super) fn set_projection_invalidation(
        &mut self,
        build: Option<ItemProjectionBuildRecord>,
        head: Option<ItemProjectionHeadRecord>,
    ) {
        self.projection_build = build;
        self.projection_head = head;
    }

    pub(crate) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let Some(manifest) = &self.manifest {
            mutations.put::<ContentManifestsCodec>(&manifest.id(), manifest)?;
        }
        for chunk in &self.chunks {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        for span in &self.spans {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        for span in &self.text_spans {
            mutations.put::<ContentTextSpansCodec>(
                &ContentTextSpanKey {
                    owner: span.content_id(),
                    logical_start: span.logical_start(),
                },
                span,
            )?;
        }
        for piece in &self.pieces {
            mutations.put::<ContentPiecesCodec>(
                &ContentPieceKey {
                    owner: piece.content_id(),
                    ordinal: piece.ordinal(),
                },
                piece,
            )?;
        }
        if let Some(resource) = &self.resource {
            mutations.put::<ResourcesCodec>(&resource.id(), resource)?;
        }
        mutations.put::<CanonicalItemsCodec>(&self.item.id(), &self.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: self.item.turn_id(),
                ordinal: self.item.ordinal(),
            },
            &self.item_index,
        )?;
        mutations.put::<ItemSourceEventsCodec>(
            &ItemEventKey {
                owner: self.source_index.item_id(),
                ordinal: self.source_index.ordinal(),
            },
            &self.source_index,
        )?;
        mutations.put::<CasItemIndexCodec>(
            &CasItemKey::Record(
                self.cas_index.cas_thread_id().clone(),
                self.cas_index.cas_turn_id().clone(),
                self.cas_index.cas_item_id().clone(),
            ),
            &self.cas_index,
        )?;
        if let Some(build) = &self.projection_build {
            mutations.put::<ItemProjectionBuildsCodec>(
                &ItemProjectionSetKey {
                    item: build.item_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        if let Some(head) = &self.projection_head {
            mutations.put::<ItemProjectionHeadsCodec>(&head.item_id(), head)?;
        }
        Ok(())
    }
}
