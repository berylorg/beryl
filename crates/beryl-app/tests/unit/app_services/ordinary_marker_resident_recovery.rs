use super::*;

mod publication {
    use crate as beryl_app;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/syndic_composer_publication/support.rs"
    ));
}

#[test]
fn capacity_one_failed_marker_preparation_saves_two_exact_residents_without_retained_slot_deadlock()
{
    let directory = resident_fixture::selected_home_with_draft(2, 0);
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(directory.path())
    else {
        panic!("marker resident home opening failed")
    };
    let home = candidate.publish().unwrap();
    let session = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let markers = publication::service(&home, syndic.clone(), state.assets(), 1, 1);
    let asset = publication::publish_image_asset(
        &home,
        state.assets(),
        b"two exact retained marker residents",
    );
    let mut residents = Vec::new();
    for (index, record) in session.windows().iter().enumerate() {
        let mut host = crate::composer_host::SyndicComposerHost::new(syndic.clone());
        let request = crate::composer_host::ComposerHostActivationRequest::new(
            record.selected_thread().unwrap().thread_id(),
            syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([71 + index as u8; 16]),
            syndic_storage::DraftPieceOperationIdV1::from_bytes([81 + index as u8; 16]),
            std::num::NonZeroU64::MIN,
            None,
            Box::new([]),
        );
        let crate::composer_host::ComposerHostActivationOutcome::Activated { binding, .. } = host
            .test_activate(
                &home,
                request,
                &beryl_home_store::CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("marker resident activation failed")
        };
        let binding = publication::insert_two_markers_with_readiness(
            &mut host,
            &home,
            &syndic,
            binding,
            901 + index as u64 * 10,
            [asset; 2],
        );
        assert_eq!(binding.root().marker_commitment().marker_count(), 2);
        let point = gpui_text_input::SourcePosition::new(
            gpui_text_input::ByteOffset::new(0),
            gpui_text_input::InlineObjectGap::before(gpui_text_input::InlineObjectNeighbor::new(
                gpui_text_input::InlineObjectId::new(0x1001),
                gpui_text_input::InlineObjectOrder::new(1),
            )),
        );
        let seed = gpui_text_input::RangeRestorationSeed {
            binding: binding.range_binding(),
            caret: point,
            selection: gpui_text_input::RangeSourceSelection::caret(point),
            scroll: gpui_text_input::RangeRestorationScrollAnchor {
                position: point,
                intra_anchor: gpui::px(0.),
            },
            history: Some(binding.range_history_frontier()),
        };
        let slot = crate::main_window::MainWindowComposerSlot::new(
            record.window_id(),
            record.selected_thread().unwrap(),
            host,
            syndic.clone(),
            crate::main_window::MainWindowComposerMarkerMetadataAuthority::new(state.assets()),
        )
        .unwrap();
        let service = Arc::new(
            crate::main_window::MainWindowConversationComposerService::new(
                home.service_reference(),
                slot,
            ),
        );
        residents.push((service, seed, binding));
    }
    home.inject_retained_maintenance_terminal();
    assert!(beryl_state::BerylState::reacquire(&home).is_err());
    let captured = markers.capture_failed_home(&home).unwrap();
    let one = std::num::NonZeroUsize::MIN;
    let mut owner = crate::app_services::ProcessServiceOwner::new(home.home_id(), one, one);
    for (service, seed, _) in residents {
        let retired = service
            .retire_failed_resident_with_marker_custody(&captured)
            .ok()
            .expect("exact failed marker resident retires");
        owner.retain_failed_resident(retired, seed).ok().unwrap();
    }
    owner.failed_markers = Some(captured);
    let original = crate::running_owner::RunningShutdownSession::UnremovedWindows(
        crate::running_owner::UnremovedWindows::new(
            home.home_id(),
            home.canonical_path().to_owned(),
            session
                .windows()
                .iter()
                .map(|window| (window.window_id(), window.selected_thread()))
                .collect(),
        ),
    );
    let mut candidate = home.recover_same_home().unwrap();
    owner
        .prepare_failed_residents(
            &mut candidate,
            &original,
            syndic_storage::SyndicTimestamp::from_unix_millis(1001),
            &beryl_home_store::CommandCancellation::new(),
        )
        .unwrap();
    assert!(owner.failed_markers.is_none());
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let fresh = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    for source in &mut owner.failed_residents {
        assert!(
            source
                .retired
                .as_mut()
                .unwrap()
                .qualify_saved(&mut candidate, &storage, &fresh, None)
                .unwrap()
        );
        let current = storage
            .current_draft_candidate(
                &candidate.recovery_access().unwrap(),
                source
                    .retired
                    .as_ref()
                    .unwrap()
                    .selection()
                    .claim()
                    .thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            current
                .root()
                .reference()
                .marker_commitment()
                .marker_count(),
            2
        );
    }
    candidate.abort().close().unwrap();
}
