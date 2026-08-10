use beryl_model::BerylHomeId;

use super::super::{
    LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT, PANIC_AFTER_OLD_INGESTER_JOIN,
    PAUSE_BEFORE_ADOPTION_COMMIT,
};
use super::PersistentFailureRecoveryInventory;

pub(in crate::cas_projection::service::adoption) fn retain_late_authority_before_commit_if_armed(
    home_id: BerylHomeId,
    inventory: &PersistentFailureRecoveryInventory,
) {
    let armed = {
        let mut slot = LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("the adoption late-authority test hook is usable");
        if *slot == Some(home_id) {
            slot.take();
            true
        } else {
            false
        }
    };
    if armed {
        inventory.retain_late_adoption_authority_for_test();
    }
}

pub(in crate::cas_projection::service::adoption) fn panic_after_old_ingester_join_if_armed(
    home_id: BerylHomeId,
) {
    let armed = {
        let mut slot = PANIC_AFTER_OLD_INGESTER_JOIN
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("the post-join adoption-panic test hook is usable");
        if *slot == Some(home_id) {
            slot.take();
            true
        } else {
            false
        }
    };
    assert!(!armed, "injected panic after the old-ingester join");
}

pub(in crate::cas_projection::service::adoption) fn pause_before_commit_if_armed(
    home_id: BerylHomeId,
) {
    let pause = {
        let mut slot = PAUSE_BEFORE_ADOPTION_COMMIT
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("the adoption precommit-pause test hook is usable");
        match slot.take() {
            Some((expected, reached, release)) if expected == home_id => Some((reached, release)),
            other => {
                *slot = other;
                None
            }
        }
    };
    if let Some((reached, release)) = pause {
        if reached.send(()).is_ok() {
            let _ = release.recv();
        }
    }
}
