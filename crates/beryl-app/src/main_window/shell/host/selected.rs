use super::*;

#[cfg(target_os = "windows")]
impl MainWindowShellPrepared {
    pub(in crate::main_window) fn native_validation(
        &self,
    ) -> crate::main_window::restoration::NativeMemberValidation {
        let facts = self
            .acquisition
            .shell_selection_validation(self.composer.selection_identity());
        let selection = self.composer.native_selection_validation();
        Box::new(move |_, services| {
            facts(&services.acquisition)?;
            selection()
        })
    }

    pub(in crate::main_window) fn startup_placement(&self) -> &beryl_model::WindowPlacement {
        self.acquisition.placement()
    }
}

impl MainWindowShellHost for GpuiMainWindowShellHost<'_> {
    type Shell = MainWindowShell;
    type Error = String;

    fn construct_hidden(
        &mut self,
        prepared: MainWindowShellPrepared,
    ) -> Result<Self::Shell, MainWindowShellHostFailure<Self::Error>> {
        self.construct_selected_hidden(SelectedShellPrepared::from_acquired(prepared))
            .map_err(|failure| match failure {
                SelectedShellHostFailure::BeforeConstruction { error, prepared } => {
                    MainWindowShellHostFailure::BeforeConstruction {
                        error,
                        prepared: prepared.into_acquired(),
                    }
                }
                SelectedShellHostFailure::Construction { error, controller } => {
                    MainWindowShellHostFailure::Construction {
                        error,
                        unpublished: controller.into_unpublished(),
                    }
                }
            })
    }
}

impl SelectedShellPrepared {
    pub(super) fn from_acquired(prepared: MainWindowShellPrepared) -> Self {
        let MainWindowShellPrepared {
            acquisition,
            reservation,
            initial_composer,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        } = prepared;
        let selection = composer.selection_identity();
        Self {
            content: ShellContent::Acquired {
                custody: MainWindowShellUnpublished {
                    acquisition,
                    reservation,
                    initial_composer,
                },
                selection,
            },
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        }
    }

    pub(super) fn into_acquired(self) -> MainWindowShellPrepared {
        let SelectedShellPrepared {
            content,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        } = self;
        let ShellContent::Acquired { custody, .. } = content else {
            unreachable!("acquired native construction preserves custody kind")
        };
        MainWindowShellPrepared {
            acquisition: custody.acquisition,
            reservation: custody.reservation,
            initial_composer: custody.initial_composer,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        }
    }
}
