use super::*;
use crate::activity_service::{
    ActivityMetadataCompletion, ActivityMetadataRequest, ActivityViewport,
};
use beryl_model::CasThreadId;
use std::{sync::mpsc, time::Duration};

struct Pending {
    request: ActivityMetadataRequest,
    response: beryl_backend::ThreadReadMetadata,
    completion: Option<ActivityMetadataCompletion>,
}

type Response = (
    ActivityMetadataRequest,
    Result<Option<beryl_backend::ThreadReadMetadata>, ActivityReadError>,
);

#[derive(Default)]
pub(super) struct Metadata {
    period: Option<(u64, u64)>,
    attempted: Vec<CasThreadId>,
    task: Option<gpui::Task<()>>,
    receiver: Option<mpsc::Receiver<Response>>,
    retry: Option<ActivityMetadataRequest>,
    pending: Option<Pending>,
}

fn busy(error: &ActivityReadError) -> bool {
    matches!(
        error,
        ActivityReadError::Busy
            | ActivityReadError::Runtime(crate::cas_projection::RuntimeActivityReadError::Busy)
            | ActivityReadError::Coherence(beryl_home_store::HomeCoherenceError::Busy)
            | ActivityReadError::Observation(
                beryl_home_store::HomeObservedCoherenceError::Coherence(
                    beryl_home_store::HomeCoherenceError::Busy
                )
            )
    )
}

impl Metadata {
    fn synchronize(&mut self, viewport: &ActivityViewport) {
        let period = viewport.snapshot().map(|snapshot| {
            (
                snapshot.head().work_period().get(),
                snapshot.head().revision().get(),
            )
        });
        if self.period != period {
            self.period = period;
            self.attempted.clear();
            self.pending = None;
            self.retry = None;
        }
        let resident = viewport.metadata_threads();
        self.attempted.retain(|thread| resident.contains(thread));
    }

    pub(super) fn step(
        &mut self,
        viewport: &mut ActivityViewport,
        executor: &gpui::BackgroundExecutor,
    ) {
        self.synchronize(viewport);
        if let Some(receiver) = self.receiver.as_ref() {
            match receiver.try_recv() {
                Ok((request, response)) => {
                    match response {
                        Ok(Some(response)) => {
                            self.pending = Some(Pending {
                                request,
                                response,
                                completion: None,
                            })
                        }
                        Err(error) if busy(&error) => self.retry = Some(request),
                        Err(_) => self.attempted.retain(|thread| thread != request.thread()),
                        _ => {}
                    }
                    self.receiver = None;
                    self.task = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.task = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(pending) = self.pending.as_mut() {
            match pending.request.current() {
                Err(error) if busy(&error) => return,
                Err(_) => {
                    self.attempted
                        .retain(|thread| thread != pending.request.thread());
                    self.pending = None;
                }
                Ok(()) => {
                    if pending.completion.is_none() {
                        match pending.request.try_complete(&pending.response) {
                            Ok(completion) => {
                                pending.completion = completion;
                                if pending.completion.is_none() {
                                    self.pending = None;
                                }
                            }
                            Err(error) if busy(&error) => return,
                            Err(_) => {
                                self.attempted
                                    .retain(|thread| thread != pending.request.thread());
                                self.pending = None;
                            }
                        }
                    }
                    if let Some(pending) = self.pending.as_ref() {
                        match viewport.try_set_metadata(
                            pending.completion.as_ref().expect("prepared metadata"),
                        ) {
                            Err(error) if busy(&error) => return,
                            Ok(true) => self.pending = None,
                            _ => {
                                self.attempted
                                    .retain(|thread| thread != pending.request.thread());
                                self.pending = None;
                            }
                        }
                    }
                }
            }
        }
        if self.receiver.is_some() || self.pending.is_some() {
            return;
        }
        let request = if let Some(request) = self.retry.take() {
            match request.current() {
                Ok(()) => request,
                Err(error) if busy(&error) => {
                    self.retry = Some(request);
                    return;
                }
                Err(_) => {
                    self.attempted.retain(|thread| thread != request.thread());
                    return;
                }
            }
        } else {
            if self.attempted.len() >= 256 {
                return;
            }
            let Some(request) = viewport.metadata_candidate(&self.attempted) else {
                return;
            };
            self.attempted.push(request.thread().clone());
            request
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        self.task = Some(executor.spawn(async move {
            let response = request.read(Duration::from_secs(2));
            let _ = sender.send((request, response));
        }));
    }
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
pub struct ActivityMetadataSchedulingHarness {
    metadata: Metadata,
}

#[cfg(feature = "test-faults")]
impl ActivityMetadataSchedulingHarness {
    pub fn enqueue(
        &mut self,
        reader: &mut crate::ActivityWindowHarness,
        response: beryl_backend::ThreadReadMetadata,
    ) -> bool {
        let viewport = reader.viewport_mut();
        self.metadata.synchronize(viewport);
        if self.metadata.receiver.is_some()
            || self.metadata.pending.is_some()
            || self.metadata.retry.is_some()
        {
            return false;
        }
        let Some(request) = viewport.metadata_candidate(&self.metadata.attempted) else {
            return false;
        };
        if request.thread() != response.thread_id() {
            return false;
        }
        self.metadata.attempted.push(request.thread().clone());
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send((request, Ok(Some(response))))
            .unwrap_or_else(|_| panic!("retained metadata receiver"));
        self.metadata.receiver = Some(receiver);
        true
    }
    pub fn step(
        &mut self,
        reader: &mut crate::ActivityWindowHarness,
        executor: &gpui::BackgroundExecutor,
    ) {
        self.metadata.step(reader.viewport_mut(), executor);
    }
}
