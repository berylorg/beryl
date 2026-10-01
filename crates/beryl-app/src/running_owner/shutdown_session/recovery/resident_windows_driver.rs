use super::*;
use crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters;
use crate::main_window::{
    MainWindowConversationComposerCloseTicket, MainWindowConversationComposerConfigurator,
    MainWindowShellRoot,
};
use crate::theme_runtime::GpuiAppearanceWindowSet;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};

pub(crate) struct ResidentRecoveryWindow {
    window: WindowHandle<MainWindowShellRoot>,
    preparation: Option<resident::ResidentPreparationKey>,
    adapters: Option<PreparedComposerRecoveryAdapters>,
    configurator: Option<MainWindowConversationComposerConfigurator>,
    attached: Option<(
        MainWindowConversationComposerCloseTicket,
        beryl_state::SessionWindowRecord,
    )>,
    bound: bool,
}

impl ResidentRecoveryWindow {
    pub(crate) fn new(
        window: WindowHandle<MainWindowShellRoot>,
        adapters: PreparedComposerRecoveryAdapters,
        configurator: MainWindowConversationComposerConfigurator,
    ) -> Self {
        Self {
            window,
            preparation: None,
            adapters: Some(adapters),
            configurator: Some(configurator),
            attached: None,
            bound: false,
        }
    }
}

impl RunningProcessOwner {
    pub(crate) async fn prepare_and_complete_interrupted_exit_resident_windows(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        windows: &mut [ResidentRecoveryWindow],
        appearance: &Entity<GpuiAppearanceWindowSet>,
        mut admit: impl FnMut(usize, &mut App) -> Result<resident::ResidentPreparationKey, String>,
        mut current: impl FnMut(
            usize,
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        if cancellation.is_cancelled() {
            return Err("Interrupted Exit preparation was cancelled".into());
        }
        cx.update(|app| -> Result<(), String> {
            let owner = owner.borrow();
            let shells = owner.process.windows.shells();
            if windows.is_empty()
                || windows.len() != shells.len()
                || windows.iter().enumerate().any(|(index, entry)| {
                    !shells.iter().any(|shell| shell.window() == entry.window)
                        || windows[..index]
                            .iter()
                            .any(|earlier| earlier.window == entry.window)
                })
            {
                return Err(
                    "Interrupted Exit requires the complete retained selected window set".into(),
                );
            }
            for entry in windows.iter() {
                let root = entry.window.read(app).map_err(|error| error.to_string())?;
                if !root
                    .controller()
                    .is_some_and(|controller| controller.composer_mount().is_some())
                {
                    return Err("Interrupted Exit requires selected windows".into());
                }
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
        for (index, entry) in windows.iter_mut().enumerate() {
            if entry.attached.is_none() {
                Self::attach_and_retain_interrupted_exit_resident_pass(
                    owner,
                    request,
                    &mut entry.preparation,
                    |app| admit(index, app),
                    entry.window,
                    appearance,
                    &mut entry.attached,
                    &mut entry.adapters,
                    &mut entry.configurator,
                    |window, app| current(index, window, app),
                    cancellation.clone(),
                    cx,
                )
                .await?;
                entry.bound = true;
            }
            if !entry.bound {
                cx.update(|app| {
                    owner.borrow_mut().bind_interrupted_exit_appearance(
                        request,
                        entry.window,
                        appearance,
                        app,
                    )
                })
                .map_err(|error| error.to_string())??;
                entry.bound = true;
            }
        }
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await
    }
}
