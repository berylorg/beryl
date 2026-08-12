use std::{convert::Infallible, mem, path::PathBuf};

use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{AssetId, ContentRevision, ImageLabelOrdinal, SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    ContentEncoding, ContentLifecycle, ContentManifestRecord, ContentReference,
    test_faults::{
        ComposerV1AtomWriter, ComposerV1FoldError, ComposerV1RecordSink, FixtureBatch,
        FixtureRecord, fold_composer_v1, plan_composer_v1,
    },
};

use crate::{
    syndic::{Fixture, SubmittedTurn, point_limit},
    wire::{InputSpec, TEXT_PATTERN},
};

const FIXTURE_BATCH_RECORDS: usize = 64;
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

    pub fn descriptor_count(self) -> u64 {
        self.atom_count()
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

struct DurableRecordSink<'a> {
    fixture: &'a Fixture,
    batch: FixtureBatch,
    records: usize,
}

impl<'a> DurableRecordSink<'a> {
    fn new(fixture: &'a Fixture) -> Self {
        Self {
            fixture,
            batch: FixtureBatch::new(),
            records: 0,
        }
    }

    fn reserve(&mut self, records: usize) {
        assert!(records <= FIXTURE_BATCH_RECORDS);
        if self.records.checked_add(records).unwrap() > FIXTURE_BATCH_RECORDS {
            self.flush();
        }
    }

    fn put(&mut self, record: FixtureRecord) {
        self.batch.put(record).unwrap();
        self.records = self.records.checked_add(1).unwrap();
    }

    fn flush(&mut self) {
        if self.records == 0 {
            return;
        }
        let batch = mem::take(&mut self.batch);
        let home = self.fixture.home();
        let contribution = self
            .fixture
            .storage
            .fixture_contribution(self.fixture.storage.revision(&home).unwrap(), batch);
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command.add(contribution).unwrap();
        match home.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::NotCommitted { .. } => {
                panic!("expected committed content mutation, got {outcome:?}")
            }
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("expected no later failure, got {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("expected committed content mutation, got {outcome:?}")
            }
        }
        self.records = 0;
    }
}

impl ComposerV1RecordSink for DurableRecordSink<'_> {
    type Error = Infallible;

    fn chunk(
        &mut self,
        chunk: syndic_storage::ContentChunkRecord,
        span: syndic_storage::ContentByteSpanRecord,
    ) -> Result<(), Self::Error> {
        self.reserve(2);
        self.put(FixtureRecord::ContentChunk(chunk));
        self.put(FixtureRecord::ContentByteSpan(span));
        Ok(())
    }

    fn text_piece(
        &mut self,
        span: syndic_storage::ContentTextSpanRecord,
        piece: syndic_storage::ContentPieceRecord,
    ) -> Result<(), Self::Error> {
        self.reserve(2);
        self.put(FixtureRecord::ContentTextSpan(span));
        self.put(FixtureRecord::ContentPiece(piece));
        Ok(())
    }

    fn image_piece(
        &mut self,
        piece: syndic_storage::ContentPieceRecord,
    ) -> Result<(), Self::Error> {
        self.reserve(1);
        self.put(FixtureRecord::ContentPiece(piece));
        Ok(())
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
    let (reference, maximum_buffer_bytes) = {
        let mut sink = DurableRecordSink::new(fixture);
        let outcome = fold_composer_v1(plan, &mut sink, |index, writer| {
            drive_atom(shape, draft, index, writer)
        })
        .unwrap();
        sink.flush();
        assert_eq!(outcome.content_id(), plan.content_id());
        assert_eq!(outcome.summary(), plan.summary());
        assert_eq!(outcome.max_buffer_bytes(), plan.max_buffer_bytes());
        assert!(outcome.max_buffer_bytes() <= syndic_storage::CONTENT_CHUNK_MAX_BYTES);

        let summary = outcome.summary();
        let revision = ContentRevision::new(1).unwrap();
        let reference = ContentReference::new(
            outcome.content_id(),
            revision,
            ContentEncoding::ComposerV1,
            summary,
        );
        let manifest = ContentManifestRecord::new(
            reference.id(),
            revision,
            reference.encoding(),
            ContentLifecycle::Sealed,
            summary.chunk_count(),
            summary.encoded_bytes(),
            summary.digest(),
            summary,
        );
        sink.reserve(1);
        sink.put(FixtureRecord::ContentManifest(manifest));
        sink.flush();
        (reference, outcome.max_buffer_bytes())
    };

    let asset_reference_set = match shape.marker_count() {
        None => None,
        Some(marker_count) => {
            let image = shared_image.expect("marker-aware input requires the shared image");
            Some(fixture.seal_repeated_asset_reference_set(
                draft,
                reference.sealed_marker_summary().unwrap(),
                marker_count,
                image.asset,
            ))
        }
    };
    let runtime_path = shared_image.map(|image| {
        image
            .path
            .to_str()
            .expect("fixture sidecar path must be Unicode")
    });
    let wire = shape.wire_spec(runtime_path);
    let submitted = fixture.submit_reference_on(thread, reference, asset_reference_set);
    SeededInput {
        submitted,
        content: reference,
        wire,
        descriptor_count: shape.descriptor_count(),
        authored_logical_text_bytes: shape.authored_logical_text_bytes(),
        composer_max_buffer_bytes: maximum_buffer_bytes,
    }
}
