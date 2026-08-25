use beryl_app::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostServiceDisposalCompletion,
};
use beryl_home_store::CommandCancellation;
use beryl_state::BerylState;
use gpui_text_input::MutationKind;
use syndic_storage::SyndicTimestamp;

use super::{base, composer, publication};

#[test]
fn service_disposal_releases_changed_marker_flight_and_all_host_custody() {
    let (_home, mut store, storage, thread) =
        base::fixture("phase173-service-disposal-flight", 220);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let asset = publication::publish_image_asset(&store, assets, b"phase173-service-disposal");
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 221, 222);
    let edited = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let undone = composer::select_history(&mut host, &store, edited, 2, MutationKind::Undo);
    let marker = publication::insert_published_marker(&mut host, &store, undone, 3, asset).0;
    assert_eq!(host.test_last_settlement_identity_custody_count(), 3);
    let timer = host.autosave_timer().unwrap();
    let ticket = match host
        .fire_autosave(
            &store,
            timer,
            assets,
            &seals,
            composer::operation_id(4),
            Some(publication::authority(223)),
            SyndicTimestamp::from_unix_millis(4),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("changed-marker autosave was not captured: {other:?}"),
    };
    assert_eq!(
        ticket.candidate_generation(),
        marker.candidate().candidate_generation()
    );
    assert_eq!(seals.diagnostics().current_flights(), 1);
    assert_eq!(host.publication_custody_count(), 1);

    assert_eq!(
        host.dispose_composer_service(&store).unwrap(),
        ComposerHostServiceDisposalCompletion::Disposed
    );
    assert_eq!(seals.diagnostics().current_flights(), 0);
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(host.settlement_custody_in_use(), 0);
    assert_eq!(host.test_last_settlement_identity_custody_count(), 0);
    assert!(host.binding().is_none());
    let diagnostics = host.lifecycle_diagnostics();
    assert_eq!(diagnostics.timers(), 0);
    assert_eq!(diagnostics.barriers(), 0);
    assert_eq!(diagnostics.joined_publications(), 0);
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Stale
    );

    let (mut successor, empty) = composer::activated(storage, &store, thread, 224, 225);
    let marker = publication::insert_published_marker(&mut successor, &store, empty, 5, asset).0;
    let successor_ticket = match successor
        .fire_autosave(
            &store,
            successor.autosave_timer().unwrap(),
            assets,
            &seals,
            composer::operation_id(6),
            Some(publication::authority(226)),
            SyndicTimestamp::from_unix_millis(6),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("same-home successor flight was not admitted: {other:?}"),
    };
    assert_eq!(
        successor_ticket.candidate_generation(),
        marker.candidate().candidate_generation()
    );
    assert_eq!(seals.diagnostics().current_flights(), 1);
    assert_eq!(
        successor.dispose_composer_service(&store).unwrap(),
        ComposerHostServiceDisposalCompletion::Disposed
    );
    assert_eq!(seals.diagnostics().current_flights(), 0);
}
