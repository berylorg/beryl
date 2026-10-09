use super::*;
use crate::composer_host::{ComposerHostFlushCapture, ComposerHostFlushTicket};
use crate::main_window::MainWindowConversationComposerCloseTicket;
use crate::main_window::conversation_composer_owner::prepublication::MainWindowNativeLineagePrepublicationDiagnostics;

struct HealthyClose {
    close: MainWindowConversationComposerCloseTicket,
    flush: ComposerHostFlushTicket,
    selected: MainWindowComposerSelectionIdentity,
    before_publication: MainWindowComposerSelectionIdentity,
}

fn prepare_healthy_close(
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
    owner: gpui::EntityId,
) -> HealthyClose {
    let before_publication = service.selected_identity().unwrap();
    let close = MainWindowConversationComposerCloseTicket::for_test(owner, 1, before_publication);
    service.test_begin_window_close_gate(close).unwrap();
    let ComposerHostFlushAdmission::Started { ticket: flush, .. } = service
        .test_begin_window_close_flush(close)
        .unwrap()
        .unwrap()
    else {
        panic!("fresh final close must own its original flush")
    };
    let mut slot = service.slot.lock().unwrap();
    let capture = slot
        .capture_selected_flush_publication(
            &fixture.store,
            before_publication,
            flush,
            fixture.assets(),
            &fixture.marker_seals(),
            support::operation_id(231),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(2_000),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(matches!(capture, ComposerHostFlushCapture::Captured(_)));
    let mut ready = false;
    for _ in 0..32 {
        let selected = slot.selected_identity().unwrap();
        match slot
            .advance_selected_flush(&fixture.store, selected, flush)
            .unwrap()
        {
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CloseReady) => {
                ready = true;
                break;
            }
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired) => {
                let selected = slot.selected_identity().unwrap();
                let revision = fixture.store.home_revision().unwrap();
                assert_eq!(
                    slot.capture_selected_flush_publication(
                        &fixture.store,
                        selected,
                        flush,
                        fixture.assets(),
                        &fixture.marker_seals(),
                        support::operation_id(231),
                        None,
                        syndic_storage::SyndicTimestamp::from_unix_millis(2_000),
                        &CommandCancellation::new(),
                    )
                    .unwrap(),
                    ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
                );
                assert_eq!(fixture.store.home_revision().unwrap(), revision);
                assert_eq!(slot.selected_identity(), Some(selected));
            }
            ComposerHostFlushAdvance::Progress(_) => {}
            other => panic!("original Healthy publication failed: {other:?}"),
        }
    }
    assert!(ready);
    let selected = slot.selected_identity().unwrap();
    assert_ne!(selected, before_publication);
    drop(slot);
    service
        .qualify_final_prepublication_cleanup(close, flush, selected)
        .unwrap();
    HealthyClose {
        close,
        flush,
        selected,
        before_publication,
    }
}

struct NativeSession {
    session: RangePrepublicationSession,
    source: Arc<MainWindowNativeLineagePrepublicationSource>,
    ledger: RangePrepublicationCleanupLedger,
    validation: RangePrepublicationValidationRequest,
}

fn begin_native_session(
    window: &mut gpui::Window,
    selected: MainWindowComposerSelectionIdentity,
    environment_id: u64,
) -> NativeSession {
    let position = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    let seed = RangeRestorationSeed {
        binding: selected.binding().range_binding(),
        history: Some(selected.binding().range_history_frontier()),
        caret: position,
        selection: RangeSourceSelection::caret(position),
        scroll: RangeRestorationScrollAnchor {
            position,
            intra_anchor: gpui::px(0.),
        },
    };
    let config =
        widget_support::widget_config(seed.binding, selected.binding().presentation_generation());
    let ledger = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
    let environment = RangePrepublicationEnvironment::new(
        environment_id,
        config,
        window.text_system(),
        ledger.clone(),
    )
    .unwrap();
    let mut session = RangePrepublicationSession::new(seed, environment).unwrap();
    let source = MainWindowNativeLineagePrepublicationSource::new(
        selected,
        environment_id,
        session.generation(),
        ledger.clone(),
    );
    let effect = session
        .service(window.text_system())
        .effects
        .into_iter()
        .next()
        .expect("authentic session validation");
    let RangePrepublicationEffect::ValidateOwner(validation) = effect else {
        panic!("missing original owner validation")
    };
    source.begin(effect).unwrap();
    NativeSession {
        session,
        source,
        ledger,
        validation,
    }
}

fn validation_response(
    request: RangePrepublicationValidationRequest,
) -> RangePrepublicationValidationResponse {
    RangePrepublicationValidationResponse {
        key: request.key,
        binding: request.binding,
        history: request.history,
        current: true,
    }
}

fn delivered_page(
    window: &mut gpui::Window,
    fixture: &support::Fixture,
    service: &Arc<MainWindowConversationComposerService>,
    selected: MainWindowComposerSelectionIdentity,
    environment_id: u64,
) -> NativeSession {
    let mut native = begin_native_session(window, selected, environment_id);
    native.source.finish_delivery(native.validation.cleanup);
    native
        .session
        .deliver_validation(validation_response(native.validation));
    let mut page = None;
    for _ in 0..32 {
        page = native
            .session
            .service(window.text_system())
            .effects
            .into_iter()
            .find(|effect| matches!(effect, RangePrepublicationEffect::Page { .. }));
        if page.is_some() {
            break;
        }
    }
    let page = page.expect("real published nonempty draft emits Page demand");
    let RangePrepublicationEffect::Page {
        cleanup,
        generation,
        request,
    } = &page
    else {
        unreachable!()
    };
    let (cleanup, generation, request) = (*cleanup, *generation, *request);
    native.source.begin(page).unwrap();
    let response = service
        .slot
        .lock()
        .unwrap()
        .dispatch_selected_request(
            &fixture.store,
            selected,
            RangeTextInputRequest::Page(request),
            Vec::new().into_boxed_slice(),
            &CommandCancellation::new(),
        )
        .unwrap();
    native.source.finish(
        cleanup,
        MainWindowNativeLineagePrepublicationResult::Request(Ok(response)),
    );
    let MainWindowNativeLineagePrepublicationResult::Request(Ok(
        MainWindowComposerDispatchOutcome::Page(response),
    )) = native.source.take(cleanup).unwrap()
    else {
        panic!("actual Page delivery changed")
    };
    native.session.deliver_page(generation, response);
    native.source.release_owner();
    native.source.qualify_cleanup_handoff(selected).unwrap();
    assert!(native.ledger.ownership().active > 0);
    assert_eq!(native.ledger.ownership().ready, 0);
    native
}

fn source_snapshot(
    service: &MainWindowConversationComposerService,
) -> Vec<(
    *const MainWindowNativeLineagePrepublicationSource,
    MainWindowNativeLineagePrepublicationDiagnostics,
)> {
    service
        .native_lineage_sources
        .lock()
        .unwrap()
        .iter()
        .map(|source| {
            assert_eq!(Arc::strong_count(source), 1);
            assert_eq!(Arc::weak_count(source), 0);
            (Arc::as_ptr(source), source.diagnostics())
        })
        .collect()
}

fn drain_cancelled_capsule(
    session: &mut RangePrepublicationSession,
    ledger: &RangePrepublicationCleanupLedger,
    capsule: &mut crate::main_window::conversation_composer_owner::MainWindowRetiredPrepublicationCleanup,
) {
    session.cancel();
    let mut complete = false;
    for _ in 0..8 {
        complete = capsule.advance(64).unwrap();
        if complete {
            break;
        }
    }
    assert!(complete);
    let ownership = ledger.ownership();
    assert_eq!(ownership.active, 0);
    assert_eq!(ownership.ready, 0);
    assert_eq!(ownership.awaiting_acknowledgement, 0);
}

#[gpui::test]
fn healthy_final_handoff_refuses_stale_close_selection_and_failed_home_without_consuming_pages(
    cx: &mut TestAppContext,
) {
    let fixture = support::Fixture::new("healthy-final-close-refusal", 211);
    let composer_service = service(&fixture, true);
    let owner = cx.new(|_| View).entity_id();
    let prepared = prepare_healthy_close(&fixture, &composer_service, owner);
    let mut checked = false;
    cx.add_window_view(|window, _| {
        let NativeSession {
            mut session,
            source,
            ledger,
            ..
        } = delivered_page(window, &fixture, &composer_service, prepared.selected, 51);
        *composer_service.native_lineage_sources.lock().unwrap() = vec![source];
        let before_sources = source_snapshot(&composer_service);
        let before_ledger = ledger.ownership();
        let stale_close =
            MainWindowConversationComposerCloseTicket::for_test(owner, 2, prepared.selected);
        for (close, selected) in [
            (stale_close, prepared.selected),
            (prepared.close, prepared.before_publication),
        ] {
            assert!(
                composer_service
                    .take_final_prepublication_cleanup(close, prepared.flush, selected)
                    .is_err()
            );
            assert_eq!(source_snapshot(&composer_service), before_sources);
            assert_eq!(ledger.ownership(), before_ledger);
        }
        composer_service
            .qualify_final_prepublication_cleanup(prepared.close, prepared.flush, prepared.selected)
            .unwrap();
        composer_service.native_lineage_sources.lock().unwrap()[0]
            .qualify_cleanup_handoff(prepared.selected)
            .unwrap();
        fail(&fixture);
        assert!(
            composer_service
                .take_final_prepublication_cleanup(
                    prepared.close,
                    prepared.flush,
                    prepared.selected,
                )
                .is_err()
        );
        assert_eq!(source_snapshot(&composer_service), before_sources);
        assert_eq!(ledger.ownership(), before_ledger);
        let mut source = composer_service
            .native_lineage_sources
            .lock()
            .unwrap()
            .pop()
            .unwrap();
        source.qualify_cleanup_handoff(prepared.selected).unwrap();
        let mut capsule = Arc::get_mut(&mut source).unwrap().take_qualified_cleanup();
        drop(source);
        drain_cancelled_capsule(&mut session, &ledger, &mut capsule);
        assert!(capsule.accepted_page_release_acknowledgements() > 0);
        checked = true;
        View
    });
    assert!(checked);
}

#[derive(Clone, Copy, Debug)]
enum LaterSource {
    Active,
    Pending,
    Undelivered,
    Foreign,
}

#[gpui::test]
fn healthy_final_handoff_preflights_entire_vector_before_consuming_valid_first_page(
    cx: &mut TestAppContext,
) {
    for (index, state) in [
        LaterSource::Active,
        LaterSource::Pending,
        LaterSource::Undelivered,
        LaterSource::Foreign,
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = support::Fixture::new("healthy-final-vector-preflight", 221 + index as u8);
        let composer_service = service(&fixture, true);
        let foreign = support::Fixture::new("foreign-final-vector-preflight", 241 + index as u8);
        let foreign_service = service(&foreign, false);
        let owner = cx.new(|_| View).entity_id();
        let prepared = prepare_healthy_close(&fixture, &composer_service, owner);
        let mut checked = false;
        cx.add_window_view(|window, _| {
            let NativeSession {
                session: mut first_session,
                source: first,
                ledger: first_ledger,
                ..
            } = delivered_page(window, &fixture, &composer_service, prepared.selected, 61);
            let second_selected = if matches!(state, LaterSource::Foreign) {
                foreign_service.selected_identity().unwrap()
            } else {
                prepared.selected
            };
            let NativeSession {
                session: mut second_session,
                source: second,
                ledger: second_ledger,
                validation,
            } = begin_native_session(window, second_selected, 62);
            match state {
                LaterSource::Active | LaterSource::Foreign => {
                    second.finish_delivery(validation.cleanup);
                }
                LaterSource::Pending => {}
                LaterSource::Undelivered => second.finish(
                    validation.cleanup,
                    MainWindowNativeLineagePrepublicationResult::Validation(Ok(
                        validation_response(validation),
                    )),
                ),
            }
            if !matches!(state, LaterSource::Active) {
                second.release_owner();
            }
            first.qualify_cleanup_handoff(prepared.selected).unwrap();
            assert!(second.qualify_cleanup_handoff(prepared.selected).is_err());
            *composer_service.native_lineage_sources.lock().unwrap() = vec![first, second];
            let before_sources = source_snapshot(&composer_service);
            let before_first = first_ledger.ownership();
            let before_second = second_ledger.ownership();
            assert!(
                composer_service
                    .take_final_prepublication_cleanup(
                        prepared.close,
                        prepared.flush,
                        prepared.selected,
                    )
                    .is_err(),
                "later {state:?} source must refuse the complete handoff"
            );
            assert_eq!(source_snapshot(&composer_service), before_sources);
            assert_eq!(first_ledger.ownership(), before_first);
            assert_eq!(second_ledger.ownership(), before_second);
            {
                let sources = composer_service.native_lineage_sources.lock().unwrap();
                let second = &sources[1];
                match state {
                    LaterSource::Pending => second.finish_delivery(validation.cleanup),
                    LaterSource::Undelivered => {
                        let MainWindowNativeLineagePrepublicationResult::Validation(Ok(response)) =
                            second.take(validation.cleanup).unwrap()
                        else {
                            panic!("actual validation delivery changed")
                        };
                        second_session.deliver_validation(response);
                    }
                    _ => {}
                }
                second.release_owner();
                second.qualify_cleanup_handoff(second_selected).unwrap();
            }
            let mut capsules = if matches!(state, LaterSource::Foreign) {
                let mut sources = composer_service.native_lineage_sources.lock().unwrap();
                let capsules = sources
                    .iter_mut()
                    .map(|source| Arc::get_mut(source).unwrap().take_qualified_cleanup())
                    .collect::<Vec<_>>();
                sources.clear();
                capsules
            } else {
                composer_service
                    .take_final_prepublication_cleanup(
                        prepared.close,
                        prepared.flush,
                        prepared.selected,
                    )
                    .unwrap()
                    .expect("actual Healthy delivered sources qualify together")
            };
            assert_eq!(capsules.len(), 2);
            assert!(
                composer_service
                    .native_lineage_sources
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(first_ledger.ownership(), before_first);
            assert_eq!(capsules[0].accepted_page_release_acknowledgements(), 0);
            assert!(!capsules[0].advance(64).unwrap());
            assert_eq!(first_ledger.ownership(), before_first);
            drain_cancelled_capsule(&mut first_session, &first_ledger, &mut capsules[0]);
            drain_cancelled_capsule(&mut second_session, &second_ledger, &mut capsules[1]);
            assert!(capsules[0].accepted_page_release_acknowledgements() > 0);
            checked = true;
            View
        });
        assert!(checked, "{state:?}");
    }
}
