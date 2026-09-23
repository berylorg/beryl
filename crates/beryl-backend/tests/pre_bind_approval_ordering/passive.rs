use super::support::{connect_foreground, expect_close_without_text, send_approval, spawn_server};
use beryl_backend::{
    ApprovalOperationCompletion, ApprovalRequest, ApprovalRequestKind, ManagedBackendError,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamProgress,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitError, ResponseWorkObserver,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, decode_incoming_json_for_test,
    },
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

struct UnansweredSink {
    observed: Arc<Mutex<Vec<ResponseWorkObserver>>>,
    foreign: Option<ApprovalRequest>,
}

impl OrderedTurnStreamSink for UnansweredSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let OrderedTurnStreamOperation::Approval(request) = operation else {
            panic!("only approvals are supplied")
        };
        self.observed.lock().unwrap().push(request.response_work());
        Ok(OrderedTurnStreamCompletion::Approval(
            ApprovalOperationCompletion::Unanswered {
                request: self.foreign.take().unwrap_or(request),
            },
        ))
    }
}

#[test]
fn all_passive_approval_kinds_release_custody_without_response_and_keep_receiving() {
    let kinds = [
        ApprovalRequestKind::CommandExecution,
        ApprovalRequestKind::FileChange,
        ApprovalRequestKind::Permissions,
    ];
    let (endpoint, server) = spawn_server(move |socket| {
        for (index, kind) in kinds.into_iter().enumerate() {
            send_approval(socket, index as i64 + 1, kind);
        }
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 4);
    session.enable_full_turn_stream_for_lifecycle_test();
    let observed = Arc::new(Mutex::new(Vec::new()));
    session
        .bind_ordered_turn_stream_sink(Box::new(UnansweredSink {
            observed: observed.clone(),
            foreign: None,
        }))
        .unwrap();
    for count in 1..=3 {
        assert_eq!(
            session
                .poll_ordered_turn_stream_progress(Duration::from_secs(2))
                .unwrap(),
            OrderedTurnStreamProgress::Progress
        );
        let observers = observed.lock().unwrap();
        assert_eq!(observers.len(), count);
        for observer in observers.iter() {
            let snapshot = observer.snapshot().unwrap();
            assert!(!snapshot.response_written());
            assert_eq!(snapshot.retained_capabilities(), 0);
        }
    }
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn foreign_unanswered_completion_fails_without_automatic_denial() {
    let input = br#"{"method":"item/commandExecution/requestApproval","id":9,"params":{"threadId":"foreign"}}"#;
    let IncomingJsonTestOutcome::Approval { request, .. } =
        decode_incoming_json_for_test(input, 3, IncomingJsonExpectation::Idle).outcome
    else {
        panic!("fixture must produce a foreign approval")
    };
    let foreign_observer = request.response_work();
    let (endpoint, server) = spawn_server(|socket| {
        send_approval(socket, 1, ApprovalRequestKind::CommandExecution);
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let observed = Arc::new(Mutex::new(Vec::new()));
    session
        .bind_ordered_turn_stream_sink(Box::new(UnansweredSink {
            observed: observed.clone(),
            foreign: Some(request),
        }))
        .unwrap();
    assert!(matches!(
        session.poll_ordered_turn_stream_progress(Duration::from_secs(2)),
        Err(ManagedBackendError::OrderedTurnStreamUnexpectedCompletion { .. })
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    for observer in observed
        .lock()
        .unwrap()
        .iter()
        .chain(std::iter::once(&foreign_observer))
    {
        let snapshot = observer.snapshot().unwrap();
        assert!(!snapshot.response_written());
        assert_eq!(snapshot.retained_capabilities(), 0);
    }
    drop(session);
    server.join().unwrap();
}
