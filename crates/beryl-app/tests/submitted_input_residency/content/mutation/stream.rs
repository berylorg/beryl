use super::*;

const COMPOSER_MUTATION_PAGE_ITEMS: usize = 16;
const TEXT_PATTERN_REPETITIONS_PER_ITEM: u64 = 64;
const MAX_STREAMED_MUTATION_PAGE_OWNED_BYTES: usize = 65_536;

struct StreamedMutation {
    key: MutationKey,
    cursor: MutationCursor,
    ordinal: u64,
    prior: MutationIdentity,
    totals: MutationTotals,
    items: Vec<MutationPageItem>,
    metadata: Vec<ComposerHostImageMarkerMetadata>,
    text_bytes: u64,
    line_count: u64,
}

#[derive(Clone, Copy)]
pub(super) struct StreamedMutationFinish {
    pub(super) finish: MutationStreamFinish,
    pub(super) extent: LogicalExtent,
    pub(super) positions: MutationPositions,
}

pub(super) fn stream_logical_input(
    key: MutationKey,
    segment: InputSegment,
    extent: LogicalExtent,
    draft: SyndicDraftId,
    assets: &[AssetId],
    mut deliver: impl FnMut(MutationPage, Box<[ComposerHostImageMarkerMetadata]>),
) -> StreamedMutationFinish {
    let mut stream = StreamedMutation::new(key);
    match segment {
        InputSegment::Marker(label) => stream.push_image(
            draft,
            label,
            extent.byte_len(),
            *assets
                .first()
                .expect("marker-aware input requires one asset"),
            &mut deliver,
        ),
        InputSegment::Text { repetitions, .. } => {
            stream.push_repeated_text(repetitions, &mut deliver)
        }
    }
    stream.flush(&mut deliver);
    let positions = MutationPositions::collapsed(SourcePosition::new(
        ByteOffset::new(extent.byte_len().checked_add(stream.text_bytes).unwrap()),
        match segment {
            InputSegment::Marker(label) => after_marker(draft, label),
            InputSegment::Text { .. } => InlineObjectGap::NoObjects,
        },
    ));
    StreamedMutationFinish {
        finish: stream.finish(),
        extent: LogicalExtent::new(
            extent.byte_len().checked_add(stream.text_bytes).unwrap(),
            extent
                .line_count()
                .max(1)
                .checked_add(stream.line_count - 1)
                .unwrap(),
        ),
        positions,
    }
}

impl StreamedMutation {
    fn new(key: MutationKey) -> Self {
        Self {
            key,
            cursor: MutationCursor::new(0),
            ordinal: 0,
            prior: MutationIdentity::ROOT,
            totals: MutationTotals::default(),
            items: Vec::with_capacity(COMPOSER_MUTATION_PAGE_ITEMS),
            metadata: Vec::new(),
            text_bytes: 0,
            line_count: 1,
        }
    }

    fn push_repeated_text(
        &mut self,
        repetitions: u64,
        deliver: &mut impl FnMut(MutationPage, Box<[ComposerHostImageMarkerMetadata]>),
    ) {
        let line_breaks =
            u64::try_from(TEXT_PATTERN.bytes().filter(|byte| *byte == b'\n').count()).unwrap();
        let pattern_bytes = u64::try_from(TEXT_PATTERN.len()).unwrap();
        let mut remaining = repetitions;
        while remaining != 0 {
            let item_repetitions = remaining.min(TEXT_PATTERN_REPETITIONS_PER_ITEM);
            self.items.push(MutationPageItem::Utf8 {
                inserted_offset: self.text_bytes,
                text: TEXT_PATTERN
                    .repeat(usize::try_from(item_repetitions).unwrap())
                    .into(),
            });
            self.text_bytes = self
                .text_bytes
                .checked_add(pattern_bytes.checked_mul(item_repetitions).unwrap())
                .unwrap();
            self.line_count = self
                .line_count
                .checked_add(line_breaks.checked_mul(item_repetitions).unwrap())
                .unwrap();
            self.flush_if_full(deliver);
            remaining = remaining.checked_sub(item_repetitions).unwrap();
        }
    }

    fn push_image(
        &mut self,
        draft: SyndicDraftId,
        label: ImageLabelOrdinal,
        offset: u64,
        asset: AssetId,
        deliver: &mut impl FnMut(MutationPage, Box<[ComposerHostImageMarkerMetadata]>),
    ) {
        let marker = Fixture::draft_marker_id(draft, label.get());
        let object = InlineObjectId::new(u128::from_be_bytes(*marker.as_bytes()));
        let order = InlineObjectOrder::new(u128::from(label.get()));
        self.items
            .push(MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(object, ByteOffset::new(offset), order, 17, 5),
            }));
        self.metadata
            .push(ComposerHostImageMarkerMetadata::new(object, asset));
        self.flush_if_full(deliver);
    }

    fn flush_if_full(
        &mut self,
        deliver: &mut impl FnMut(MutationPage, Box<[ComposerHostImageMarkerMetadata]>),
    ) {
        if self.items.len() == COMPOSER_MUTATION_PAGE_ITEMS {
            self.flush(deliver);
        }
    }

    fn flush(
        &mut self,
        deliver: &mut impl FnMut(MutationPage, Box<[ComposerHostImageMarkerMetadata]>),
    ) {
        if self.items.is_empty() {
            return;
        }
        let next_cursor = MutationCursor::new(self.cursor.get().checked_add(1).unwrap());
        let page = MutationPage::new(
            MutationPageKey::new(
                self.key,
                MutationLane::Proposal,
                self.cursor,
                self.ordinal,
                self.prior,
            ),
            next_cursor,
            mem::take(&mut self.items),
        )
        .unwrap();
        let owned_bytes = page
            .items()
            .iter()
            .try_fold(
                page.items()
                    .len()
                    .checked_mul(std::mem::size_of::<MutationPageItem>())
                    .unwrap(),
                |total, item| {
                    let text_bytes = match item {
                        MutationPageItem::Utf8 { text, .. } => text.len(),
                        MutationPageItem::Object(_) => 0,
                        MutationPageItem::Atom(_) => {
                            panic!("submitted-input fixture contains an unsupported atom")
                        }
                    };
                    total.checked_add(text_bytes)
                },
            )
            .and_then(|total| {
                total.checked_add(
                    self.metadata
                        .len()
                        .checked_mul(std::mem::size_of::<ComposerHostImageMarkerMetadata>())
                        .unwrap(),
                )
            })
            .unwrap();
        assert!(
            owned_bytes <= MAX_STREAMED_MUTATION_PAGE_OWNED_BYTES,
            "streamed fixture mutation page owns {owned_bytes} bytes beyond its bounded residency"
        );
        self.totals = add_totals(self.totals, page.totals());
        self.cursor = page.next_cursor();
        self.ordinal = self.ordinal.checked_add(1).unwrap();
        self.prior = page.cumulative_identity();
        deliver(page, mem::take(&mut self.metadata).into_boxed_slice());
    }

    fn finish(&self) -> MutationStreamFinish {
        MutationStreamFinish {
            next_cursor: self.cursor,
            next_ordinal: self.ordinal,
            cumulative_identity: self.prior,
            totals: self.totals,
        }
    }
}

fn add_totals(left: MutationTotals, right: MutationTotals) -> MutationTotals {
    MutationTotals {
        pages: left.pages.checked_add(right.pages).unwrap(),
        items: left.items.checked_add(right.items).unwrap(),
        retained_bytes: left
            .retained_bytes
            .checked_add(right.retained_bytes)
            .unwrap(),
        inserted_bytes: left
            .inserted_bytes
            .checked_add(right.inserted_bytes)
            .unwrap(),
        inserted_line_breaks: left
            .inserted_line_breaks
            .checked_add(right.inserted_line_breaks)
            .unwrap(),
        objects: left.objects.checked_add(right.objects).unwrap(),
        object_bytes: left.object_bytes.checked_add(right.object_bytes).unwrap(),
        presentation_bytes: left
            .presentation_bytes
            .checked_add(right.presentation_bytes)
            .unwrap(),
    }
}
