use beryl_home_store::DomainReader;

use crate::{BindingState, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::invariant;
use crate::validation::scan::{require, scan};

pub(super) fn require_reservation(
    reader: &DomainReader<'_, SyndicDomain>,
    cas_thread: &beryl_model::CasThreadId,
    thread: beryl_model::SyndicThreadId,
    observed_revision: beryl_model::BindingRevision,
    require_retired: bool,
) -> Result<(), SyndicValidationError> {
    let index = require::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(cas_thread.clone()),
        "CAS thread reservation is missing",
    )?;
    if index.thread_id() != thread
        || index.first_binding_revision() > observed_revision
        || index.latest_binding_revision() < observed_revision
    {
        return invariant("CAS thread reservation owner or revision range disagrees");
    }
    match (require_retired, index.retired_binding_revision()) {
        (true, Some(retired)) if retired <= observed_revision => {}
        (true, _) => return invariant("stale CAS binding lacks prior retirement authority"),
        (false, Some(retired)) if retired <= observed_revision => {
            return invariant("valid or active binding revives a retired CAS thread");
        }
        (false, _) => {}
    }
    let first = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread,
            revision: index.first_binding_revision(),
        },
        "CAS thread first binding is missing",
    )?;
    if first.state().cas_thread_id() != Some(cas_thread) {
        return invariant("CAS thread reservation does not name its first binding");
    }
    Ok(())
}

pub(super) fn validate_cas_thread_reservations(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CasThreadIndexFamily>(reader, |key, index| {
        let CasThreadKey::Record(cas_thread) = key else {
            return invariant("stored CAS-thread cursor sentinel");
        };
        if cas_thread != index.cas_thread_id() {
            return invariant("CAS thread reservation key disagrees");
        }
        if index.latest_binding_revision() < index.first_binding_revision() {
            return invariant("CAS thread latest binding predates its first binding");
        }
        require::<ThreadsFamily>(
            reader,
            &index.thread_id(),
            "CAS thread reservation owner is missing",
        )?;
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: index.thread_id(),
                revision: index.first_binding_revision(),
            },
            "CAS thread first binding is missing",
        )?;
        if binding.state().cas_thread_id() != Some(cas_thread) {
            return invariant("CAS thread reservation first binding disagrees");
        }
        let latest = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: index.thread_id(),
                revision: index.latest_binding_revision(),
            },
            "CAS thread latest binding is missing",
        )?;
        if latest.state().cas_thread_id() != Some(cas_thread) {
            return invariant("CAS thread reservation latest binding disagrees");
        }
        if let Some(retired) = index.retired_binding_revision() {
            if retired != index.latest_binding_revision() {
                return invariant("CAS thread retirement is not its latest binding");
            }
            let retired_binding = require::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: index.thread_id(),
                    revision: retired,
                },
                "CAS thread retirement binding is missing",
            )?;
            let BindingState::Stale(stale) = retired_binding.state() else {
                return invariant("CAS thread retirement does not name stale provenance");
            };
            if stale.cas_thread_id() != cas_thread {
                return invariant("CAS thread retirement names different stale provenance");
            }
        }
        Ok(())
    })
}
