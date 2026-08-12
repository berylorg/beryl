use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use beryl_app::theme_runtime::{
    AdapterFailureClass, AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
    AppearanceWindowAdapter, DurablePublicationIdentity, PreparedPreviewAppearance,
    PreparedWindowAppearance, PreviewPublicationRequest, WindowAdapterId,
};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::DomainRevision;
use beryl_state::{PreparedThemeAppearance, ThemeDraftIdentity, ThemeDraftRevision, ThemeService};

pub struct StateFixture {
    _directory: tempfile::TempDir,
    pub store: HomeStore,
    pub service: ThemeService,
}

impl StateFixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary Beryl home");
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .expect("open Beryl home");
        let service = ThemeService::acquire(&store).expect("acquire theme service");
        Self {
            _directory: directory,
            store,
            service,
        }
    }

    pub fn prepared(&self, revision: u64) -> PreparedThemeAppearance {
        let settings = self.service.settings_identity(
            DomainRevision::new(revision).expect("nonzero settings revision"),
            None,
        );
        PreparedThemeAppearance::fallback(settings)
    }

    pub fn fresh_service(&self) -> ThemeService {
        ThemeService::acquire(&self.store).expect("fresh theme service")
    }
}

pub fn coordinator(fixture: &StateFixture, capacity: usize) -> AppearanceCoordinator {
    AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(capacity).unwrap()),
        fixture.prepared(1),
    )
}

pub fn settings_identity(
    prepared: &PreparedThemeAppearance,
    draft: u64,
    revision: u64,
) -> DurablePublicationIdentity {
    DurablePublicationIdentity::Settings {
        draft: ThemeDraftIdentity::new(NonZeroU64::new(draft).unwrap()),
        revision: ThemeDraftRevision::new(NonZeroU64::new(revision).unwrap()),
        committed: prepared.settings(),
    }
}

pub fn preview_completion(
    request: &PreviewPublicationRequest,
    prepared: PreparedThemeAppearance,
) -> PreparedPreviewAppearance {
    PreparedPreviewAppearance::new(request.candidate().clone(), prepared)
}

#[derive(Default)]
pub struct AdapterState {
    reject: AtomicBool,
    prepare_count: AtomicUsize,
    commit_count: AtomicUsize,
    current: Mutex<Option<Arc<AppearanceGeneration>>>,
    history: Mutex<Vec<Arc<AppearanceGeneration>>>,
}

impl AdapterState {
    pub fn reject(&self, reject: bool) {
        self.reject.store(reject, Ordering::SeqCst);
    }

    pub fn commit_count(&self) -> usize {
        self.commit_count.load(Ordering::SeqCst)
    }

    pub fn current(&self) -> Arc<AppearanceGeneration> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .expect("adapter must be ready")
    }

    pub fn history(&self) -> Vec<Arc<AppearanceGeneration>> {
        self.history.lock().unwrap().clone()
    }
}

pub struct TestAdapter {
    id: WindowAdapterId,
    pub state: Arc<AdapterState>,
}

impl TestAdapter {
    pub fn new(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id: WindowAdapterId::new(NonZeroU64::new(id).unwrap()),
            state: Arc::new(AdapterState::default()),
        })
    }

    pub fn id(&self) -> WindowAdapterId {
        self.id
    }
}

struct PreparedAdapterPublication {
    state: Arc<AdapterState>,
    generation: Arc<AppearanceGeneration>,
}

impl PreparedWindowAppearance for PreparedAdapterPublication {
    fn commit(self: Box<Self>) {
        *self.state.current.lock().unwrap() = Some(Arc::clone(&self.generation));
        self.state.history.lock().unwrap().push(self.generation);
        self.state.commit_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl AppearanceWindowAdapter for TestAdapter {
    fn id(&self) -> WindowAdapterId {
        self.id
    }

    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        self.state.prepare_count.fetch_add(1, Ordering::SeqCst);
        if self.state.reject.load(Ordering::SeqCst) {
            return Err(AdapterFailureClass::Rejected);
        }
        Ok(Box::new(PreparedAdapterPublication {
            state: Arc::clone(&self.state),
            generation,
        }))
    }
}
