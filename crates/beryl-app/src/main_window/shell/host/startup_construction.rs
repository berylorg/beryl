use super::*;

pub enum MainWindowStartupShellPrepared {
    Acquired(MainWindowShellPrepared),
    Restored(RestoredWindowShellPrepared),
    Threadless(ThreadlessWindowShellPrepared),
}

pub enum MainWindowStartupShellHostFailure {
    BeforeConstruction {
        prepared: MainWindowStartupShellPrepared,
        error: String,
    },
    Native {
        shell: MainWindowShell,
        error: String,
    },
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowStartupConstructionFault {
    ReceiptRegistration,
    ComposerMount,
    ComposerSetup,
    AppearanceRegistration,
}

impl GpuiMainWindowShellHost<'_> {
    #[cfg(feature = "test-faults")]
    pub fn test_fail_startup_construction_at(&mut self, fault: MainWindowStartupConstructionFault) {
        self.startup_fault = Some(fault);
    }

    pub fn construct_startup_hidden(
        &mut self,
        prepared: MainWindowStartupShellPrepared,
    ) -> Result<MainWindowShell, MainWindowStartupShellHostFailure> {
        let construction = self.construction(true);
        let shell = match prepared {
            MainWindowStartupShellPrepared::Acquired(prepared) => self
                .allocate_selected_hidden(
                    SelectedShellPrepared::from_acquired(prepared),
                    construction,
                )
                .map_err(
                    |failure| MainWindowStartupShellHostFailure::BeforeConstruction {
                        error: failure.error,
                        prepared: MainWindowStartupShellPrepared::Acquired(
                            failure.prepared.into_acquired(),
                        ),
                    },
                )?,
            MainWindowStartupShellPrepared::Restored(prepared) => {
                if let Err(error) = self.validate_restored_preparation(&prepared) {
                    return Err(MainWindowStartupShellHostFailure::BeforeConstruction {
                        prepared: MainWindowStartupShellPrepared::Restored(prepared),
                        error,
                    });
                }
                self.allocate_selected_hidden(prepared.selected, construction)
                    .map_err(
                        |failure| MainWindowStartupShellHostFailure::BeforeConstruction {
                            error: failure.error,
                            prepared: MainWindowStartupShellPrepared::Restored(
                                RestoredWindowShellPrepared {
                                    selected: failure.prepared,
                                },
                            ),
                        },
                    )?
            }
            MainWindowStartupShellPrepared::Threadless(prepared) => self
                .allocate_threadless_hidden(prepared, construction)
                .map_err(
                    |failure| MainWindowStartupShellHostFailure::BeforeConstruction {
                        error: failure.error,
                        prepared: MainWindowStartupShellPrepared::Threadless(failure.prepared),
                    },
                )?,
        };
        let error = shell.root.read(self.app).construction_error.clone();
        match error {
            Some(error) => Err(MainWindowStartupShellHostFailure::Native { shell, error }),
            None => Ok(shell),
        }
    }
}
