use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS;

use crate::{
    app_support::{close_seeded, restart_service, seeded_home},
    phase62_support::UnavailableProvider,
    support as storage_support,
};

fn ordered_thread(value: usize) -> SyndicThreadId {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(
        &u64::try_from(value)
            .expect("fixture ordinal fits u64")
            .to_be_bytes(),
    );
    SyndicThreadId::from_bytes(bytes)
}

fn ordered_draft(value: usize) -> SyndicDraftId {
    SyndicDraftId::from_bytes(*ordered_thread(value).as_bytes())
}

fn ordered_item(value: usize) -> SyndicItemId {
    SyndicItemId::from_bytes(*ordered_thread(value).as_bytes())
}

#[test]
fn startup_diagnostics_report_bounded_multipage_eof() {
    let home = seeded_home();
    let row_count = DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS + 44;
    storage_support::commit(
        &home.store,
        home.storage,
        storage_support::batch((1..=row_count).flat_map(|value| {
            storage_support::empty_thread_records(
                ordered_thread(value),
                ordered_draft(row_count + value),
            )
        })),
    );
    home.store.validate_registered_domains().unwrap();
    let revision_before = home.storage.revision(&home.store).unwrap();
    let directory = close_seeded(home);

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_page_reads(), 2);
    assert_eq!(diagnostics.startup_recovery_cases(), 0);
    assert_eq!(diagnostics.startup_active_convergences(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 0);
    assert_eq!(diagnostics.startup_pending_turns(), 0);
    assert_eq!(diagnostics.startup_deferred_compactions(), 0);
    assert_eq!(service.initial_storage_revision(), revision_before);
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            storage.revision(command_home.home()).unwrap(),
            revision_before
        );
    }
    service.close().unwrap();
}

#[test]
fn startup_recovers_more_pending_cases_than_one_physical_page() {
    let home = seeded_home();
    let row_count = DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS + 1;
    storage_support::commit(
        &home.store,
        home.storage,
        storage_support::batch((1..=row_count).flat_map(|value| {
            storage_support::empty_thread_records(
                ordered_thread(value),
                ordered_draft(row_count + value),
            )
        })),
    );
    for value in 1..=row_count {
        let text = format!("phase63 multipage pending recovery {value}");
        storage_support::exact_cas::submit_current_draft(
            &home.store,
            home.storage,
            ordered_thread(value),
            ordered_draft((row_count * 2) + value),
            ordered_item((row_count * 3) + value),
            &text,
            storage_support::timestamp(63_300),
        );
    }
    home.store.validate_registered_domains().unwrap();
    let revision_before = home.storage.revision(&home.store).unwrap();
    let directory = close_seeded(home);

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_page_reads(), 2);
    assert_eq!(diagnostics.startup_recovery_cases(), row_count as u64);
    assert_eq!(diagnostics.startup_pending_turns(), row_count as u64);
    assert_eq!(diagnostics.startup_active_convergences(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 0);
    assert_eq!(diagnostics.startup_deferred_compactions(), 0);
    assert_eq!(service.initial_storage_revision(), revision_before);
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            storage.revision(command_home.home()).unwrap(),
            revision_before
        );
    }
    service.close().unwrap();
}
