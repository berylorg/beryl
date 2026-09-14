use std::{
    sync::{
        Mutex, OnceLock,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use crate::cas_projection::ProjectionServiceGeneration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquisitionBarrierStage {
    SessionPrepared,
    ProjectionAdmitted,
}

struct Pending {
    service: ProjectionServiceGeneration,
    stage: AcquisitionBarrierStage,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();

pub struct AcquisitionBarrierController {
    service: ProjectionServiceGeneration,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub fn install_acquisition_barrier(
    service: ProjectionServiceGeneration,
    stage: AcquisitionBarrierStage,
) -> AcquisitionBarrierController {
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
    assert!(pending.is_none());
    *pending = Some(Pending {
        service,
        stage,
        arrived,
        release: released,
    });
    AcquisitionBarrierController {
        service,
        arrived: arrival,
        release,
    }
}

impl AcquisitionBarrierController {
    pub fn wait(&self) {
        self.arrived.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    pub fn release(&self) {
        let _ = self.release.try_send(());
    }
}

impl Drop for AcquisitionBarrierController {
    fn drop(&mut self) {
        self.release();
        let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.service == self.service)
        {
            *pending = None;
        }
    }
}

pub(crate) fn pause_acquisition(
    service: ProjectionServiceGeneration,
    stage: AcquisitionBarrierStage,
) {
    let pending = {
        let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.service == service && pending.stage == stage)
        {
            pending.take()
        } else {
            None
        }
    };
    if let Some(pending) = pending {
        let _ = pending.arrived.send(());
        let _ = pending.release.recv_timeout(Duration::from_secs(15));
    }
}
