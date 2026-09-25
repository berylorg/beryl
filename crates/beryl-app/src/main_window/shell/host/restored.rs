use super::*;
use crate::main_window::{
    RestoredWindowComposer, RestoredWindowComposerPrepared, RestoredWindowComposerRetirement,
    RestoredWindowPreparationAttempt,
};

pub struct RestoredWindowShellPrepared {
    selected: SelectedShellPrepared,
}

pub struct RestoredWindowShellPreparationFailure {
    pub prepared: RestoredWindowComposerPrepared,
    pub error: String,
}

pub struct RestoredWindowShellUnpublished {
    pub(super) composer: RestoredWindowComposer,
    reservation: RuntimeBackedWindowMainWindowReservation,
}

pub enum RestoredWindowShellRetirement {
    Retired,
    Pending {
        unpublished: RestoredWindowShellUnpublished,
        error: String,
    },
}

impl RestoredWindowShellUnpublished {
    pub fn window_id(&self) -> beryl_model::WindowId {
        self.composer.window_id()
    }

    pub fn retire(self, cancellation: CommandCancellation) -> RestoredWindowShellRetirement {
        let Self {
            composer,
            reservation,
        } = self;
        match composer.retire(cancellation) {
            RestoredWindowComposerRetirement::Retired => RestoredWindowShellRetirement::Retired,
            RestoredWindowComposerRetirement::Pending(failure) => {
                RestoredWindowShellRetirement::Pending {
                    error: failure.error,
                    unpublished: Self {
                        composer: failure.custody,
                        reservation,
                    },
                }
            }
        }
    }
}

impl RestoredWindowShellPrepared {
    pub(in crate::main_window) fn revalidate(
        &self,
        attempt: &RestoredWindowPreparationAttempt,
    ) -> Result<(), String> {
        let ShellContent::Restored { custody, .. } = &self.selected.content else {
            unreachable!("restored shell preparation preserves custody kind")
        };
        custody.composer.validate_attempt(attempt)?;
        self.selected.composer.validate_current()
    }

    pub fn prepare(
        prepared: RestoredWindowComposerPrepared,
        attempt: &RestoredWindowPreparationAttempt,
        registry: &RuntimeBackedWindowProcessRegistry,
        composer_configurator: MainWindowShellComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        appearance: Arc<AppearanceGeneration>,
    ) -> Result<Self, RestoredWindowShellPreparationFailure> {
        let validation = prepared.revalidate(attempt).and_then(|()| {
            let binding = prepared.selection_identity().binding();
            let home = appearance.prepared().home();
            if home.home_id() != binding.home_id()
                || home.home_generation() != binding.home_generation()
            {
                return Err(
                    "restored shell appearance belongs to another home generation".to_owned(),
                );
            }
            Ok(())
        });
        if let Err(error) = validation {
            return Err(RestoredWindowShellPreparationFailure { prepared, error });
        }
        let selection = prepared.selection_identity();
        let reservation = match registry.reserve_main_window(selection.window_id()) {
            Ok(reservation) => reservation,
            Err(error) => {
                return Err(RestoredWindowShellPreparationFailure {
                    prepared,
                    error: format!("restored shell reservation rejected: {error:?}"),
                });
            }
        };
        let (composer, custody) = prepared.into_shell_parts();
        Ok(Self {
            selected: SelectedShellPrepared {
                content: ShellContent::Restored {
                    custody: Box::new(RestoredWindowShellUnpublished {
                        composer: custody,
                        reservation,
                    }),
                    selection,
                },
                composer,
                composer_configurator,
                marker_seals,
                submission_request_source,
                appearance: MainWindowShellAppearance::prepare(appearance),
            },
        })
    }

    pub fn window_id(&self) -> beryl_model::WindowId {
        self.selected.composer.selection_identity().window_id()
    }

    pub fn placement(&self) -> &beryl_model::WindowPlacement {
        let ShellContent::Restored { custody, .. } = &self.selected.content else {
            unreachable!("restored shell preparation preserves custody kind")
        };
        custody.composer.placement()
    }

    pub fn into_unpublished(self) -> RestoredWindowShellUnpublished {
        let ShellContent::Restored { custody, .. } = self.selected.content else {
            unreachable!("restored shell preparation preserves custody kind")
        };
        *custody
    }
}

pub enum RestoredWindowShellHostFailure {
    BeforeConstruction {
        error: String,
        prepared: RestoredWindowShellPrepared,
    },
    Construction {
        error: String,
        unpublished: RestoredWindowShellUnpublished,
    },
}

impl RestoredWindowShellHostFailure {
    pub fn into_unpublished(self) -> RestoredWindowShellUnpublished {
        match self {
            Self::BeforeConstruction { prepared, .. } => prepared.into_unpublished(),
            Self::Construction { unpublished, .. } => unpublished,
        }
    }
}

impl MainWindowShellController {
    fn into_restored_unpublished(self) -> RestoredWindowShellUnpublished {
        let ShellContent::Restored { custody, .. } = self.content else {
            unreachable!("restored shell disposal preserves custody kind")
        };
        *custody
    }
}

impl GpuiMainWindowShellHost<'_> {
    pub fn construct_restored_hidden(
        &mut self,
        prepared: RestoredWindowShellPrepared,
    ) -> Result<MainWindowShell, RestoredWindowShellHostFailure> {
        use crate::theme_runtime::AppearancePublicationTarget;
        let ShellContent::Restored { custody, .. } = &prepared.selected.content else {
            unreachable!("restored shell preparation preserves custody kind")
        };
        let snapshot = self.appearance_owner.read(self.app).target().snapshot();
        let validation = custody.composer.validate_shell_lifetime().and_then(|()| {
            if snapshot.active
                && Arc::ptr_eq(&snapshot.current, &prepared.selected.appearance.generation)
            {
                Ok(())
            } else {
                Err("restored shell appearance is no longer current".to_owned())
            }
        });
        if let Err(error) = validation {
            return Err(RestoredWindowShellHostFailure::BeforeConstruction { error, prepared });
        }
        self.construct_selected_hidden(prepared.selected)
            .map_err(|failure| match failure {
                SelectedShellHostFailure::BeforeConstruction { error, prepared } => {
                    RestoredWindowShellHostFailure::BeforeConstruction {
                        error,
                        prepared: RestoredWindowShellPrepared { selected: prepared },
                    }
                }
                SelectedShellHostFailure::Construction { error, controller } => {
                    RestoredWindowShellHostFailure::Construction {
                        error,
                        unpublished: controller.into_restored_unpublished(),
                    }
                }
            })
    }
}

impl MainWindowShell {
    pub fn close_restored_before_publication(
        self,
        app: &mut App,
    ) -> Result<RestoredWindowShellUnpublished, Self> {
        if self.published
            || !self
                .root
                .read(app)
                .controller
                .as_ref()
                .is_some_and(|controller| {
                    matches!(controller.content, ShellContent::Restored { .. })
                })
        {
            return Err(self);
        }
        self.appearance_owner
            .update(app, |owner, _| owner.unregister(self.adapter_id))
            .expect("bounded window-set epoch");
        let _ = self.window.update(app, |root, window, cx| {
            root.retire_notices(window, cx);
            window.remove_window();
        });
        let controller = self.root.update(app, |root, _| {
            root.controller
                .take()
                .expect("restored shell retains its controller")
        });
        Ok(controller.into_restored_unpublished())
    }
}
