#[path = "provider_broker_checked_user/support.rs"]
mod support;

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

use beryl_backend::{
    NormalTurnTerminalStatus, OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
    UserMessageEchoLifecycle,
};
use beryl_home_store::{
    HomeCommand, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasItemId, CasTurnId};
use syndic_storage::{
    BindingState, CasItemSource, CasTurnSource, InputGateState, ProviderFrameOrdinalV1,
    ProviderItemLifecycle, SourceEventPayload, SourceEventSequence, TurnEndStatus,
    TurnIncompleteReason, TurnLifecycle, TurnTerminalOutcome,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

use support::*;

struct CheckedUserPreparationObservation {
    home_id: beryl_model::BerylHomeId,
    lifecycle: UserMessageEchoLifecycle,
    preparation_attempts: AtomicUsize,
    activation_dispatches: AtomicUsize,
}

static CHECKED_USER_PREPARATION_OBSERVATIONS: OnceLock<
    Mutex<Vec<Arc<CheckedUserPreparationObservation>>>,
> = OnceLock::new();

struct SourceActivationAuthorityHook {
    home_id: beryl_model::BerylHomeId,
    attempts: AtomicUsize,
    fail_non_health: std::sync::atomic::AtomicBool,
    paused: std::sync::mpsc::SyncSender<()>,
    resume: Mutex<std::sync::mpsc::Receiver<()>>,
}

static SOURCE_ACTIVATION_AUTHORITY_HOOKS: OnceLock<
    Mutex<Vec<Arc<SourceActivationAuthorityHook>>>,
> = OnceLock::new();

struct SourceActivationAuthorityController {
    hook: Arc<SourceActivationAuthorityHook>,
    paused: std::sync::mpsc::Receiver<()>,
    resume: std::sync::mpsc::SyncSender<()>,
}

struct CheckedUserPreparationController {
    observation: Arc<CheckedUserPreparationObservation>,
}

impl CheckedUserPreparationController {
    fn install(
        home_id: beryl_model::BerylHomeId,
        lifecycle: UserMessageEchoLifecycle,
    ) -> Self {
        let observation = Arc::new(CheckedUserPreparationObservation {
            home_id,
            lifecycle,
            preparation_attempts: AtomicUsize::new(0),
            activation_dispatches: AtomicUsize::new(0),
        });
        CHECKED_USER_PREPARATION_OBSERVATIONS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap()
            .push(Arc::clone(&observation));
        Self { observation }
    }

    fn preparation_attempts(&self) -> usize {
        self.observation
            .preparation_attempts
            .load(Ordering::SeqCst)
    }

    fn activation_dispatches(&self) -> usize {
        self.observation
            .activation_dispatches
            .load(Ordering::SeqCst)
    }
}

impl Drop for CheckedUserPreparationController {
    fn drop(&mut self) {
        let Some(observations) = CHECKED_USER_PREPARATION_OBSERVATIONS.get() else {
            return;
        };
        observations
            .lock()
            .unwrap()
            .retain(|candidate| !Arc::ptr_eq(candidate, &self.observation));
    }
}

impl SourceActivationAuthorityController {
    fn install(home_id: beryl_model::BerylHomeId) -> Self {
        let (paused_sender, paused) = std::sync::mpsc::sync_channel(0);
        let (resume, resume_receiver) = std::sync::mpsc::sync_channel(0);
        let hook = Arc::new(SourceActivationAuthorityHook {
            home_id,
            attempts: AtomicUsize::new(0),
            fail_non_health: std::sync::atomic::AtomicBool::new(false),
            paused: paused_sender,
            resume: Mutex::new(resume_receiver),
        });
        SOURCE_ACTIVATION_AUTHORITY_HOOKS
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
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("source activation authority did not reach the deterministic barrier");
    }

    fn fail_next_with_non_health_error(&self) {
        self.hook.fail_non_health.store(true, Ordering::SeqCst);
    }

    fn release(&self) {
        self.resume.send(()).unwrap();
    }

    fn attempts(&self) -> usize {
        self.hook.attempts.load(Ordering::SeqCst)
    }
}

impl Drop for SourceActivationAuthorityController {
    fn drop(&mut self) {
        let Some(hooks) = SOURCE_ACTIVATION_AUTHORITY_HOOKS.get() else {
            return;
        };
        hooks
            .lock()
            .unwrap()
            .retain(|candidate| !Arc::ptr_eq(candidate, &self.hook));
    }
}

pub(in crate::cas_projection::connection::provider_broker::ingester) fn pause_source_activation_authority_and_take_non_health_failure(
    home_id: beryl_model::BerylHomeId,
) -> bool {
    let hook = SOURCE_ACTIVATION_AUTHORITY_HOOKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .iter()
        .find(|hook| hook.home_id == home_id)
        .cloned();
    let Some(hook) = hook else {
        return false;
    };
    if hook.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
        hook.paused.send(()).unwrap();
        hook.resume.lock().unwrap().recv().unwrap();
    }
    hook.fail_non_health.swap(false, Ordering::SeqCst)
}

pub(in crate::cas_projection::connection::provider_broker::ingester) fn record_checked_user_preparation_attempt(
    home_id: beryl_model::BerylHomeId,
    lifecycle: UserMessageEchoLifecycle,
) {
    if let Some(observation) = checked_user_preparation_observation(home_id, Some(lifecycle)) {
        observation
            .preparation_attempts
            .fetch_add(1, Ordering::SeqCst);
    }
}

pub(in crate::cas_projection::connection::provider_broker::ingester) fn record_source_activation_dispatch(
    home_id: beryl_model::BerylHomeId,
) {
    if let Some(observation) = checked_user_preparation_observation(home_id, None) {
        observation
            .activation_dispatches
            .fetch_add(1, Ordering::SeqCst);
    }
}

fn checked_user_preparation_observation(
    home_id: beryl_model::BerylHomeId,
    lifecycle: Option<UserMessageEchoLifecycle>,
) -> Option<Arc<CheckedUserPreparationObservation>> {
    CHECKED_USER_PREPARATION_OBSERVATIONS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .iter()
        .find(|observation| {
            observation.home_id == home_id
                && lifecycle.is_none_or(|lifecycle| observation.lifecycle == lifecycle)
        })
        .cloned()
}

fn enter_verifying(fixture: &CheckedUserFixture, faults: &FaultController) {
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

fn attach_successful_supervisor(
    fixture: &CheckedUserFixture,
) -> (std::thread::JoinHandle<()>, Arc<AtomicUsize>) {
    let (signal, receiver) = std::sync::mpsc::sync_channel(1);
    fixture
        .failure_notification
        .attach_recovery_supervisor(signal)
        .unwrap();
    let home = Arc::clone(&fixture.home);
    let notification = fixture.failure_notification.clone();
    let signals = Arc::new(AtomicUsize::new(0));
    let observed_signals = Arc::clone(&signals);
    let worker = std::thread::spawn(move || {
        receiver.recv().unwrap();
        observed_signals.fetch_add(1, Ordering::SeqCst);
        home.verify_health().unwrap();
        let completed = notification.publish_verified_current_completion().unwrap();
        notification.finish_completed_recovery_supervisor_flight(completed, true);
    });
    (worker, signals)
}

#[test]
fn checked_user_acknowledgements_follow_exact_activation_and_same_item_publication() {
    let mut fixture = CheckedUserFixture::new(171);
    let cas_item_id = CasItemId::new("checked-user-item-171").unwrap();
    assert!(
        fixture
            .storage
            .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
            .unwrap()
            .is_none()
    );

    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());

    let active = fixture
        .storage
        .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(active.thread_id(), fixture.thread_id);
    assert_eq!(active.turn_id(), fixture.turn_id);
    assert_eq!(active.cas_thread_id(), &fixture.cas_thread_id);
    assert_eq!(active.cas_turn_id(), &fixture.cas_turn_id);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    let activation = fixture.source_event(1);
    assert_eq!(
        activation.source(),
        Some(&CasTurnSource::new(
            fixture.cas_thread_id.clone(),
            fixture.cas_turn_id.clone(),
        ))
    );
    assert!(matches!(
        activation.payload(),
        SourceEventPayload::TurnActivated
    ));
    let started_event = fixture.source_event(2);
    let SourceEventPayload::ItemFrame {
        item_id: started_item_id,
        frame: started_reference,
    } = started_event.payload()
    else {
        panic!("checked-user start did not publish an item frame")
    };
    assert_eq!(*started_item_id, fixture.item_id);
    let started_reference = started_reference.as_ref().clone();
    assert_eq!(
        started_reference.frame().ordinal(),
        ProviderFrameOrdinalV1::FIRST
    );
    let started_item = fixture.canonical_item();
    assert_eq!(started_item.id(), fixture.item_id);
    assert_eq!(
        started_item.presentation_content(),
        Some(fixture.submitted_content)
    );
    assert_eq!(
        started_item.provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert_eq!(
        started_item.source_event(),
        Some(SourceEventSequence::new(2).unwrap())
    );
    assert_eq!(
        started_item.cas_source(),
        Some(&CasItemSource::new(
            CasTurnSource::new(fixture.cas_thread_id.clone(), fixture.cas_turn_id.clone(),),
            cas_item_id.clone(),
        ))
    );
    assert_eq!(started_item.provider(), Some(&started_reference));
    assert_user_message_frame(
        &read_provider_frame(&fixture.home, fixture.storage, &started_reference),
        ProviderFrameOrdinalV1::FIRST,
        UserMessageEchoLifecycle::Started,
        &cas_item_id,
        fixture.submitted_content,
    );

    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id.clone());

    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        3
    );
    let completed_event = fixture.source_event(3);
    let SourceEventPayload::ItemFrame {
        item_id: completed_item_id,
        frame: completed_reference,
    } = completed_event.payload()
    else {
        panic!("checked-user completion did not publish an item frame")
    };
    assert_eq!(*completed_item_id, fixture.item_id);
    let completed_reference = completed_reference.as_ref().clone();
    assert_eq!(
        completed_reference.frame().ordinal(),
        ProviderFrameOrdinalV1::new(2).unwrap()
    );
    let completed_item = fixture.canonical_item();
    assert_eq!(completed_item.id(), fixture.item_id);
    assert_eq!(
        completed_item.presentation_content(),
        Some(fixture.submitted_content)
    );
    assert_eq!(
        completed_item.provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    assert_eq!(
        completed_item.source_event(),
        Some(SourceEventSequence::new(3).unwrap())
    );
    assert_eq!(completed_item.provider(), Some(&completed_reference));
    assert_user_message_frame(
        &read_provider_frame(&fixture.home, fixture.storage, &completed_reference),
        ProviderFrameOrdinalV1::new(2).unwrap(),
        UserMessageEchoLifecycle::Completed,
        &cas_item_id,
        fixture.submitted_content,
    );

    assert_eq!(
        completed_reference.content().id(),
        started_reference.content().id()
    );
    assert_eq!(
        completed_reference.frame().encoded_start(),
        started_reference.content().summary().encoded_bytes()
    );
    assert_eq!(
        started_reference
            .content()
            .revision()
            .checked_next()
            .unwrap(),
        completed_reference.content().revision()
    );
    assert_eq!(
        completed_reference.stream_state().started_at(),
        started_reference.stream_state().started_at()
    );
    assert!(completed_reference.stream_state().is_complete());
    assert!(
        fixture
            .storage
            .provider_item_build(&fixture.home, fixture.item_id, point_limit())
            .unwrap()
            .is_none()
    );

    let home_revision_before = fixture.home.home_revision().unwrap();
    let syndic_revision_before = fixture.storage.revision(&fixture.home).unwrap();
    let turn_before = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    let item_before = fixture.canonical_item();
    fixture
        .broker
        .as_ref()
        .unwrap()
        .prove_response_activation(&fixture.registration.proof(), &fixture.cas_turn_id)
        .unwrap();
    assert_eq!(fixture.home.home_revision().unwrap(), home_revision_before);
    assert_eq!(
        fixture.storage.revision(&fixture.home).unwrap(),
        syndic_revision_before
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap(),
        turn_before
    );
    assert_eq!(fixture.canonical_item(), item_before);

    fixture.close();
}

#[test]
fn checked_user_publication_barrier_holds_one_real_permit_and_releases_it() {
    let mut fixture = CheckedUserFixture::new(174);
    let cas_item_id = CasItemId::new("checked-user-item-174").unwrap();

    fixture.submit_checked_while_publication_paused(
        UserMessageEchoLifecycle::Started,
        cas_item_id,
        |fixture| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            let blocked = loop {
                let snapshot = fixture.broker_snapshot();
                if snapshot.in_flight().current() == 1 {
                    break snapshot;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "checked-user submission did not enter its capacity-one acknowledgement path"
                );
                std::thread::yield_now();
            };
            assert_eq!(blocked.in_flight().high_water(), 1);
            assert_eq!(blocked.submitted(), 1);
            assert_eq!(blocked.acked(), 0);
            assert_eq!(blocked.checked_user_publications().activity().current(), 1);
            assert_eq!(
                blocked.checked_user_publications().activity().high_water(),
                1
            );
            assert_eq!(blocked.checked_user_publications().publications(), 1);
        },
    );

    let released = fixture.broker_snapshot();
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.submitted(), 1);
    assert_eq!(released.acked(), 1);
    assert_eq!(
        released.checked_user_publications().activity().current(),
        0
    );
    assert_eq!(
        released
            .checked_user_publications()
            .activity()
            .high_water(),
        1
    );
    assert_eq!(released.checked_user_publications().publications(), 1);

    fixture.close();
}

#[test]
fn checked_user_preserves_non_health_activation_authority_failure_after_verified_current() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(230, faults.clone());
    let observation = CheckedUserPreparationController::install(
        fixture.home.home_id(),
        UserMessageEchoLifecycle::Started,
    );
    let authority = SourceActivationAuthorityController::install(fixture.home.home_id());
    let (supervisor, verification_signals) = attach_successful_supervisor(&fixture);
    let message = fixture.checked_message(
        UserMessageEchoLifecycle::Started,
        CasItemId::new("checked-user-item-230").unwrap(),
    );
    let mut sink = fixture.sink.take().unwrap();
    let (returned_sink, result) = std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let result = sink.submit(
                beryl_backend::OrderedTurnStreamOperation::CheckedUserMessage(message),
            );
            (sink, result)
        });
        authority.wait_until_paused();
        authority.fail_next_with_non_health_error();
        enter_verifying(&fixture, &faults);
        authority.release();
        worker.join().unwrap()
    });
    fixture.sink = Some(returned_sink);
    assert!(matches!(
        result,
        Ok(beryl_backend::OrderedTurnStreamCompletion::Applied)
    ));
    supervisor.join().unwrap();
    fixture.storage = syndic_storage::SyndicStorage::reacquire(&fixture.home).unwrap();

    assert_eq!(verification_signals.load(Ordering::SeqCst), 1);
    assert_eq!(authority.attempts(), 1);
    assert_eq!(observation.activation_dispatches(), 0);
    assert_eq!(observation.preparation_attempts(), 0);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        0
    );
    assert_eq!(fixture.broker_snapshot().submitted(), 1);
    assert_eq!(fixture.broker_snapshot().acked(), 1);
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);

    drop(authority);
    drop(observation);
    fixture.close();
}

#[test]
fn checked_user_activation_reconciles_health_gate_without_redispatch() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(226, faults.clone());
    let observation = CheckedUserPreparationController::install(
        fixture.home.home_id(),
        UserMessageEchoLifecycle::Started,
    );
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::active_cas_turn_fault_scope(),
    );
    let (supervisor, verification_signals) = attach_successful_supervisor(&fixture);
    let cas_item_id = CasItemId::new("checked-user-item-226").unwrap();

    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id);
    supervisor.join().unwrap();
    fixture.storage = syndic_storage::SyndicStorage::reacquire(&fixture.home).unwrap();

    assert_eq!(verification_signals.load(Ordering::SeqCst), 1);
    assert_eq!(observation.activation_dispatches(), 1);
    assert_eq!(fixture.registration.terminal_reason(), None);
    assert_eq!(observation.preparation_attempts(), 1);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    assert_eq!(fixture.broker_snapshot().submitted(), 1);
    assert_eq!(fixture.broker_snapshot().acked(), 1);
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);

    drop(observation);
    fixture.close();
}

#[test]
fn checked_user_non_health_failure_concurrent_with_verified_current_is_not_retried() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(227, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-227").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    let active_before = fixture
        .storage
        .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
        .unwrap();
    let observation = CheckedUserPreparationController::install(
        fixture.home.home_id(),
        UserMessageEchoLifecycle::Started,
    );
    let (supervisor, verification_signals) = attach_successful_supervisor(&fixture);

    fixture.submit_checked_while_publication_paused(
        UserMessageEchoLifecycle::Started,
        cas_item_id,
        |fixture| enter_verifying(fixture, &faults),
    );
    supervisor.join().unwrap();

    assert_eq!(verification_signals.load(Ordering::SeqCst), 1);
    assert_eq!(observation.activation_dispatches(), 0);
    assert_eq!(observation.preparation_attempts(), 1);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert_eq!(
        fixture
            .storage
            .active_cas_turn(&fixture.home, fixture.snapshot_id, point_limit())
            .unwrap(),
        active_before
    );
    assert_eq!(
        fixture.canonical_item().provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    assert_eq!(fixture.broker_snapshot().submitted(), 2);
    assert_eq!(fixture.broker_snapshot().acked(), 2);
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);

    drop(observation);
    fixture.close();
}

#[test]
fn checked_user_final_event_reconciles_verified_current_without_duplicate_dispatch() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(193, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-193").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );
    let (signal, receiver) = std::sync::mpsc::sync_channel(1);
    fixture
        .failure_notification
        .attach_recovery_supervisor(signal)
        .unwrap();
    let home = std::sync::Arc::clone(&fixture.home);
    let notification = fixture.failure_notification.clone();
    std::thread::scope(|scope| {
        let supervisor = scope.spawn(move || {
            receiver.recv().unwrap();
            home.verify_health().unwrap();
            let completed = notification
                .publish_verified_current_completion()
                .unwrap();
            notification.finish_completed_recovery_supervisor_flight(completed, false);
        });
        fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);
        supervisor.join().unwrap();
    });

    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        3
    );
    assert_eq!(
        fixture.canonical_item().provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    assert_eq!(fixture.broker_snapshot().submitted(), 2);
    assert_eq!(fixture.broker_snapshot().acked(), 2);
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.registration.terminal_reason(), None);

    fixture.close();
}

#[test]
fn checked_user_shutdown_authority_drops_permits_without_target_invalidation() {
    let mut fixture = CheckedUserFixture::new(194);
    let cas_item_id = CasItemId::new("checked-user-item-194").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    fixture
        .failure_notification
        .publish_shutdown_completion()
        .unwrap();

    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);

    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    assert_eq!(
        fixture.canonical_item().provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert!(fixture.registration.terminal_reason().is_none());

    fixture.close();
}

#[test]
fn mismatched_completed_item_closes_the_exact_target_without_advancing_lifecycle() {
    let mut fixture = CheckedUserFixture::new(172);
    let cas_item_id = CasItemId::new("checked-user-item-172").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id);
    let started_item = fixture.canonical_item();
    let syndic_revision_before = fixture.storage.revision(&fixture.home).unwrap();

    fixture.submit_checked(
        UserMessageEchoLifecycle::Completed,
        CasItemId::new("wrong-checked-user-item-172").unwrap(),
    );

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert_eq!(fixture.canonical_item(), started_item);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        2
    );
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(3).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.storage.revision(&fixture.home).unwrap(),
        syndic_revision_before
    );

    fixture.close();
}

include!("provider_broker_checked_user/terminal.rs");

#[test]
fn normal_terminal_before_turn_start_closes_only_the_exact_target() {
    let mut fixture = CheckedUserFixture::before_turn_start(178);
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::EventBeforeTurnStart
        )
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
    assert!(state.end_status().is_none());

    fixture.close();
}

#[test]
fn normal_terminal_route_mismatch_does_not_publish_a_terminal_event() {
    let mut fixture = CheckedUserFixture::new(179);
    let cas_item_id = CasItemId::new("checked-user-item-179").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id);
    let rejected = fixture
        .try_submit_terminal_for_route(
            NormalTurnTerminalStatus::Completed,
            fixture.cas_thread_id.clone(),
            CasTurnId::new("wrong-terminal-turn-179").unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        rejected.cause(),
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl)
    );
    assert!(
        fixture
            .try_submit_terminal_for_route(
                NormalTurnTerminalStatus::Completed,
                fixture.cas_thread_id.clone(),
                fixture.cas_turn_id.clone(),
            )
            .is_err()
    );

    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::ConflictingTurnIdentity
        )
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Active);
    assert_eq!(state.source_event_count(), 2);
    assert!(state.end_status().is_none());
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(3).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );

    fixture.close();
}
