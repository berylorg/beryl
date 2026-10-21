use super::{ACTIVE, CANCELLED, PUBLISHING};
use crate::atomic_save::{AtomicSaveError, AtomicSaveStage};
use futures_channel::oneshot;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
pub(in super::super::super) enum SaveOperation {
    Open(gpui::SavePathChoice),
    Write(Vec<u8>),
    Prepare,
    Publish,
}
pub(in super::super::super) struct SaveWork {
    pub operation: SaveOperation,
    pub next: oneshot::Receiver<SaveWork>,
    pub reply: oneshot::Sender<SaveResult>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super::super) enum SaveResult {
    Opened,
    Wrote(usize),
    Prepared,
    Published,
    Failed(&'static str),
}

#[derive(Default)]
pub(in super::super::super) struct SaveWriterOptions {
    pub lease: Option<crate::main_window::WorkerLease>,
    #[cfg(test)]
    pub fault: Option<SaveFault>,
    #[cfg(test)]
    pub hold: Option<Arc<SaveWriterGate>>,
}

#[cfg(test)]
pub(crate) struct SaveWriterGate {
    publish: bool,
    entered: std::sync::atomic::AtomicBool,
    released: std::sync::Mutex<bool>,
    wake: std::sync::Condvar,
}

#[cfg(test)]
impl SaveWriterGate {
    pub(crate) fn new(publish: bool) -> Arc<Self> {
        Arc::new(Self {
            publish,
            entered: Default::default(),
            released: Default::default(),
            wake: Default::default(),
        })
    }
    pub(crate) fn entered(&self) -> bool {
        self.entered.load(Ordering::Acquire)
    }
    pub(crate) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
    fn hold(&self, operation: &SaveOperation) {
        if !matches!(
            (self.publish, operation),
            (true, SaveOperation::Publish) | (false, SaveOperation::Prepare)
        ) {
            return;
        }
        let released = self.released.lock().unwrap();
        self.entered.store(true, Ordering::Release);
        let (released, timeout) = self
            .wake
            .wait_timeout_while(released, std::time::Duration::from_secs(10), |released| {
                !*released
            })
            .unwrap();
        assert!(
            *released && !timeout.timed_out(),
            "Save writer gate timed out"
        );
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum SaveFault {
    Write,
    Publish,
}

fn cleanup(stage: Option<AtomicSaveStage>) -> Result<(), &'static str> {
    if let Some(stage) = stage {
        stage.cleanup().map_err(|_| {
            tracing::error!(
                "Code Save temporary output cleanup failed; a temporary file may remain"
            );
            "Save unavailable: temporary output cleanup failed; a temporary file may remain"
        })?;
    }
    Ok(())
}

pub(in super::super::super) async fn run(
    mut receiver: oneshot::Receiver<SaveWork>,
    gate: Arc<AtomicU8>,
    mut _options: SaveWriterOptions,
) {
    let mut stage: Option<AtomicSaveStage> = None;
    while let Ok(work) = receiver.await {
        #[cfg(test)]
        if let Some(hold) = &_options.hold {
            hold.hold(&work.operation);
        }
        let result = if gate.load(Ordering::Acquire) == CANCELLED {
            Err("Save cancelled")
        } else {
            (|| match work.operation {
                SaveOperation::Open(choice) => {
                    stage = Some(
                        AtomicSaveStage::create(choice, ".beryl-code-save-").map_err(|error| {
                            if error == AtomicSaveError::Destination {
                                "Save unavailable: destination refused"
                            } else {
                                "Save unavailable: staging file could not be created"
                            }
                        })?,
                    );
                    Ok(SaveResult::Opened)
                }
                SaveOperation::Write(bytes) => {
                    if bytes.len() > 8192 {
                        return Err("Save unavailable: source page refused");
                    }
                    #[cfg(test)]
                    if matches!(_options.fault, Some(SaveFault::Write)) {
                        stage
                            .as_mut()
                            .ok_or("Save unavailable: writer retired")?
                            .write(&bytes[..bytes.len().min(3)])
                            .map_err(|_| "Save unavailable: writing failed")?;
                        return Err("Save unavailable: writing failed");
                    }
                    stage
                        .as_mut()
                        .ok_or("Save unavailable: writer retired")?
                        .write(&bytes)
                        .map_err(|_| "Save unavailable: writing failed")?;
                    Ok(SaveResult::Wrote(bytes.len()))
                }
                SaveOperation::Prepare => {
                    let file = stage.as_mut().ok_or("Save unavailable: writer retired")?;
                    file.prepare()
                        .map_err(|_| "Save unavailable: writing failed")?;
                    Ok(SaveResult::Prepared)
                }
                SaveOperation::Publish => {
                    if gate.load(Ordering::Acquire) != PUBLISHING {
                        return Err("Save unavailable: publication retired");
                    }
                    #[cfg(test)]
                    if matches!(_options.fault, Some(SaveFault::Publish)) {
                        return Err("Save unavailable: publication failed");
                    }
                    stage
                        .as_mut()
                        .ok_or("Save unavailable: writer retired")?
                        .publish()
                        .map_err(|_| "Save unavailable: publication failed")?;
                    Ok(SaveResult::Published)
                }
            })()
        };
        match result {
            Ok(SaveResult::Published) => {
                drop(_options.lease.take());
                let _ = work.reply.send(SaveResult::Published);
                return;
            }
            Ok(result) => {
                if work.reply.send(result).is_err() {
                    break;
                }
            }
            Err(reason) => {
                let reason = cleanup(stage.take()).err().unwrap_or(reason);
                drop(_options.lease.take());
                let _ = work.reply.send(SaveResult::Failed(reason));
                return;
            }
        }
        receiver = work.next;
    }
    let _ = cleanup(stage);
    drop(_options.lease.take());
}
