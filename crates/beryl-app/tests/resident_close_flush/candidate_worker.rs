use std::{cell::Cell, rc::Rc};

use beryl_app::main_window::{
    MainWindowComposerCandidateCustody as Custody, MainWindowComposerCandidateRead as Read,
    MainWindowComposerCandidateSource as Source, MainWindowComposerCandidateWorker as Worker,
};
use beryl_home_store::test_faults::FaultPoint;
use beryl_state::BerylState;
use gpui::{AppContext, TestAppContext, px};
use gpui_text_input::*;
use syndic_storage::SyndicStorage;

use super::{composer, support::slot_close, widget_support};
use widget_support::fixture::Fixture;

struct View;
impl gpui::Render for View {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

fn prepared(
    cx: &mut TestAppContext,
    store: beryl_home_store::HomeStore,
    facts: beryl_app::main_window::MainWindowComposerRetiredClose,
) -> (
    Worker,
    Custody,
    RangePrepublicationSession,
    RangePrepublicationValidationRequest,
) {
    let position = composer::position(0);
    let seed = RangeRestorationSeed {
        binding: facts.selection().binding().range_binding(),
        history: Some(facts.selection().binding().range_history_frontier()),
        caret: position,
        selection: RangeSourceSelection::caret(position),
        scroll: RangeRestorationScrollAnchor {
            position,
            intra_anchor: px(0.),
        },
    };
    let mut candidate = store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let source = Source::new(&mut candidate, facts, storage, &state, seed)
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    let mut prepared = None;
    cx.add_window_view(|window, _| {
        let config = widget_support::widget_config(
            source.seed().binding,
            source.selection().binding().presentation_generation(),
        );
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        let environment =
            RangePrepublicationEnvironment::new(1, config, window.text_system(), cleanup).unwrap();
        let mut session = RangePrepublicationSession::new(source.seed(), environment).unwrap();
        let request = session
            .service(window.text_system())
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                RangePrepublicationEffect::ValidateOwner(request) => Some(request),
                _ => None,
            })
            .unwrap();
        prepared = Some((session, request));
        View
    });
    let (session, request) = prepared.unwrap();
    let (worker, custody) = Worker::new(candidate, source, session.generation());
    (worker, custody, session, request)
}

fn finish(custody: &Custody, cx: &mut TestAppContext) {
    cx.run_until_parked();
    assert!(!custody.pending());
}

#[gpui::test]
fn candidate_worker_returns_resources_before_notification_and_bounds_results(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("candidate-worker-read", 181);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (worker, custody, _session, request) = prepared(cx, fixture.store, facts);
    let worker = Rc::new(std::cell::RefCell::new(worker));
    let custody = Rc::new(std::cell::RefCell::new(custody));
    let weak = Rc::downgrade(&custody);
    let notified = Rc::new(Cell::new(false));
    let notification = notified.clone();
    cx.update(|app| {
        worker
            .borrow_mut()
            .start(
                RangePrepublicationEffect::ValidateOwner(request),
                app,
                move |_| {
                    let owner = weak.upgrade().unwrap();
                    assert!(!owner.borrow().pending());
                    notification.set(true);
                },
            )
            .unwrap()
    });
    assert!(custody.borrow().pending());
    assert!(custody.borrow_mut().take_resources().is_none());
    assert!(
        cx.update(|app| worker.borrow_mut().start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    finish(&custody.borrow(), cx);
    assert!(notified.get());
    assert!(custody.borrow_mut().take_resources().is_none());
    assert!(
        cx.update(|app| worker.borrow_mut().start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    let completion = custody.borrow_mut().take_completion().unwrap();
    let Read::Validation(response) = completion.result.unwrap() else {
        panic!("validation expected")
    };
    assert_eq!(response.key, request.key);
    assert!(response.current);
    let page = PageRequest::new(
        PageRequestKey::adjacent(
            PageRequestId::new(21),
            request.binding.binding(),
            request.binding.revision(),
            PagePurpose::Viewport,
            ByteOffset::new(0),
            PageDirection::Forward,
            4,
        )
        .unwrap(),
    );
    cx.update(|app| {
        worker
            .borrow_mut()
            .start(
                RangePrepublicationEffect::Page {
                    cleanup: request.cleanup,
                    generation: request.key.generation,
                    request: page,
                },
                app,
                |_| {},
            )
            .unwrap()
    });
    finish(&custody.borrow(), cx);
    let Read::Page(response) = custody
        .borrow_mut()
        .take_completion()
        .unwrap()
        .result
        .unwrap()
    else {
        panic!("page expected")
    };
    assert_eq!(response.key(), page.key());
    assert!(response.retained_bytes() <= 4);
    let objects = ObjectRequest::new(
        ObjectRequestKey::new(
            ObjectRequestId::new(22),
            request.binding.binding(),
            request.binding.revision(),
            PresentationGeneration::new(1),
            ObjectPurpose::Viewport,
            ObjectDemandEnvelope::anchor(
                ByteOffset::new(0),
                None,
                ObjectDirection::Forward,
                1,
                4096,
            )
            .unwrap(),
        )
        .unwrap(),
    );
    cx.update(|app| {
        worker
            .borrow_mut()
            .start(
                RangePrepublicationEffect::ObjectPage {
                    cleanup: request.cleanup,
                    generation: request.key.generation,
                    request: objects,
                },
                app,
                |_| {},
            )
            .unwrap()
    });
    finish(&custody.borrow(), cx);
    let Read::ObjectPage(response) = custody
        .borrow_mut()
        .take_completion()
        .unwrap()
        .result
        .unwrap()
    else {
        panic!("object page expected")
    };
    assert_eq!(response.key(), objects.key());
    assert!(response.objects().is_empty());
    let (candidate, source) = custody.borrow_mut().take_resources().unwrap();
    assert!(custody.borrow_mut().take_resources().is_none());
    assert!(candidate.service_reference().home_revision().is_err());
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn cancelled_candidate_worker_retains_custody_until_read_returns(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-worker-cancel", 182);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, _session, request) = prepared(cx, fixture.store, facts);
    let (release, wait) = futures_channel::oneshot::channel::<()>();
    cx.update(|app| {
        worker
            .test_start_with(
                RangePrepublicationEffect::ValidateOwner(request),
                app,
                |_| {},
                async move {
                    wait.await.unwrap();
                },
            )
            .unwrap()
    });
    cx.run_until_parked();
    worker.cancel();
    assert!(custody.cancelled());
    assert!(custody.pending());
    assert!(custody.take_resources().is_none());
    assert!(custody.take_completion().is_none());
    assert!(
        cx.update(|app| worker.start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    release.send(()).unwrap();
    finish(&custody, cx);
    assert!(custody.cancelled());
    assert!(custody.take_resources().is_none());
    assert!(custody.take_completion().unwrap().result.is_ok());
    assert!(
        cx.update(|app| worker.start(
            RangePrepublicationEffect::ValidateOwner(request),
            app,
            |_| {}
        ))
        .is_err()
    );
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn abandoned_candidate_worker_lives_until_actual_completion(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-worker-abandon", 183);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, _session, request) = prepared(cx, fixture.store, facts);

    let notified = Rc::new(Cell::new(false));
    let notification = notified.clone();
    let consumer = Rc::new(());
    let weak_consumer = Rc::downgrade(&consumer);
    let (release, wait) = futures_channel::oneshot::channel::<()>();
    cx.update(|app| {
        worker
            .test_start_with(
                RangePrepublicationEffect::ValidateOwner(request),
                app,
                move |_| {
                    if weak_consumer.upgrade().is_some() {
                        notification.set(true);
                    }
                },
                async move {
                    wait.await.unwrap();
                },
            )
            .unwrap()
    });
    drop(worker);
    drop(consumer);
    cx.run_until_parked();
    assert!(custody.pending());
    assert!(custody.cancelled());
    assert!(custody.take_resources().is_none());
    assert!(!notified.get());
    release.send(()).unwrap();
    cx.run_until_parked();
    assert!(!custody.pending());
    assert!(custody.take_completion().unwrap().result.is_ok());
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
    assert!(!notified.get());
}

#[gpui::test]
fn candidate_worker_refuses_other_session_and_returns_read_failure(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-worker-failure", 184);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let (mut worker, mut custody, _session, request) = prepared(cx, fixture.store, facts);
    let other = Fixture::new("candidate-worker-other", 185);
    let facts = slot_close::retired(&other, cx.new(|_| ()).entity_id());
    other.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(other.store.home_revision().is_err());
    let (_other_worker, mut other_custody, _other_session, other_request) =
        prepared(cx, other.store, facts);
    assert!(
        cx.update(|app| worker.start(
            RangePrepublicationEffect::ValidateOwner(other_request),
            app,
            |_| {}
        ))
        .is_err()
    );
    assert!(!custody.pending());
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
    let completion = custody.take_completion().unwrap();
    assert!(
        matches!(completion.effect, RangePrepublicationEffect::ValidateOwner(r) if r == request)
    );
    assert!(completion.result.is_err());
    let (candidate, source) = custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
    let (candidate, source) = other_custody.take_resources().unwrap();
    drop(source);
    candidate.abort().close().unwrap();
}
