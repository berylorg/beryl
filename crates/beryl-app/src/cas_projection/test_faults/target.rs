use std::{
    sync::{
        Mutex, OnceLock,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use beryl_model::SyndicThreadId;

use super::{NEXT_TOKEN, ProviderTestKey};
use crate::cas_projection::{AdmittedProjectionSession, LiveEventTarget};

static TARGET_ABANDONMENT: OnceLock<Mutex<Option<(TargetKey, PendingAbandonment)>>> =
    OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TargetKey {
    connection: ProviderTestKey,
    thread_id: SyndicThreadId,
}

struct PendingAbandonment {
    token: u64,
    abandoned: SyncSender<()>,
}

/// Observes one requested abandonment of an exact registered live-event target.
pub struct LiveEventTargetAbandonmentController {
    token: u64,
    key: TargetKey,
    abandoned: Receiver<()>,
}

/// Requests receiver abandonment for one exact connection and Syndic thread.
///
/// Ordinary execution still performs real target registration. The feature-only seam acts at the
/// owner immediately afterward and exposes no target handle or content.
pub fn install_live_event_target_abandonment(
    session: &AdmittedProjectionSession,
    thread_id: SyndicThreadId,
) -> LiveEventTargetAbandonmentController {
    let token = NEXT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let key = TargetKey {
        connection: session.provider_test_key(),
        thread_id,
    };
    let (abandoned, observation) = sync_channel(1);
    let mut pending = target_abandonment()
        .lock()
        .expect("target-abandonment fault registry is usable");
    assert!(
        pending.is_none(),
        "only one exact target-abandonment request may be installed"
    );
    *pending = Some((key, PendingAbandonment { token, abandoned }));
    drop(pending);
    LiveEventTargetAbandonmentController {
        token,
        key,
        abandoned: observation,
    }
}

impl LiveEventTargetAbandonmentController {
    /// Waits until ordinary execution has registered and abandoned the exact receiver.
    #[must_use]
    pub fn wait_until_abandoned(&self, timeout: Duration) -> bool {
        match self.abandoned.recv_timeout(timeout) {
            Ok(()) => true,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("ordinary execution stopped before target abandonment")
            }
        }
    }
}

impl Drop for LiveEventTargetAbandonmentController {
    fn drop(&mut self) {
        let mut pending = target_abandonment()
            .lock()
            .expect("target-abandonment fault registry is usable");
        if pending
            .as_ref()
            .is_some_and(|(key, entry)| *key == self.key && entry.token == self.token)
        {
            *pending = None;
        }
    }
}

pub(crate) fn abandon_live_event_target_if_requested(target: &mut LiveEventTarget) {
    let key = TargetKey {
        connection: target.provider_test_key(),
        thread_id: target.syndic_thread_id(),
    };
    let pending = {
        let mut slot = target_abandonment()
            .lock()
            .expect("target-abandonment fault registry is usable");
        if slot
            .as_ref()
            .is_some_and(|(installed, _)| *installed == key)
        {
            slot.take().map(|(_, pending)| pending)
        } else {
            None
        }
    };
    if let Some(pending) = pending {
        target.abandon_receiver_for_test();
        let _ = pending.abandoned.send(());
    }
}

fn target_abandonment() -> &'static Mutex<Option<(TargetKey, PendingAbandonment)>> {
    TARGET_ABANDONMENT.get_or_init(|| Mutex::new(None))
}
