use super::{composer, publication, support, widget_support};
use beryl_app::{
    composer_host::{ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture},
    main_window::{
        MainWindowComposerRecoveryResources, MainWindowComposerSlot,
        MainWindowConversationComposerService, MainWindowFailedResidentMountResources,
    },
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use std::sync::Arc;
use syndic_storage::SyndicTimestamp;

#[test]
fn zero_flight_collision_keeps_original_marker_authority_and_refuses_retirement() {
    let fixture = widget_support::fixture::Fixture::new("failed-terminal-marker-custody", 191);
    let claim = fixture.claims().0;
    let window = fixture.window_id;
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let seals = fixture.marker_seals();
    let faults = fixture.faults.clone();
    let (_directory, store, storage) = fixture.into_store();
    let asset =
        publication::publish_image_asset(&store, assets.clone(), b"real staged marker custody");
    let (mut host, empty) = composer::activated(storage.clone(), &store, thread, 192, 193);
    let dirty = publication::insert_two_markers_with_readiness(
        &mut host, &store, &storage, empty, 911, [asset; 2],
    );
    let timer = host.autosave_timer().unwrap();
    let ticket = match host
        .fire_autosave(
            &store,
            timer,
            assets.clone(),
            &seals,
            composer::operation_id(912),
            Some(publication::authority(194)),
            SyndicTimestamp::from_unix_millis(912),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("actual marker publication was not captured: {other:?}"),
    };
    assert_eq!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Progress
    );
    assert_eq!(seals.diagnostics().current_flights(), 1);
    seals.test_fail_next_drive_as_collision();
    assert!(matches!(
        host.advance_autosave(&store, ticket).unwrap(),
        ComposerHostAutosaveAdvance::Unsatisfied(_)
    ));
    assert_eq!(seals.diagnostics().current_flights(), 0);
    assert!(host.publication_unavailable().is_some());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let host = Box::new(host)
        .retire_failed_resident(&store)
        .err()
        .expect("zero flights cannot erase unresolved original marker authority");
    assert_eq!(host.binding(), Some(dirty));
    assert!(host.publication_unavailable().is_some());
    let slot = MainWindowComposerSlot::new(
        window,
        claim,
        *host,
        storage,
        beryl_app::main_window::MainWindowComposerMarkerMetadataAuthority::new(assets.clone()),
    )
    .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.service_reference(),
        slot,
    ));
    let selection = service.selected_identity().unwrap();
    let resources = MainWindowFailedResidentMountResources {
        resident: MainWindowComposerRecoveryResources {
            service: Some(service.clone()),
            clipboard_writer: None,
            mutation_failure: None,
        },
        service: Some(service.clone()),
        publication_adapters: Some((assets, seals)),
        configurator: Some(Box::new(support::configure)),
        submission_source: None,
        native_lineage_control: None,
    };
    let resources = resources
        .retire()
        .err()
        .expect("terminal host refusal must precede adapter drops");
    assert!(
        resources
            .service
            .as_ref()
            .is_some_and(|retained| Arc::ptr_eq(retained, &service))
    );
    assert!(resources.publication_adapters.is_some());
    assert!(resources.configurator.is_some());
    assert_eq!(
        resources
            .publication_adapters
            .as_ref()
            .unwrap()
            .1
            .diagnostics()
            .current_flights(),
        0
    );
    assert_eq!(service.selected_identity(), Some(selection));
}
