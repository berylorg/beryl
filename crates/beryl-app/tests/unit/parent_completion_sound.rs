use super::*;
use crate::notification_audio::{Backend, Control, NotificationAudioLane, SoundEvent};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{AdmittedHostPath, CasProcessGeneration, PathFlavor};
use beryl_state::{ApplySettings, ExpectedSettingRevision, SettingUpdate, SettingValue};
use std::{path::Path, sync::mpsc, time::Duration};
use syndic_storage::SyndicTimestamp;

#[path = "../normal_terminal/server.rs"]
mod server;
#[path = "../projection/syndic.rs"]
mod syndic;

struct Recorder(mpsc::SyncSender<SoundEvent>);
impl Backend for Recorder {
    fn attempt(&mut self, event: SoundEvent, _: &Arc<Control>) -> Result<(), String> {
        self.0.try_send(event).map_err(|e| e.to_string())
    }
}

fn facts(monitor: &PlatformAttentionMonitor) -> Arc<DesktopFacts> {
    Arc::new(DesktopFacts {
        closed: AtomicBool::new(false),
        focus: AtomicU8::new(2),
        platform: monitor.reader(),
        attention_override: std::sync::Mutex::new(Some(PlatformAttentionState {
            local_input_idle: AttentionTriggerState::Unknown,
            session_locked: AttentionTriggerState::Unsupported,
            lid_closed: AttentionTriggerState::Inactive,
            display_inactive: AttentionTriggerState::Unknown,
        })),
        attempts: std::sync::atomic::AtomicUsize::new(0),
    })
}

fn setting(fixture: &syndic::Fixture, path: Option<&str>) {
    let home = fixture.home();
    let settings = fixture.state.settings();
    let prior = settings.setting(&home, SettingKey::EndTurnSound).unwrap();
    let expected = prior.map_or(ExpectedSettingRevision::Absent, |r| {
        ExpectedSettingRevision::Exact(r.revision())
    });
    let value = SettingValue::end_turn_sound(
        path.map(|p| AdmittedHostPath::from_admitted(PathFlavor::Windows, p).unwrap()),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&home).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::EndTurnSound,
                    expected,
                    value,
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

struct Noop;
impl crate::LifecycleYieldRequestHandler for Noop {
    fn respond_lifecycle_yield(
        &mut self,
        _: crate::cas_projection::OrdinaryDynamicToolContext,
        _: crate::LifecycleYieldRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        beryl_backend::DynamicToolCallResponse::success_text("unused")
    }
}
impl crate::BranchDiscussionResolutionRequestHandler for Noop {
    fn respond_branch_discussion_resolution(
        &mut self,
        _: crate::cas_projection::BranchDiscussionResolutionContext,
        _: crate::BranchDiscussionResolutionRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        beryl_backend::DynamicToolCallResponse::success_text("unused")
    }
}

fn live_terminal(status: &'static str, enabled: bool, focus: u8, active_trigger: bool) {
    use crate::cas_projection::*;
    let mut fixture = syndic::Fixture::new(137);
    eprintln!(
        "parent completion sound fixture: {}",
        fixture.home_path().display()
    );
    fixture.submit_text(server::SUBMITTED_TEXT);
    setting(&fixture, Some("C:\\sounds\\earlier.wav"));
    setting(&fixture, enabled.then_some("C:\\sounds\\current.wav"));
    let mut monitor = PlatformAttentionMonitor::spawn();
    let facts = facts(&monitor);
    facts.focus.store(focus, Ordering::Release);
    if active_trigger {
        facts
            .attention_override
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .session_locked = AttentionTriggerState::Active;
    }
    let (tx, rx) = mpsc::sync_channel(2);
    let mut audio = NotificationAudioLane::with_backend(Recorder(tx));
    fixture.store.bind_parent_sound(ParentSoundSink {
        facts: Arc::downgrade(&facts),
        audio: audio.ingress(),
        settings: fixture.state.settings(),
        service_generation: fixture.store.service_generation(),
    });
    let duplicate = status == "duplicate";
    let server = if duplicate {
        server::NormalTerminalServer::spawn_duplicate_parent_terminal()
    } else {
        server::NormalTerminalServer::spawn_parent_terminal(status)
    };
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), server::AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding(),
            CasProcessGeneration::new(37_137).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            server::TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                syndic::execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                SyndicTimestamp::from_unix_millis(37_000),
                server::TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let mut lifecycle = Noop;
    let mut branch = Noop;
    let result = coordinator.execute_ordinary_turn(
        &*fixture.home(),
        &fixture.storage,
        &fixture.state.assets(),
        None,
        projection,
        &fixture.cancellation,
        &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), server::TIMEOUT),
        OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
    );
    assert!(
        matches!(result, Ok(OrdinaryTurnExecutionOutcome::Terminal { .. })),
        "{result:?}"
    );
    if duplicate {
        server.release_stop_terminal();
        let Ok(OrdinaryTurnExecutionOutcome::Terminal { projection, .. }) = &result else {
            unreachable!()
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while projection.is_live().unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "duplicate terminal was not consumed"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(facts.attempts.load(Ordering::Acquire), 1);
    if enabled && (focus == 2 || active_trigger) {
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(event.kind, SoundKind::EndTurn);
        assert_eq!(event.path, Path::new("C:\\sounds\\current.wav"));
    } else {
        assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
    }
    assert!(rx.try_recv().is_err());
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    audio.finish();
    monitor.close();
    monitor.take_worker().unwrap().join().unwrap();
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn live_item_then_complete_offers_current_sound_once_without_selected_window() {
    live_terminal("completed", true, 2, false);
}
#[test]
fn live_interrupted_without_error_offers_sound_once() {
    live_terminal("interrupted", true, 2, false);
}
#[test]
fn live_failed_parent_offers_sound_once() {
    live_terminal("failed", true, 2, false);
}
#[test]
fn current_disabled_sound_consumes_terminal_attempt() {
    live_terminal("completed", false, 2, true);
}
#[test]
fn focused_unknown_attention_does_not_offer() {
    live_terminal("completed", true, 1, false);
}
#[test]
fn focused_known_attention_overrides_unknown_other_facts() {
    live_terminal("completed", true, 1, true);
}

#[test]
fn duplicate_live_terminal_cannot_offer_again() {
    live_terminal("duplicate", true, 2, false);
}

#[test]
fn parent_classification_and_service_fences_control_metadata_admission() {
    use syndic_storage::TurnKind;
    let fixture = syndic::Fixture::new(138);
    eprintln!(
        "parent completion sound classification fixture: {}",
        fixture.home_path().display()
    );
    setting(&fixture, Some("C:\\sounds\\classified.wav"));
    let mut monitor = PlatformAttentionMonitor::spawn();
    let facts = facts(&monitor);
    let (tx, rx) = mpsc::sync_channel(2);
    let mut audio = NotificationAudioLane::with_backend(Recorder(tx));
    let sink = ParentSoundSink {
        facts: Arc::downgrade(&facts),
        audio: audio.ingress(),
        settings: fixture.state.settings(),
        service_generation: fixture.store.service_generation(),
    };
    let authorizer = fixture.store.live_command_authorizer();
    let reference = fixture.home().service_reference();
    let generation = fixture.store.home_generation();
    for kind in [TurnKind::OrdinaryUser, TurnKind::BerylDiscussionHandoff] {
        sink.notify(&reference, generation, &authorizer, kind, || true);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap().kind,
            SoundKind::EndTurn
        );
    }
    for kind in [
        TurnKind::BerylLifecycleContinuation,
        TurnKind::ProviderOperation(syndic_storage::ProviderOperationKind::ContextCompaction),
    ] {
        sink.notify(&reference, generation, &authorizer, kind, || true);
    }
    sink.notify(
        &reference,
        generation,
        &authorizer,
        TurnKind::OrdinaryUser,
        || false,
    );
    let current_checks = std::sync::atomic::AtomicUsize::new(0);
    sink.notify(
        &reference,
        generation,
        &authorizer,
        TurnKind::OrdinaryUser,
        || current_checks.fetch_add(1, Ordering::AcqRel) == 0,
    );
    assert_eq!(current_checks.load(Ordering::Acquire), 2);
    let other = syndic::Fixture::new(139);
    eprintln!(
        "parent completion sound replacement fixture: {}",
        other.home_path().display()
    );
    setting(&other, Some("C:\\sounds\\replacement.wav"));
    sink.notify(
        &other.home().service_reference(),
        other.store.home_generation(),
        &other.store.live_command_authorizer(),
        TurnKind::OrdinaryUser,
        || true,
    );
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    sink.notify(
        &reference,
        generation,
        &authorizer,
        TurnKind::OrdinaryUser,
        || true,
    );
    assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
    let (other_directory, service) = other.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    audio.finish();
    monitor.take_worker().unwrap().join().unwrap();
    drop(directory);
    drop(other_directory);
}

#[test]
fn native_monitor_close_before_initialization_drains_worker() {
    platform_attention::test_stopped_before_start();
    for _ in 0..8 {
        let mut monitor = PlatformAttentionMonitor::spawn();
        monitor.close();
        monitor.take_worker().unwrap().join().unwrap();
        assert!(monitor.is_finished());
    }
}

#[test]
fn native_creation_failure_releases_only_window_context_reference() {
    assert!(platform_attention::test_rejected_creation_balances_context());
}
