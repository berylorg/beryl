use super::super::*;
use super::*;
use preparation::{PreparedPaste, PreparedPasteSource};

pub(in super::super::super) struct ActiveComposerPasteProducer {
    removals: ActivePropagatedCut,
    source: Arc<PreparedPasteSource>,
    replay: Option<replay::PrivateReplay>,
    text_cursor: usize,
    source_complete: bool,
    marker_ordinal: u64,
    marker_seed: u128,
    marker_count: u64,
    marker_anchor: Option<u64>,
    anchor_ordinal: u64,
    replacement: SourceRange,
    inserted_bytes: u64,
    limits: MutationLimits,
    metadata: Box<[crate::composer_host::ComposerHostImageMarkerMetadata]>,
    _permit: Option<Arc<PasteQueuePermit>>,
}

impl PreparedPaste {
    pub(super) fn begin(
        self,
        input: &mut RangeTextInput,
        cx: &mut gpui::Context<RangeTextInput>,
    ) -> Result<
        (
            ActiveComposerPasteProducer,
            Option<(
                String,
                super::super::source::MainWindowPrivateClipboardDescriptor,
            )>,
        ),
        String,
    > {
        let lease = input
            .lease_host_operation()
            .map_err(|_| "captured paste operation unavailable".to_owned())?;
        let binding = self.selection.binding().range_binding();
        let key = gpui_text_input::MutationKey::new(
            binding.binding(),
            binding.revision(),
            lease.operation(),
        );
        let proposal = gpui_text_input::MutationProposal::new(
            key,
            gpui_text_input::MutationKind::Edit,
            self.positions,
            self.replacement,
            self.replacement_breaks,
        );
        let begin =
            MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0))
                .with_replayable_producer(gpui_text_input::MutationProducerIdentity::new(
                    lease.operation().get(),
                ));
        let mut positions = Vec::with_capacity(5);
        for position in [
            self.positions.caret(),
            self.positions.selection_anchor(),
            self.positions.selection_head(),
            self.replacement.start(),
            self.replacement.end(),
        ] {
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
        let base = binding.extent();
        let bytes = base
            .byte_len()
            .checked_sub(
                self.replacement.end().byte_offset.get()
                    - self.replacement.start().byte_offset.get(),
            )
            .and_then(|bytes| bytes.checked_add(self.inserted_bytes))
            .ok_or("paste byte extent exhausted")?;
        let breaks = base
            .line_count()
            .checked_sub(u64::from(base.byte_len() != 0))
            .and_then(|breaks| breaks.checked_sub(self.replacement_breaks))
            .and_then(|breaks| breaks.checked_add(self.inserted_breaks))
            .ok_or("paste line extent exhausted")?;
        let lines = if bytes == 0 {
            if breaks != 0 {
                return Err("paste empty extent incoherent".into());
            }
            0
        } else {
            breaks.checked_add(1).ok_or("paste line extent exhausted")?
        };
        let caret = caret_after_text(self.replacement, self.inserted_bytes)?;
        let replay = match self.source.as_ref() {
            PreparedPasteSource::Private { descriptor, .. } => {
                Some(replay::PrivateReplay::new(*descriptor))
            }
            _ => None,
        };
        let preceding = preceding(self.replacement).map_or(0, |neighbor| neighbor.order().get());
        let following = following(self.replacement).map(|neighbor| neighbor.order().get());
        if preceding
            .checked_add(u128::from(self.leading_markers))
            .is_none_or(|order| order > u128::from(u64::MAX))
            || following.is_some_and(|end| {
                if self.inserted_bytes == 0 {
                    end.saturating_sub(preceding) <= u128::from(self.inserted_markers)
                } else {
                    self.trailing_markers != 0 && end <= u128::from(self.trailing_markers)
                }
            })
        {
            return Err("paste marker order domain exhausted".into());
        }
        input
            .begin_host_mutation(
                lease,
                begin,
                &positions,
                &self.proof.text,
                &self.proof.objects,
                cx,
            )
            .map_err(|error| format!("captured paste begin rejected: {error}"))?;
        Ok((
            ActiveComposerPasteProducer {
                removals: ActivePropagatedCut {
                    key,
                    scan: self.scan,
                    initial_scan: self.initial_scan,
                    prepared_items: (!self.removal_items.is_empty()).then_some(self.removal_items),
                    totals: MutationTotals::default(),
                    text_deletion_pending: false,
                    next_cursor: MutationCursor::new(0),
                    next_ordinal: 0,
                    cumulative_identity: MutationIdentity::ROOT,
                    intended_extent: gpui_text_input::LogicalExtent::new(bytes, lines),
                    intended: MutationPositions::collapsed(caret),
                    staging_pass: None,
                    finish_submitted: false,
                },
                source: self.source,
                replay,
                text_cursor: 0,
                source_complete: false,
                marker_ordinal: 0,
                marker_seed: self.marker_seed,
                marker_count: self.inserted_markers,
                marker_anchor: None,
                anchor_ordinal: 0,
                replacement: self.replacement,
                inserted_bytes: self.inserted_bytes,
                limits: self.mutation_limits,
                metadata: Box::new([]),
                _permit: None,
            },
            self.private,
        ))
    }
}

impl ActiveComposerPasteProducer {
    pub(super) fn retain_permit(&mut self, permit: Arc<PasteQueuePermit>) {
        self._permit = Some(permit);
    }
    pub(in super::super::super) fn key(&self) -> gpui_text_input::MutationKey {
        self.removals.key()
    }
    pub(in super::super::super) fn is_staging(&self) -> bool {
        self.removals.is_staging()
    }
    pub(in super::super::super) fn needs_page(&self) -> bool {
        self.removals.prepared_items.is_none()
            && (!self.removals.scan.complete || !self.source_complete)
    }
    pub(in super::super::super) fn metadata(
        &self,
    ) -> Box<[crate::composer_host::ComposerHostImageMarkerMetadata]> {
        self.metadata.clone()
    }
    pub(in super::super::super) fn next_input(
        &mut self,
    ) -> Result<Option<CutMutationInput>, String> {
        if self.needs_page() {
            return Ok(None);
        }
        self.removals.next_input()
    }
    pub(in super::super::super) fn restart(
        &mut self,
        pass: gpui_text_input::MutationPass,
    ) -> Result<(), String> {
        let extent = self.removals.intended_extent;
        self.removals.restart(pass)?;
        self.removals.text_deletion_pending = false;
        self.removals.intended_extent = extent;
        self.removals.intended =
            MutationPositions::collapsed(caret_after_text(self.replacement, self.inserted_bytes)?);
        self.text_cursor = 0;
        self.source_complete = false;
        self.marker_ordinal = 0;
        self.metadata = Box::new([]);
        self.marker_anchor = None;
        self.anchor_ordinal = 0;
        self.replay = match self.source.as_ref() {
            PreparedPasteSource::Private { descriptor, .. } => {
                Some(replay::PrivateReplay::new(*descriptor))
            }
            _ => None,
        };
        Ok(())
    }
    pub(in super::super::super) fn submit_next(
        &mut self,
        input: &mut RangeTextInput,
        cx: &mut gpui::Context<RangeTextInput>,
    ) -> Result<(), String> {
        self.removals.submit_next(input, cx)
    }
    pub(in super::super::super) fn load_next(
        &mut self,
        service: &Arc<super::super::super::MainWindowConversationComposerService>,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.metadata = Box::new([]);
        if let Some(request) = self.removals.next_page_request() {
            let page = prepare_next_cut_page(service, selection, request)?;
            self.removals.admit_prepared_page(page)?;
            return Ok(());
        }
        let item = match self.source.as_ref() {
            PreparedPasteSource::Text(text) => {
                if self.text_cursor == text.len() {
                    None
                } else {
                    let mut end = (self.text_cursor + self.limits.max_page_bytes()).min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end == self.text_cursor {
                        return Err("paste text page cannot hold one scalar".into());
                    }
                    let item = replay::ReplayItem::Text {
                        offset: self.text_cursor as u64,
                        text: text[self.text_cursor..end].into(),
                    };
                    self.text_cursor = end;
                    Some(item)
                }
            }
            PreparedPasteSource::Private { text, storage, .. } => self
                .replay
                .as_mut()
                .unwrap()
                .next(storage, &service.store, text, self.limits.max_page_bytes())
                .map_err(|error| error.to_string())?,
            PreparedPasteSource::Image(_) => None,
        };
        let (item, metadata) = match item {
            Some(replay::ReplayItem::Text { offset, text }) => (
                Some(MutationPageItem::Utf8 {
                    inserted_offset: offset,
                    text,
                }),
                None,
            ),
            Some(replay::ReplayItem::Marker { offset, marker }) => {
                let PreparedPasteSource::Private { descriptor, .. } = self.source.as_ref() else {
                    unreachable!()
                };
                let selector = descriptor.source.marker_selector(marker.marker_id());
                let asset = marker.asset_id();
                let (object, metadata) = self.marker(offset, asset, Some(selector))?;
                (
                    Some(MutationPageItem::Object(ObjectChange::Insert { object })),
                    Some(metadata),
                )
            }
            None if matches!(self.source.as_ref(), PreparedPasteSource::Image(_))
                && self.marker_ordinal == 0 =>
            {
                let PreparedPasteSource::Image(asset) = self.source.as_ref() else {
                    unreachable!()
                };
                let (object, metadata) = self.marker(0, *asset, None)?;
                (
                    Some(MutationPageItem::Object(ObjectChange::Insert { object })),
                    Some(metadata),
                )
            }
            None => (None, None),
        };
        if let Some(item) = item {
            self.removals.prepared_items = Some(vec![item]);
            if let Some(metadata) = metadata {
                self.metadata = Box::new([metadata]);
            }
        } else {
            if self.marker_ordinal != self.marker_count {
                return Err("paste immutable marker count changed".into());
            }
            self.source_complete = true;
            if self.inserted_bytes == 0
                && self.replacement.start().byte_offset != self.replacement.end().byte_offset
            {
                self.removals.prepared_items = Some(vec![MutationPageItem::Utf8 {
                    inserted_offset: 0,
                    text: "".into(),
                }]);
            }
        }
        Ok(())
    }
    fn marker(
        &mut self,
        offset: u64,
        asset: beryl_model::AssetId,
        selector: Option<syndic_storage::DraftMarkerReadinessSourceSelectorV1>,
    ) -> Result<
        (
            gpui_text_input::SuccessorObject,
            crate::composer_host::ComposerHostImageMarkerMetadata,
        ),
        String,
    > {
        self.marker_ordinal = self
            .marker_ordinal
            .checked_add(1)
            .ok_or("paste marker count exhausted")?;
        if self.marker_ordinal > self.marker_count {
            return Err("paste immutable marker count changed".into());
        }
        if self.marker_anchor != Some(offset) {
            self.marker_anchor = Some(offset);
            self.anchor_ordinal = 0;
        }
        self.anchor_ordinal = self
            .anchor_ordinal
            .checked_add(1)
            .ok_or("paste marker order exhausted")?;
        let id = gpui_text_input::InlineObjectId::new(
            self.marker_seed
                .checked_add(u128::from(self.marker_ordinal))
                .ok_or("paste marker identity exhausted")?,
        );
        let metadata = selector.map_or_else(
            || crate::composer_host::ComposerHostImageMarkerMetadata::new(id, asset),
            |selector| {
                crate::composer_host::ComposerHostImageMarkerMetadata::from_source(
                    id, asset, selector,
                )
            },
        );
        let prior = if offset == 0 {
            preceding(self.replacement).map_or(0, |neighbor| neighbor.order().get())
        } else {
            0
        };
        let order = prior
            .checked_add(u128::from(self.anchor_ordinal))
            .ok_or("paste marker order exhausted")?;
        if u64::try_from(order).is_err() {
            return Err("paste marker order domain exhausted".into());
        }
        let anchor = self
            .replacement
            .start()
            .byte_offset
            .get()
            .checked_add(offset)
            .ok_or("paste marker anchor exhausted")?;
        let order = gpui_text_input::InlineObjectOrder::new(order);
        let object = gpui_text_input::SuccessorObject::new(
            id,
            gpui_text_input::ByteOffset::new(anchor),
            order,
            metadata.retained_bytes(),
            0,
        );
        if offset == self.inserted_bytes {
            let neighbor = gpui_text_input::InlineObjectNeighbor::new(id, order);
            let gap = following(self.replacement)
                .map_or(
                    Ok(gpui_text_input::InlineObjectGap::after(neighbor)),
                    |next| gpui_text_input::InlineObjectGap::between(neighbor, next),
                )
                .map_err(|_| "paste terminal marker gap exhausted")?;
            self.removals.intended = MutationPositions::collapsed(
                gpui_text_input::SourcePosition::new(gpui_text_input::ByteOffset::new(anchor), gap),
            );
        }
        Ok((object, metadata))
    }
}

fn preceding(range: SourceRange) -> Option<gpui_text_input::InlineObjectNeighbor> {
    match range.start().gap {
        gpui_text_input::InlineObjectGap::After(neighbor)
        | gpui_text_input::InlineObjectGap::Between {
            preceding: neighbor,
            ..
        } => Some(neighbor),
        _ => None,
    }
}
fn following(range: SourceRange) -> Option<gpui_text_input::InlineObjectNeighbor> {
    match range.end().gap {
        gpui_text_input::InlineObjectGap::Before(neighbor)
        | gpui_text_input::InlineObjectGap::Between {
            following: neighbor,
            ..
        } => Some(neighbor),
        _ => None,
    }
}
fn caret_after_text(
    range: SourceRange,
    inserted_bytes: u64,
) -> Result<gpui_text_input::SourcePosition, String> {
    if inserted_bytes == 0 {
        return Ok(super::super::paging::deletion_caret(range));
    }
    let offset = range
        .start()
        .byte_offset
        .get()
        .checked_add(inserted_bytes)
        .ok_or("paste caret extent exhausted")?;
    let gap = following(range).map_or(
        gpui_text_input::InlineObjectGap::NoObjects,
        gpui_text_input::InlineObjectGap::before,
    );
    Ok(gpui_text_input::SourcePosition::new(
        gpui_text_input::ByteOffset::new(offset),
        gap,
    ))
}
