use std::sync::mpsc::{self, Receiver, SyncSender};

use beryl_backend::{
    ApprovalInterruption, ApprovalOperationCompletion, ApprovalRequest, DynamicToolCall,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitError, ResponseWorkError, ResponseWorkObserver,
};

pub struct ObservationSink {
    observations: SyncSender<ResponseWorkObserver>,
    approval_requests: Option<SyncSender<ApprovalRequest>>,
    dynamic_calls: SyncSender<DynamicToolCall>,
}

pub struct SinkHarness {
    pub sink: Box<dyn OrderedTurnStreamSink>,
    pub observations: Receiver<ResponseWorkObserver>,
    pub approval_requests: Receiver<ApprovalRequest>,
    pub dynamic_calls: Receiver<DynamicToolCall>,
}

pub fn sink_harness(retain_approval: bool) -> SinkHarness {
    let (observations, observed) = mpsc::sync_channel(1);
    let (approval_requests, received_approvals) = mpsc::sync_channel(1);
    let (dynamic_calls, received_calls) = mpsc::sync_channel(1);
    SinkHarness {
        sink: Box::new(ObservationSink {
            observations,
            approval_requests: retain_approval.then_some(approval_requests),
            dynamic_calls,
        }),
        observations: observed,
        approval_requests: received_approvals,
        dynamic_calls: received_calls,
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
                assert_eq!(before.retained_capabilities(), 1);
                assert!(!before.response_written());
                assert!(before.session_generation().is_some());
                self.dynamic_calls.try_send(call).unwrap();
                assert_eq!(observer.snapshot().unwrap(), before);
                self.observations.try_send(observer).unwrap();
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::DynamicArgumentControl(_)
            | OrderedTurnStreamOperation::DynamicSeal => Ok(OrderedTurnStreamCompletion::Applied),
            other => panic!("unexpected response-work fixture operation: {other:?}"),
        }
    }
}
