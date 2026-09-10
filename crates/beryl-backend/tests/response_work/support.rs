use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Wake, Waker};

use beryl_backend::{
    ApprovalInterruption, ApprovalOperationCompletion, ApprovalRequest, DynamicToolCall,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitError, ResponseWorkError, ResponseWorkObserver, ResponseWorkSnapshot,
};

pub struct CompletionProbe {
    observer: ResponseWorkObserver,
    count: AtomicUsize,
    snapshot: Mutex<Option<ResponseWorkSnapshot>>,
}

impl CompletionProbe {
    pub fn new(observer: &ResponseWorkObserver) -> Arc<Self> {
        Arc::new(Self {
            observer: observer.clone(),
            count: AtomicUsize::new(0),
            snapshot: Mutex::new(None),
        })
    }

    pub fn register(self: &Arc<Self>) -> Result<(), ResponseWorkError> {
        self.observer
            .register_completion_waker(Waker::from(Arc::clone(self)))
    }

    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    pub fn snapshot(&self) -> ResponseWorkSnapshot {
        self.snapshot.lock().unwrap().clone().unwrap()
    }
}

impl Wake for CompletionProbe {
    fn wake(self: Arc<Self>) {
        let snapshot = self.observer.snapshot().unwrap();
        assert!(snapshot.response_written() || snapshot.retained_capabilities() == 0);
        self.observer
            .validate_revision(snapshot.revision())
            .unwrap();
        *self.snapshot.lock().unwrap() = Some(snapshot);
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct ObservationSink {
    observations: SyncSender<ResponseWorkObserver>,
    approval_requests: Option<SyncSender<ApprovalRequest>>,
    dynamic_calls: SyncSender<DynamicToolCall>,
    completions: SyncSender<Arc<CompletionProbe>>,
}

pub struct SinkHarness {
    pub sink: Box<dyn OrderedTurnStreamSink>,
    pub observations: Receiver<ResponseWorkObserver>,
    pub approval_requests: Receiver<ApprovalRequest>,
    pub dynamic_calls: Receiver<DynamicToolCall>,
    pub completions: Receiver<Arc<CompletionProbe>>,
}

pub fn sink_harness(retain_approval: bool) -> SinkHarness {
    let (observations, observed) = mpsc::sync_channel(1);
    let (approval_requests, received_approvals) = mpsc::sync_channel(1);
    let (dynamic_calls, received_calls) = mpsc::sync_channel(1);
    let (completions, received_completions) = mpsc::sync_channel(1);
    SinkHarness {
        sink: Box::new(ObservationSink {
            observations,
            approval_requests: retain_approval.then_some(approval_requests),
            dynamic_calls,
            completions,
        }),
        observations: observed,
        approval_requests: received_approvals,
        dynamic_calls: received_calls,
        completions: received_completions,
    }
}

impl OrderedTurnStreamSink for ObservationSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        match operation {
            OrderedTurnStreamOperation::Approval(request) => {
                let observer = request.response_work();
                let before = observer.snapshot().unwrap();
                let completion = CompletionProbe::new(&observer);
                completion.register().unwrap();
                assert_eq!(before.retained_capabilities(), 2);
                assert!(!before.response_written());
                assert!(before.session_generation().is_some());
                assert_eq!(observer.snapshot().unwrap(), before);
                if let Some(sender) = &self.approval_requests {
                    sender.try_send(request).unwrap();
                    assert_eq!(observer.snapshot().unwrap(), before);
                } else {
                    drop(request);
                    assert_eq!(
                        observer.validate_revision(before.revision()),
                        Err(ResponseWorkError::StaleRevision),
                    );
                    let after = observer.snapshot().unwrap();
                    assert_eq!(after.retained_capabilities(), 1);
                    assert!(!after.response_written());
                }
                assert_eq!(completion.count(), 0);
                self.completions.try_send(completion).ok().unwrap();
                self.observations.try_send(observer).unwrap();
                Ok(OrderedTurnStreamCompletion::Approval(
                    ApprovalOperationCompletion::Routed {
                        interruption: ApprovalInterruption::NotRequired,
                    },
                ))
            }
            OrderedTurnStreamOperation::DynamicBegin(call) => {
                let observer = call.response_work();
                let before = observer.snapshot().unwrap();
                let completion = CompletionProbe::new(&observer);
                completion.register().unwrap();
                assert_eq!(before.retained_capabilities(), 1);
                assert!(!before.response_written());
                assert!(before.session_generation().is_some());
                self.dynamic_calls.try_send(call).unwrap();
                assert_eq!(observer.snapshot().unwrap(), before);
                assert_eq!(completion.count(), 0);
                self.completions.try_send(completion).ok().unwrap();
                self.observations.try_send(observer).unwrap();
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::DynamicArgumentControl(_)
            | OrderedTurnStreamOperation::DynamicSeal => Ok(OrderedTurnStreamCompletion::Applied),
            other => panic!("unexpected response-work fixture operation: {other:?}"),
        }
    }
}
