use super::{composer, support::slot_close, widget_support};
use beryl_app::{
    composer_host::{
        ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushPurpose,
    },
    main_window::{
        MainWindowComposerCandidateSource as Source, MainWindowComposerRetiredClose,
        MainWindowConversationComposerCloseTicket,
    },
};
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};
use beryl_state::BerylState;
use gpui::{AppContext, TestAppContext, px};
use gpui_text_input::*;
use syndic_storage::{SyndicStorage, SyndicTimestamp};
#[path = "../syndic_composer_publication/support.rs"]
mod publication;
use widget_support::fixture::Fixture;

fn seed(
    facts: &MainWindowComposerRetiredClose,
    caret: SourcePosition,
    anchor: SourcePosition,
) -> RangeRestorationSeed {
    RangeRestorationSeed {
        binding: facts.selection().binding().range_binding(),
        history: Some(facts.selection().binding().range_history_frontier()),
        caret,
        selection: RangeSourceSelection {
            anchor,
            head: caret,
        },
        scroll: RangeRestorationScrollAnchor {
            position: caret,
            intra_anchor: px(3.5),
        },
    }
}

fn page(
    binding: RangeBinding,
    id: u64,
    anchor: u64,
    direction: PageDirection,
    bytes: u64,
) -> PageRequest {
    PageRequest::new(
        PageRequestKey::adjacent(
            PageRequestId::new(id),
            binding.binding(),
            binding.revision(),
            PagePurpose::Viewport,
            ByteOffset::new(anchor),
            direction,
            bytes,
        )
        .unwrap(),
    )
}

fn objects(
    binding: RangeBinding,
    presentation: u64,
    cursor: Option<ObjectCursor>,
    limit: usize,
) -> ObjectRequest {
    ObjectRequest::new(
        ObjectRequestKey::new(
            ObjectRequestId::new(30),
            binding.binding(),
            binding.revision(),
            PresentationGeneration::new(presentation),
            ObjectPurpose::Viewport,
            ObjectDemandEnvelope::anchor(
                ByteOffset::new(0),
                cursor,
                ObjectDirection::Forward,
                limit,
                4096,
            )
            .unwrap(),
        )
        .unwrap(),
    )
}

fn edited_retirement(
    fixture: &Fixture,
    cx: &mut TestAppContext,
    markers: bool,
) -> (MainWindowComposerRetiredClose, RangeRestorationSeed) {
    let mut host = fixture.activated_host(fixture.selected_thread, 221, 222, 1);
    let binding = host.binding().unwrap();
    let (caret, anchor) = if markers {
        let asset =
            publication::publish_image_asset(&fixture.store, fixture.assets(), b"candidate marker");
        publication::insert_two_markers_with_readiness(
            &mut host,
            &fixture.store,
            &fixture.storage,
            binding,
            301,
            [asset; 2],
        );
        let before = SourcePosition::new(
            ByteOffset::new(0),
            InlineObjectGap::before(InlineObjectNeighbor::new(
                InlineObjectId::new(0x1001),
                InlineObjectOrder::new(1),
            )),
        );
        let after = SourcePosition::new(
            ByteOffset::new(0),
            InlineObjectGap::after(InlineObjectNeighbor::new(
                InlineObjectId::new(0x1002),
                InlineObjectOrder::new(2),
            )),
        );
        (before, after)
    } else {
        composer::commit_text(
            &mut host,
            &fixture.store,
            binding,
            301,
            0,
            0,
            "é日\ntext",
            10,
            2,
        );
        (composer::position(2), composer::position(10))
    };
    let mut slot = Box::new(
        beryl_app::main_window::MainWindowComposerSlot::new(
            fixture.window_id,
            fixture.claims().0,
            host,
            fixture.storage.clone(),
            beryl_app::main_window::MainWindowComposerMarkerMetadataAuthority::new(
                fixture.assets(),
            ),
        )
        .unwrap(),
    );
    let selection = slot.selected_identity().unwrap();
    let close = MainWindowConversationComposerCloseTicket::for_test(
        cx.new(|_| ()).entity_id(),
        1,
        selection,
    );
    slot.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket, .. } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("close not started")
    };
    let seals = fixture.marker_seals();
    let captured = slot
        .capture_selected_flush_publication(
            &fixture.store,
            selection,
            ticket,
            fixture.assets(),
            &seals,
            composer::operation_id(310),
            markers.then(|| publication::authority(240)),
            SyndicTimestamp::from_unix_millis(5),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(
        matches!(captured, ComposerHostFlushCapture::Captured(_)),
        "{captured:?}"
    );
    let mut settled = false;
    for _ in 0..100 {
        if slot
            .advance_selected_flush(&fixture.store, selection, ticket)
            .unwrap()
            == beryl_app::composer_host::ComposerHostFlushAdvance::Progress(
                beryl_app::composer_host::ComposerHostFlushState::CaptureRequired,
            )
        {
            settled = true;
            break;
        }
    }
    assert!(settled, "fixture publication did not settle");
    slot_close::ready(fixture, &mut slot, ticket);
    let facts = slot.retire_clean_window_close(close, ticket).ok().unwrap();
    let seed = seed(&facts, caret, anchor);
    (facts, seed)
}

#[gpui::test]
fn candidate_source_preserves_directed_seed_and_bounds_exact_pages(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-source-text", 151);
    let (mut facts, old_seed) = edited_retirement(&fixture, cx, false);
    let predecessor = facts.close_ticket();
    let expected_window = BerylState::reacquire(&fixture.store)
        .unwrap()
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
        .windows()
        .iter()
        .find(|window| window.window_id() == fixture.window_id)
        .unwrap()
        .clone();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    for invalid in [
        RangeRestorationSeed {
            history: None,
            ..old_seed
        },
        RangeRestorationSeed {
            caret: composer::position(1),
            selection: RangeSourceSelection::caret(composer::position(1)),
            ..old_seed
        },
        RangeRestorationSeed {
            selection: RangeSourceSelection::caret(composer::position(0)),
            ..old_seed
        },
    ] {
        facts = Source::new(&mut candidate, facts, storage.clone(), &state, invalid)
            .err()
            .unwrap()
            .0;
        assert_eq!(facts.close_ticket(), predecessor);
    }
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    let source = Source::new(&mut candidate, facts, storage, &state, old_seed)
        .unwrap_or_else(|(_, e)| panic!("{e}"));
    assert_eq!(source.predecessor(), predecessor);
    assert_eq!(source.window(), &expected_window);
    assert_ne!(source.close_ticket(), predecessor);
    let fresh = source.seed();
    assert_eq!(fresh.caret, old_seed.caret);
    assert_eq!(fresh.selection, old_seed.selection);
    assert_eq!(fresh.scroll, old_seed.scroll);
    assert_ne!(fresh.binding, old_seed.binding);
    validate_response(&source, &mut candidate, old_seed, cx);
    let ordinary = candidate.service_reference();
    let access = candidate.recovery_access().unwrap();
    let forward = page(fresh.binding, 10, 0, PageDirection::Forward, 4);
    let result = source.text_page(&access, forward).unwrap();
    assert_eq!(result.key(), forward.key());
    assert_eq!(result.text(), "é");
    assert!(result.retained_bytes() <= 4);
    assert_eq!(
        source
            .text_page(
                &access,
                page(fresh.binding, 11, 10, PageDirection::Backward, 4)
            )
            .unwrap()
            .text(),
        "text"
    );
    assert!(
        source
            .text_page(
                &access,
                page(old_seed.binding, 12, 0, PageDirection::Forward, 4)
            )
            .is_err()
    );
    assert!(
        source
            .text_page(
                &access,
                page(fresh.binding, 13, 1, PageDirection::Forward, 4)
            )
            .is_err()
    );
    assert!(
        source
            .text_page(
                &access,
                page(fresh.binding, 14, 0, PageDirection::Forward, u64::MAX)
            )
            .is_err()
    );
    assert!(
        source
            .object_page(&access, objects(fresh.binding, 999, None, 2))
            .is_err()
    );
    assert!(
        source
            .object_page(&access, objects(fresh.binding, 1, None, 2))
            .unwrap()
            .objects()
            .is_empty()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    assert!(ordinary.home_revision().is_err());
    drop(access);
    drop(ordinary);
    let store = candidate.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    drop(source);
    store.close().unwrap();
}

struct ValidationView;

impl gpui::Render for ValidationView {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

fn validate_response(
    source: &Source,
    candidate: &mut beryl_home_store::HomeRecoveryCandidate,
    predecessor: RangeRestorationSeed,
    cx: &mut TestAppContext,
) {
    cx.add_window_view(|window, _| {
        let config = widget_support::widget_config(
            source.seed().binding,
            source.selection().binding().presentation_generation(),
        );
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        let environment =
            RangePrepublicationEnvironment::new(1, config, window.text_system(), cleanup).unwrap();
        let mut session = RangePrepublicationSession::new(source.seed(), environment).unwrap();
        let step = session.service(window.text_system());
        let request = step
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                RangePrepublicationEffect::ValidateOwner(request) => Some(request),
                _ => None,
            })
            .expect("owner validation is the first preparation effect");
        let access = candidate.recovery_access().unwrap();
        let response = source.validate(&access, request).unwrap();
        assert_eq!(response.key, request.key);
        assert_eq!(response.binding, source.seed().binding);
        assert_eq!(response.history, source.seed().history);
        assert!(response.current);
        assert!(
            source
                .validate(
                    &access,
                    RangePrepublicationValidationRequest {
                        history: None,
                        ..request
                    }
                )
                .is_err()
        );
        assert!(
            source
                .validate(
                    &access,
                    RangePrepublicationValidationRequest {
                        binding: predecessor.binding,
                        ..request
                    }
                )
                .is_err()
        );
        session.cancel();
        ValidationView
    });
}

#[gpui::test]
fn candidate_source_preserves_inline_gaps_and_marker_continuation(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-source-markers", 161);
    let (facts, old_seed) = edited_retirement(&fixture, cx, true);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let source = Source::new(&mut candidate, facts, storage, &state, old_seed)
        .unwrap_or_else(|(_, e)| panic!("{e}"));
    assert_eq!(source.seed().caret, old_seed.caret);
    assert_eq!(source.seed().selection, old_seed.selection);
    assert_eq!(source.seed().scroll, old_seed.scroll);
    let access = candidate.recovery_access().unwrap();
    let request = objects(source.seed().binding, 1, None, 1);
    let first = source.object_page(&access, request).unwrap();
    assert_eq!(first.key(), request.key());
    assert_eq!(first.objects().len(), 1);
    assert_eq!(first.objects()[0].order(), InlineObjectOrder::new(1));
    assert_eq!(first.objects()[0].presentation().display(), "[A]");
    let second = source
        .object_page(
            &access,
            objects(source.seed().binding, 1, first.continuation(), 2),
        )
        .unwrap();
    assert_eq!(second.objects().len(), 1);
    assert_eq!(second.objects()[0].order(), InlineObjectOrder::new(2));
    assert!(second.continuation().is_none());
    drop(access);
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        source
            .text_page(
                &candidate.recovery_access().unwrap(),
                page(source.seed().binding, 40, 0, PageDirection::Forward, 4)
            )
            .is_err()
    );
    assert!(candidate.recovery_access().is_err());
    drop(source);
    candidate.abort().close().unwrap();
}

#[gpui::test]
fn candidate_source_rejects_foreign_access_and_stale_handles(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-source-stale", 171);
    let foreign = Fixture::new("candidate-source-foreign", 172);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    let old_seed = seed(&facts, composer::position(0), composer::position(0));
    let old_state = BerylState::reacquire(&fixture.store).unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let facts = Source::new(
        &mut candidate,
        facts,
        fixture.storage.clone(),
        &state,
        old_seed,
    )
    .err()
    .unwrap()
    .0;
    let facts = Source::new(&mut candidate, facts, storage.clone(), &old_state, old_seed)
        .err()
        .unwrap()
        .0;
    let source = Source::new(&mut candidate, facts, storage, &state, old_seed)
        .unwrap_or_else(|(_, e)| panic!("{e}"));
    foreign.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(foreign.store.home_revision().is_err());
    let mut other = foreign.store.recover_same_home().unwrap();
    let request = page(source.seed().binding, 41, 0, PageDirection::Forward, 4);
    assert!(
        source
            .text_page(&other.recovery_access().unwrap(), request)
            .is_err()
    );
    let store = candidate.publish().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut newer = store.recover_same_home().unwrap();
    assert!(
        source
            .text_page(&newer.recovery_access().unwrap(), request)
            .is_err()
    );
    drop(source);
    newer.abort().close().unwrap();
    other.abort().close().unwrap();
}

#[path = "candidate_window.rs"]
mod window;
