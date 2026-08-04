use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{CommandError, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasItemId, CasThreadId, CasTurnId, ProviderObservationId, SyndicContentId, SyndicItemId,
    SyndicTurnId,
};
use syndic_storage::*;

#[path = "phase18_provider_observation_compiler/large.rs"]
mod large;
#[path = "phase18_provider_observation_compiler/parity.rs"]
mod parity;
#[path = "phase18_provider_observation_compiler/structured.rs"]
mod structured;

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-provider-observation-compiler-{name}-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn route() -> ProviderObservationRoute {
    ProviderObservationRoute::new(
        CasThreadId::new("compiler-thread").unwrap(),
        CasTurnId::new("compiler-turn").unwrap(),
    )
}

fn source(item: &str) -> CasItemSource {
    CasItemSource::new(
        CasTurnSource::new(
            CasThreadId::new("compiler-thread").unwrap(),
            CasTurnId::new("compiler-turn").unwrap(),
        ),
        CasItemId::new(item).unwrap(),
    )
}

fn observation_callback(
    store: &HomeStore,
    storage: SyndicStorage,
) -> impl FnMut(&ProviderObservationStageBatch) -> Result<(), CommandError> + '_ {
    move |batch| {
        store
            .execute_current(storage.current_stage_provider_observation_batch(batch.clone()))
            .map(|_| ())
    }
}

fn control(
    stager: &mut ProviderObservationStager,
    value: ProviderObservationControl,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    stager.control(value, callback).unwrap();
}

fn scalar(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderScalar,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    control(
        stager,
        ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(field),
            value,
        },
        callback,
    );
}

fn enum_value(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    control(
        stager,
        ProviderObservationControl::Enum {
            context: ProviderValueContext::Field(field),
            value,
        },
        callback,
    );
}

fn text(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    pieces: &[&[u8]],
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    control(
        stager,
        ProviderObservationControl::BeginField(context),
        callback,
    );
    for piece in pieces {
        stager
            .fragment(
                ProviderObservationStagingBytes::new(context, piece).unwrap(),
                callback,
            )
            .unwrap();
    }
    control(
        stager,
        ProviderObservationControl::EndField(context),
        callback,
    );
}

fn field_text(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    pieces: &[&[u8]],
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    text(stager, ProviderValueContext::Field(field), pieces, callback);
}

fn begin_container(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    control(
        stager,
        ProviderObservationControl::BeginContainer { context, container },
        callback,
    );
}

fn end_container(
    stager: &mut ProviderObservationStager,
    context: ProviderValueContext,
    container: ProviderContainer,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) {
    control(
        stager,
        ProviderObservationControl::EndContainer { context, container },
        callback,
    );
}

fn bind_sealed(
    stager: ProviderObservationStager,
    callback: &mut impl ProviderObservationStageCallback<Error = CommandError>,
) -> BoundProviderObservation {
    stager
        .seal(callback)
        .unwrap()
        .bind(route(), route())
        .unwrap()
}

fn prepare_first(
    storage: &SyndicStorage,
    store: &HomeStore,
    bound: BoundProviderObservation,
    item_id: &str,
    expected_kind: ProviderItemKind,
    content_byte: u8,
) -> PreparedProviderObservationFrame {
    let inspected = inspect_provider_observation(storage, store, bound, limit()).unwrap();
    assert_eq!(inspected.item_id().as_str(), item_id);
    assert_eq!(inspected.item_kind(), expected_kind);
    assert_eq!(inspected.route(), &route());
    prepare_provider_observation_frame(
        storage,
        store,
        inspected,
        ProviderObservationFramePreparationPlan::first(
            SyndicItemId::from_bytes([2; 16]),
            SyndicTurnId::from_bytes([3; 16]),
            source(item_id),
            SourceEventSequence::FIRST,
            SyndicContentId::from_bytes([content_byte; 16]),
        ),
        limit(),
    )
    .unwrap()
}

#[derive(Default)]
struct MaterializedCapture {
    bytes: Vec<u8>,
    spans: Vec<ProviderFrameTextSpanV1>,
}

impl ProviderFrameSinkV1 for MaterializedCapture {
    type Error = Infallible;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.push(span);
        Ok(())
    }
}

fn materialized(frame: &ProviderItemFrameV1) -> (MaterializedCapture, ProviderFrameReferenceV1) {
    let mut capture = MaterializedCapture::default();
    let reference = encode_provider_item_frame_v1(frame, 0, &mut capture).unwrap();
    (capture, reference)
}

#[derive(Default)]
struct CompilerCapture {
    bytes: Vec<u8>,
    batches: usize,
    narrative_spans: usize,
}

fn stage_compiler(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedProviderObservationFrame,
) -> (CompilerCapture, ProviderItemBuildRecord) {
    let mut capture = CompilerCapture::default();
    let final_build = stage_provider_observation_frame(
        storage,
        store,
        prepared,
        prepared.initial_build().clone(),
        limit(),
        &mut |batch: &ProviderFrameStageBatch| {
            capture.batches += 1;
            assert!(batch.chunks().len() <= CONTENT_APPEND_MAX_CHUNKS);
            assert!(batch.narrative_spans().len() <= PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS);
            for chunk in batch.chunks() {
                capture.bytes.extend_from_slice(chunk.bytes());
            }
            capture.narrative_spans += batch.narrative_spans().len();
            Ok::<_, Infallible>(())
        },
    )
    .unwrap();
    (capture, final_build)
}
