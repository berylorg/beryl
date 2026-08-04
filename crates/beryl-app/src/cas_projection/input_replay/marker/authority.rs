use beryl_backend::{
    StreamedInputDescriptor, StreamedInputDescriptorKind, StreamedInputHeader,
    StreamedInputSequenceDigest, StreamedInputSequenceDigestAccumulator, StreamedInputSourceError,
    StreamedLocalImageDescriptor, StreamedTextPage, StreamedTextSourceId,
};
use beryl_home_store::HomeStore;
use beryl_model::SealedAssetReferenceSetProof;
use beryl_state::{AssetOwnerHeadRecord, AssetState};
use syndic_storage::{ContentReference, SyndicStorage};

use super::{
    page::TextPageState,
    source::MarkerSource,
    walk::{DescriptorBlueprint, DescriptorWalk},
};
#[cfg(feature = "test-faults")]
use crate::cas_projection::input_replay::diagnostics::OrdinaryInputReplayDiagnostics;
use crate::cas_projection::input_replay::{
    InputReplayContext, InputReplayPrepareError, InputReplayRecord,
};
use crate::cas_projection::{ProjectionCancellationToken, connection::StreamedInputBrokerService};

pub(in crate::cas_projection) struct MarkerReplayAuthority {
    source: MarkerSource,
    header: StreamedInputHeader,
    pass: Option<ReplayPass>,
}

struct ReplayPass {
    walk: DescriptorWalk,
    page: Option<TextPageState>,
}

impl MarkerReplayAuthority {
    #[allow(
        clippy::too_many_arguments,
        reason = "preparation keeps each exact durable authority explicit"
    )]
    pub(in crate::cas_projection) fn prepare(
        store: &HomeStore,
        storage: SyndicStorage,
        assets: AssetState,
        context: InputReplayContext,
        record: InputReplayRecord,
        content: ContentReference,
        asset_proof: SealedAssetReferenceSetProof,
        owner_head: AssetOwnerHeadRecord,
        cancellation: &ProjectionCancellationToken,
        #[cfg(feature = "test-faults")] diagnostics: OrdinaryInputReplayDiagnostics,
    ) -> Result<Self, InputReplayPrepareError> {
        let source = MarkerSource::prepare(
            store,
            storage,
            assets,
            context,
            record,
            content,
            asset_proof,
            owner_head,
            cancellation,
            #[cfg(feature = "test-faults")]
            diagnostics,
        )
        .map_err(|error| error.into_preparation())?;
        let item_count = structural_count(&source, store, storage, cancellation)?;
        if item_count == 0 {
            return Err(InputReplayPrepareError::EmptyInput);
        }
        let sequence_digest = sequence_digest(&source, store, storage, cancellation, item_count)?;
        let header = StreamedInputHeader::new(
            source.source_identity(),
            source.source_revision(),
            item_count,
            sequence_digest,
        );
        Ok(Self {
            source,
            header,
            pass: None,
        })
    }

    pub(in crate::cas_projection) const fn header(&self) -> StreamedInputHeader {
        self.header
    }

    pub(in crate::cas_projection) fn fresh(&self) -> Self {
        Self {
            source: self.source.fresh(),
            header: self.header,
            pass: None,
        }
    }

    pub(in crate::cas_projection) fn service<'a>(
        &'a mut self,
        store: &'a HomeStore,
        storage: SyndicStorage,
        cancellation: &'a ProjectionCancellationToken,
    ) -> MarkerReplayService<'a> {
        MarkerReplayService {
            authority: self,
            store,
            storage,
            cancellation,
        }
    }

    fn begin_pass(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        if self.pass.is_some() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        self.source
            .check_authority(store, storage, cancellation)
            .map_err(|error| error.into_source())?;
        self.pass = Some(ReplayPass {
            walk: DescriptorWalk::new(),
            page: None,
        });
        Ok(self.header)
    }

    fn next_descriptor(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        if self.pass.is_none() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        self.source
            .check_authority(store, storage, cancellation)
            .map_err(|error| error.into_source())?;
        let pass = self
            .pass
            .as_mut()
            .expect("checked marker replay pass remains present");
        if pass.page.as_ref().is_some_and(|page| !page.is_complete()) {
            return Err(StreamedInputSourceError::VerifierUnavailable);
        }
        pass.page = None;
        let blueprint = pass
            .walk
            .next(&self.source, store, storage, cancellation)
            .map_err(|error| error.into_source())?;
        let Some(blueprint) = blueprint else {
            self.source
                .check_authority(store, storage, cancellation)
                .map_err(|error| error.into_source())?;
            self.pass = None;
            return Ok(None);
        };
        let (item_ordinal, kind) = match blueprint {
            DescriptorBlueprint::Text(run) => {
                let item_ordinal = run.descriptor_ordinal;
                let page = TextPageState::new(run);
                let descriptor = page.descriptor();
                pass.page = Some(page);
                (item_ordinal, StreamedInputDescriptorKind::Text(descriptor))
            }
            DescriptorBlueprint::Image(image) => {
                let path = self
                    .source
                    .verified_runtime_path(store, cancellation, &image.entry)
                    .map_err(|error| error.into_source())?;
                (
                    image.descriptor_ordinal,
                    StreamedInputDescriptorKind::LocalImage(local_image_descriptor(
                        &self.source,
                        path,
                    )),
                )
            }
        };
        Ok(Some(StreamedInputDescriptor::new(
            self.header.source_identity(),
            self.header.source_revision(),
            item_ordinal,
            kind,
        )))
    }

    fn read_text_page(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        source_id: StreamedTextSourceId,
        start: u64,
        maximum: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.pass
            .as_mut()
            .and_then(|pass| pass.page.as_mut())
            .ok_or(StreamedInputSourceError::InvalidSource)?
            .read(
                &self.source,
                store,
                storage,
                cancellation,
                source_id,
                start,
                maximum,
            )
    }
}

pub(in crate::cas_projection) struct MarkerReplayService<'a> {
    authority: &'a mut MarkerReplayAuthority,
    store: &'a HomeStore,
    storage: SyndicStorage,
    cancellation: &'a ProjectionCancellationToken,
}

impl StreamedInputBrokerService for MarkerReplayService<'_> {
    fn header(&self) -> StreamedInputHeader {
        self.authority.header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.authority
            .begin_pass(self.store, self.storage, self.cancellation)
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.authority
            .next_descriptor(self.store, self.storage, self.cancellation)
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.authority.read_text_page(
            self.store,
            self.storage,
            self.cancellation,
            source_id,
            start,
            max_utf8_bytes,
        )
    }
}

#[cfg(feature = "test-faults")]
impl MarkerReplayService<'_> {
    pub(in crate::cas_projection::input_replay) const fn diagnostics(
        &self,
    ) -> &OrdinaryInputReplayDiagnostics {
        self.authority.source.diagnostics()
    }
}

fn local_image_descriptor(_source: &MarkerSource, path: Box<str>) -> StreamedLocalImageDescriptor {
    StreamedLocalImageDescriptor::new(path, None)
}

fn structural_count(
    source: &MarkerSource,
    store: &HomeStore,
    storage: SyndicStorage,
    cancellation: &ProjectionCancellationToken,
) -> Result<u64, InputReplayPrepareError> {
    source
        .check_authority(store, storage, cancellation)
        .map_err(|error| error.into_preparation())?;
    let mut walk = DescriptorWalk::new();
    let mut count = 0_u64;
    while walk
        .next(source, store, storage, cancellation)
        .map_err(|error| error.into_preparation())?
        .is_some()
    {
        count = count
            .checked_add(1)
            .ok_or(InputReplayPrepareError::DescriptorInvalid)?;
    }
    source
        .check_authority(store, storage, cancellation)
        .map_err(|error| error.into_preparation())?;
    Ok(count)
}

fn sequence_digest(
    source: &MarkerSource,
    store: &HomeStore,
    storage: SyndicStorage,
    cancellation: &ProjectionCancellationToken,
    item_count: u64,
) -> Result<StreamedInputSequenceDigest, InputReplayPrepareError> {
    source
        .check_authority(store, storage, cancellation)
        .map_err(|error| error.into_preparation())?;
    let mut walk = DescriptorWalk::new();
    let mut digest = StreamedInputSequenceDigestAccumulator::new(item_count);
    while let Some(descriptor) = walk
        .next(source, store, storage, cancellation)
        .map_err(|error| error.into_preparation())?
    {
        match descriptor {
            DescriptorBlueprint::Text(text) => digest
                .push_text(text.descriptor_ordinal, text.proof, text.utf8_len)
                .map_err(|_| InputReplayPrepareError::DescriptorInvalid)?,
            DescriptorBlueprint::Image(image) => {
                let path = source
                    .verified_runtime_path(store, cancellation, &image.entry)
                    .map_err(|error| error.into_preparation())?;
                let descriptor = local_image_descriptor(source, path);
                digest
                    .push_local_image(
                        image.descriptor_ordinal,
                        descriptor.detail(),
                        descriptor.path(),
                    )
                    .map_err(|_| InputReplayPrepareError::DescriptorInvalid)?;
            }
        }
    }
    source
        .check_authority(store, storage, cancellation)
        .map_err(|error| error.into_preparation())?;
    digest
        .finish()
        .map_err(|_| InputReplayPrepareError::DescriptorInvalid)
}
