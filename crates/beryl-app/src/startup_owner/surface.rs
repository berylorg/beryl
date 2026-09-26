use crate::startup_surface::{StartupAttempt, StartupSurface, StartupSurfaceEvent};
use gpui::{App, AsyncApp, WindowHandle, WindowsNativeWindowDestroyed};

pub(crate) struct OwnedStartupSurface {
    pub(crate) window: WindowHandle<StartupSurface>,
    pub(crate) attempt: StartupAttempt,
    pub(crate) busy: bool,
    destroyed: Option<WindowsNativeWindowDestroyed>,
    failure: Option<String>,
}

impl OwnedStartupSurface {
    pub(super) fn open(
        detail: Option<&str>,
        handler: impl Fn(StartupSurfaceEvent, &mut App) + 'static,
        app: &mut App,
    ) -> Result<Self, String> {
        let window = match detail {
            Some(detail) => StartupSurface::open_failure(detail, handler, app)?,
            None => StartupSurface::open_busy(handler, app)?,
        };
        let attempt = window
            .update(app, |surface, _, cx| surface.attempt(cx))
            .expect("new startup surface retains its root");
        let receipt = window
            .update(app, |_, window, _| {
                window.observe_windows_native_destruction()
            })
            .map_err(|e| e.to_string())
            .and_then(|result| result.map_err(|e| e.to_string()));
        let (destroyed, failure) = match receipt {
            Ok(receipt) => (Some(receipt), None),
            Err(error) => (None, Some(error)),
        };
        Ok(Self {
            window,
            attempt,
            busy: detail.is_none(),
            destroyed,
            failure,
        })
    }

    pub(super) fn registration_failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub(crate) async fn close(&mut self, cx: &mut AsyncApp) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        self.window
            .update(cx, |_, window, _| window.remove_window())
            .map_err(|e| e.to_string())?;
        let receipt = self
            .destroyed
            .take()
            .ok_or("startup native destruction authority is unavailable")?;
        if let Err(error) = receipt.await {
            let detail = error.to_string();
            self.failure = Some(detail.clone());
            return Err(detail);
        }
        Ok(())
    }
}
