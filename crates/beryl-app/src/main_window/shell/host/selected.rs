use super::*;

impl MainWindowShellHost for GpuiMainWindowShellHost<'_> {
    type Shell = MainWindowShell;
    type Error = String;

    fn construct_hidden(
        &mut self,
        prepared: MainWindowShellPrepared,
    ) -> Result<Self::Shell, MainWindowShellHostFailure<Self::Error>> {
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
        self.construct_selected_hidden(SelectedShellPrepared {
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
        })
        .map_err(|failure| match failure {
            SelectedShellHostFailure::BeforeConstruction { error, prepared } => {
                let SelectedShellPrepared {
                    content,
                    composer,
                    composer_configurator,
                    marker_seals,
                    submission_request_source,
                    appearance,
                } = prepared;
                let ShellContent::Acquired { custody, .. } = content else {
                    unreachable!("acquired native construction preserves custody kind")
                };
                MainWindowShellHostFailure::BeforeConstruction {
                    error,
                    prepared: MainWindowShellPrepared {
                        acquisition: custody.acquisition,
                        reservation: custody.reservation,
                        initial_composer: custody.initial_composer,
                        composer,
                        composer_configurator,
                        marker_seals,
                        submission_request_source,
                        appearance,
                    },
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
