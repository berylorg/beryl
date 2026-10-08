use super::*;
use crate::main_window::{
    MainWindowComposerRetiredClose, MainWindowComposerSelectionIdentity,
    MainWindowConversationComposerConfig,
};
use gpui_text_input::{RangePrepublicationCurrent, RangeSurfaceCharge};

pub(crate) type ResidentRecoveryConfigurator = Box<
    dyn FnMut(
        MainWindowComposerSelectionIdentity,
    ) -> Result<(MainWindowConversationComposerConfig, RangeSurfaceCharge), String>,
>;

pub(super) struct ResidentWindowConfiguration {
    configure: ResidentRecoveryConfigurator,
    selection: Option<MainWindowComposerSelectionIdentity>,
}

impl ResidentWindowConfiguration {
    pub(super) fn new(
        configure: ResidentRecoveryConfigurator,
    ) -> (
        Rc<RefCell<Self>>,
        MainWindowConversationComposerConfigurator,
    ) {
        let retained = Rc::new(RefCell::new(Self {
            configure,
            selection: None,
        }));
        let configurator = Self::mount_configurator(&retained);
        (retained, configurator)
    }

    pub(super) fn mount_configurator(
        retained: &Rc<RefCell<Self>>,
    ) -> MainWindowConversationComposerConfigurator {
        let mounted = retained.clone();
        Box::new(move |selection| {
            (mounted.borrow_mut().configure)(selection).map(|(config, _)| config)
        })
    }

    pub(super) fn prepare(
        retained: Rc<RefCell<Self>>,
        owner: &Rc<RefCell<RunningProcessOwner>>,
        request: &impl RecoveryIdentity,
        window: WindowHandle<MainWindowShellRoot>,
        generation: HomeGeneration,
        retirement: &mut Option<MainWindowComposerRetiredClose>,
        app: &mut App,
    ) -> Result<resident::ResidentPreparationKey, String> {
        RunningProcessOwner::prepare_interrupted_exit_window_resident(
            owner,
            request,
            window,
            generation,
            retirement,
            move |_, selection, _| {
                let mut retained = retained.borrow_mut();
                let configured = (retained.configure)(selection)?;
                retained.selection = Some(selection);
                Ok(configured)
            },
            app,
            |_, _| {},
        )
    }

    pub(super) fn current(&mut self) -> Result<RangePrepublicationCurrent, String> {
        let selection = self
            .selection
            .ok_or("Recovery resident configuration is unavailable")?;
        let (config, _) = (self.configure)(selection)?;
        Ok(config.native_lineage_current())
    }
}
