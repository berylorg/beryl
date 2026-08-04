use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use beryl_backend::{
    ManagedBackendError, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamProgress, OrderedTurnStreamRejection, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError, ProviderObservationAbandonReason,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationRoute,
    ProviderValueContext,
    lifecycle_test_support::{
        decode_provider_json_for_test, decode_provider_transport_loss_for_test,
    },
};
use beryl_stream::{PagePool, PagePoolDiagnostics};

#[derive(Clone, Copy)]
pub struct SinkOptions {
    pub page_capacity: usize,
    pub fail_context: Option<ProviderValueContext>,
    pub fragment_failure: Option<OrderedTurnStreamSubmitCause>,
}

impl SinkOptions {
    pub const fn with_page_capacity(page_capacity: usize) -> Self {
        Self {
            page_capacity,
            fail_context: None,
            fragment_failure: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub begins: Vec<ProviderObservationBegin>,
    pub controls: Vec<ProviderObservationControl>,
    pub fragments: Vec<(ProviderValueContext, Vec<u8>)>,
    pub seal_routes: Vec<ProviderObservationRoute>,
    pub abandons: Vec<ProviderObservationAbandonReason>,
    pub leased_at_abandon: Vec<usize>,
    pub leased_at_seal: Vec<usize>,
}

pub struct CaseResult {
    pub outcome: Result<OrderedTurnStreamProgress, ManagedBackendError>,
    pub trace: Trace,
    pub pool: PagePoolDiagnostics,
}

pub fn drive(message: String, options: SinkOptions) -> CaseResult {
    drive_with_fragments(message, options, None)
}

pub fn drive_fragmented(
    message: String,
    options: SinkOptions,
    fragment_bytes: usize,
) -> CaseResult {
    drive_with_fragments(message, options, Some(fragment_bytes))
}

pub fn drive_truncated(
    message: String,
    options: SinkOptions,
    transmitted_bytes: usize,
) -> CaseResult {
    assert!(transmitted_bytes > 0 && transmitted_bytes < message.len());
    let (mut sink, trace, pool) = TraceSink::new(options);
    let outcome = decode_provider_transport_loss_for_test(
        &message.as_bytes()[..transmitted_bytes],
        7,
        &mut sink,
    );
    let trace = std::mem::take(&mut *trace.lock().unwrap_or_else(|poison| poison.into_inner()));
    CaseResult {
        outcome,
        trace,
        pool: pool.diagnostics(),
    }
}

fn drive_with_fragments(
    message: String,
    options: SinkOptions,
    fragment_bytes: Option<usize>,
) -> CaseResult {
    let (mut sink, trace, pool) = TraceSink::new(options);
    let outcome = decode_provider_json_for_test(
        message.as_bytes(),
        fragment_bytes.unwrap_or(1024),
        &mut sink,
    );
    let trace = std::mem::take(&mut *trace.lock().unwrap_or_else(|poison| poison.into_inner()));
    CaseResult {
        outcome,
        trace,
        pool: pool.diagnostics(),
    }
}

pub fn fragments_for(trace: &Trace, context: ProviderValueContext) -> Vec<u8> {
    trace
        .fragments
        .iter()
        .filter(|(actual, _)| *actual == context)
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect()
}

pub struct TraceSink {
    pool: PagePool,
    trace: Arc<Mutex<Trace>>,
    fail_context: Option<ProviderValueContext>,
    fragment_failure: Option<OrderedTurnStreamSubmitCause>,
}

impl TraceSink {
    pub fn new(options: SinkOptions) -> (Self, Arc<Mutex<Trace>>, PagePool) {
        let pool = PagePool::new(nonzero(options.page_capacity), nonzero(1)).unwrap();
        let trace = Arc::new(Mutex::new(Trace::default()));
        (
            Self {
                pool: pool.clone(),
                trace: Arc::clone(&trace),
                fail_context: options.fail_context,
                fragment_failure: options.fragment_failure,
            },
            trace,
            pool,
        )
    }

    fn trace(&self) -> std::sync::MutexGuard<'_, Trace> {
        self.trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl OrderedTurnStreamSink for TraceSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        match operation {
            OrderedTurnStreamOperation::Approval(request) => {
                Err(OrderedTurnStreamSubmitError::new(
                    OrderedTurnStreamOperation::Approval(request),
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::SchemaMismatch,
                    ),
                ))
            }
            operation @ (OrderedTurnStreamOperation::ThreadStatusChanged(_)
            | OrderedTurnStreamOperation::ThreadClosed(_)
            | OrderedTurnStreamOperation::TurnStarted(_)
            | OrderedTurnStreamOperation::CheckedUserMessage(_)
            | OrderedTurnStreamOperation::NormalTurnTerminal(_)
            | OrderedTurnStreamOperation::DynamicBegin(_)
            | OrderedTurnStreamOperation::DynamicArgumentControl(_)
            | OrderedTurnStreamOperation::DynamicAcquirePage
            | OrderedTurnStreamOperation::DynamicArgumentFragment(_)
            | OrderedTurnStreamOperation::DynamicSeal
            | OrderedTurnStreamOperation::DynamicAbandon(_)) => {
                Err(OrderedTurnStreamSubmitError::new(
                    operation,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::SchemaMismatch,
                    ),
                ))
            }
            OrderedTurnStreamOperation::ProviderBegin(begin) => {
                self.trace().begins.push(begin);
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::ProviderControl(control) => {
                self.trace().controls.push(control);
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::ProviderAcquirePage => self
                .pool
                .try_lease()
                .map(OrderedTurnStreamCompletion::PageLease)
                .map_err(|_| {
                    OrderedTurnStreamSubmitError::new(
                        OrderedTurnStreamOperation::ProviderAcquirePage,
                        OrderedTurnStreamSubmitCause::CapacityFull,
                    )
                }),
            OrderedTurnStreamOperation::ProviderFragment(fragment) => {
                let context = fragment.context();
                self.trace()
                    .fragments
                    .push((context, fragment.bytes().to_vec()));
                if self.fail_context == Some(context)
                    && let Some(cause) = self.fragment_failure.take()
                {
                    return Err(OrderedTurnStreamSubmitError::new(
                        OrderedTurnStreamOperation::ProviderFragment(fragment),
                        cause,
                    ));
                }
                let mut lease = fragment.into_lease();
                lease.clear();
                Ok(OrderedTurnStreamCompletion::PageLease(lease))
            }
            OrderedTurnStreamOperation::ProviderSeal(route) => {
                let leased = self.pool.diagnostics().leased;
                let mut trace = self.trace();
                trace.seal_routes.push(route);
                trace.leased_at_seal.push(leased);
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            OrderedTurnStreamOperation::ProviderAbandon(reason) => {
                let leased = self.pool.diagnostics().leased;
                let mut trace = self.trace();
                trace.abandons.push(reason);
                trace.leased_at_abandon.push(leased);
                Ok(OrderedTurnStreamCompletion::Applied)
            }
        }
    }
}

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

pub fn agent_started(text: &str) -> String {
    format!(
        "{{\"method\":\"item/started\",\"params\":{{\"item\":{{\"type\":\"agentMessage\",\"id\":\"item_1\",\"text\":{}}},\"threadId\":\"thread_1\",\"turnId\":\"turn_1\",\"startedAtMs\":123}}}}",
        serde_json::to_string(text).unwrap()
    )
}

pub fn lifecycle(item: &str, completed: bool) -> String {
    let (method, timestamp) = if completed {
        ("item/completed", "completedAtMs")
    } else {
        ("item/started", "startedAtMs")
    };
    format!(
        "{{\"method\":\"{method}\",\"params\":{{\"item\":{item},\"threadId\":\"thread_1\",\"turnId\":\"turn_1\",\"{timestamp}\":123}}}}"
    )
}

pub fn mcp_item(arguments: &str, result: Option<&str>) -> String {
    format!(
        "{{\"type\":\"mcpToolCall\",\"id\":\"item_1\",\"server\":\"server\",\"tool\":\"tool\",\"status\":\"completed\",\"arguments\":{arguments}{}}}",
        result
            .map(|value| format!(",\"result\":{value}"))
            .unwrap_or_default()
    )
}

pub fn delta_message(method: &str, payload: &str) -> String {
    format!(
        "{{\"method\":\"{method}\",\"params\":{{\"threadId\":\"thread_1\",\"turnId\":\"turn_1\",\"itemId\":\"item_1\",{payload}}}}}"
    )
}
