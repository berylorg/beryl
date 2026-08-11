use std::sync::Arc;

/// Initial construction fence shared by workers in one service generation.
///
/// A service is published only after construction completes, so workers created by the
/// constructor may start immediately. The fence remains explicit to keep that ownership
/// boundary visible without supporting later service generation installation.
pub(super) struct InitialStartGate;

impl InitialStartGate {
    pub(super) fn ready() -> Arc<Self> {
        Arc::new(Self)
    }

    pub(super) const fn wait(&self) -> bool {
        true
    }
}
