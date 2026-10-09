use super::*;

pub(crate) struct MainWindowSelectionInvocation {
    root: gpui::EntityId,
    window: beryl_model::WindowId,
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn running_selection_invocation(
        &self,
        cx: &Context<Self>,
    ) -> Result<MainWindowSelectionInvocation, String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("running selection lost its controller")?;
        if !matches!(
            controller.content,
            ShellContent::Acquired { .. }
                | ShellContent::Restored { .. }
                | ShellContent::Selected { .. }
        ) {
            return Err("running selection requires its original selected controller".into());
        }
        Ok(MainWindowSelectionInvocation {
            root: cx.entity_id(),
            window: controller.window_id(),
        })
    }
}

impl MainWindowSelectionInvocation {
    pub(crate) fn window_id(&self) -> beryl_model::WindowId {
        self.window
    }

    fn qualify_published_roots(
        &self,
        roots: impl Iterator<Item = gpui::EntityId>,
    ) -> Result<(), String> {
        let mut count = 0usize;
        let mut invoking = 0usize;
        for root in roots {
            count += 1;
            if count > beryl_state::MAX_RESTORABLE_WINDOWS {
                return Err("running selection published membership exceeds its bound".into());
            }
            invoking += usize::from(root == self.root);
        }
        if invoking != 1 {
            return Err("running selection requires exactly one original published root".into());
        }
        Ok(())
    }

    pub(crate) fn published_members(
        &self,
        shells: &[MainWindowShell],
        app: &App,
    ) -> Result<Vec<beryl_model::WindowId>, String> {
        self.qualify_published_roots(
            shells
                .iter()
                .filter(|shell| shell.is_published())
                .map(|shell| shell.root.entity_id()),
        )?;
        shells
            .iter()
            .filter(|shell| shell.is_published())
            .map(|shell| {
                if shell.root.entity_id() == self.root {
                    Ok(self.window)
                } else {
                    shell.retained_window_id(app)
                }
            })
            .collect()
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/selection_invocation.rs"]
mod tests;
