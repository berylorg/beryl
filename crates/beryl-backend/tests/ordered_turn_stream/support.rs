use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
};

use beryl_backend::{
    DynamicToolArgumentControl, DynamicToolArgumentScalarKind, DynamicToolCall,
    DynamicToolCallAbandonReason, DynamicToolCallRequestId, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
    OrderedTurnStreamSubmitError,
};
use beryl_stream::{PagePool, PagePoolDiagnostics};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailOn {
    DynamicBegin,
    DynamicControl,
    DynamicAcquirePage,
    Fragment,
    DynamicSeal,
}

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub dynamic_begins: Vec<DynamicBeginTrace>,
    pub dynamic_controls: Vec<DynamicToolArgumentControl>,
    pub dynamic_fragments: Vec<DynamicFragmentTrace>,
    pub dynamic_seals: usize,
    pub dynamic_abandons: Vec<DynamicToolCallAbandonReason>,
    pub dynamic_leased_at_seal: Vec<usize>,
    pub dynamic_leased_at_abandon: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBeginTrace {
    pub request_id: DynamicRequestIdTrace,
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub namespace: Option<String>,
    pub tool: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicRequestIdTrace {
    Integer(i64),
    String(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFragmentTrace {
    pub kind: DynamicToolArgumentScalarKind,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

pub struct SinkHarness {
    pub sink: Box<dyn OrderedTurnStreamSink>,
    pub trace: Arc<Mutex<Trace>>,
    pub pool: PagePool,
    pub dynamic_calls: mpsc::Receiver<DynamicToolCall>,
}

struct TestSink {
    pool: PagePool,
    trace: Arc<Mutex<Trace>>,
    failure: Option<(FailOn, OrderedTurnStreamSubmitCause)>,
    dynamic_call_sender: mpsc::Sender<DynamicToolCall>,
}

pub fn sink_harness(
    page_capacity: usize,
    failure: Option<(FailOn, OrderedTurnStreamSubmitCause)>,
) -> SinkHarness {
    let pool = PagePool::new(nonzero(page_capacity), nonzero(1)).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let (dynamic_call_sender, dynamic_calls) = mpsc::channel();
    let sink = TestSink {
        pool: pool.clone(),
        trace: Arc::clone(&trace),
        failure,
        dynamic_call_sender,
    };
    SinkHarness {
        sink: Box::new(sink),
        trace,
        pool,
        dynamic_calls,
    }
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

impl OrderedTurnStreamSink for TestSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        match operation {
            OrderedTurnStreamOperation::DynamicBegin(call) => {
                let request_id = match call.request_id() {
                    DynamicToolCallRequestId::Integer(value) => {
                        DynamicRequestIdTrace::Integer(*value)
                    }
                    DynamicToolCallRequestId::String(value) => {
                        DynamicRequestIdTrace::String(value.to_string())
                    }
                };
                self.trace().dynamic_begins.push(DynamicBeginTrace {
                    request_id,
                    thread_id: call.thread_id().as_str().to_string(),
                    turn_id: call.turn_id().as_str().to_string(),
                    call_id: call.call_id().as_str().to_string(),
                    namespace: call.namespace().map(str::to_string),
                    tool: call.tool().as_str().to_string(),
                });
                let operation = OrderedTurnStreamOperation::DynamicBegin(call);
                self.fail_if(FailOn::DynamicBegin, operation)
                    .and_then(|operation| {
                        let OrderedTurnStreamOperation::DynamicBegin(call) = operation else {
                            unreachable!();
                        };
                        self.dynamic_call_sender.send(call).map_or_else(
                            |error| {
                                Err(OrderedTurnStreamSubmitError::new(
                                    OrderedTurnStreamOperation::DynamicBegin(error.0),
                                    OrderedTurnStreamSubmitCause::ReceiverLost,
                                ))
                            },
                            |_| Ok(OrderedTurnStreamCompletion::Applied),
                        )
                    })
            }
            OrderedTurnStreamOperation::DynamicArgumentControl(control) => {
                self.trace().dynamic_controls.push(control);
                self.fail_if(
                    FailOn::DynamicControl,
                    OrderedTurnStreamOperation::DynamicArgumentControl(control),
                )
                .map(|_| OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::DynamicAcquirePage => self
                .fail_if(
                    FailOn::DynamicAcquirePage,
                    OrderedTurnStreamOperation::DynamicAcquirePage,
                )
                .and_then(|_| {
                    self.pool
                        .try_lease()
                        .map(OrderedTurnStreamCompletion::PageLease)
                        .map_err(|_| {
                            OrderedTurnStreamSubmitError::new(
                                OrderedTurnStreamOperation::DynamicAcquirePage,
                                OrderedTurnStreamSubmitCause::CapacityFull,
                            )
                        })
                }),
            OrderedTurnStreamOperation::DynamicArgumentFragment(fragment) => {
                self.trace().dynamic_fragments.push(DynamicFragmentTrace {
                    kind: fragment.kind(),
                    offset: fragment.offset(),
                    bytes: fragment.bytes().to_vec(),
                });
                let operation = OrderedTurnStreamOperation::DynamicArgumentFragment(fragment);
                self.fail_if(FailOn::Fragment, operation).map(|operation| {
                    let OrderedTurnStreamOperation::DynamicArgumentFragment(fragment) = operation
                    else {
                        unreachable!();
                    };
                    let mut lease = fragment.into_lease();
                    lease.clear();
                    OrderedTurnStreamCompletion::PageLease(lease)
                })
            }
            OrderedTurnStreamOperation::DynamicSeal => {
                let leased = self.pool.diagnostics().leased;
                let mut trace = self.trace();
                trace.dynamic_seals += 1;
                trace.dynamic_leased_at_seal.push(leased);
                drop(trace);
                self.fail_if(FailOn::DynamicSeal, OrderedTurnStreamOperation::DynamicSeal)
                    .map(|_| OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::DynamicAbandon(reason) => {
                let leased = self.pool.diagnostics().leased;
                let mut trace = self.trace();
                trace.dynamic_abandons.push(reason);
                trace.dynamic_leased_at_abandon.push(leased);
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            operation => Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(
                    beryl_backend::OrderedTurnStreamRejection::SchemaMismatch,
                ),
            )),
        }
    }
}

impl TestSink {
    fn trace(&self) -> std::sync::MutexGuard<'_, Trace> {
        self.trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn fail_if(
        &mut self,
        expected: FailOn,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamOperation, OrderedTurnStreamSubmitError> {
        if self.failure.is_some_and(|(actual, _)| actual == expected) {
            let (_, cause) = self.failure.take().unwrap();
            Err(OrderedTurnStreamSubmitError::new(operation, cause))
        } else {
            Ok(operation)
        }
    }
}

pub fn trace_snapshot(trace: &Arc<Mutex<Trace>>) -> Trace {
    trace
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

pub fn diagnostics(pool: &PagePool) -> PagePoolDiagnostics {
    pool.diagnostics()
}
