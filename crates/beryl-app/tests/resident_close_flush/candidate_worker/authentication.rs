use super::*;
use beryl_app::main_window::MainWindowComposerRetiredClose;
use std::cell::RefCell;

fn seed(facts: &MainWindowComposerRetiredClose) -> RangeRestorationSeed {
    let position = composer::position(0);
    RangeRestorationSeed {
        binding: facts.selection().binding().range_binding(),
        history: Some(facts.selection().binding().range_history_frontier()),
        caret: position,
        selection: RangeSourceSelection::caret(position),
        scroll: RangeRestorationScrollAnchor {
            position,
            intra_anchor: px(0.),
        },
    }
}

fn session(
    cx: &mut TestAppContext,
    seed: RangeRestorationSeed,
    presentation: std::num::NonZeroU64,
) -> (
    RangePrepublicationSession,
    RangePrepublicationValidationRequest,
) {
    let mut result = None;
    cx.add_window_view(|window, _| {
        let config = widget_support::widget_config(seed.binding, presentation);
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        let environment =
            RangePrepublicationEnvironment::new(1, config, window.text_system(), cleanup).unwrap();
        let mut session = RangePrepublicationSession::new(seed, environment).unwrap();
        let request = session
            .service(window.text_system())
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                RangePrepublicationEffect::ValidateOwner(request) => Some(request),
                _ => None,
            })
            .unwrap();
        result = Some((session, request));
        View
    });
    result.unwrap()
}

#[gpui::test]
fn authentication_returns_source_before_notification_and_binds_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-authenticate", 186);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    let predecessor = facts.close_ticket();
    let seed = seed(&facts);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let receiver = Rc::new(RefCell::new(None::<Custody>));
    let weak = Rc::downgrade(&receiver);
    let notified = Rc::new(Cell::new(false));
    let notification = notified.clone();
    let (mut worker, custody) = cx.update(|app| {
        Worker::prepare(candidate, facts, storage, state, seed, app, move |_| {
            let receiver = weak.upgrade().unwrap();
            let receiver = receiver.borrow();
            let custody = receiver.as_ref().unwrap();
            assert!(!custody.pending());
            assert!(custody.source().is_some());
            notification.set(true);
        })
    });
    assert!(custody.pending());
    assert!(custody.source().is_none());
    *receiver.borrow_mut() = Some(custody);
    cx.run_until_parked();
    assert!(notified.get());
    let mut custody = receiver.borrow_mut().take().unwrap();
    let source = custody.source().unwrap();
    assert_eq!(source.predecessor(), predecessor);
    assert_eq!(source.seed().selection, seed.selection);
    assert_eq!(source.seed().scroll, seed.scroll);
    let (session, request) = session(
        cx,
        source.seed(),
        source.selection().binding().presentation_generation(),
    );
    drop(source);
    assert!(
        cx.update(|app| worker.start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    worker.bind_generation(session.generation()).unwrap();
    assert!(worker.bind_generation(session.generation()).is_err());
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
    assert!(custody.take_completion().unwrap().result.is_ok());
    let (candidate, source) = custody.take_resources().unwrap();
    assert!(worker.bind_generation(session.generation()).is_err());
    assert!(custody.take_resources().is_none());
    assert!(candidate.service_reference().home_revision().is_err());
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn authentication_failure_returns_original_retirement_custody(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-authentication-refused", 187);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    let predecessor = facts.close_ticket();
    let selection = facts.selection();
    let seed = seed(&facts);
    let (session, _) = session(cx, seed, selection.binding().presentation_generation());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let (mut worker, mut custody) =
        cx.update(|app| Worker::prepare(candidate, facts, storage, state, seed, app, |_| {}));
    assert!(worker.bind_generation(session.generation()).is_err());
    assert!(custody.take_refused_resources().is_none());
    finish(&custody, cx);
    assert!(custody.source().is_none());
    assert!(custody.take_resources().is_none());
    assert!(worker.bind_generation(session.generation()).is_err());
    let error = custody.preparation_error().unwrap();
    let (candidate, facts, returned_error) = custody.take_refused_resources().unwrap();
    assert_eq!(returned_error, error);
    assert_eq!(facts.close_ticket(), predecessor);
    assert_eq!(facts.selection(), selection);
    assert_eq!(facts.host().binding(), selection.binding());
    assert!(custody.take_refused_resources().is_none());
    assert!(custody.preparation_error().is_none());
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn authentication_cancellation_and_abandoned_delivery_retain_resources(cx: &mut TestAppContext) {
    for abandon in [false, true] {
        let fixture = Fixture::new(
            "candidate-authentication-cancel",
            if abandon { 189 } else { 188 },
        );
        let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
        let seed = seed(&facts);
        let (session, _) = session(
            cx,
            seed,
            facts.selection().binding().presentation_generation(),
        );
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let candidate = fixture.store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let consumer = Rc::new(());
        let weak = Rc::downgrade(&consumer);
        let notified = Rc::new(Cell::new(false));
        let notification = notified.clone();
        let (release, wait) = futures_channel::oneshot::channel::<()>();
        let (worker, mut custody) = cx.update(|app| {
            Worker::test_prepare_with(
                candidate,
                facts,
                storage,
                state,
                seed,
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
        });
        let mut worker = Some(worker);
        cx.run_until_parked();
        assert!(
            worker
                .as_mut()
                .unwrap()
                .bind_generation(session.generation())
                .is_err()
        );
        if abandon {
            drop(worker.take());
            drop(consumer);
        } else {
            worker.as_mut().unwrap().cancel();
        }
        assert!(custody.pending());
        assert!(custody.cancelled());
        assert!(custody.take_resources().is_none());
        assert!(custody.take_refused_resources().is_none());
        assert!(!notified.get());
        release.send(()).unwrap();
        finish(&custody, cx);
        assert!(custody.cancelled());
        assert_eq!(notified.get(), !abandon);
        if let Some(worker) = worker.as_mut() {
            assert!(worker.bind_generation(session.generation()).is_err());
        }
        let (candidate, source) = custody.take_resources().unwrap();
        drop(source);
        candidate.abort().close().unwrap();
    }
}
