use super::*;

type ConstructionHook = Box<dyn FnMut(usize, &mut GpuiMainWindowShellHost<'_>) + Send>;
type ShellHook = Box<dyn FnMut(usize, &mut MainWindowShell, &mut App) + Send>;
type PublicationHook =
    Box<dyn FnMut(usize, &mut MainWindowShell, &mut App) -> Result<(), String> + Send>;
type ReleaseHook = Box<dyn FnOnce(&[MainWindowShell], &mut App) + Send>;
type RetirementHook = Box<dyn FnMut(WindowId, &mut MainWindowStartupRetirement) + Send>;

#[derive(Default)]
pub(super) struct NativeRestoreSetFaults {
    pub(super) before_construction: Option<ConstructionHook>,
    pub(super) before_desktop: Option<ShellHook>,
    pub(super) after_desktop: Option<ShellHook>,
    pub(super) before_final_validation: Option<Box<dyn FnOnce() + Send>>,
    pub(super) before_publication: Option<PublicationHook>,
    pub(super) before_interaction_release: Option<ReleaseHook>,
    pub(super) before_retirement: Option<RetirementHook>,
    pub(super) hold_readiness: Option<Box<dyn Fn() + Send>>,
}

impl PreparedNativeMainWindowRestoreSet {
    pub fn test_hold_readiness(&mut self, observed: impl Fn() + Send + 'static) {
        self.faults.hold_readiness = Some(Box::new(observed));
    }

    pub fn test_before_construction(
        &mut self,
        hook: impl FnMut(usize, &mut GpuiMainWindowShellHost<'_>) + Send + 'static,
    ) {
        self.faults.before_construction = Some(Box::new(hook));
    }

    pub fn test_before_desktop(
        &mut self,
        hook: impl FnMut(usize, &mut MainWindowShell, &mut App) + Send + 'static,
    ) {
        self.faults.before_desktop = Some(Box::new(hook));
    }

    pub fn test_after_desktop(
        &mut self,
        hook: impl FnMut(usize, &mut MainWindowShell, &mut App) + Send + 'static,
    ) {
        self.faults.after_desktop = Some(Box::new(hook));
    }

    pub fn test_before_final_validation(&mut self, hook: impl FnOnce() + Send + 'static) {
        self.faults.before_final_validation = Some(Box::new(hook));
    }

    pub fn test_before_publication(
        &mut self,
        hook: impl FnMut(usize, &mut MainWindowShell, &mut App) -> Result<(), String> + Send + 'static,
    ) {
        self.faults.before_publication = Some(Box::new(hook));
    }

    pub fn test_before_interaction_release(
        &mut self,
        hook: impl FnOnce(&[MainWindowShell], &mut App) + Send + 'static,
    ) {
        self.faults.before_interaction_release = Some(Box::new(hook));
    }

    pub fn test_before_retirement(
        &mut self,
        hook: impl FnMut(WindowId, &mut MainWindowStartupRetirement) + Send + 'static,
    ) {
        self.faults.before_retirement = Some(Box::new(hook));
    }
}

impl PublishedMainWindowRestoreSet {
    #[cfg(test)]
    pub(crate) fn prepare_virtual_restored_test(
        mut prepared: PreparedMainWindowRestoreSet,
        appearance: Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<VirtualRestoredMainWindowRestoreSet, String> {
        prepared.revalidate()?;
        if prepared.owner.expected_windows.len() != 1 || prepared.owner.members.len() != 1 {
            return Err("virtual restored fixture requires one exact original member".into());
        }
        let member = prepared.owner.members.pop().expect("exact single member");
        let PreparedRestoreSetMember::Restored(member) = member else {
            return Err("virtual restored fixture requires actual restored preparation".into());
        };
        let window_id = member.window_id();
        let check = member.native_validation();
        let mut host = GpuiMainWindowShellHost::new(app, appearance.clone())
            .with_virtual_placement(window_id, member.placement().clone());
        let shell = host
            .construct_restored_hidden(*member)
            .map_err(|failure| match failure {
                RestoredWindowShellHostFailure::BeforeConstruction { error, .. }
                | RestoredWindowShellHostFailure::Construction { error, .. } => error,
            })?;
        Ok(VirtualRestoredMainWindowRestoreSet {
            owner: Some(Box::new(prepared.owner)),
            shell: Some(shell),
            validation: NativeValidation { window_id, check },
            appearance,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_virtual_prepared_test(
        mut prepared: PreparedMainWindowRestoreSet,
        appearance: Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<Self, String> {
        prepared.revalidate()?;
        let mut shells = Vec::new();
        for member in std::mem::take(&mut prepared.owner.members) {
            let PreparedRestoreSetMember::Threadless(member) = member else {
                return Err(
                    "virtual recovery fixture requires exact threadless preparation".into(),
                );
            };
            let mut host = GpuiMainWindowShellHost::new(app, appearance.clone())
                .with_virtual_placement(member.window_id(), member.placement().clone());
            let mut shell = host.construct_threadless_hidden(member)?;
            app.update_window(shell.window().into(), |_, window, app| {
                window.draw(app).clear()
            })
            .map_err(|e| e.to_string())?;
            shell.publish(app)?;
            shells.push(shell);
        }
        Ok(Self {
            owner: Box::new(prepared.owner),
            shells,
            appearance,
        })
    }

    pub(crate) fn test_swap_recovery_shell(&mut self, shell: &mut MainWindowShell) {
        assert_eq!(self.shells.len(), 1);
        std::mem::swap(&mut self.shells[0], shell);
    }

    pub fn test_dispose(
        self,
        completion: impl FnOnce(MainWindowNativeRestoreSetFailure, &mut App) + 'static,
        app: &mut App,
    ) {
        let members = self
            .shells
            .into_iter()
            .zip(self.owner.expected_windows.iter().copied())
            .map(|(mut shell, window_id)| {
                let state = match shell.test_regate_startup(app) {
                    Ok(()) => NativeMemberState::Hidden(shell),
                    Err(error) => NativeMemberState::Retained { shell, error },
                };
                NativeMember { window_id, state }
            })
            .collect();
        let cancellation = MainWindowNativeRestoreSetCancellation {
            cancellation: self.owner.cancellation.clone(),
            wake: Arc::new(signal::StartupWake::default()),
        };
        let flight = NativeRestoreSetFlight {
            owner: Some(self.owner),
            members,
            validation: Vec::new(),
            appearance: self.appearance,
            cancellation,
            faults: NativeRestoreSetFaults::default(),
        };
        app.spawn(async move |cx| {
            let failure = flight
                .dispose("test teardown of untouched startup set".to_owned(), cx)
                .await;
            cx.update(|app| completion(failure, app))
                .expect("test startup teardown retains a live GUI executor");
        })
        .detach();
    }
}

#[cfg(test)]
pub(crate) struct VirtualRestoredMainWindowRestoreSet {
    owner: Option<Box<MainWindowRestoreSet>>,
    shell: Option<MainWindowShell>,
    validation: NativeValidation,
    appearance: Entity<GpuiAppearanceWindowSet>,
}

#[cfg(test)]
impl VirtualRestoredMainWindowRestoreSet {
    pub(crate) fn advance(
        &mut self,
        app: &mut App,
    ) -> Result<Option<PublishedMainWindowRestoreSet>, String> {
        let owner = self
            .owner
            .as_ref()
            .ok_or("virtual restored fixture already published")?;
        let shell = self
            .shell
            .as_mut()
            .ok_or("virtual restored fixture lost its exact shell")?;
        app.update_window(shell.window().into(), |_, window, app| {
            window.draw(app).clear()
        })
        .map_err(|e| e.to_string())?;
        if !shell.ready_to_publish(app) {
            return Ok(None);
        }
        owner.revalidate_members(1, |_, expected| {
            if self.validation.window_id != expected {
                return Err("virtual restored fixture validation identity changed".into());
            }
            (self.validation.check)(&owner.attempt, &owner.services)
        })?;
        shell.publish(app)?;
        Ok(Some(PublishedMainWindowRestoreSet {
            owner: self.owner.take().expect("validated original owner"),
            shells: vec![self.shell.take().expect("published original shell")],
            appearance: self.appearance.clone(),
        }))
    }
}
