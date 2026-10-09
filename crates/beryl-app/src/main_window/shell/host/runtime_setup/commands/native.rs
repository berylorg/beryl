use super::*;
use beryl_model::RuntimeLaunchForm;
use std::path::PathBuf;

type PathPromptFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Vec<PathBuf>>, String>>>>;

fn selected_form(choice: usize) -> Option<RuntimeLaunchForm> {
    match choice {
        0 => Some(RuntimeLaunchForm::StandaloneAppServer),
        1 => Some(RuntimeLaunchForm::CodexCli),
        _ => None,
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn choose_runtime_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_setup.native_dialog = true;
        self.setup_command_state("Add runtime", false, cx);
        if let Some(picker) = self.setup_command_picker() {
            picker.update(cx, |picker, pcx| picker.set_native_dialog_open(true, pcx));
        }
        let choice = window.prompt(
            gpui::PromptLevel::Info,
            "Add runtime",
            None,
            &["Codex App Server binary", "Codex CLI binary", "Cancel"],
            cx,
        );
        self.runtime_setup.native_task = Some(cx.spawn_in(window, async move |this, cx| {
            let choice = choice.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup.native_task = None;
                if !root.setup_enabled() {
                    root.finish_setup_command(window, cx);
                    return;
                }
                match choice.ok().and_then(selected_form) {
                    Some(form) => root.prompt_setup_path(Some(form), None, window, cx),
                    None => root.finish_setup_command(window, cx),
                }
            });
        }));
    }

    pub(in crate::main_window::shell::host) fn prompt_setup_path(
        &mut self,
        form: Option<RuntimeLaunchForm>,
        runtime: Option<RuntimeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_setup.native_dialog = true;
        self.setup_command_state(
            if runtime.is_some() {
                "Add root"
            } else {
                "Add runtime"
            },
            false,
            cx,
        );
        if let Some(picker) = self.setup_command_picker() {
            picker.update(cx, |picker, pcx| picker.set_native_dialog_open(true, pcx));
        }
        let options = gpui::PathPromptOptions {
            files: runtime.is_none(),
            directories: runtime.is_some(),
            multiple: false,
            prompt: Some(
                if runtime.is_some() {
                    "Choose a root directory"
                } else if form == Some(RuntimeLaunchForm::CodexCli) {
                    "Choose a Codex CLI binary"
                } else {
                    "Choose a Codex App Server binary"
                }
                .into(),
            ),
        };
        #[cfg(all(test, feature = "test-faults"))]
        let paths: PathPromptFuture = if self.runtime_setup.fixture_native {
            let (sender, receiver) = futures_channel::oneshot::channel();
            self.runtime_setup.fixture_path_prompt = Some((options, sender));
            Box::pin(async move { receiver.await.map_err(|error| error.to_string())? })
        } else {
            let response = cx.prompt_for_paths(options);
            Box::pin(async move {
                response
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
            })
        };
        #[cfg(not(all(test, feature = "test-faults")))]
        let paths: PathPromptFuture = {
            let response = cx.prompt_for_paths(options);
            Box::pin(async move {
                response
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
            })
        };
        self.runtime_setup.native_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup.native_task = None;
                root.finish_setup_native_path(form, runtime, result, window, cx);
            });
        }));
    }

    pub(in crate::main_window::shell::host) fn finish_setup_native_path(
        &mut self,
        form: Option<RuntimeLaunchForm>,
        runtime: Option<RuntimeId>,
        result: Result<Option<Vec<PathBuf>>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_setup.native_dialog = false;
        if let Some(picker) = self.setup_command_picker() {
            picker.update(cx, |picker, pcx| picker.set_native_dialog_open(false, pcx));
        }
        if !self.setup_enabled() {
            self.finish_setup_command(window, cx);
            return;
        }
        match result {
            Ok(None) => self.finish_setup_command(window, cx),
            Ok(Some(mut paths)) if paths.len() == 1 => {
                self.start_setup_admission(form, runtime, paths.remove(0), window, cx);
            }
            Ok(Some(_)) => {
                self.setup_failure("The native picker did not return one exact path.", false);
                self.finish_setup_command(window, cx);
            }
            Err(error) => {
                self.setup_failure(&error, false);
                self.finish_setup_command(window, cx);
            }
        }
    }
}
