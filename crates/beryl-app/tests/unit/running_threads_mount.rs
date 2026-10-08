use super::*;
use crate::composer_host::ComposerHostFlushAdmission;
use crate::main_window::{
    MainWindowComposerActivationAdvance, MainWindowComposerMarkerMetadataAuthority,
    MainWindowComposerSlot,
};
use beryl_home_store::CommandCancellation;

#[path = "../main_window_composer_slot/support.rs"]
mod support;

#[path = "running_threads_mount/selection_save.rs"]
mod selection_save;
#[path = "running_threads_mount/creation_save.rs"]
mod creation_save;

fn clipboard_payload() -> gpui_text_input::ClipboardWriteRequest {
    use gpui_text_input::*;
    let text = "retained unsettled clipboard payload";
    let binding = RangeBinding::new(
        BindingId::new(41),
        SourceRevision::new(7),
        LogicalExtent::new(text.len() as u64, 0),
    );
    let mut clipboard = RangeClipboardCoordinator::new(
        binding,
        TextInputAtomClipboardPolicy::Propagate,
        ClipboardLimits::new(4096, 4096).unwrap(),
    )
    .unwrap();
    let mut progress = clipboard
        .begin_selection(
            ClipboardId::new(5),
            ClipboardKind::Copy,
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects),
            SourcePosition::new(
                ByteOffset::new(text.len() as u64),
                InlineObjectGap::NoObjects,
            ),
        )
        .unwrap();
    for sequence in 1..=8 {
        let mut commit = match progress {
            ClipboardProgress::NeedObjectPage { key, .. } => {
                let request = clipboard
                    .request_object_page(key, ObjectRequestId::new(sequence))
                    .unwrap();
                let page = ObjectPage::new(
                    ObjectPageId::new(sequence),
                    request.key(),
                    Vec::new(),
                    ObjectPageEdgeFact::EnvelopeBoundary,
                    ObjectPageEdgeFact::EnvelopeBoundary,
                    true,
                    None,
                )
                .unwrap();
                let step = clipboard.prepare_object_page(&page).unwrap();
                clipboard.commit_object_page(page, step).unwrap()
            }
            ClipboardProgress::NeedTextPage { key, .. } => {
                let request = clipboard
                    .request_text_page(key, PageRequestId::new(sequence))
                    .unwrap();
                let page = RangePage::new(
                    PageId::new(sequence),
                    request.key(),
                    ByteRange::from_u64(0, text.len() as u64).unwrap(),
                    text.into(),
                    Vec::new(),
                    PageEdgeFact::DocumentBoundary,
                    PageEdgeFact::DocumentBoundary,
                    true,
                )
                .unwrap();
                let step = clipboard.prepare_text_page(&page).unwrap();
                clipboard.commit_text_page(page, step).unwrap()
            }
            ClipboardProgress::Write(payload) => return payload,
            _ => panic!("clipboard fixture returned an unexpected outcome"),
        };
        progress = loop {
            if let Some(progress) = commit.into_progress() {
                break progress;
            }
            let step = clipboard.prepare_next().unwrap();
            commit = clipboard.commit_prepared(step).unwrap();
        };
    }
    panic!("clipboard fixture exceeded its bounded preparation budget")
}

#[test]
fn publication_failure_retains_owned_payload_on_stale_unsettled_and_poisoned_source() {
    let fixture = support::Fixture::new("claim-payload-custody", 181);
    let (selected_claim, target_claim) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 190, 191, 1);
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    let MainWindowComposerActivationAdvance::Ready(receipt) = service
        .begin_activation(
            target_claim,
            support::activation(fixture.target_thread, 192, 193, 2),
            support::operation_id(194),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target source was not ready")
    };
    let expected = service.selected_identity().unwrap();
    let payload = clipboard_payload();
    let key = payload.key();
    let text_pointer = payload.text().as_ptr();
    let requests = vec![
        gpui_text_input::RangeTextInputRequest::ClipboardWrite(payload),
        gpui_text_input::RangeTextInputRequest::CancelClipboardWrite(key),
    ];
    let requests_pointer = requests.as_ptr();
    let mut work = MainWindowComposerClaimWidgetWork::Requests {
        selection: expected,
        requests,
    };
    for failure in 0..3 {
        if failure == 1 {
            service.publish_preflight(receipt).unwrap();
            assert!(matches!(
                service.begin_publish(receipt).unwrap(),
                ComposerHostFlushAdmission::Satisfied(_)
            ));
            service.advance_claim_publication_source(receipt).unwrap();
            service.begin_final_publish(receipt, expected).unwrap();
        } else if failure == 2 {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _guard = service.slot.lock().unwrap();
                    panic!("poison the publication source fixture");
                }))
                .is_err()
            );
        }
        let revision = fixture.store.home_revision().unwrap();
        work = match service.complete_claim_publication_source(receipt, work) {
            Err((work, _)) => work,
            Ok(_) => panic!("unsettled payload incorrectly completed publication"),
        };
        let MainWindowComposerClaimWidgetWork::Requests {
            selection,
            requests,
        } = &work
        else {
            panic!("unsettled payload was replaced by a release proof")
        };
        assert_eq!(*selection, expected);
        assert_eq!(requests.as_ptr(), requests_pointer);
        assert_eq!(requests.len(), 2);
        let gpui_text_input::RangeTextInputRequest::ClipboardWrite(payload) = &requests[0] else {
            panic!("owned clipboard payload was lost")
        };
        assert_eq!(payload.key(), key);
        assert_eq!(payload.text(), "retained unsettled clipboard payload");
        assert_eq!(payload.text().as_ptr(), text_pointer);
        assert!(
            matches!(&requests[1], gpui_text_input::RangeTextInputRequest::CancelClipboardWrite(actual) if *actual == key)
        );
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
    }
    service.slot.clear_poison();
    assert_eq!(service.selected_identity(), Some(expected));
    assert_eq!(service.pending_receipt(), Some(receipt));
}

#[test]
fn completed_source_token_elects_resident_publication_and_rejects_busy_or_retiring_service() {
    let fixture = support::Fixture::new("claim-publication", 161);
    let (selected_claim, target_claim) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 170, 171, 1);
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    let MainWindowComposerActivationAdvance::Ready(receipt) = service
        .begin_activation(
            target_claim,
            support::activation(fixture.target_thread, 172, 173, 2),
            support::operation_id(174),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target source was not ready")
    };
    let prepared = service.prepare_claim_presentation_source(receipt).unwrap();
    assert_eq!(prepared.selection.claim(), target_claim);
    let expected = service.publish_preflight(receipt).unwrap();
    assert!(matches!(
        service.begin_publish(receipt).unwrap(),
        ComposerHostFlushAdmission::Satisfied(_)
    ));
    let advance = service.advance_claim_publication_source(receipt).unwrap();
    assert_eq!(advance.selected, expected);
    assert!(matches!(
        advance.advance,
        MainWindowComposerPublishAdvance::WidgetReleaseRequired(_)
    ));
    service.begin_final_publish(receipt, expected).unwrap();
    let completion = service
        .complete_claim_publication_source(
            receipt,
            MainWindowComposerClaimWidgetWork::Requests {
                selection: expected,
                requests: Vec::new(),
            },
        )
        .unwrap();
    let MainWindowComposerClaimCompletion::Published {
        release,
        publication,
    } = completion
    else {
        panic!("source completion did not publish")
    };
    assert_eq!(release.selection(), expected);
    assert_eq!(publication.selection().claim(), target_claim);
    let guard = service.slot.lock().unwrap();
    assert!(
        publication
            .try_elect_current(|| panic!("busy service published"))
            .is_err()
    );
    drop(guard);
    assert_eq!(publication.try_elect_current(|| 7).unwrap(), 7);
    service.begin_disposal().unwrap();
    assert!(
        publication
            .try_elect_current(|| panic!("retiring service published"))
            .is_err()
    );
}

#[test]
fn settled_flush_can_abort_fresh_target_before_release_but_never_after_finalization() {
    for finalizing in [false, true] {
        let fixture = support::Fixture::new("claim-abort", 181);
        let (selected_claim, target_claim) = fixture.claims();
        let host = fixture.activated_host(fixture.selected_thread, 190, 191, 1);
        let slot = MainWindowComposerSlot::new(
            fixture.window_id,
            selected_claim,
            host,
            fixture.storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
        )
        .unwrap();
        let service = Arc::new(MainWindowConversationComposerService::new(
            fixture.store.service_reference(),
            slot,
        ));
        let MainWindowComposerActivationAdvance::Ready(receipt) = service
            .begin_activation(
                target_claim,
                support::activation(fixture.target_thread, 192, 193, 2),
                support::operation_id(194),
                &CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("target source was not ready")
        };
        let expected = service.publish_preflight(receipt).unwrap();
        assert!(matches!(
            service.begin_publish(receipt).unwrap(),
            ComposerHostFlushAdmission::Satisfied(_)
        ));
        assert!(matches!(
            service
                .advance_claim_publication_source(receipt)
                .unwrap()
                .advance,
            MainWindowComposerPublishAdvance::WidgetReleaseRequired(_)
        ));
        if finalizing {
            service.begin_final_publish(receipt, expected).unwrap();
            assert!(
                service
                    .abort_claim_publication_before_release(receipt, expected)
                    .is_err()
            );
            let MainWindowComposerClaimCompletion::Published { publication, .. } = service
                .complete_claim_publication_source(
                    receipt,
                    MainWindowComposerClaimWidgetWork::Requests {
                        selection: expected,
                        requests: Vec::new(),
                    },
                )
                .unwrap()
            else {
                panic!("source completion did not publish")
            };
            assert!(
                service
                    .abort_claim_publication_before_release(receipt, expected)
                    .is_err()
            );
            assert_eq!(service.selected_identity(), Some(publication.selection()));
        } else {
            assert_eq!(
                service
                    .abort_claim_publication_before_release(receipt, expected)
                    .unwrap(),
                crate::main_window::MainWindowComposerRetirementAdvance::Retired
            );
            assert_eq!(service.selected_identity(), Some(expected));
            assert!(service.pending_receipt().is_none());
        }
    }
}
