use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use super::super::{PersistentFailureCutIdentity, PersistentFailurePendingProjectionQuarantine};

pub(super) enum AdversarialQuarantineTopologyForTest {
    ExtraConnectionOwner(PersistentFailurePendingProjectionQuarantine),
    RetiredConnectionOwner,
    ForeignFailureCutOwner(PersistentFailurePendingProjectionQuarantine),
}

pub(in crate::cas_projection) struct AdversarialQuarantineTopologyArmForTest {
    cut: PersistentFailureCutIdentity,
}

fn armed_topologies()
-> &'static Mutex<HashMap<PersistentFailureCutIdentity, AdversarialQuarantineTopologyForTest>> {
    static ARMED: OnceLock<
        Mutex<HashMap<PersistentFailureCutIdentity, AdversarialQuarantineTopologyForTest>>,
    > = OnceLock::new();
    ARMED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn take_adversarial_quarantine_topology_for_test(
    cut: PersistentFailureCutIdentity,
) -> Option<AdversarialQuarantineTopologyForTest> {
    armed_topologies()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&cut)
}

fn arm_adversarial_quarantine_topology_for_test(
    cut: PersistentFailureCutIdentity,
    topology: AdversarialQuarantineTopologyForTest,
) -> AdversarialQuarantineTopologyArmForTest {
    let previous = armed_topologies()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(cut, topology);
    assert!(
        previous.is_none(),
        "one adversarial topology is armed per cut"
    );
    AdversarialQuarantineTopologyArmForTest { cut }
}

fn quarantine_cut(
    quarantine: &PersistentFailurePendingProjectionQuarantine,
) -> PersistentFailureCutIdentity {
    PersistentFailureCutIdentity::new(
        quarantine.home_id(),
        quarantine.home_generation(),
        quarantine.service_generation(),
        quarantine.failure_generation(),
    )
}

impl PersistentFailurePendingProjectionQuarantine {
    pub(in crate::cas_projection) fn arm_extra_connection_owner_for_adoption_test(
        &self,
        secondary: Self,
    ) -> AdversarialQuarantineTopologyArmForTest {
        arm_adversarial_quarantine_topology_for_test(
            quarantine_cut(self),
            AdversarialQuarantineTopologyForTest::ExtraConnectionOwner(secondary),
        )
    }

    pub(in crate::cas_projection) fn arm_retired_connection_owner_for_adoption_test(
        &self,
    ) -> AdversarialQuarantineTopologyArmForTest {
        arm_adversarial_quarantine_topology_for_test(
            quarantine_cut(self),
            AdversarialQuarantineTopologyForTest::RetiredConnectionOwner,
        )
    }

    pub(in crate::cas_projection) fn arm_foreign_failure_cut_owner_for_adoption_test(
        &self,
        secondary: Self,
    ) -> AdversarialQuarantineTopologyArmForTest {
        arm_adversarial_quarantine_topology_for_test(
            quarantine_cut(self),
            AdversarialQuarantineTopologyForTest::ForeignFailureCutOwner(secondary),
        )
    }
}

impl AdversarialQuarantineTopologyArmForTest {
    pub(in crate::cas_projection) fn was_consumed(&self) -> bool {
        !armed_topologies()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains_key(&self.cut)
    }
}

impl Drop for AdversarialQuarantineTopologyArmForTest {
    fn drop(&mut self) {
        armed_topologies()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&self.cut);
    }
}
