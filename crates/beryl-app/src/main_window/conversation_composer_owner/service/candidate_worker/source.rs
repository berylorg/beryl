use super::*;
use crate::main_window::MainWindowComposerSelectionIdentity;
use gpui_text_input::RangeRestorationSeed;

pub trait MainWindowComposerPrepublicationSource: Send + 'static {
    fn seed(&self) -> RangeRestorationSeed;
    fn selection(&self) -> MainWindowComposerSelectionIdentity;
    fn retain_cleanup(
        &self,
        cleanup: Arc<MainWindowNativeLineagePrepublicationSource>,
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String>;
}

impl MainWindowComposerPrepublicationSource for MainWindowComposerCandidateSource {
    fn seed(&self) -> RangeRestorationSeed {
        self.seed()
    }
    fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection()
    }
    fn retain_cleanup(
        &self,
        cleanup: Arc<MainWindowNativeLineagePrepublicationSource>,
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String> {
        self.retain_cleanup(cleanup, executor)
    }
}

impl<C, S: MainWindowComposerPrepublicationSource, R> MainWindowComposerCandidateWorker<C, S, R> {
    pub(in crate::main_window) fn from_source(
        candidate: C,
        source: S,
        read: CandidateRead<C, S>,
    ) -> (Self, MainWindowComposerCandidateCustody<C, S, R>) {
        let state = Rc::new(RefCell::new(State {
            generation: None,
            resources: Some((candidate, Ok(source))),
            read,
            completion: None,
            pending: false,
            cancelled: false,
            cleanup: None,
        }));
        (
            Self {
                state: state.clone(),
            },
            MainWindowComposerCandidateCustody { state },
        )
    }
}
