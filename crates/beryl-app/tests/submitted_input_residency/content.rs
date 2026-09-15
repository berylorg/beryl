use std::path::PathBuf;

use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    ContentReference,
    test_faults::{ComposerV1AtomWriter, ComposerV1FoldError, plan_composer_v1},
};

use crate::{
    syndic::{Fixture, SubmittedTurn, point_limit},
    wire::{InputSpec, TEXT_PATTERN},
};

const SHARED_IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nbounded-sidecar";

#[path = "content/mutation.rs"]
mod mutation;
use mutation::commit_logical_input;

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
    let runtime_path = shared_image.map(|image| {
        image
            .path
            .to_str()
            .expect("fixture sidecar path must be Unicode")
    });
    let wire = shape.wire_spec(runtime_path);
    let marker_assets = shared_image
        .map(|image| vec![image.asset])
        .unwrap_or_default();
    let assets = fixture.state.assets();
    let (kind, source_draft) = fixture.submit_via_composer(thread, move |host, home, binding| {
        commit_logical_input(host, home, binding, shape, &assets, &marker_assets)
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
        descriptor_count: shape.atom_count(),
        authored_logical_text_bytes: shape.authored_logical_text_bytes(),
        composer_max_buffer_bytes: maximum_buffer_bytes,
    }
}
