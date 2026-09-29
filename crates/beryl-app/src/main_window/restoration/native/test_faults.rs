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
