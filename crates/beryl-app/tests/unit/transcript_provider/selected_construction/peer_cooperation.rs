use super::*;

#[test]
fn exact_peer_start_and_advance_are_progress_but_unrelated_home_writes_are_not() {
    for already_started in [false, true] {
        let fixture = Fixture::new();
        history::create_inherited_discussion(&fixture, 2);
        if already_started {
            history::start(&fixture);
        }
        let authority = fixture.service.selected_transcript_construction_authority();
        let progress = authority
            .step_after_observation(child(), &AtomicBool::new(false), || {
                if already_started {
                    history::advance(&fixture);
                } else {
                    history::start(&fixture);
                }
            })
            .unwrap();
        assert!(matches!(
            progress,
            SelectedTranscriptConstructionProgress::PeerProgress
        ));
        assert_eq!(retained_count(&authority), 1);
        construct(&fixture);
        quiescent(&fixture);
    }
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    let result = authority
        .step_after_observation(child(), &AtomicBool::new(false), || {
            fixture.seed_head(17);
        })
        .unwrap();
    assert!(matches!(
        result,
        SelectedTranscriptConstructionProgress::Current
    ));
    authority.inspect_retained(|outcomes, terminal| {
        assert!(terminal.is_none());
        assert!(outcomes.iter().all(|outcome| matches!(
            outcome,
            crate::cas_projection::RetainedSelectedTranscriptOutcome::Command(
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            )
        )));
    });
    quiescent(&fixture);
}

#[test]
fn cas_terminal_transcript_owner_yields_exact_start_and_advance_conflicts_to_selected_construction()
{
    for already_started in [false, true] {
        let fixture = Fixture::new();
        history::create_inherited_discussion(&fixture, 2);
        if already_started {
            history::start(&fixture);
        }
        let authority = fixture.service.selected_transcript_construction_authority();
        let before = fixture.home.home_revision().unwrap();
        let complete = crate::cas_projection::step_selected_transcript_after_observation_for_test(
            &fixture.home,
            &fixture.storage,
            child(),
            || {
                assert!(matches!(
                    authority.step(child(), &AtomicBool::new(false)).unwrap(),
                    SelectedTranscriptConstructionProgress::Current
                ));
            },
        )
        .unwrap();
        assert!(!complete);
        assert_eq!(head(&fixture).lifecycle(), ProjectionLifecycle::Current);
        let after_peer = fixture.home.home_revision().unwrap();
        assert!(after_peer > before);
        crate::cas_projection::converge_selected_transcript_after_observation_for_test(
            &fixture.home,
            &fixture.storage,
            child(),
            || panic!("a coherent current transcript needs no canonical command"),
        )
        .unwrap();
        assert_eq!(fixture.home.home_revision().unwrap(), after_peer);
        assert_eq!(head(&fixture).entry_count(), 4);
        quiescent(&fixture);
    }
}

#[test]
fn selected_construction_yields_canonical_conflict_to_the_actual_cas_transcript_owner() {
    for already_started in [false, true] {
        let fixture = Fixture::new();
        history::create_inherited_discussion(&fixture, 2);
        if already_started {
            history::start(&fixture);
        }
        let authority = fixture.service.selected_transcript_construction_authority();
        assert!(matches!(
            authority
                .step_after_observation(child(), &AtomicBool::new(false), || {
                    crate::cas_projection::converge_selected_transcript_after_observation_for_test(
                        &fixture.home,
                        &fixture.storage,
                        child(),
                        || (),
                    )
                    .unwrap();
                })
                .unwrap(),
            SelectedTranscriptConstructionProgress::PeerProgress
        ));
        authority.inspect_retained(|outcomes, terminal| {
            assert!(terminal.is_none());
            assert!(matches!(
                outcomes,
                [RetainedSelectedTranscriptOutcome::Command(
                    CommandOutcome::NotCommitted { .. }
                )]
            ));
        });
        let revision = fixture.home.home_revision().unwrap();
        assert!(matches!(
            authority.step(child(), &AtomicBool::new(false)).unwrap(),
            SelectedTranscriptConstructionProgress::Current
        ));
        assert_eq!(fixture.home.home_revision().unwrap(), revision);
        quiescent(&fixture);
    }
}
