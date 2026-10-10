use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ManualProjectionStamp {
    key: LoadedThreadKey,
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedThreadGeneration,
    binding: beryl_model::BindingRevision,
}

impl ManualProjectionStamp {
    pub(in crate::cas_projection) fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        CasLoadedSessionGeneration::new(self.key.process_generation, self.generation)
    }

    pub(in crate::cas_projection) fn is_current(&self) -> bool {
        let Ok(state) = registry().try_lock() else {
            return false;
        };
        state.entries.get(&self.key).is_some_and(|entry| {
            entry.connection == self.connection
                && entry.owner == self.owner
                && entry.generation == self.generation
                && entry.context_binding == Some(self.binding)
                && !entry.leases.is_empty()
        })
    }
}

pub(in crate::cas_projection) fn read_manual_projection(
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    cas_thread: &beryl_model::CasThreadId,
    binding: beryl_model::BindingRevision,
) -> Option<ManualProjectionStamp> {
    let state = lock().ok()?;
    let mut matching = state.entries.iter().filter(|(key, entry)| {
        entry.connection == connection
            && entry.owner == owner
            && &key.cas_thread_id == cas_thread
            && entry.context_binding == Some(binding)
            && !entry.leases.is_empty()
    });
    let (key, entry) = matching.next()?;
    let stamp = ManualProjectionStamp {
        key: key.clone(),
        connection,
        owner,
        generation: entry.generation,
        binding,
    };
    matching.next().is_none().then_some(stamp)
}
