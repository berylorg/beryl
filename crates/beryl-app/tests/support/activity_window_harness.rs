use super::*;
use beryl_model::{SyndicItemId, WindowId};
use beryl_state::{BerylState, WindowClaimSelection};
use std::{ops::Range, sync::atomic::AtomicBool};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityWindowTestStep {
    Pending,
    Ready,
    Failed,
    Retired,
}

#[derive(Debug)]
pub struct ActivityWindowTestSnapshot {
    pub total: u64,
    pub visible: bool,
    pub rows: Vec<(u64, SyndicItemId, String, String)>,
}

pub struct ActivityWindowTestPrepared(ActivityPreparedWindow);
pub struct ActivityWindowTestMetadataRequest(viewport::ActivityMetadataRequest);
pub struct ActivityWindowTestMetadataCompletion(viewport::ActivityMetadataCompletion);

impl ActivityWindowTestMetadataRequest {
    pub fn complete(
        &self,
        metadata: &beryl_backend::ThreadReadMetadata,
    ) -> Option<ActivityWindowTestMetadataCompletion> {
        self.0
            .complete(metadata)
            .map(ActivityWindowTestMetadataCompletion)
    }
}

impl ActivityWindowTestPrepared {
    pub fn first_thread(&self) -> Option<beryl_model::CasThreadId> {
        self.0
            .pages()
            .iter()
            .flat_map(|page| &page.rows)
            .next()
            .map(|row| row.source().cas_item().turn().thread_id().clone())
    }
    pub fn metadata_request(
        &self,
        thread: &beryl_model::CasThreadId,
    ) -> Option<ActivityWindowTestMetadataRequest> {
        self.0
            .metadata_request(thread)
            .map(ActivityWindowTestMetadataRequest)
    }
    pub fn publish(&self) -> Result<ActivityWindowTestSnapshot, String> {
        self.0
            .with_current(|prepared| ActivityWindowTestSnapshot {
                total: prepared.head().logical_row_count(),
                visible: prepared.auto_visible(),
                rows: prepared
                    .pages()
                    .iter()
                    .flat_map(|page| {
                        page.rows.iter().enumerate().map(|(index, row)| {
                            (
                                page.start + index as u64,
                                row.item_id(),
                                row.agent().to_owned(),
                                row.value().to_owned(),
                            )
                        })
                    })
                    .collect(),
            })
            .map_err(|error| error.to_string())
    }
}

pub struct ActivityWindowHarness {
    reader: ActivityWindowReader,
    selection: ActivityWindowSelection,
    service: ActivityService,
    viewport: ActivityViewport,
    demands: ActivityDemandOwner,
    graph: Option<Arc<()>>,
    window: Option<Arc<()>>,
    cancellation: AtomicBool,
}

impl ActivityWindowHarness {
    pub fn new(
        home: &HomeStore,
        storage: &SyndicStorage,
        state: &BerylState,
        runtime: &crate::cas_projection::RuntimeInterestTestHarness,
        window: WindowId,
        claim: WindowClaimSelection,
        rows: usize,
        raw_pages: usize,
    ) -> Self {
        let runtime = runtime
            .activity_read_source(home.home_id(), home.health().generation().unwrap())
            .unwrap();
        let service = ActivityService::dormant(
            Resources {
                home: home.service_reference(),
                storage: storage.clone(),
                runtime,
            },
            ActivityServiceLimits::new(
                NonZeroUsize::new(4).unwrap(),
                NonZeroUsize::new(raw_pages).unwrap(),
                CursorReadLimits::new(rows, 262144).unwrap(),
            ),
            Some(home.generation_identity().unwrap()),
        );
        service.shared.state.lock().unwrap().live = true;
        let graph = Arc::new(());
        let lifetime = Arc::new(());
        let mutations = home
            .observe_mutations(std::task::Waker::noop().clone())
            .unwrap();
        let reader = service.window_reader(state.session(), mutations, Arc::downgrade(&graph));
        let selection = reader.select(window, claim, Arc::downgrade(&lifetime));
        let demands = ActivityDemandOwner::new();
        let viewport = ActivityViewport::new(selection.clone(), demands.request(0..1).unwrap());
        Self {
            reader,
            selection,
            service,
            viewport,
            demands,
            graph: Some(graph),
            window: Some(lifetime),
            cancellation: AtomicBool::new(false),
        }
    }

    pub(crate) fn window_reader(&self) -> ActivityWindowReader {
        self.reader.clone()
    }

    pub(crate) fn viewport_mut(&mut self) -> &mut ActivityViewport {
        &mut self.viewport
    }

    pub fn collection_count(&self) -> usize {
        self.service.shared.collections.load(Ordering::Acquire)
    }

    pub fn save_height(&self, value: f32) -> Result<(), String> {
        self.selection
            .save_height(beryl_state::ActivityPanelHeight::new(value).unwrap())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn retained_height_outcome(&self) -> Option<(&'static str, bool)> {
        let window = self.selection.window_id();
        self.reader
            .height_flight(window)?
            .with_outcome(|result| match result {
                Ok(ActivityHeightCommit::Committed {
                    local_finalization, ..
                }) => ("committed", local_finalization.is_some()),
                Ok(ActivityHeightCommit::NotCommitted { .. }) => ("not_committed", false),
                Ok(ActivityHeightCommit::Indeterminate { .. }) => ("indeterminate", true),
                Err(_) => ("rejected", false),
            })
    }

    pub fn demand(&mut self, range: Range<u64>) {
        self.viewport
            .set_demand(self.demands.request(range).unwrap());
    }
    pub fn step(&mut self) -> ActivityWindowTestStep {
        match self.viewport.step(&self.cancellation) {
            ActivityViewportStep::Pending => ActivityWindowTestStep::Pending,
            ActivityViewportStep::Ready => ActivityWindowTestStep::Ready,
            ActivityViewportStep::Failed => ActivityWindowTestStep::Failed,
            ActivityViewportStep::Retired => ActivityWindowTestStep::Retired,
        }
    }
    pub fn prepared(&self) -> Option<ActivityWindowTestPrepared> {
        self.viewport.prepared().map(ActivityWindowTestPrepared)
    }
    pub fn retry(&mut self) -> bool {
        self.viewport.retry()
    }
    pub fn apply_metadata(&mut self, completion: &ActivityWindowTestMetadataCompletion) -> bool {
        self.viewport.set_metadata(&completion.0)
    }
    pub fn request_state(&self) -> (bool, bool, u64) {
        self.viewport.request_state()
    }
    pub fn with_page_capacity_held<T>(&mut self, callback: impl FnOnce(&mut Self) -> T) -> T {
        let capacity = self.service.shared.reserve(CapacityKind::Page).unwrap();
        let result = callback(self);
        drop(capacity);
        result
    }
    pub fn with_publication_gate_held<T>(&mut self, callback: impl FnOnce(&mut Self) -> T) -> T {
        let shared = Arc::clone(&self.service.shared);
        let _guard = shared.state.lock().unwrap();
        callback(self)
    }
    pub fn failure(&self) -> Option<&str> {
        self.viewport.failure()
    }
    pub fn eligibility(&self) -> Result<bool, String> {
        self.viewport
            .publish_facts(|facts| facts.auto_visible)
            .map_err(|error| error.to_string())
    }
    pub fn diagnostics(&self) -> (usize, usize, bool, usize) {
        let (checkpoints, resident, preparing) = self.viewport.diagnostics();
        (
            checkpoints,
            resident,
            preparing,
            self.service.shared.pages.load(Ordering::Acquire),
        )
    }
    pub fn retire_graph(&mut self) {
        self.graph.take();
    }
    pub fn retire_window(&mut self) {
        self.window.take();
    }
    pub fn cancel(&self) {
        self.cancellation.store(true, Ordering::Release);
    }
    pub fn retire_service(&self) {
        self.service.retire();
    }
}
