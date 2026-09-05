use super::*;

pub struct MainWindowConversationComposerPreparedSelection {
    pub(super) config: MainWindowConversationComposerConfig,
    pub(super) service: Arc<MainWindowConversationComposerService>,
    pub(super) residency_bound: MainWindowComposerResidencyBound,
    pub(super) activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
    assets: beryl_state::AssetState,
}

impl MainWindowConversationComposerPreparedSelection {
    pub(in crate::main_window) fn shell_minimum_size(&self) -> gpui::Size<gpui::Pixels> {
        self.config.shell_minimum_size()
    }

    pub fn new(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
    ) -> Result<Self, String> {
        let residency_bound = config.residency_bound()?;
        let assets = service.assets()?;
        let initial = service.take_initial_presentation(config.selection())?;
        let activation_seeds =
            MainWindowConversationComposer::activation_seeds(config.selection(), initial)?;
        Ok(Self {
            config,
            service,
            residency_bound,
            activation_seeds,
            assets,
        })
    }

    pub fn selection_identity(&self) -> MainWindowComposerSelectionIdentity {
        self.config.selection()
    }

    pub const fn residency_bound(&self) -> MainWindowComposerResidencyBound {
        self.residency_bound
    }

    pub(in crate::main_window) fn service(&self) -> Arc<MainWindowConversationComposerService> {
        self.service.clone()
    }

    pub(in crate::main_window) fn assets(&self) -> beryl_state::AssetState {
        self.assets.clone()
    }

    pub(super) fn validate_current(&self) -> Result<(), String> {
        if self.service.selected_identity() != Some(self.selection_identity()) {
            return Err("prepared conversation composer selection is stale".to_owned());
        }
        Ok(())
    }

    pub fn mount(
        self,
        clipboard_writer: ComposerClipboardWriter,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<MainWindowConversationComposer>, String> {
        self.validate_current()?;
        Ok(cx.new(|composer_cx| {
            MainWindowConversationComposer::consume_prepared(
                self,
                clipboard_writer,
                window,
                composer_cx,
            )
        }))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_seed_count(&self) -> usize {
        self.activation_seeds.len()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_seed_text_bytes(&self) -> usize {
        self.activation_seeds
            .iter()
            .map(|seed| match seed {
                MainWindowConversationComposerActivationSeed::Page(response) => {
                    let crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) =
                        response.value()
                    else {
                        unreachable!();
                    };
                    candidate.value().bytes().len()
                }
                MainWindowConversationComposerActivationSeed::ObjectPage(_) => 0,
            })
            .sum()
    }
}
