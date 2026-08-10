use std::{error::Error, fmt};

#[cfg(feature = "test-faults")]
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

#[cfg(feature = "test-faults")]
use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, ProviderField, ProviderItemKind,
    ProviderItemLifecycle, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationRoute, ProviderScalar, ProviderValueContext,
    lifecycle_test_support::provider_observation_fragment,
};
#[cfg(feature = "test-faults")]
use beryl_home_store::{
    HomeCommand, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
#[cfg(feature = "test-faults")]
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::{ProviderObservationValidatorError, SyndicRecordError};

use super::*;

#[cfg(feature = "test-faults")]
#[path = "provider_broker_checked_user/support.rs"]
mod ordinary_support;

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparationHookPoint {
    Begin,
    Seal,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparationObservation {
    identity: beryl_model::ProviderObservationId,
    route: Option<syndic_storage::ProviderObservationRoute>,
}

#[cfg(feature = "test-faults")]
struct PreparationHook {
    home_id: beryl_model::BerylHomeId,
    point: PreparationHookPoint,
    attempts: AtomicUsize,
    fail_non_health_authority: std::sync::atomic::AtomicBool,
    observations: Mutex<Vec<PreparationObservation>>,
    paused: mpsc::SyncSender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

#[cfg(feature = "test-faults")]
static PREPARATION_HOOKS: OnceLock<Mutex<Vec<Arc<PreparationHook>>>> = OnceLock::new();

#[cfg(feature = "test-faults")]
struct PreparationHookController {
    hook: Arc<PreparationHook>,
    paused: mpsc::Receiver<()>,
    resume: mpsc::SyncSender<()>,
}

#[cfg(feature = "test-faults")]
impl PreparationHookController {
    fn install(home_id: beryl_model::BerylHomeId, point: PreparationHookPoint) -> Self {
        let (paused_sender, paused) = mpsc::sync_channel(0);
        let (resume, resume_receiver) = mpsc::sync_channel(0);
        let hook = Arc::new(PreparationHook {
            home_id,
            point,
            attempts: AtomicUsize::new(0),
            fail_non_health_authority: std::sync::atomic::AtomicBool::new(false),
            observations: Mutex::new(Vec::new()),
            paused: paused_sender,
            resume: Mutex::new(resume_receiver),
        });
        PREPARATION_HOOKS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap()
            .push(Arc::clone(&hook));
        Self {
            hook,
            paused,
            resume,
        }
    }

    fn wait_until_paused(&self) {
        self.paused
            .recv_timeout(Duration::from_secs(1))
            .expect("provider preparation did not reach the deterministic barrier");
    }

    fn release(&self) {
        self.resume.send(()).unwrap();
    }

    fn attempts(&self) -> usize {
        self.hook.attempts.load(Ordering::SeqCst)
    }

    fn observations(&self) -> Vec<PreparationObservation> {
        self.hook.observations.lock().unwrap().clone()
    }

    fn fail_next_authority_with_non_health_error(&self) {
        self.hook
            .fail_non_health_authority
            .store(true, Ordering::SeqCst);
    }
}

#[cfg(feature = "test-faults")]
impl Drop for PreparationHookController {
    fn drop(&mut self) {
        let Some(hooks) = PREPARATION_HOOKS.get() else {
            return;
        };
        hooks
            .lock()
            .unwrap()
            .retain(|candidate| !Arc::ptr_eq(candidate, &self.hook));
    }
}

#[cfg(feature = "test-faults")]
impl PreparationHook {
    fn observe(
        &self,
        identity: beryl_model::ProviderObservationId,
        route: Option<syndic_storage::ProviderObservationRoute>,
    ) {
        self.observations
            .lock()
            .unwrap()
            .push(PreparationObservation { identity, route });
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            self.paused.send(()).unwrap();
            self.resume.lock().unwrap().recv().unwrap();
        }
    }
}

#[cfg(feature = "test-faults")]
fn preparation_hook(
    home_id: beryl_model::BerylHomeId,
    point: PreparationHookPoint,
) -> Option<Arc<PreparationHook>> {
    PREPARATION_HOOKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .iter()
        .find(|hook| hook.home_id == home_id && hook.point == point)
        .cloned()
}

#[cfg(feature = "test-faults")]
pub(super) fn pause_begin_preparation(
    home_id: beryl_model::BerylHomeId,
    identity: beryl_model::ProviderObservationId,
) {
    if let Some(hook) = preparation_hook(home_id, PreparationHookPoint::Begin) {
        hook.observe(identity, None);
    }
}

#[cfg(feature = "test-faults")]
pub(in crate::cas_projection::connection::provider_broker::ingester) fn take_begin_non_health_authority_failure(
    home_id: beryl_model::BerylHomeId,
) -> bool {
    preparation_hook(home_id, PreparationHookPoint::Begin).is_some_and(|hook| {
        hook.fail_non_health_authority.swap(false, Ordering::SeqCst)
    })
}

#[cfg(feature = "test-faults")]
pub(super) fn pause_seal_preparation(
    home_id: beryl_model::BerylHomeId,
    identity: beryl_model::ProviderObservationId,
    route: &syndic_storage::ProviderObservationRoute,
) {
    if let Some(hook) = preparation_hook(home_id, PreparationHookPoint::Seal) {
        hook.observe(identity, Some(route.clone()));
    }
}

#[cfg(feature = "test-faults")]
fn enter_verifying(fixture: &ordinary_support::CheckedUserFixture, faults: &FaultController) {
    let state = BerylState::reacquire(&fixture.home).unwrap();
    let settings = state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(1),
    );
    let mut command = HomeCommand::new(fixture.home.home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(&fixture.home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    fixture.home.execute(command).unwrap_err();
    assert_eq!(fixture.home.health().state(), HomeHealthState::Verifying);
}

#[cfg(feature = "test-faults")]
fn attach_successful_supervisor(
    fixture: &ordinary_support::CheckedUserFixture,
) -> (std::thread::JoinHandle<()>, mpsc::SyncSender<()>) {
    let (signal, receiver) = mpsc::sync_channel(1);
    fixture
        .failure_notification
        .attach_recovery_supervisor(signal)
        .unwrap();
    let home = Arc::clone(&fixture.home);
    let notification = fixture.failure_notification.clone();
    let (done, done_receiver) = mpsc::sync_channel(0);
    let supervisor = std::thread::spawn(move || {
        receiver.recv().unwrap();
        home.verify_health().unwrap();
        let completed = notification.publish_verified_current_completion().unwrap();
        done_receiver.recv().unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        notification.finish_completed_recovery_supervisor_flight(completed, false);
    });
    (supervisor, done)
}

#[cfg(feature = "test-faults")]
fn submit_applied(
    fixture: &mut ordinary_support::CheckedUserFixture,
    operation: OrderedTurnStreamOperation,
) {
    assert!(matches!(
        fixture.sink.as_mut().unwrap().submit(operation),
        Ok(OrderedTurnStreamCompletion::Applied)
    ));
}

#[cfg(feature = "test-faults")]
fn submit_fragment(
    fixture: &mut ordinary_support::CheckedUserFixture,
    context: ProviderValueContext,
    bytes: &[u8],
) {
    let mut page = match fixture
        .sink
        .as_mut()
        .unwrap()
        .submit(OrderedTurnStreamOperation::ProviderAcquirePage)
        .unwrap()
    {
        OrderedTurnStreamCompletion::PageLease(page) => page,
        completion => panic!("unexpected provider page completion: {completion:?}"),
    };
    page.buffer_mut()[..bytes.len()].copy_from_slice(bytes);
    page.set_len(bytes.len()).unwrap();
    let fragment = provider_observation_fragment(context, page);
    assert!(matches!(
        fixture
            .sink
            .as_mut()
            .unwrap()
            .submit(OrderedTurnStreamOperation::ProviderFragment(fragment)),
        Ok(OrderedTurnStreamCompletion::PageLease(_))
    ));
}

#[cfg(feature = "test-faults")]
fn begin_agent_observation(fixture: &mut ordinary_support::CheckedUserFixture) {
    submit_applied(
        fixture,
        OrderedTurnStreamOperation::ProviderBegin(ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }),
    );
    submit_applied(
        fixture,
        OrderedTurnStreamOperation::ProviderControl(ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(10),
        }),
    );
    for (context, bytes) in [
        (
            ProviderValueContext::Field(ProviderField::ItemId),
            b"ordinary-provider-item".as_slice(),
        ),
        (
            ProviderValueContext::Field(ProviderField::AgentMessageText),
            b"ordinary provider text".as_slice(),
        ),
    ] {
        submit_applied(
            fixture,
            OrderedTurnStreamOperation::ProviderControl(ProviderObservationControl::BeginField(
                context,
            )),
        );
        submit_fragment(fixture, context, bytes);
        submit_applied(
            fixture,
            OrderedTurnStreamOperation::ProviderControl(ProviderObservationControl::EndField(
                context,
            )),
        );
    }
}

#[cfg(feature = "test-faults")]
fn submit_while_preparation_paused(
    fixture: &mut ordinary_support::CheckedUserFixture,
    operation: OrderedTurnStreamOperation,
    barrier: &PreparationHookController,
    paused: impl FnOnce(&ordinary_support::CheckedUserFixture),
) -> Result<OrderedTurnStreamCompletion, beryl_backend::OrderedTurnStreamSubmitError> {
    let mut sink = fixture.sink.take().unwrap();
    let (sink, result) = std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let result = sink.submit(operation);
            (sink, result)
        });
        barrier.wait_until_paused();
        paused(fixture);
        barrier.release();
        worker.join().unwrap()
    });
    fixture.sink = Some(sink);
    result
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_begin_joins_pre_read_verifying_and_dispatches_one_exact_batch() {
    let faults = FaultController::new();
    let mut fixture = ordinary_support::CheckedUserFixture::with_faults(221, faults.clone());
    let (supervisor, done) = attach_successful_supervisor(&fixture);
    enter_verifying(&fixture, &faults);

    submit_applied(
        &mut fixture,
        OrderedTurnStreamOperation::ProviderBegin(ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }),
    );

    assert_eq!(fixture.broker_snapshot().provider_staging_batches(), 1);
    assert!(fixture.registration.terminal_reason().is_none());
    done.send(()).unwrap();
    supervisor.join().unwrap();
    fixture.close();
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_begin_reacquires_after_verified_current_without_redispatch() {
    let faults = FaultController::new();
    let mut fixture = ordinary_support::CheckedUserFixture::with_faults(222, faults.clone());
    let barrier =
        PreparationHookController::install(fixture.home.home_id(), PreparationHookPoint::Begin);
    let (supervisor, done) = attach_successful_supervisor(&fixture);
    let result = submit_while_preparation_paused(
        &mut fixture,
        OrderedTurnStreamOperation::ProviderBegin(ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }),
        &barrier,
        |fixture| enter_verifying(fixture, &faults),
    );

    assert!(matches!(result, Ok(OrderedTurnStreamCompletion::Applied)));
    assert_eq!(barrier.attempts(), 2);
    let observations = barrier.observations();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0].identity, observations[1].identity);
    assert_eq!(fixture.broker_snapshot().provider_staging_batches(), 1);
    assert!(fixture.registration.terminal_reason().is_none());
    done.send(()).unwrap();
    supervisor.join().unwrap();
    drop(barrier);
    fixture.close();
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_begin_preserves_non_health_authority_failure_after_verified_current() {
    let faults = FaultController::new();
    let mut fixture = ordinary_support::CheckedUserFixture::with_faults(229, faults.clone());
    let barrier =
        PreparationHookController::install(fixture.home.home_id(), PreparationHookPoint::Begin);
    let (supervisor, done) = attach_successful_supervisor(&fixture);
    let result = submit_while_preparation_paused(
        &mut fixture,
        OrderedTurnStreamOperation::ProviderBegin(ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }),
        &barrier,
        |fixture| {
            barrier.fail_next_authority_with_non_health_error();
            enter_verifying(fixture, &faults);
        },
    );
    done.send(()).unwrap();
    supervisor.join().unwrap();

    assert!(matches!(
        result.as_ref().map_err(|error| error.cause()),
        Err(OrderedTurnStreamSubmitCause::Rejected(
            beryl_backend::OrderedTurnStreamRejection::StagingConflict
        ))
    ));
    assert_eq!(barrier.attempts(), 1);
    assert_eq!(fixture.broker_snapshot().provider_staging_batches(), 0);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(crate::cas_projection::connection::router::LiveEventTargetCloseReason::StreamFailure)
    );
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    drop(barrier);
    fixture.close();
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_seal_reopens_only_the_exact_health_gated_observation() {
    let faults = FaultController::new();
    let mut fixture = ordinary_support::CheckedUserFixture::with_faults(223, faults.clone());
    begin_agent_observation(&mut fixture);
    let source_events_before = fixture
        .storage
        .turn_state(
            &fixture.home,
            fixture.turn_id,
            ordinary_support::point_limit(),
        )
        .unwrap()
        .unwrap()
        .source_event_count();
    let barrier =
        PreparationHookController::install(fixture.home.home_id(), PreparationHookPoint::Seal);
    let (supervisor, done) = attach_successful_supervisor(&fixture);
    let route =
        ProviderObservationRoute::new(fixture.cas_thread_id.clone(), fixture.cas_turn_id.clone());
    let exact_route = syndic_storage::ProviderObservationRoute::new(
        fixture.cas_thread_id.clone(),
        fixture.cas_turn_id.clone(),
    );

    let result = submit_while_preparation_paused(
        &mut fixture,
        OrderedTurnStreamOperation::ProviderSeal(route),
        &barrier,
        |fixture| enter_verifying(fixture, &faults),
    );

    done.send(()).unwrap();
    supervisor.join().unwrap();
    assert!(matches!(result, Ok(OrderedTurnStreamCompletion::Applied)));
    assert_eq!(barrier.attempts(), 2);
    let observations = barrier.observations();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0].identity, observations[1].identity);
    assert!(
        observations
            .iter()
            .all(|observation| observation.route.as_ref() == Some(&exact_route))
    );
    assert_eq!(fixture.broker_snapshot().provider_seal_acks(), 1);
    assert_eq!(
        fixture
            .storage
            .turn_state(
                &fixture.home,
                fixture.turn_id,
                ordinary_support::point_limit()
            )
            .unwrap()
            .unwrap()
            .source_event_count(),
        source_events_before + 2
    );
    let _event = fixture.source_event(source_events_before + 2);
    assert!(fixture.registration.terminal_reason().is_none());
    drop(barrier);
    fixture.close();
}

#[cfg(feature = "test-faults")]
#[test]
fn provider_seal_rejects_terminal_and_foreign_health_gates_after_verified_current() {
    const EXPECTED_GENERATION: u64 = 41;

    assert!(
        super::super::super::consumer::health_gate_values_match(
            HomeHealthState::Verifying,
            EXPECTED_GENERATION,
            EXPECTED_GENERATION,
        )
    );
    assert!(
        [HomeHealthState::Failed, HomeHealthState::Reopening]
            .into_iter()
            .all(|state| !super::super::super::consumer::health_gate_values_match(
                state,
                EXPECTED_GENERATION,
                EXPECTED_GENERATION,
            ))
    );
    assert!(!super::super::super::consumer::health_gate_values_match(
        HomeHealthState::Verifying,
        EXPECTED_GENERATION + 1,
        EXPECTED_GENERATION,
    ));
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy)]
enum TerminalPreparationCompletion {
    FailedOrStale,
    Shutdown,
}

#[cfg(feature = "test-faults")]
fn assert_terminal_pre_read_completion(completion: TerminalPreparationCompletion, seed: u8) {
    let faults = FaultController::new();
    let mut fixture = ordinary_support::CheckedUserFixture::with_faults(seed, faults.clone());
    let (signal, receiver) = mpsc::sync_channel(1);
    fixture
        .failure_notification
        .attach_recovery_supervisor(signal)
        .unwrap();
    enter_verifying(&fixture, &faults);
    let commands = fixture.commands.clone();
    let notification = fixture.failure_notification.clone();
    let supervisor = std::thread::spawn(move || {
        receiver.recv().unwrap();
        assert_eq!(commands.active_command_count_for_test(), 1);
        match completion {
            TerminalPreparationCompletion::FailedOrStale => {
                assert!(matches!(
                    notification.elect_and_publish_stale_completion(),
                    Err(crate::cas_projection::LiveCommandAdmissionError::Unavailable)
                ));
            }
            TerminalPreparationCompletion::Shutdown => {
                notification.publish_shutdown_completion().unwrap();
            }
        }
    });

    submit_applied(
        &mut fixture,
        OrderedTurnStreamOperation::ProviderBegin(ProviderObservationBegin::Item {
            lifecycle: ProviderItemLifecycle::Started,
            kind: ProviderItemKind::AgentMessage,
        }),
    );

    supervisor.join().unwrap();
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.broker_snapshot().provider_staging_batches(), 0);
    assert!(fixture.registration.terminal_reason().is_none());
    fixture.close();
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_begin_returns_typed_failed_or_stale_before_command_drain() {
    assert_terminal_pre_read_completion(TerminalPreparationCompletion::FailedOrStale, 224);
}

#[cfg(feature = "test-faults")]
#[test]
fn ordinary_begin_returns_typed_shutdown_before_command_drain() {
    assert_terminal_pre_read_completion(TerminalPreparationCompletion::Shutdown, 225);
}

#[derive(Debug)]
struct PhysicalFailure;

impl fmt::Display for PhysicalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("physical staging failure")
    }
}

impl Error for PhysicalFailure {}

#[test]
fn staging_rejection_preserves_schema_and_infrastructure_taxonomy() {
    let schema = [
        ProviderObservationStagingError::<PhysicalFailure>::Validation(
            ProviderObservationValidatorError::StructureMismatch,
        ),
        ProviderObservationStagingError::Batch(ProviderObservationStageBatchError::EmptyFragment),
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::FragmentTooLarge { actual: 65_537 },
        ),
    ];
    for error in schema {
        assert_eq!(
            staging_rejection(&error),
            OrderedTurnStreamRejection::SchemaMismatch
        );
    }

    let infrastructure = [
        ProviderObservationStagingError::<PhysicalFailure>::Batch(
            ProviderObservationStageBatchError::InvalidTransition,
        ),
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::FrontierOverflow,
        ),
        ProviderObservationStagingError::Batch(ProviderObservationStageBatchError::ReplayMismatch),
        ProviderObservationStagingError::Record(SyndicRecordError::LengthOverflow {
            kind: "provider observation",
        }),
        ProviderObservationStagingError::Callback(PhysicalFailure),
    ];
    for error in infrastructure {
        assert_eq!(
            staging_rejection(&error),
            OrderedTurnStreamRejection::StagingConflict
        );
    }
}
