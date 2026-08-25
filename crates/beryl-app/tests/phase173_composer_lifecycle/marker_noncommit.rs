use beryl_app::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostFlushAdvance,
    ComposerHostFlushCapture, ComposerHostFlushFailure, ComposerHostFlushPurpose,
};
use beryl_home_store::CommandCancellation;
use beryl_state::BerylState;
use syndic_storage::SyndicTimestamp;

use super::{base, composer, publication, started_flush};

#[test]
fn marker_noncommit_ends_each_autosave_stage_once_and_rearms() {
    for (index, normal_advances) in [0, 1, 2].into_iter().enumerate() {
        let seed = 10_u8.wrapping_add((index as u8).wrapping_mul(12));
        let (_home, mut store, storage, thread) =
            base::fixture("phase173-marker-autosave-noncommit", seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let asset = publication::publish_image_asset(&store, assets, &[seed; 16]);
        let seals = publication::service(&store, storage, assets, 1, 1);
        let (mut host, empty) = composer::activated(storage, &store, thread, seed + 1, seed + 2);
        let _ = publication::insert_published_marker(&mut host, &store, empty, 1, asset).0;
        let timer = host.autosave_timer().unwrap();
        let ticket = match host
            .fire_autosave(
                &store,
                timer,
                assets,
                &seals,
                composer::operation_id(2),
                Some(publication::authority(seed + 3)),
                SyndicTimestamp::from_unix_millis(2),
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostAutosaveCapture::Captured(ticket) => ticket,
            other => panic!("marker autosave was not captured: {other:?}"),
        };
        for _ in 0..normal_advances {
            assert_eq!(
                host.advance_autosave(&store, ticket).unwrap(),
                ComposerHostAutosaveAdvance::Progress
            );
        }
        seals.test_arm_before_command_fault(move |store| {
            base::bump_home_revision(storage, store, seed + 4)
        });
        assert_eq!(
            host.advance_autosave(&store, ticket).unwrap(),
            ComposerHostAutosaveAdvance::Unsatisfied(ComposerHostFlushFailure::NotCommitted)
        );
        assert_eq!(host.publication_custody_count(), 0);
        assert_eq!(seals.diagnostics().current_flights(), 0);
        assert_eq!(host.lifecycle_diagnostics().joined_publications(), 0);
        assert!(host.autosave_timer().is_some());
        assert_eq!(
            host.advance_autosave(&store, ticket).unwrap(),
            ComposerHostAutosaveAdvance::Stale
        );
    }
}

#[test]
fn marker_noncommit_ends_each_flush_stage_once_and_rearms() {
    for (index, normal_advances) in [0, 1, 2].into_iter().enumerate() {
        let seed = 50_u8.wrapping_add((index as u8).wrapping_mul(12));
        let (_home, mut store, storage, thread) =
            base::fixture("phase173-marker-flush-noncommit", seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let asset = publication::publish_image_asset(&store, assets, &[seed; 16]);
        let seals = publication::service(&store, storage, assets, 1, 1);
        let (mut host, empty) = composer::activated(storage, &store, thread, seed + 1, seed + 2);
        let _ = publication::insert_published_marker(&mut host, &store, empty, 1, asset).0;
        let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
        let publication = match host
            .capture_flush_publication(
                &store,
                flush,
                assets,
                &seals,
                composer::operation_id(2),
                Some(publication::authority(seed + 3)),
                SyndicTimestamp::from_unix_millis(2),
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostFlushCapture::Captured(ticket) => ticket,
            other => panic!("marker flush was not captured: {other:?}"),
        };
        for _ in 0..normal_advances {
            assert!(matches!(
                host.advance_flush(&store, flush).unwrap(),
                ComposerHostFlushAdvance::Progress(_)
            ));
        }
        seals.test_arm_before_command_fault(move |store| {
            base::bump_home_revision(storage, store, seed + 4)
        });
        assert_eq!(
            host.advance_flush(&store, flush).unwrap(),
            ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::NotCommitted)
        );
        assert_eq!(host.publication_custody_count(), 0);
        assert_eq!(seals.diagnostics().current_flights(), 0);
        assert_eq!(host.lifecycle_diagnostics().barriers(), 0);
        assert!(host.autosave_timer().is_some());
        assert_eq!(
            host.advance_flush(&store, flush).unwrap(),
            ComposerHostFlushAdvance::Stale
        );
        assert_eq!(
            host.advance_autosave(&store, publication).unwrap(),
            ComposerHostAutosaveAdvance::Stale
        );
    }
}
