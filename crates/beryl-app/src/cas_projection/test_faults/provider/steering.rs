use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use beryl_backend::UserMessageEchoLifecycle;

use super::{NEXT_TOKEN, ProviderTestKey};

struct SteeringSelectionBarrier {
    token: u64,
    lifecycle: UserMessageEchoLifecycle,
    arrived: SyncSender<()>,
    release: Mutex<Receiver<()>>,
}

pub(crate) struct SteeringSelectionBarrierController {
    key: ProviderTestKey,
    barrier: Arc<SteeringSelectionBarrier>,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub(crate) fn install_steering_selection_barrier_for_key(
    key: ProviderTestKey,
    lifecycle: UserMessageEchoLifecycle,
) -> SteeringSelectionBarrierController {
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let barrier = Arc::new(SteeringSelectionBarrier {
        token: NEXT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        lifecycle,
        arrived,
        release: Mutex::new(released),
    });
    let mut barriers = selection_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    assert!(
        !barriers.contains_key(&key),
        "one broker cannot install overlapping steering-selection barriers"
    );
    barriers.insert(key, Arc::clone(&barrier));
    drop(barriers);
    SteeringSelectionBarrierController {
        key,
        barrier,
        arrived: arrival,
        release,
    }
}

impl SteeringSelectionBarrierController {
    pub(crate) fn wait_until_paused(&self, timeout: Duration) -> bool {
        self.arrived.recv_timeout(timeout).is_ok()
    }

    pub(crate) fn release(&self) {
        let _ = self.release.try_send(());
    }
}

impl Drop for SteeringSelectionBarrierController {
    fn drop(&mut self) {
        self.release();
        let mut barriers = selection_barriers()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if barriers
            .get(&self.key)
            .is_some_and(|barrier| barrier.token == self.barrier.token)
        {
            barriers.remove(&self.key);
        }
    }
}

pub(crate) fn pause_steering_after_selection(
    key: ProviderTestKey,
    lifecycle: UserMessageEchoLifecycle,
) {
    let barrier = selection_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned();
    let Some(barrier) = barrier.filter(|barrier| barrier.lifecycle == lifecycle) else {
        return;
    };
    barrier
        .arrived
        .send(())
        .expect("steering-selection barrier controller remains live");
    barrier
        .release
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .recv()
        .expect("steering-selection barrier controller releases the ingester");
}

fn selection_barriers() -> &'static Mutex<HashMap<ProviderTestKey, Arc<SteeringSelectionBarrier>>> {
    static BARRIERS: OnceLock<Mutex<HashMap<ProviderTestKey, Arc<SteeringSelectionBarrier>>>> =
        OnceLock::new();
    BARRIERS.get_or_init(|| Mutex::new(HashMap::new()))
}
