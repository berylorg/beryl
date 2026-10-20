use super::*;

pub struct MainWindowConversationComposerPreparedSelection {
    pub(super) config: MainWindowConversationComposerConfig,
    pub(super) service: Arc<MainWindowConversationComposerService>,
    pub(super) residency_bound: MainWindowComposerResidencyBound,
    pub(super) activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
    pub(super) submission_intent: Option<syndic_storage::DraftSubmissionIntent>,
    assets: beryl_state::AssetState,
    pub(super) discussion: Option<crate::discussion_presentation::DiscussionPresentationSnapshot>,
}

impl MainWindowConversationComposerPreparedSelection {
    pub(in crate::main_window) fn discussion_visible(&self) -> bool {
        self.discussion
            .as_ref()
            .is_some_and(|snapshot| snapshot.status().is_some())
    }
    pub(in crate::main_window) fn refresh_discussion_preparation(&mut self) -> Result<(), String> {
        self.service
            .prepare_discussion_presentation(self.selection_identity())?;
        self.discussion = self
            .service
            .prepared_discussion_presentation(self.selection_identity());
        Ok(())
    }
    pub(in crate::main_window) fn panel_minimum_size(&self) -> gpui::Size<gpui::Pixels> {
        self.config.panel_minimum_size()
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
        let submission_intent = service.selected_submission_intent(config.selection());
        let discussion = service.prepared_discussion_presentation(config.selection());
        if service.discussion_presentation_reader().is_some() && discussion.is_none() {
            return Err("prepared discussion presentation is missing".into());
        }
        Ok(Self {
            config,
            service,
            residency_bound,
            activation_seeds,
            submission_intent,
            assets,
            discussion,
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

    pub(in crate::main_window) fn validate_current(&self) -> Result<(), String> {
        if self.service.selected_identity() != Some(self.selection_identity()) {
            return Err("prepared conversation composer selection is stale".to_owned());
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub(in crate::main_window) fn native_selection_validation(
        &self,
    ) -> impl Fn() -> Result<(), String> + Send + use<> {
        let service = self.service.clone();
        let selection = self.selection_identity();
        move || {
            if service.selected_identity() != Some(selection) {
                return Err("prepared conversation composer selection is stale".to_owned());
            }
            Ok(())
        }
    }

    pub fn mount(
        self,
        clipboard_writer: ComposerClipboardWriter,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<MainWindowConversationComposer>, String> {
        self.validate_current()?;
        let snapshot = self.discussion.clone();
        let mount = || {
            cx.new(|composer_cx| {
                MainWindowConversationComposer::consume_prepared(
                    self,
                    clipboard_writer,
                    window,
                    composer_cx,
                )
            })
        };
        if let Some(snapshot) = snapshot {
            snapshot.try_elect(mount).map_err(|error| error.to_string())
        } else {
            Ok(mount())
        }
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
