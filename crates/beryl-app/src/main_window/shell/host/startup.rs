use super::*;

impl MainWindowShell {
    pub fn gate_startup_interaction(&mut self, app: &mut App) -> Result<(), String> {
        if self.published || self.root.read(app).startup_interaction.is_some() {
            return Err("startup interaction requires a new hidden shell".to_owned());
        }
        self.window
            .update(app, |root, window, cx| {
                let gate = Rc::new(std::cell::Cell::new(true));
                root.startup_interaction = Some(gate.clone());
                window.on_window_should_close(cx, move |_, _| !gate.get());
                let result = root.regate_startup_composer(cx);
                root.refresh_startup_notice_gate(window, cx);
                result
            })
            .map_err(|error| error.to_string())?
    }

    pub fn release_startup_interaction(shells: &[Self], app: &mut App) -> Result<(), String> {
        if shells.is_empty() || shells.len() > 256 {
            return Err("startup interaction requires a bounded nonempty set".to_owned());
        }
        for (index, shell) in shells.iter().enumerate() {
            if !shell.published
                || !shell.root.read(app).startup_interaction_gated()
                || shells[..index]
                    .iter()
                    .any(|previous| previous.window == shell.window)
            {
                return Err("startup interaction set is not fully published and gated".to_owned());
            }
        }
        let (first, rest) = shells.split_first().unwrap();
        first
            .window
            .update(app, |root, window, cx| {
                let prepared = root.prepare_startup_interaction_release(cx).and_then(|()| {
                    for shell in rest {
                        shell
                            .window
                            .update(cx, |root, _, cx| {
                                root.prepare_startup_interaction_release(cx)
                            })
                            .map_err(|error| error.to_string())??;
                    }
                    Ok(())
                });
                if let Err(error) = prepared {
                    let mut rollback_error = root.regate_startup_composer(cx).err();
                    for shell in rest {
                        let result = shell
                            .root
                            .update(cx, |root, cx| root.regate_startup_composer(cx));
                        if rollback_error.is_none() {
                            rollback_error = result.err();
                        }
                    }
                    return Err(match rollback_error {
                        Some(rollback) => {
                            format!("{error}; input disabling remains unproven: {rollback}")
                        }
                        None => error,
                    });
                }
                root.commit_startup_interaction_release(window, cx);
                for shell in rest {
                    shell
                        .window
                        .update(cx, |root, window, cx| {
                            root.commit_startup_interaction_release(window, cx)
                        })
                        .expect("validated startup windows remain live during widget-only release");
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?
    }
}

impl MainWindowShellRoot {
    pub fn startup_interaction_gated(&self) -> bool {
        self.startup_interaction
            .as_ref()
            .is_some_and(|gate| gate.get())
    }

    pub(super) fn startup_interaction_ready(&self, app: &App) -> bool {
        if !self.startup_interaction_gated() {
            return true;
        }
        match self.startup_composer(app) {
            Ok(None) => true,
            Ok(Some(composer)) => {
                let composer = composer.read(app);
                composer.startup_interaction_gated()
                    && !composer.gpui_input().read(app).is_enabled()
            }
            Err(_) => false,
        }
    }

    fn startup_composer(
        &self,
        app: &App,
    ) -> Result<Option<Entity<crate::main_window::MainWindowConversationComposer>>, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| "startup shell lost its controller".to_owned())?;
        if matches!(controller.content, ShellContent::Threadless { .. }) {
            return Ok(None);
        }
        controller
            .composer_mount
            .as_ref()
            .and_then(|mount| mount.read(app).contribution())
            .map(Some)
            .ok_or_else(|| "startup shell lost its composer".to_owned())
    }

    fn regate_startup_composer(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(composer) = self.startup_composer(cx)? {
            composer.update(cx, |composer, cx| composer.gate_startup_interaction(cx))?;
        }
        Ok(())
    }

    fn prepare_startup_interaction_release(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(composer) = self.startup_composer(cx)? {
            composer.update(cx, |composer, cx| {
                composer.prepare_startup_interaction_release(cx)
            })?;
        }
        Ok(())
    }

    fn commit_startup_interaction_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(composer) = self
            .startup_composer(cx)
            .expect("validated startup composer remains resident during widget-only release")
        {
            composer.update(cx, |composer, cx| {
                composer.commit_startup_interaction_release(cx)
            });
        }
        self.startup_interaction.as_ref().unwrap().set(false);
        self.refresh_startup_notice_gate(window, cx);
        cx.notify();
    }
}
