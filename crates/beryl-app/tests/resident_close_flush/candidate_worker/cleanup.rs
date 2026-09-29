use super::*;

fn drain(custody: &Custody, environment: &RangePrepublicationEnvironment) {
    for _ in 0..64 {
        custody.drive_cleanup(2);
        if custody.cleanup_drained() {
            break;
        }
    }
    assert!(custody.cleanup_drained());
    let ownership = environment.cleanup().ownership();
    assert_eq!(
        (
            ownership.active,
            ownership.ready,
            ownership.awaiting_acknowledgement
        ),
        (0, 0, 0)
    );
}

#[gpui::test]
fn candidate_reads_realize_and_release_real_session_effects(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-ledger-realization", 190);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, mut session, request, environment, text_system) =
        prepared(cx, fixture.store, facts);
    let mut effects = vec![RangePrepublicationEffect::ValidateOwner(request)];
    let mut counts = [0usize; 3];
    for _ in 0..256 {
        for effect in effects.drain(..) {
            counts[match effect {
                RangePrepublicationEffect::ValidateOwner(_) => 0,
                RangePrepublicationEffect::Page { .. } => 1,
                RangePrepublicationEffect::ObjectPage { .. } => 2,
            }] += 1;
            cx.update(|app| worker.start(effect, app, |_| {}).unwrap());
            finish(&custody, cx);
            let mut foreign = RangePrepublicationSession::new(
                custody.source().unwrap().seed(),
                environment.clone(),
            )
            .unwrap();
            assert!(custody.deliver_completion(&mut foreign).is_err());
            assert!(custody.completion().is_some());
            drop(foreign);
            assert_eq!(
                custody.deliver_completion(&mut session).unwrap(),
                Some(RangePrepublicationDelivery::Accepted)
            );
            custody.drive_cleanup(2);
        }
        let step = session.service(&text_system);
        effects = step.effects;
        if step.status == RangePrepublicationStatus::Ready {
            break;
        }
        assert!(!matches!(
            step.status,
            RangePrepublicationStatus::Failed(_)
                | RangePrepublicationStatus::Stale
                | RangePrepublicationStatus::Cancelled
        ));
    }
    assert_eq!(session.status(), RangePrepublicationStatus::Ready);
    assert!(counts.into_iter().all(|count| count > 0));
    assert!(effects.is_empty());
    let candidate = session.take_candidate().unwrap();
    assert!(!custody.cleanup_drained());
    drop(candidate);
    drop(session);
    worker.cancel();
    drain(&custody, &environment);
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn candidate_cleanup_waits_for_return_and_survives_abandoned_notification(cx: &mut TestAppContext) {
    for abandon in [false, true] {
        let fixture = Fixture::new("candidate-ledger-held", if abandon { 191 } else { 192 });
        let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let (mut worker, mut custody, mut session, request, environment, text_system) =
            prepared(cx, fixture.store, facts);
        cx.update(|app| {
            worker
                .start(
                    RangePrepublicationEffect::ValidateOwner(request),
                    app,
                    |_| {},
                )
                .unwrap()
        });
        finish(&custody, cx);
        assert_eq!(
            custody.deliver_completion(&mut session).unwrap(),
            Some(RangePrepublicationDelivery::Accepted)
        );
        let mut effects = session.service(&text_system).effects;
        let effect = effects.remove(0);
        assert!(matches!(
            &effect,
            RangePrepublicationEffect::Page { .. } | RangePrepublicationEffect::ObjectPage { .. }
        ));
        let consumer = Rc::new(());
        let weak = Rc::downgrade(&consumer);
        let notified = Rc::new(Cell::new(false));
        let notification = notified.clone();
        let (release, wait) = futures_channel::oneshot::channel::<()>();
        cx.update(|app| {
            worker
                .test_start_with(
                    effect,
                    app,
                    move |_| {
                        if weak.upgrade().is_some() {
                            notification.set(true);
                        }
                    },
                    async move {
                        wait.await.unwrap();
                    },
                )
                .unwrap()
        });
        cx.run_until_parked();
        drop(session);
        if abandon {
            drop(worker);
            drop(consumer);
        } else {
            worker.cancel();
        }
        for effect in effects {
            custody.settle_undispatched(effect).unwrap();
        }
        for _ in 0..4 {
            custody.drive_cleanup(2);
        }
        assert!(custody.pending());
        assert!(!custody.cleanup_drained());
        assert!(environment.cleanup().ownership().awaiting_acknowledgement > 0);
        assert!(custody.take_resources().is_none());
        release.send(()).unwrap();
        finish(&custody, cx);
        assert_eq!(notified.get(), !abandon);
        assert!(custody.completion().is_none());
        drain(&custody, &environment);
        let (candidate, source) = custody.take_resources().unwrap();
        drop(source);
        candidate.abort().close().unwrap();
    }
}

#[gpui::test]
fn candidate_cleanup_preserves_unconsumed_payload_and_settles_unsent_effects(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("candidate-ledger-unconsumed", 193);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, mut session, request, environment, text_system) =
        prepared(cx, fixture.store, facts);
    cx.update(|app| {
        worker
            .start(
                RangePrepublicationEffect::ValidateOwner(request),
                app,
                |_| {},
            )
            .unwrap()
    });
    finish(&custody, cx);
    // Reusing the exact token remains refused even after worker return.
    assert!(
        cx.update(|app| worker.start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    assert_eq!(
        custody.deliver_completion(&mut session).unwrap(),
        Some(RangePrepublicationDelivery::Accepted)
    );
    let mut effects = session.service(&text_system).effects;
    assert!(!effects.is_empty());
    let effect = effects.remove(0);
    cx.update(|app| worker.start(effect, app, |_| {}).unwrap());
    finish(&custody, cx);
    drop(session);
    let before = environment.cleanup().ownership();
    let completion = custody.completion().unwrap();
    assert!(matches!(
        &completion.result,
        Ok(Read::Page(_)) | Ok(Read::ObjectPage(_))
    ));
    custody.drive_cleanup(64);
    assert_eq!(environment.cleanup().ownership(), before);
    drop(completion);
    assert!(custody.take_resources().is_none());
    worker.cancel();
    assert!(custody.completion().is_none());
    assert!(
        custody
            .settle_undispatched(RangePrepublicationEffect::ValidateOwner(request))
            .is_err()
    );
    for effect in effects {
        custody.settle_undispatched(effect).unwrap();
    }
    drain(&custody, &environment);
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn candidate_read_failure_releases_its_exact_ledger_obligation(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-ledger-failure", 194);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, mut session, request, environment, _) =
        prepared(cx, fixture.store, facts);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    cx.update(|app| {
        worker
            .start(
                RangePrepublicationEffect::ValidateOwner(request),
                app,
                |_| {},
            )
            .unwrap()
    });
    finish(&custody, cx);
    assert!(custody.completion().unwrap().result.is_err());
    assert!(custody.deliver_completion(&mut session).is_err());
    assert!(custody.completion().is_none());
    drop(session);
    worker.cancel();
    drain(&custody, &environment);
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn abandoned_worker_leaves_unsent_effect_cleanup_with_recovery_custody(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-ledger-unsent", 195);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (worker, mut custody, session, request, environment, _) =
        prepared(cx, fixture.store, facts);
    let effect = RangePrepublicationEffect::ValidateOwner(request);
    let (effect, _) = custody.settle_undispatched(effect).unwrap_err();
    drop(worker);
    drop(session);
    custody.drive_cleanup(2);
    assert!(!custody.cleanup_drained());
    custody.settle_undispatched(effect).unwrap();
    drain(&custody, &environment);
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}
