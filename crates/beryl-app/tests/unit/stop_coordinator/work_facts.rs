use super::*;
use crate::cas_projection::stop_work::*;

fn revision(fixture: &StopFixture) -> StopWorkRevision {
    StopWorkRevision {
        owner: Arc::new(()),
        home_id: fixture.home.home_id(),
        home_generation: fixture.home.health().generation().unwrap(),
        service_generation: fixture.command_gate.authorizer().service_generation(),
        stamp: fixture.coordinator.work_revision().unwrap(),
    }
}

fn page(
    fixture: &StopFixture,
    revision: &StopWorkRevision,
    cursor: Option<&StopWorkCursor>,
    limits: StopWorkPageLimits,
) -> Result<StopWorkPage, StopWorkError> {
    let mut builder = StopWorkPageBuilder::new(cursor.map(|cursor| &cursor.after), limits);
    fixture
        .coordinator
        .collect_work_records(revision.stamp, &mut builder)?;
    Ok(builder.finish(revision.clone()))
}

fn limits(count: usize) -> StopWorkPageLimits {
    StopWorkPageLimits::new(count, 65_536).unwrap()
}

#[test]
fn stop_work_pages_preserve_removed_primary_and_terminal_driver_cleanup() {
    let fixture = StopFixture::new(211);
    let home_before = fixture.home.home_revision().unwrap();
    let empty = revision(&fixture);
    assert!(
        page(&fixture, &empty, None, limits(8))
            .unwrap()
            .records()
            .is_empty()
    );
    assert_eq!(fixture.home.home_revision().unwrap(), home_before);
    let StopOwnership::Primary(owner) = fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
        )
        .unwrap()
    else {
        panic!("first stop must own dispatch");
    };
    assert_eq!(
        page(&fixture, &empty, None, limits(8)),
        Err(StopWorkError::StaleRevision)
    );
    let admitted = revision(&fixture);
    let fact = page(&fixture, &admitted, None, limits(8)).unwrap();
    let [StopWorkRecord::Stop(stop)] = fact.records() else {
        panic!("one stop");
    };
    assert_eq!(
        stop.local_dispatch,
        Some(StopDispatchWorkState::ClaimedNotDispatched)
    );
    assert!(stop.primary_custody);
    let cleanup = owner.retain_driver_custody();
    owner.begin_dispatch().unwrap();
    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);
    let removed = revision(&fixture);
    let fact = page(&fixture, &removed, None, limits(8)).unwrap();
    let [StopWorkRecord::Stop(stop)] = fact.records() else {
        panic!("one removed stop");
    };
    assert!(stop.local_dispatch.is_none());
    assert!(stop.primary_custody && stop.driver_custody);
    drop(owner);
    let driver = revision(&fixture);
    let fact = page(&fixture, &driver, None, limits(8)).unwrap();
    let [StopWorkRecord::Stop(stop)] = fact.records() else {
        panic!("one driver tail");
    };
    assert!(!stop.primary_custody && stop.driver_custody);
    assert_eq!(fact, page(&fixture, &driver, None, limits(8)).unwrap());
    drop(cleanup);
    assert_eq!(
        page(&fixture, &driver, None, limits(8)),
        Err(StopWorkError::StaleRevision)
    );
    assert!(
        page(&fixture, &revision(&fixture), None, limits(8))
            .unwrap()
            .records()
            .is_empty()
    );
}

#[test]
fn stop_work_pages_bound_retained_records_and_merge_same_thread_owners() {
    let fixture = StopFixture::new(212);
    let StopOwnership::Primary(owner) = fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
        )
        .unwrap()
    else {
        panic!("first owner");
    };
    let first_id = owner.operation_id();
    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let source = state.stops.get(&fixture.thread).unwrap().clone();
        for index in 0u64..300 {
            let mut thread_bytes = [0; 16];
            thread_bytes[..8].copy_from_slice(&index.to_be_bytes());
            let thread = SyndicThreadId::from_bytes(thread_bytes);
            let mut record = source.clone();
            record.operation_id = StopOperationId::new(
                thread,
                syndic_storage::StopOperationNonce::from_bytes([1; 16]),
            );
            record.target = StopOperationTarget::new(
                thread,
                fixture.turn,
                source.target.turn_kind(),
                source.target.binding_revision(),
                source.target.snapshot_id(),
                source.target.runtime_id(),
                source.target.loaded_generation(),
                CasThreadId::new(format!("retained-thread-{index}")).unwrap(),
                source.target.cas_turn_id().clone(),
            );
            record.dispatch = LocalDispatchState::DurablyAbandoned;
            state.stops.insert(thread, record);
        }
        let current = state.stops.get_mut(&fixture.thread).unwrap();
        let different = if first_id.nonce().as_bytes() == &[0; 16] {
            [255; 16]
        } else {
            [0; 16]
        };
        current.operation_id = StopOperationId::new(
            fixture.thread,
            syndic_storage::StopOperationNonce::from_bytes(different),
        );
    }
    let version = revision(&fixture);
    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let result = page(
            &fixture,
            &version,
            cursor.as_ref(),
            StopWorkPageLimits::new(usize::MAX, usize::MAX).unwrap(),
        )
        .unwrap();
        assert!(result.records().len() <= 256 && result.bytes() <= 65_536);
        for row in result.records() {
            let StopWorkRecord::Stop(stop) = row else {
                panic!("stop record");
            };
            ids.push(stop.operation_id);
        }
        cursor = result.next_cursor().cloned();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 302);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    let first = page(&fixture, &version, None, limits(1)).unwrap();
    assert_eq!(
        page(
            &fixture,
            &version,
            None,
            StopWorkPageLimits::new(256, first.bytes()).unwrap()
        )
        .unwrap(),
        first
    );
    assert_eq!(
        page(
            &fixture,
            &version,
            None,
            StopWorkPageLimits::new(1, 1).unwrap()
        ),
        Err(StopWorkError::ByteLimit)
    );
    drop(owner);
    assert_eq!(
        page(&fixture, &version, first.next_cursor(), limits(1)),
        Err(StopWorkError::StaleRevision)
    );
}

#[test]
fn permission_work_facts_follow_preparation_and_disposal_without_primary_authority() {
    let fixture = StopFixture::new(213);
    let before = revision(&fixture);
    let custody = fixture
        .coordinator
        .observe_permission(fixture.proof.permission_work_fact(None))
        .unwrap();
    assert_eq!(
        page(&fixture, &before, None, limits(8)),
        Err(StopWorkError::StaleRevision)
    );
    let reserved = page(&fixture, &revision(&fixture), None, limits(8)).unwrap();
    let [StopWorkRecord::Permission(fact)] = reserved.records() else {
        panic!("reserved permission");
    };
    assert_eq!(fact.stage, PermissionInterruptionWorkStage::Reserved);
    assert_eq!(fact.operation_id, None);
    let operation = StopOperationId::new(
        fixture.thread,
        syndic_storage::StopOperationNonce::from_bytes([5; 16]),
    );
    custody.prepared(operation);
    for stage in [
        PermissionInterruptionWorkStage::Prepared,
        PermissionInterruptionWorkStage::Pending,
        PermissionInterruptionWorkStage::Driver,
    ] {
        custody.set_stage(stage);
        let snapshot = page(&fixture, &revision(&fixture), None, limits(8)).unwrap();
        let [StopWorkRecord::Permission(fact)] = snapshot.records() else {
            panic!("owned permission");
        };
        assert_eq!(fact.stage, stage);
        assert_eq!(fact.operation_id, Some(operation));
        assert_eq!(fact.thread_id, fixture.thread);
    }
    let final_version = revision(&fixture);
    drop(custody);
    assert_eq!(
        page(&fixture, &final_version, None, limits(8)),
        Err(StopWorkError::StaleRevision)
    );
    assert!(
        page(&fixture, &revision(&fixture), None, limits(8))
            .unwrap()
            .records()
            .is_empty()
    );
}

#[test]
fn exhausted_permission_serial_permanently_rejects_observation_without_admission_effects() {
    let fixture = StopFixture::new(214);
    fixture.coordinator.state.lock().unwrap().next_permission = u64::MAX;
    let home_revision = fixture.home.home_revision().unwrap();
    assert!(
        fixture
            .coordinator
            .observe_permission(fixture.proof.permission_work_fact(None))
            .is_none()
    );
    assert_eq!(
        fixture.coordinator.work_revision(),
        Err(StopWorkError::RevisionUnavailable)
    );
    let ownership = fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
        )
        .unwrap();
    assert!(matches!(ownership, StopOwnership::Primary(_)));
    drop(ownership);
    assert_ne!(fixture.home.home_revision().unwrap(), home_revision);
    assert_eq!(
        fixture.coordinator.work_revision(),
        Err(StopWorkError::RevisionUnavailable)
    );
}

#[test]
fn rejected_permission_disposes_custody_after_router_and_command_fences_release() {
    use crate::cas_projection::{
        LiveEventTargetCloseReason,
        connection::{ApprovalRouteOutcome, PreparedApprovalInterruption},
    };
    use beryl_backend::{
        ApprovalRequestKind, ApprovalResponseDisposition, StopAttemptCorrelation,
        lifecycle_test_support::approval_request,
    };
    let fixture = StopFixture::new(215);
    let custody = fixture
        .coordinator
        .observe_permission(fixture.proof.permission_work_fact(None))
        .unwrap();
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        Some(fixture.target.cas_thread_id().clone()),
        Some(fixture.target.cas_turn_id().clone()),
        None,
    );
    let prepared = PreparedApprovalInterruption::new(
        ApprovalInterruption::DurableStopOwned {
            operation: StopOperationCorrelation::from_bytes([215; 16]),
            target: ExactForegroundTurn::new(
                fixture.target.runtime_id(),
                fixture.target.loaded_generation(),
                fixture.target.cas_thread_id().clone(),
                fixture.target.cas_turn_id().clone(),
            ),
            attempt_disposition: StopAttemptDisposition::PossiblyDispatched(
                StopAttemptCorrelation::from_bytes([215; 16]),
            ),
        },
        None,
        Some(custody),
    );
    fixture
        .router
        .retire(LiveEventTargetCloseReason::ConnectionRetired);
    let pause = fixture
        .coordinator
        .install_race_pause(StopRaceStage::PermissionCustodyDrop);
    let state = fixture.coordinator.state.lock().unwrap();
    let command = fixture.command_gate.authorizer().authorize().unwrap();
    std::thread::scope(|scope| {
        let route = scope.spawn(|| {
            fixture
                .router
                .route_approval(&command, request, Some(prepared))
        });
        let reached = pause.wait_until_reached(Duration::from_secs(2));
        let (sent, received) = mpsc::sync_channel(1);
        let authorizer = fixture.command_gate.authorizer();
        let gate_check = scope.spawn(move || {
            let permitted = authorizer.authorize().is_ok();
            sent.send(permitted).unwrap();
        });
        let available = received.recv_timeout(Duration::from_secs(2));
        pause.release();
        drop(state);
        gate_check.join().unwrap();
        assert!(matches!(
            route.join().unwrap(),
            ApprovalRouteOutcome::Rejected { .. }
        ));
        assert!(reached);
        assert_eq!(
            available,
            Ok(true),
            "custody disposal must not retain the command gate"
        );
    });
    assert!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .permissions
            .is_empty()
    );
}
