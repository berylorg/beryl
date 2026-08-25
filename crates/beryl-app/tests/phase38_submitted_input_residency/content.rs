use std::{mem, path::PathBuf};

use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{AssetId, ContentRevision, ImageLabelOrdinal, SyndicDraftId, SyndicThreadId};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, ObjectChange, OperationId, SourcePosition, SourceRange,
    SourceRevision, SuccessorObject,
};
use syndic_storage::{
    ContentEncoding, ContentReference,
    test_faults::{ComposerV1AtomWriter, ComposerV1FoldError, plan_composer_v1},
};

use crate::{
    syndic::{Fixture, SubmittedTurn, point_limit},
    wire::{InputSpec, TEXT_PATTERN},
};

const SHARED_IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nphase38-bounded-sidecar";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalInput {
    MarkerFree {
        repetitions: u64,
    },
    AlternatingImages {
        marker_count: u64,
        repetitions_per_text: u64,
    },
}

impl LogicalInput {
    pub const fn marker_free(repetitions: u64) -> Self {
        assert!(repetitions != 0);
        Self::MarkerFree { repetitions }
    }

    pub const fn alternating_images(marker_count: u64, repetitions_per_text: u64) -> Self {
        assert!(marker_count != 0);
        assert!(repetitions_per_text != 0);
        Self::AlternatingImages {
            marker_count,
            repetitions_per_text,
        }
    }

    pub fn atom_count(self) -> u64 {
        match self {
            Self::MarkerFree { .. } => 1,
            Self::AlternatingImages { marker_count, .. } => marker_count
                .checked_mul(2)
                .and_then(|count| count.checked_add(1))
                .expect("synthetic composer atom frontier must fit u64"),
        }
    }

    pub fn authored_logical_text_bytes(self) -> u64 {
        let pattern_bytes = TEXT_PATTERN.len() as u64;
        match self {
            Self::MarkerFree { repetitions } => pattern_bytes
                .checked_mul(repetitions)
                .expect("synthetic text frontier must fit u64"),
            Self::AlternatingImages {
                marker_count,
                repetitions_per_text,
            } => pattern_bytes
                .checked_mul(repetitions_per_text)
                .and_then(|bytes| bytes.checked_mul(marker_count.checked_add(1)?))
                .expect("synthetic marker-aware text frontier must fit u64"),
        }
    }

    const fn marker_count(self) -> Option<u64> {
        match self {
            Self::MarkerFree { .. } => None,
            Self::AlternatingImages { marker_count, .. } => Some(marker_count),
        }
    }

    pub const fn image_count(self) -> u64 {
        match self {
            Self::MarkerFree { .. } => 0,
            Self::AlternatingImages { marker_count, .. } => marker_count,
        }
    }

    fn wire_spec(self, runtime_path: Option<&str>) -> InputSpec {
        match self {
            Self::MarkerFree { repetitions } => InputSpec::marker_free(repetitions),
            Self::AlternatingImages {
                marker_count,
                repetitions_per_text,
            } => InputSpec::alternating_images(
                marker_count,
                repetitions_per_text,
                runtime_path.expect("marker-aware input requires one verified runtime path"),
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SharedImage {
    pub asset: AssetId,
    pub path: PathBuf,
}

#[derive(Clone, Copy)]
pub struct SeededInput {
    pub submitted: SubmittedTurn,
    pub content: ContentReference,
    pub wire: InputSpec,
    pub descriptor_count: u64,
    pub authored_logical_text_bytes: u64,
    pub composer_max_buffer_bytes: usize,
}

pub fn publish_shared_image(fixture: &mut Fixture) -> SharedImage {
    let (asset, path) = fixture.publish_asset_metadata(SHARED_IMAGE_BYTES);
    SharedImage { asset, path }
}

fn drive_atom<E>(
    shape: LogicalInput,
    draft: SyndicDraftId,
    index: u64,
    writer: &mut dyn ComposerV1AtomWriter<SinkError = E>,
) -> Result<(), ComposerV1FoldError<E>> {
    match shape {
        LogicalInput::MarkerFree { repetitions } => {
            assert_eq!(index, 0);
            write_repeated_text(writer, repetitions)
        }
        LogicalInput::AlternatingImages {
            marker_count,
            repetitions_per_text,
        } if index % 2 == 0 => {
            assert!(index <= marker_count.checked_mul(2).unwrap());
            write_repeated_text(writer, repetitions_per_text)
        }
        LogicalInput::AlternatingImages { marker_count, .. } => {
            let ordinal = index.checked_add(1).unwrap() / 2;
            assert!(ordinal <= marker_count);
            writer.image_marker(
                Fixture::draft_marker_id(draft, ordinal),
                ImageLabelOrdinal::new(ordinal).unwrap(),
            )
        }
    }
}

fn write_repeated_text<E>(
    writer: &mut dyn ComposerV1AtomWriter<SinkError = E>,
    repetitions: u64,
) -> Result<(), ComposerV1FoldError<E>> {
    let bytes = (TEXT_PATTERN.len() as u64)
        .checked_mul(repetitions)
        .expect("synthetic text frontier must fit u64");
    writer.begin_text(bytes)?;
    for _ in 0..repetitions {
        writer.text_fragment(TEXT_PATTERN)?;
    }
    writer.end_text()
}

const COMPOSER_MUTATION_PAGE_ITEMS: usize = 64;

fn commit_logical_input(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    shape: LogicalInput,
    assets: &[AssetId],
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(1),
    );
    let origin = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(origin),
                SourceRange::new(origin, origin).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();

    let mut stream = StreamedMutation::new(key);
    match shape {
        LogicalInput::MarkerFree { repetitions } => {
            stream.push_repeated_text(host, store, repetitions);
        }
        LogicalInput::AlternatingImages {
            marker_count,
            repetitions_per_text,
        } => {
            for segment in 0..=marker_count {
                stream.push_repeated_text(host, store, repetitions_per_text);
                if segment < marker_count {
                    let ordinal = segment.checked_add(1).unwrap();
                    stream.push_image(
                        host,
                        store,
                        binding.candidate().draft_id(),
                        ImageLabelOrdinal::new(ordinal).unwrap(),
                        *assets
                            .first()
                            .expect("marker-aware input requires one asset"),
                    );
                }
            }
        }
    }
    stream.flush(host, store);
    let proposal_finish = stream.finish();
    let caret = SourcePosition::new(
        ByteOffset::new(stream.text_bytes),
        stream
            .last_neighbor
            .map_or(InlineObjectGap::NoObjects, InlineObjectGap::after),
    );
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            MutationStreamFinish {
                next_cursor: MutationCursor::new(0),
                next_ordinal: 0,
                cumulative_identity: MutationIdentity::ROOT,
                totals: MutationTotals::default(),
            },
            proposal_finish,
            LogicalExtent::new(stream.text_bytes, stream.line_count),
            MutationPositions::collapsed(caret),
        ),
    )
    .unwrap();

    let maximum_steps = usize::try_from(stream.totals.pages)
        .unwrap()
        .saturating_mul(4)
        .saturating_add(32);
    for _ in 0..maximum_steps {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("streamed fixture mutation did not commit: {other:?}"),
        }
    }
    panic!("streamed fixture mutation remained pending")
}

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
    last_neighbor: Option<InlineObjectNeighbor>,
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
            last_neighbor: None,
        }
    }

    fn push_repeated_text(
        &mut self,
        host: &mut SyndicComposerHost,
        store: &HomeStore,
        repetitions: u64,
    ) {
        for _ in 0..repetitions {
            self.items.push(MutationPageItem::Utf8 {
                inserted_offset: self.text_bytes,
                text: TEXT_PATTERN.into(),
            });
            self.text_bytes = self
                .text_bytes
                .checked_add(u64::try_from(TEXT_PATTERN.len()).unwrap())
                .unwrap();
            self.line_count = self
                .line_count
                .checked_add(
                    u64::try_from(TEXT_PATTERN.bytes().filter(|byte| *byte == b'\n').count())
                        .unwrap(),
                )
                .unwrap();
            self.flush_if_full(host, store);
        }
    }

    fn push_image(
        &mut self,
        host: &mut SyndicComposerHost,
        store: &HomeStore,
        draft: SyndicDraftId,
        label: ImageLabelOrdinal,
        asset: AssetId,
    ) {
        let marker = Fixture::draft_marker_id(draft, label.get());
        let object = InlineObjectId::new(u128::from_be_bytes(*marker.as_bytes()));
        let order = InlineObjectOrder::new(u128::from(label.get()));
        self.items
            .push(MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    object,
                    ByteOffset::new(self.text_bytes),
                    order,
                    17,
                    5,
                ),
            }));
        self.metadata
            .push(ComposerHostImageMarkerMetadata::new(object, label, asset));
        self.last_neighbor = Some(InlineObjectNeighbor::new(object, order));
        self.flush_if_full(host, store);
    }

    fn flush_if_full(&mut self, host: &mut SyndicComposerHost, store: &HomeStore) {
        if self.items.len() == COMPOSER_MUTATION_PAGE_ITEMS {
            self.flush(host, store);
        }
    }

    fn flush(&mut self, host: &mut SyndicComposerHost, store: &HomeStore) {
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
        self.totals = add_totals(self.totals, page.totals());
        self.cursor = page.next_cursor();
        self.ordinal = self.ordinal.checked_add(1).unwrap();
        self.prior = page.cumulative_identity();
        host.stage_mutation_page(
            store,
            MutationPageRequest::new(page),
            mem::take(&mut self.metadata).into_boxed_slice(),
        )
        .unwrap();
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

pub fn seed_submitted_input(
    fixture: &mut Fixture,
    thread: SyndicThreadId,
    shape: LogicalInput,
    shared_image: Option<&SharedImage>,
) -> SeededInput {
    let draft = fixture
        .storage
        .current_draft(&*fixture.home(), thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .id();
    let atom_count = shape.atom_count();
    let plan = plan_composer_v1(atom_count, |index, writer| {
        drive_atom(shape, draft, index, writer)
    })
    .unwrap();
    let maximum_buffer_bytes = plan.max_buffer_bytes();
    assert!(maximum_buffer_bytes <= syndic_storage::CONTENT_CHUNK_MAX_BYTES);
    let planned_reference = ContentReference::new(
        plan.content_id(),
        ContentRevision::new(1).unwrap(),
        ContentEncoding::ComposerV1,
        plan.summary(),
    );

    let _asset_reference_set = match shape.marker_count() {
        None => None,
        Some(marker_count) => {
            let image = shared_image.expect("marker-aware input requires the shared image");
            Some(
                fixture.seal_repeated_asset_reference_set(
                    draft,
                    planned_reference
                        .sealed_marker_summary()
                        .unwrap()
                        .sequential(),
                    marker_count,
                    image.asset,
                ),
            )
        }
    };
    let runtime_path = shared_image.map(|image| {
        image
            .path
            .to_str()
            .expect("fixture sidecar path must be Unicode")
    });
    let wire = shape.wire_spec(runtime_path);
    let assets = shared_image
        .map(|image| vec![image.asset])
        .unwrap_or_default();
    let (kind, source_draft) = fixture.submit_via_composer(thread, move |host, home, binding| {
        commit_logical_input(host, home, binding, shape, &assets)
    });
    let syndic_storage::FirstAcceptanceKind::Idle { user_item_id } = kind else {
        panic!("submitted-content fixture expected an idle thread")
    };
    let submitted = SubmittedTurn {
        turn: source_draft.submitted_turn_id(),
        user_item: user_item_id,
    };
    let reference = fixture.submitted_content(submitted);
    SeededInput {
        submitted,
        content: reference,
        wire,
        descriptor_count: reference.summary().atom_count(),
        authored_logical_text_bytes: shape.authored_logical_text_bytes(),
        composer_max_buffer_bytes: maximum_buffer_bytes,
    }
}
