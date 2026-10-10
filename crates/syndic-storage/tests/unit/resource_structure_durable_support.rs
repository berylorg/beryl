use super::*;
use crate::codec::{
    ContentByteSpanKey, ContentByteSpansCodec, ContentChunkKey, ContentChunksCodec,
    ContentManifestsCodec, ContentTextSpanKey, ContentTextSpansCodec, ResourcesCodec,
};
use crate::domain::SyndicDomain;
use crate::resource_structure::{
    advance::AdvanceCode,
    construction::{AdmitBuild, ConstructionError, initial_build},
    table_advance::AdvanceTable,
    transition::ControlTransition,
};
use beryl_home_store::{
    CommandOutcome, DomainHandle, DomainMutation, DomainReader, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, MutationBuilder, PointReadLimit, ReconciliationReservation,
    ReconciliationResolution, RecordCodec,
    test_faults::{FaultController, FaultPoint},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct TestHome(pub(super) PathBuf);

impl TestHome {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.aipm/tasks/structure-durability");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.aipm/tasks/structure-durability")
            .canonicalize()
            .unwrap();
        assert!(self.0.starts_with(root) && self.0.is_dir());
        assert!(
            std::process::Command::new("cleanup-dir.exe")
                .arg("-rf")
                .arg(&self.0)
                .status()
                .unwrap()
                .success()
        );
    }
}

pub(super) struct Seed {
    pub(super) binding: Binding,
    pub(super) bytes: Vec<u8>,
}

impl DomainMutation<SyndicDomain> for Seed {
    type Error = ConstructionError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ResourcesCodec>(1)?;
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        reservation.reserve_records::<ContentChunksCodec>(1)?;
        reservation.reserve_records::<ContentByteSpansCodec>(1)?;
        reservation.reserve_records::<ContentTextSpansCodec>(1)?;
        Ok(())
    }

    fn contribute(
        seed: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let ProjectionTextSource::Composer(content) = seed.binding.source else {
            panic!()
        };
        let mut bytes = vec![b'x'; content.summary().logical_utf8_bytes() as usize];
        bytes[seed.binding.range.start as usize..seed.binding.range.end as usize]
            .copy_from_slice(&seed.bytes);
        let ordinal = crate::ContentChunkOrdinal::new(1).unwrap();
        let chunk =
            crate::ContentChunkRecord::new(content.id(), ordinal, bytes.as_slice()).unwrap();
        let manifest = crate::ContentManifestRecord::new(
            content.id(),
            content.revision(),
            content.encoding(),
            crate::ContentLifecycle::Sealed,
            1,
            bytes.len() as u64,
            content.summary().digest(),
            content.summary(),
        );
        mutations.put::<ResourcesCodec>(&seed.binding.resource.id, &seed.binding.metadata)?;
        mutations.put::<ContentManifestsCodec>(&content.id(), &manifest)?;
        mutations.put::<ContentChunksCodec>(
            &ContentChunkKey {
                owner: content.id(),
                ordinal,
            },
            &chunk,
        )?;
        mutations.put::<ContentByteSpansCodec>(
            &ContentByteSpanKey {
                owner: content.id(),
                start: 0,
            },
            &crate::ContentByteSpanRecord::for_chunk(&chunk, 0).unwrap(),
        )?;
        mutations.put::<ContentTextSpansCodec>(
            &ContentTextSpanKey {
                owner: content.id(),
                logical_start: 0,
            },
            &crate::ContentTextSpanRecord::new(
                content.id(),
                crate::ContentPieceOrdinal::new(1).unwrap(),
                ordinal,
                0,
                0,
                bytes.len() as u64,
                0,
                bytes.len() as u64,
                false,
                [0; 32],
            )
            .unwrap(),
        )?;
        Ok(())
    }
}

pub(super) fn open(
    home: &TestHome,
    faults: &FaultController,
) -> (HomeStore, DomainHandle<SyndicDomain>) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let handle = candidate.register_domain::<SyndicDomain>().unwrap();
    let store = candidate
        .prepare_publication(crate::SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    (store, handle)
}

pub(super) fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

pub(super) fn read<R: RecordCodec<SyndicDomain>>(
    store: &HomeStore,
    handle: &DomainHandle<SyndicDomain>,
    key: &R::Key,
) -> Option<R::Value> {
    store
        .read_point::<SyndicDomain, R>(handle, key, PointReadLimit::new(65_664).unwrap())
        .unwrap()
}

pub(super) fn reconcile(outcome: CommandOutcome, store: &HomeStore) {
    let CommandOutcome::Indeterminate { reconciliation, .. } = outcome else {
        panic!("{outcome:?}")
    };
    let generation = store.health().generation().unwrap();
    assert!(store.try_elect_coherent(generation, || ()).is_err());
    let custody = reconciliation.install_and_handle();
    assert_eq!(store.pending_reconciliations().len(), 1);
    assert!(matches!(
        store.reconcile(&custody).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(store.try_elect_coherent(generation, || ()), Ok(()));
}
