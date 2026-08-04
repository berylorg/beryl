use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use beryl_model::SyndicAcceptedInputId;

static NEXT_PAUSE_TOKEN: AtomicU64 = AtomicU64::new(1);
static DELIVERY_PAUSES: OnceLock<Mutex<HashMap<PauseKey, PendingPause>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) enum DeliveryPause {
    BeforeDeliveryClaim,
    BeforeLifecycleArm,
    BeforeCommandAuthorization,
    BeforeRetryDisposition,
    BeforeCompleteDisposition,
    BeforeRejectionDisposition,
    BeforeLossAbandonment,
    AfterRetryDisposition,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PauseKey {
    input_id: SyndicAcceptedInputId,
    stage: DeliveryPause,
}

struct PendingPause {
    token: u64,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

pub(super) struct DeliveryPauseController {
    key: PauseKey,
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub(in crate::cas_projection) fn install_delivery_pause(
    input_id: SyndicAcceptedInputId,
    stage: DeliveryPause,
) -> DeliveryPauseController {
    let key = PauseKey { input_id, stage };
    let token = NEXT_PAUSE_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, observation) = sync_channel(1);
    let (release, continuation) = sync_channel(1);
    let mut pauses = delivery_pauses()
        .lock()
        .expect("active-steering delivery-pause registry is usable");
    assert!(
        pauses
            .insert(
                key,
                PendingPause {
                    token,
                    arrived,
                    release: continuation,
                },
            )
            .is_none(),
        "one exact input and stage may own only one delivery pause",
    );
    drop(pauses);
    DeliveryPauseController {
        key,
        token,
        arrived: observation,
        release,
    }
}

pub(in crate::cas_projection) fn pause_delivery_if_requested(
    input_id: SyndicAcceptedInputId,
    stage: DeliveryPause,
) {
    let key = PauseKey { input_id, stage };
    let pending = delivery_pauses()
        .lock()
        .expect("active-steering delivery-pause registry is usable")
        .remove(&key);
    let Some(pending) = pending else {
        return;
    };
    pending
        .arrived
        .send(())
        .expect("active-steering test still observes the delivery pause");
    pending
        .release
        .recv_timeout(Duration::from_secs(10))
        .expect("active-steering test releases the paused delivery");
}

impl DeliveryPauseController {
    pub(super) fn wait_until_paused(&self, timeout: Duration) {
        self.arrived
            .recv_timeout(timeout)
            .expect("active-steering delivery reached its requested pause");
    }

    pub(super) fn release(self) {
        self.release
            .send(())
            .expect("paused active-steering delivery still awaits release");
    }
}

impl Drop for DeliveryPauseController {
    fn drop(&mut self) {
        let mut pauses = delivery_pauses()
            .lock()
            .expect("active-steering delivery-pause registry is usable");
        if pauses
            .get(&self.key)
            .is_some_and(|pending| pending.token == self.token)
        {
            pauses.remove(&self.key);
        }
    }
}

fn delivery_pauses() -> &'static Mutex<HashMap<PauseKey, PendingPause>> {
    DELIVERY_PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}
