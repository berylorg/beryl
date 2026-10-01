use super::*;
use crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters;
use crate::main_window::{
    MainWindowConversationComposerCloseTicket, MainWindowConversationComposerConfigurator,
    MainWindowShellRoot,
};
use crate::theme_runtime::GpuiAppearanceWindowSet;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, Entity, WindowHandle};

impl RunningProcessOwner {
    pub(crate) async fn prepare_and_complete_interrupted_exit_resident_window(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        retired: HomeGeneration,
        generation: HomeGeneration,
        preparation: &mut Option<resident::ResidentPreparationKey>,
        admit: impl FnOnce(&mut App) -> Result<resident::ResidentPreparationKey, String>,
        window: WindowHandle<MainWindowShellRoot>,
        appearance: &Entity<GpuiAppearanceWindowSet>,
        adapters: &mut Option<PreparedComposerRecoveryAdapters>,
        configurator: &mut Option<MainWindowConversationComposerConfigurator>,
        current: impl FnOnce(
            &gpui::Window,
            &mut App,
        ) -> Result<gpui_text_input::RangePrepublicationCurrent, String>,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let attached = Self::prepare_and_attach_interrupted_exit_resident_pass(
            owner,
            request,
            preparation,
            admit,
            window,
            appearance,
            adapters,
            configurator,
            current,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            appearance,
            cancellation,
            cx,
        )
        .await?;
        Ok(attached)
    }
}
