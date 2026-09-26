use super::*;

#[derive(Clone, Copy)]
pub(super) struct ShellConstruction {
    pub(super) startup: bool,
    #[cfg(feature = "test-faults")]
    reject_mount: bool,
    #[cfg(feature = "test-faults")]
    pub(super) reject_setup: bool,
    #[cfg(feature = "test-faults")]
    reject_appearance: bool,
    #[cfg(all(target_os = "windows", feature = "test-faults"))]
    reject_receipt: bool,
}

impl ShellConstruction {
    pub(super) fn reject_mount(self) -> bool {
        #[cfg(feature = "test-faults")]
        {
            self.reject_mount
        }
        #[cfg(not(feature = "test-faults"))]
        {
            false
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn begin_native(
        self,
        window: &mut Window,
        app: &mut App,
        admission: &RefCell<Option<startup_disposal::ShellStartupDisposalAdmission>>,
    ) -> Option<String> {
        if !self.startup {
            return None;
        }
        window.on_window_should_close(app, |_, _| false);
        let mut state = startup_disposal::ShellStartupDisposalAdmission {
            receipt: None,
            preserve_records: false,
            started: false,
            editor_never_mounted: true,
            gate_proven: false,
        };
        let mut register = || {
            window
                .observe_windows_native_destruction()
                .map_err(|error| error.to_string())
        };
        #[cfg(feature = "test-faults")]
        let registered = if self.reject_receipt {
            Err("injected startup destruction receipt rejection".to_owned())
        } else {
            register()
        };
        #[cfg(not(feature = "test-faults"))]
        let registered = register();
        let error = match registered {
            Ok(receipt) => {
                state.receipt = Some(Box::pin(receipt));
                None
            }
            Err(error) => Some(error),
        };
        *admission.borrow_mut() = Some(state);
        error
    }
}

impl GpuiMainWindowShellHost<'_> {
    pub(super) fn construction(&mut self, startup: bool) -> ShellConstruction {
        #[cfg(all(target_os = "windows", feature = "test-faults"))]
        let fault = if startup {
            self.startup_fault.take()
        } else {
            None
        };
        #[cfg(feature = "test-faults")]
        let mut reject_mount = std::mem::take(&mut self.reject_mount);
        #[cfg(all(target_os = "windows", feature = "test-faults"))]
        {
            reject_mount |= fault == Some(MainWindowStartupConstructionFault::ComposerMount);
        }
        ShellConstruction {
            startup,
            #[cfg(feature = "test-faults")]
            reject_mount,
            #[cfg(feature = "test-faults")]
            reject_setup: {
                #[cfg(target_os = "windows")]
                {
                    fault == Some(MainWindowStartupConstructionFault::ComposerSetup)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    false
                }
            },
            #[cfg(feature = "test-faults")]
            reject_appearance: {
                #[cfg(target_os = "windows")]
                {
                    fault == Some(MainWindowStartupConstructionFault::AppearanceRegistration)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    false
                }
            },
            #[cfg(all(target_os = "windows", feature = "test-faults"))]
            reject_receipt: fault == Some(MainWindowStartupConstructionFault::ReceiptRegistration),
        }
    }

    pub(super) fn complete_hidden_shell(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        construction: ShellConstruction,
        #[cfg(target_os = "windows")] admission: Option<
            startup_disposal::ShellStartupDisposalAdmission,
        >,
    ) -> MainWindowShell {
        let root = window.entity(self.app).expect("new hidden shell root");
        root.update(self.app, |root, cx| {
            if let Some(composer) = root
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount.as_ref())
                .and_then(|mount| mount.read(cx).contribution())
            {
                let input = composer.read(cx).gpui_input();
                root.composer_observer = Some(cx.observe(&input, |_, _, cx| cx.notify()));
            }
        });
        let adapter_id = crate::theme_runtime::WindowAdapterId::new(
            std::num::NonZeroU64::new(root.entity_id().as_u64()).expect("GPUI entity identity"),
        );
        let mut shell = MainWindowShell {
            window,
            root,
            appearance_owner: self.appearance_owner.clone(),
            adapter_id,
            appearance_registered: false,
            published: false,
            #[cfg(target_os = "windows")]
            startup_disposal: admission,
            #[cfg(target_os = "windows")]
            desktop_placement: None,
            #[cfg(all(target_os = "windows", feature = "test-faults"))]
            desktop_worker_gate: None,
        };
        if shell.root.read(self.app).construction_error.is_some() {
            return shell;
        }
        #[cfg(feature = "test-faults")]
        if construction.reject_appearance {
            shell.root.update(self.app, |root, _| {
                root.construction_error =
                    Some("injected startup appearance registration rejection".to_owned());
            });
            return shell;
        }
        #[cfg(not(feature = "test-faults"))]
        let _ = construction;
        let registration = self.appearance_owner.update(self.app, |owner, cx| {
            owner.register(
                Box::new(appearance::ShellAppearanceAdapter {
                    id: adapter_id,
                    window,
                }),
                cx,
            )
        });
        match registration {
            Ok(()) => shell.appearance_registered = true,
            Err(error) => shell.root.update(self.app, |root, _| {
                root.construction_error = Some(error.to_string());
            }),
        }
        shell
    }
}
