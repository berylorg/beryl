use crate::running_owner::RunningShutdownSession;
use beryl_home_store::CommandError;

#[test]
fn exit_session_readiness_requires_current_home_and_session_receipt() {
    let (directory, home, session) = open(1);
    let (_foreign_directory, foreign, foreign_session) = open(1);
    let outcome =
        RunningShutdownSession::Settled(execute_exit_session(&home, &session, placements(1)));
    let revision = home.home_revision().unwrap();
    assert!(outcome.require_ready(&home, &session).is_ok());
    assert!(outcome.require_ready(&foreign, &foreign_session).is_err());
    assert!(outcome.require_ready(&home, &foreign_session).is_err());
    assert_eq!(home.home_revision().unwrap(), revision);
    foreign.close().unwrap();
    home.close().unwrap();

    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let reopened = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert!(outcome.require_ready(&reopened, &state.session()).is_err());
    reopened.close().unwrap();
}

#[test]
fn exit_session_readiness_rejects_nonpublication_and_unsupported_outcomes() {
    let (_directory, home, session) = open(1);
    let clean = execute_exit_session(&home, &session, placements(1)).unwrap();
    let ExitSessionExecution::Committed { receipt, .. } = clean else {
        panic!("clean commit")
    };
    let outcomes = [
        RunningShutdownSession::Pending,
        RunningShutdownSession::Reconciling,
        RunningShutdownSession::Unwound,
        RunningShutdownSession::Settled(Err(ExitSessionPreparationError::Changed)),
        RunningShutdownSession::Settled(Ok(ExitSessionExecution::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission,
        })),
        RunningShutdownSession::Reconciled(ExitSessionReconciled::ExactOld {
            original_failure: CommandError::CancelledBeforeAdmission,
        }),
        RunningShutdownSession::Reconciled(ExitSessionReconciled::Blocked {
            original_failure: CommandError::CancelledBeforeAdmission,
            resolution: ReconciliationResolution::Collision,
        }),
        RunningShutdownSession::Reconciled(ExitSessionReconciled::Blocked {
            original_failure: CommandError::CancelledBeforeAdmission,
            resolution: ReconciliationResolution::ExactSuccessor { receipt },
        }),
    ];
    for outcome in outcomes {
        let before = format!("{outcome:?}");
        assert!(
            outcome.require_ready(&home, &session).is_err(),
            "{outcome:?}"
        );
        assert_eq!(format!("{outcome:?}"), before);
    }
    home.close().unwrap();
}

#[test]
fn exit_session_readiness_preserves_postcommit_failure_and_capability() {
    for kind in [std::io::ErrorKind::Other, std::io::ErrorKind::StorageFull] {
        let faults = FaultController::new();
        let (_directory, home, session) = open_with_faults(1, faults.clone());
        faults.fail_next_with_kind(FaultPoint::AfterPersist, kind);
        let outcome =
            RunningShutdownSession::Settled(execute_exit_session(&home, &session, placements(1)));
        let before = format!("{outcome:?}");
        assert!(
            outcome
                .require_ready(&home, &session)
                .unwrap_err()
                .contains("later failure")
        );
        assert_eq!(format!("{outcome:?}"), before);
        assert!(matches!(
            outcome,
            RunningShutdownSession::Settled(Ok(ExitSessionExecution::Committed {
                later_failure: Some(_),
                local_finalization: Some(_),
                ..
            }))
        ));
        home.close().unwrap();
    }
}

#[test]
fn exit_session_readiness_rechecks_health_and_rejects_unaffected_domain() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    let clean =
        RunningShutdownSession::Settled(execute_exit_session(&home, &session, placements(1)));
    assert!(clean.require_ready(&home, &session).is_ok());
    let settings = BerylState::reacquire(&home).unwrap().settings();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&home).unwrap(),
                beryl_state::ApplySettings::new(vec![beryl_state::SettingUpdate::new(
                    beryl_state::SettingKey::ActiveThemeId,
                    beryl_state::ExpectedSettingRevision::Absent,
                    beryl_state::SettingValue::active_theme_id("readiness").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    let CommandOutcome::Committed { receipt, .. } = home.execute(command) else {
        panic!("settings commit")
    };
    let unrelated = RunningShutdownSession::Settled(Ok(ExitSessionExecution::Committed {
        receipt,
        later_failure: None,
        local_finalization: None,
    }));
    assert!(
        unrelated
            .require_ready(&home, &session)
            .unwrap_err()
            .contains("does not affect")
    );
    assert!(clean.require_ready(&home, &session).is_ok());

    let setting = settings
        .setting(&home, beryl_state::SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&home).unwrap(),
                beryl_state::ApplySettings::new(vec![beryl_state::SettingUpdate::new(
                    beryl_state::SettingKey::ActiveThemeId,
                    beryl_state::ExpectedSettingRevision::Exact(setting.revision()),
                    beryl_state::SettingValue::active_theme_id("health-probe").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        home.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(home.home_revision().is_err());
    assert!(clean.require_ready(&home, &session).is_err());
    home.close().unwrap();
}
