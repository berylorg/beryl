use super::*;
use crate::main_window::MainWindowComposerDispatchOutcome;
use crate::main_window::conversation_composer_owner::prepublication::{
    MainWindowNativeLineagePrepublicationResult, MainWindowNativeLineagePrepublicationSource,
};
use gpui::{AppContext, TestAppContext};
use gpui_text_input::*;

#[path = "../../pending_composer_activation/support.rs"]
mod widget_support;

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

#[gpui::test]
fn delivered_prepublication_cleanup_preserves_actual_page_tokens_until_original_release(
    cx: &mut TestAppContext,
) {
    let fixture = support::Fixture::new("retired-prepublication-cleanup", 161);
    let composer_service = service(&fixture, true);
    let selected = composer_service.selected_identity().unwrap();
    let mut prepared = None;
    cx.add_window_view(|window, _| {
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
        let config = widget_support::widget_config(
            seed.binding,
            selected.binding().presentation_generation(),
        );
        let ledger = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        let environment =
            RangePrepublicationEnvironment::new(31, config, window.text_system(), ledger.clone())
                .unwrap();
        let mut session = RangePrepublicationSession::new(seed, environment).unwrap();
        let mut native = MainWindowNativeLineagePrepublicationSource::new(
            selected,
            31,
            session.generation(),
            ledger.clone(),
        );
        let effect = session
            .service(window.text_system())
            .effects
            .into_iter()
            .next()
            .unwrap();
        let RangePrepublicationEffect::ValidateOwner(request) = effect else {
            panic!("missing original validation")
        };
        native.begin(effect).unwrap();
        assert!(native.qualify_cleanup_handoff(selected).is_err());
        native.finish_delivery(request.cleanup);
        session.deliver_validation(RangePrepublicationValidationResponse {
            key: request.key,
            binding: request.binding,
            history: request.history,
            current: true,
        });
        let mut page = None;
        for _ in 0..32 {
            let effects = session.service(window.text_system()).effects;
            if let Some(effect) = effects
                .into_iter()
                .find(|effect| matches!(effect, RangePrepublicationEffect::Page { .. }))
            {
                page = Some(effect);
                break;
            }
        }
        let page = page.expect("real nonempty source emits an original page demand");
        let RangePrepublicationEffect::Page {
            cleanup,
            generation,
            request,
        } = &page
        else {
            unreachable!()
        };
        let (cleanup, generation, request) = (*cleanup, *generation, *request);
        native.begin(page).unwrap();
        native.release_owner();
        assert!(native.qualify_cleanup_handoff(selected).is_err());
        let response = composer_service
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
        native.finish(
            cleanup,
            MainWindowNativeLineagePrepublicationResult::Request(Ok(response)),
        );
        assert!(native.qualify_cleanup_handoff(selected).is_err());
        let MainWindowNativeLineagePrepublicationResult::Request(Ok(
            MainWindowComposerDispatchOutcome::Page(response),
        )) = native.take(cleanup).unwrap()
        else {
            panic!("page source outcome changed")
        };
        session.deliver_page(generation, response);
        let foreign = support::Fixture::new("foreign-prepublication-cleanup", 171);
        let foreign_service = service(&foreign, false);
        assert!(
            native
                .qualify_cleanup_handoff(foreign_service.selected_identity().unwrap())
                .is_err()
        );
        native.qualify_cleanup_handoff(selected).unwrap();
        let before = ledger.ownership();
        assert!(before.active > 0);
        assert_eq!(before.ready, 0);
        let mut capsule = Arc::get_mut(&mut native).unwrap().take_qualified_cleanup();
        drop(native);
        assert_eq!(capsule.selection(), selected);
        assert_eq!(capsule.accepted_page_release_acknowledgements(), 0);
        assert_eq!(ledger.ownership(), before);
        assert!(!capsule.advance(64).unwrap());
        assert_eq!(ledger.ownership(), before);
        session.cancel();
        let mut complete = false;
        for _ in 0..8 {
            complete = capsule.advance(64).unwrap();
            if complete {
                break;
            }
        }
        assert!(complete);
        let accepted = capsule.accepted_page_release_acknowledgements();
        assert!(accepted > 0);
        assert_eq!(ledger.ownership().active, 0);
        assert_eq!(ledger.ownership().ready, 0);
        assert_eq!(ledger.ownership().awaiting_acknowledgement, 0);
        assert!(capsule.advance(64).unwrap());
        assert_eq!(capsule.accepted_page_release_acknowledgements(), accepted);
        prepared = Some(());
        View
    });
    assert!(prepared.is_some());
}

#[gpui::test]
fn prepublication_transfer_refuses_later_foreign_source_without_consuming_original_vector(
    cx: &mut TestAppContext,
) {
    let fixture = support::Fixture::new("prepublication-transfer-refusal", 181);
    let composer_service = service(&fixture, false);
    let selected = composer_service.selected_identity().unwrap();
    let foreign = support::Fixture::new("foreign-prepublication-transfer", 191);
    let foreign_service = service(&foreign, false);
    let mut prepared = None;
    cx.add_window_view(|window, _| {
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
        let ledger = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        let config = widget_support::widget_config(
            seed.binding,
            selected.binding().presentation_generation(),
        );
        let environment =
            RangePrepublicationEnvironment::new(41, config, window.text_system(), ledger.clone())
                .unwrap();
        let mut session = RangePrepublicationSession::new(seed, environment).unwrap();
        let native = MainWindowNativeLineagePrepublicationSource::new(
            selected,
            41,
            session.generation(),
            ledger.clone(),
        );
        let effect = session
            .service(window.text_system())
            .effects
            .into_iter()
            .next()
            .unwrap();
        let RangePrepublicationEffect::ValidateOwner(request) = effect else {
            panic!("missing real validation")
        };
        native.begin(effect).unwrap();
        native.finish_delivery(request.cleanup);
        native.release_owner();
        let other = MainWindowNativeLineagePrepublicationSource::new(
            foreign_service.selected_identity().unwrap(),
            41,
            session.generation(),
            ledger.clone(),
        );
        other.release_owner();
        let weak_original = Arc::downgrade(&native);
        composer_service
            .native_lineage_sources
            .lock()
            .unwrap()
            .extend([native, other]);
        fail(&fixture);
        let original = source(selected);
        let before = ledger.ownership();
        assert!(
            composer_service
                .take_failed_thread_creation_prepublication_cleanup(&original)
                .is_err()
        );
        assert!(weak_original.upgrade().is_some());
        drop(weak_original);
        assert!(
            composer_service
                .take_failed_thread_creation_prepublication_cleanup(&original)
                .is_err()
        );
        let sources = composer_service.native_lineage_sources.lock().unwrap();
        assert_eq!(sources.len(), 2);
        sources[0].qualify_cleanup_handoff(selected).unwrap();
        assert_eq!(sources[0].selection(), selected);
        assert_eq!(
            sources[1].selection(),
            foreign_service.selected_identity().unwrap()
        );
        assert_eq!(ledger.ownership(), before);
        drop(sources);
        session.cancel();
        composer_service
            .native_lineage_sources
            .lock()
            .unwrap()
            .clear();
        prepared = Some(());
        View
    });
    assert!(prepared.is_some());
}
