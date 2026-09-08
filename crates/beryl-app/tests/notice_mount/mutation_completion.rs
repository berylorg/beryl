use super::composer_feedback::{asset, composer, configured_mount, notice, prepare_editor};
use super::*;
use beryl_app::composer_host::{ComposerHostError, ComposerHostImageMarkerMetadata};
use beryl_home_store::test_faults::FaultPoint;
use gpui_text_input::{InlineObjectId, InlineObjectOrder};
use syndic_storage::{
    DraftMarkerAdmissionLimitsV1, DraftPieceReconciledCommandV1, DraftPieceTransactionOutcomeV1,
    StagedDraftPieceOutcomeStateV1 as State,
};

#[derive(Clone, Copy, Debug)]
struct BuildState {
    state: Option<State>,
    committed: bool,
    pending: bool,
    cleanup_failure: bool,
    local_failure: bool,
}

fn build_state(service: &MainWindowConversationComposerService) -> Option<BuildState> {
    service.test_with_selected_host(|host| {
        host.mutation_build_diagnostics()
            .map(|diagnostics| BuildState {
                state: diagnostics.state,
                committed: matches!(
                    diagnostics.result,
                    Some(DraftPieceReconciledCommandV1::Terminal(
                        DraftPieceTransactionOutcomeV1::Committed(_)
                    ))
                ),
                pending: matches!(
                    diagnostics.result,
                    Some(DraftPieceReconciledCommandV1::Pending(_))
                ),
                cleanup_failure: diagnostics.cleanup_failure.is_some(),
                local_failure: diagnostics.local_failure.is_some(),
            })
    })
}

fn unavailable_completion(cx: &mut gpui::TestAppContext, committed: bool, retired: bool) {
    let (mounted, service) = configured_mount(
        cx,
        if committed { 160 } else { 170 },
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |host| host.test_set_mutation_transition_limit(1),
    );
    let composer = composer(&mounted, cx);
    prepare_editor(&mounted, &composer, cx);
    let asset = asset(&mounted, cx);
    let selection = service.selected_identity().unwrap();
    let mut gate = service.test_block_next_selected_dispatch();
    let key = mounted
        .window
        .update(cx, |_, _, app| {
            composer.update(app, |composer, cx| {
                composer
                    .insert_authenticated_image_marker(
                        ComposerHostImageMarkerMetadata::new(InlineObjectId::new(1), asset),
                        InlineObjectOrder::new(1),
                        cx,
                    )
                    .unwrap()
            })
        })
        .unwrap();
    support::drive_until(cx, |_| gate.is_blocked());
    let mut armed = false;
    let mut reached_failure = false;
    for _ in 0..256 {
        let state = build_state(&service);
        if armed
            && state.is_some_and(|state| {
                if committed {
                    state.state == Some(State::CleanupPending)
                        && state.committed
                        && state.cleanup_failure
                } else {
                    state.state == Some(State::Finalizing) && state.pending && state.local_failure
                }
            })
        {
            reached_failure = true;
            break;
        }
        if !armed
            && state.is_some_and(|state| {
                if committed {
                    state.state == Some(State::CleanupPending) && state.committed
                } else {
                    state.state.is_none() && state.pending
                }
            })
        {
            let faults = mounted.fixture.faults.clone();
            service.test_with_selected_host(|host| {
                host.test_arm_mutation_before_execute_fault(move |_, _| {
                    faults.fail_next(if committed {
                        FaultPoint::BeforeCommit
                    } else {
                        FaultPoint::AfterPersist
                    });
                });
            });
            armed = true;
        }
        let next = service.test_block_next_selected_dispatch();
        gate.release();
        gate = next;
        support::drive_until(cx, |_| gate.is_blocked());
        assert!(composer.read_with(cx, |composer, _| composer.last_error().is_none()));
    }
    assert!(
        reached_failure,
        "did not reach the intended build failure: {:?}",
        build_state(&service)
    );
    assert_eq!(service.selected_identity(), Some(selection));
    assert!(composer.read_with(cx, |composer, _| composer.mutation_feedback().is_none()));
    service.test_fail_next_mutation_dispatch(if committed {
        ComposerHostError::MutationCommittedUnavailable
    } else {
        ComposerHostError::MutationAdmittedWorkUnavailable
    });
    if retired {
        mounted
            .window
            .update(cx, |root, window, cx| root.retire_notices(window, cx))
            .unwrap();
    }
    gate.release();
    support::drive_until(cx, |cx| {
        composer.read_with(cx, |composer, _| composer.mutation_feedback().is_some())
    });
    let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
    assert_eq!(feedback.selection, selection);
    assert_eq!(feedback.key, key);
    assert_eq!(
        feedback.kind,
        if committed {
            MainWindowComposerMutationFeedbackKind::CommittedUnavailable
        } else {
            MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
        }
    );
    assert_eq!(service.selected_identity(), Some(selection));
    service.test_with_selected_host(|host| {
        assert_eq!(host.binding(), Some(selection.binding()));
        assert_eq!(host.settlement_custody_in_use(), 1);
    });
    support::draw(cx);
    if retired {
        assert!(projection(mounted.window, cx).is_none());
    } else {
        let (token, content) = notice(&mounted, cx);
        assert_eq!(content.dismissal, NoticeDismissal::Persistent);
        assert_eq!(content.commands().count(), 0);
        assert!(content.detail().as_str().contains(if committed {
            "request committed"
        } else {
            "edit outcome is unavailable"
        }));
        let ingress = support::ingress(mounted.window, cx);
        assert!(matches!(
            cx.update(
                |app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token.clone()), app)
            ),
            Err(MainWindowNoticeRouteRejection::Notice(
                NoticeRejection::Persistent
            ))
        ));
        for _ in 0..3 {
            support::draw(cx);
        }
        assert_eq!(notice(&mounted, cx).0, token);
    }
    drop((composer, service));
    support::finish(mounted, cx);
}

#[gpui::test]
fn intermediate_build_unavailable_keeps_persistent_exact_feedback(cx: &mut gpui::TestAppContext) {
    unavailable_completion(cx, false, false);
}

#[gpui::test]
fn committed_cleanup_unavailable_keeps_persistent_committed_feedback(
    cx: &mut gpui::TestAppContext,
) {
    unavailable_completion(cx, true, false);
}

#[gpui::test]
fn late_committed_cleanup_feedback_cannot_reenter_retired_notice_owner(
    cx: &mut gpui::TestAppContext,
) {
    unavailable_completion(cx, true, true);
}

#[gpui::test]
fn committed_successor_read_failure_keeps_exact_persistent_feedback(cx: &mut gpui::TestAppContext) {
    let (mounted, service) = configured_mount(
        cx,
        180,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |_| {},
    );
    let composer = composer(&mounted, cx);
    prepare_editor(&mounted, &composer, cx);
    let asset = asset(&mounted, cx);
    let selection = service.selected_identity().unwrap();
    let faults = mounted.fixture.faults.clone();
    service.test_arm_successor_proof_fault(move || {
        faults.fail_next(FaultPoint::BeforeReadConfirmation)
    });
    let key = mounted
        .window
        .update(cx, |_, _, app| {
            composer.update(app, |composer, cx| {
                composer
                    .insert_authenticated_image_marker(
                        ComposerHostImageMarkerMetadata::new(InlineObjectId::new(1), asset),
                        InlineObjectOrder::new(1),
                        cx,
                    )
                    .unwrap()
            })
        })
        .unwrap();
    support::drive_until(cx, |cx| {
        composer.read_with(cx, |composer, _| composer.mutation_feedback().is_some())
    });
    let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
    assert_eq!(feedback.selection, selection);
    assert_eq!(feedback.key, key);
    assert_eq!(
        feedback.kind,
        MainWindowComposerMutationFeedbackKind::CommittedUnavailable
    );
    let successor = service.selected_identity().unwrap();
    assert_ne!(successor, selection);
    service.test_with_selected_host(|host| {
        assert_eq!(host.binding(), Some(successor.binding()));
        assert_eq!(host.binding().unwrap().root().summary().marker_count(), 1);
        assert_eq!(host.settlement_custody_in_use(), 0);
    });
    assert!(composer.read_with(cx, |composer, _| {
        composer
            .last_error()
            .unwrap()
            .contains("committed composer presentation failed")
    }));
    support::draw(cx);
    let (token, content) = notice(&mounted, cx);
    assert_eq!(content.dismissal, NoticeDismissal::Persistent);
    assert_eq!(content.commands().count(), 0);
    assert!(content.detail().as_str().contains("request committed"));
    for _ in 0..4 {
        support::draw(cx);
    }
    assert_eq!(notice(&mounted, cx).0, token);
    assert_eq!(service.selected_identity(), Some(successor));
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.mutation_feedback()),
        Some(feedback)
    );
    drop((composer, service));
    support::finish(mounted, cx);
}
