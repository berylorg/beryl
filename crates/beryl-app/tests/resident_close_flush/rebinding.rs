use beryl_app::composer_host::{ComposerHostError, ComposerHostFlushState};
use beryl_home_store::test_faults::FaultPoint;

use super::{composer, support};
use support::host_close::{begin_close, ready};

#[test]
fn recovered_clean_host_preserves_checkpoint_and_requires_fresh_close_qualification() {
    for dirty in [false, true] {
        let mut fixture = support::host("rebind-clean-host", 221 + u8::from(dirty) * 4);
        let initial = fixture.host.binding().unwrap();
        if dirty {
            composer::commit_text(
                &mut fixture.host,
                &fixture.store,
                initial,
                1,
                0,
                0,
                "saved",
                5,
                1,
            );
        }
        let close = begin_close(&mut fixture);
        ready(&mut fixture, close, 10);
        let saved = fixture.host.binding().unwrap();
        let capacity = fixture.host.settlement_custody_capacity();
        let retired = Box::new(fixture.host)
            .retire_clean_window_close(close)
            .unwrap_or_else(|_| panic!("ready host refused retirement"));
        let selector = retired.selector();
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let mut recovery = fixture.store.recover_same_home().unwrap();
        let fresh = syndic_storage::SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let assets = beryl_state::BerylState::reacquire_candidate(&recovery)
            .unwrap()
            .assets();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        let mut host = retired.rebind_candidate(&access, fresh.clone()).unwrap();
        assert_eq!(access.home_revision().unwrap(), revision);
        let binding = host.binding().unwrap();
        assert_eq!(binding.home_id(), saved.home_id());
        assert_eq!(binding.home_generation(), access.generation());
        assert_ne!(binding.home_generation(), saved.home_generation());
        assert_eq!(
            binding.host_generation().get(),
            saved.host_generation().get() + 1
        );
        assert_ne!(binding.range_binding(), saved.range_binding());
        assert_eq!(
            binding.presentation_generation(),
            saved.presentation_generation()
        );
        assert_eq!(binding.candidate(), saved.candidate());
        assert_eq!(host.settlement_custody_capacity(), capacity);
        assert!(!host.is_dirty());
        assert_eq!(host.pending_request_count(), 0);
        assert_eq!(host.settlement_custody_in_use(), 0);
        assert_eq!(host.publication_custody_count(), 0);
        assert!(!host.release_window_close(close).unwrap());
        let fresh_close = host.window_close_ticket().unwrap();
        assert_ne!(fresh_close, close);
        assert_eq!(
            host.flush_state(fresh_close).unwrap(),
            ComposerHostFlushState::CaptureRequired
        );
        fixture.store = recovery.publish().unwrap();
        fixture.storage = fresh;
        fixture.assets = assets;
        fixture.host = *host;
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
        assert!(matches!(
            composer::begin_text(&mut fixture.host, &fixture.store, binding, 2, 0),
            Err(ComposerHostError::LifecycleBlocked)
        ));
        ready(&mut fixture, fresh_close, 11);
        assert_eq!(fixture.host.binding(), Some(binding));
        assert!(
            fixture
                .storage
                .draft_editor_candidate_is_saved(&fixture.store, binding.candidate(), selector,)
                .unwrap()
        );
        assert!(fixture.host.release_window_close(fresh_close).unwrap());
        assert!(composer::begin_text(&mut fixture.host, &fixture.store, saved, 2, 0).is_err());
        let end = binding.logical_extent().logical_utf8_bytes();
        let edited = composer::commit_text(
            &mut fixture.host,
            &fixture.store,
            binding,
            3,
            end,
            end,
            "!",
            end + 1,
            1,
        );
        assert_eq!(
            edited.candidate().session_id(),
            saved.candidate().session_id()
        );
        assert!(fixture.host.is_dirty());
    }
}
