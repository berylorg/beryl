use beryl_model::CasThreadId;

use super::{ConnectionRegistryAuthority, EventRouter};
use crate::cas_projection::ProjectionCoordinatorError;

#[cfg(test)]
struct ThreadClosedAfterRouterHook {
    connection_generation: u64,
    thread_id: CasThreadId,
    reached: std::sync::mpsc::SyncSender<usize>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct ThreadClosedAfterRouterPause {
    reached: std::sync::mpsc::Receiver<usize>,
    release: Option<std::sync::mpsc::SyncSender<()>>,
}

#[cfg(test)]
static THREAD_CLOSED_AFTER_ROUTER_HOOK: std::sync::OnceLock<
    std::sync::Mutex<Option<ThreadClosedAfterRouterHook>>,
> = std::sync::OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionThreadClosedOutcome {
    connection_retired: bool,
    registry_authority_revoked: bool,
}

impl ConnectionThreadClosedOutcome {
    pub(in crate::cas_projection) const fn connection_retired(self) -> bool {
        self.connection_retired
    }

    pub(in crate::cas_projection) const fn registry_authority_revoked(self) -> bool {
        self.registry_authority_revoked
    }
}

/// Applies one source-ordered remote close to this exact connection generation.
///
/// Router mutation finishes before connection authority is acquired. Registry invalidation still
/// runs when router mutation failed, conservatively revoking the exact remote authority before the
/// caller fails the connection closed.
pub(in crate::cas_projection) fn record_connection_thread_closed(
    authority: &ConnectionRegistryAuthority,
    router: &EventRouter,
    thread_id: &CasThreadId,
) -> Result<ConnectionThreadClosedOutcome, ProjectionCoordinatorError> {
    let router_result = router.record_thread_closed(thread_id);
    #[cfg(test)]
    pause_thread_closed_after_router_for_test(authority.generation.get(), router, thread_id);
    let registry_result = authority.record_thread_closed(thread_id);
    let connection_retired = router_result?;
    let registry_authority_revoked = registry_result?;
    Ok(ConnectionThreadClosedOutcome {
        connection_retired,
        registry_authority_revoked,
    })
}

#[cfg(test)]
impl super::ProjectionConnection {
    pub(in crate::cas_projection) fn pause_next_thread_closed_after_router_for_test(
        &self,
        thread_id: CasThreadId,
    ) -> ThreadClosedAfterRouterPause {
        let (reached, reached_receiver) = std::sync::mpsc::sync_channel(1);
        let (release, release_receiver) = std::sync::mpsc::sync_channel(1);
        let slot = THREAD_CLOSED_AFTER_ROUTER_HOOK.get_or_init(|| std::sync::Mutex::new(None));
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        assert!(
            slot.is_none(),
            "only one thread-close router pause may be armed"
        );
        *slot = Some(ThreadClosedAfterRouterHook {
            connection_generation: self.authority.generation.get(),
            thread_id,
            reached,
            release: release_receiver,
        });
        ThreadClosedAfterRouterPause {
            reached: reached_receiver,
            release: Some(release),
        }
    }
}

#[cfg(test)]
impl ThreadClosedAfterRouterPause {
    pub(in crate::cas_projection) fn wait(&self) -> usize {
        self.reached
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the exact thread close must finish old-or-new router mutation")
    }

    pub(in crate::cas_projection) fn release(mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
impl Drop for ThreadClosedAfterRouterPause {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
fn pause_thread_closed_after_router_for_test(
    connection_generation: u64,
    router: &EventRouter,
    thread_id: &CasThreadId,
) {
    let slot = THREAD_CLOSED_AFTER_ROUTER_HOOK.get_or_init(|| std::sync::Mutex::new(None));
    let hook = {
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot.as_ref().is_some_and(|hook| {
            hook.connection_generation == connection_generation && hook.thread_id == *thread_id
        }) {
            slot.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        let _ = hook.reached.send(std::ptr::from_ref(router) as usize);
        let _ = hook
            .release
            .recv_timeout(std::time::Duration::from_secs(10));
    }
}
