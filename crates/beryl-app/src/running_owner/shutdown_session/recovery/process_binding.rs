use super::*;

pub(super) struct RecoveredProcessBinding {
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    windows: Vec<RecoveredWindowCommandBinding>,
}

struct RecoveredWindowCommandBinding {
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
    root: gpui::EntityId,
    persistent: beryl_model::WindowId,
    command: crate::startup_owner::RunningWindowExit,
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_recovered_process_command_binding_count(&self) -> Option<usize> {
        self.interrupted_exit
            .as_ref()?
            .process_binding
            .as_ref()
            .map(|binding| binding.windows.len())
    }

    pub(super) fn bind_recovered_process_commands(
        &mut self,
        request: &impl RecoveryIdentity,
        app: &mut App,
    ) -> Result<(), String> {
        let home = self
            .process
            .services
            .as_ref()
            .and_then(|services| services.graph())
            .ok_or("Published recovery graph is unavailable")?
            .home()
            .service_reference();
        let generation = home
            .health()
            .generation()
            .ok_or("Recovered Home generation is missing")?;
        let recovery = self
            .interrupted_exit
            .as_mut()
            .ok_or("Captured recovery is missing")?;
        if let Some(binding) = &recovery.process_binding {
            if binding.home != home.home_id() || binding.generation != generation {
                return Err(
                    "Recovered process binding differs from its original published graph".into(),
                );
            }
            self.process
                .commands
                .validate_recovered_home_binding(request.lifecycle(), &home)?;
        } else {
            crate::main_window::MainWindowCreationOwner::validate_recovered_process(&home, app)?;
            if let Some(request) = request.lifecycle() {
                self.process.commands.bind_recovered_home(request, home)?;
            } else {
                self.process.commands.bind_recovered_running_home(home)?;
            }
            recovery.process_binding = Some(RecoveredProcessBinding {
                home: self
                    .process
                    .services
                    .as_ref()
                    .unwrap()
                    .graph()
                    .unwrap()
                    .home()
                    .home_id(),
                generation,
                windows: Vec::new(),
            });
        }
        let binding = recovery.process_binding.as_mut().unwrap();
        for completed in &binding.windows {
            if !self
                .process
                .windows
                .shells()
                .iter()
                .any(|shell| shell.window() == completed.window)
            {
                return Err("Recovered completed command window is missing".into());
            }
            completed
                .window
                .update(app, |root, _, cx| {
                    if root
                        .controller()
                        .is_none_or(|controller| controller.window_id() != completed.persistent)
                        || cx.entity_id() != completed.root
                    {
                        return Err(
                            "Recovered completed command native or persistent identity changed"
                                .into(),
                        );
                    }
                    root.validate_recovered_running_command(&completed.command)
                })
                .map_err(|error| error.to_string())??;
        }
        if self.ordinary_commands_mounted {
            for shell in self.process.windows.shells() {
                let window = shell.window();
                if binding
                    .windows
                    .iter()
                    .any(|completed| completed.window == window)
                {
                    continue;
                }
                let persistent = window
                    .read(app)
                    .map_err(|error| error.to_string())?
                    .controller()
                    .ok_or("Recovered command controller is unavailable")?
                    .window_id();
                let command = self.process.commands.window_command(persistent);
                let root = window
                    .update(app, |root, window, cx| {
                        root.mount_running_command(command.clone(), window, cx);
                        cx.entity_id()
                    })
                    .map_err(|error| error.to_string())?;
                binding.windows.push(RecoveredWindowCommandBinding {
                    window,
                    root,
                    persistent,
                    command,
                });
            }
        }
        Ok(())
    }
}
