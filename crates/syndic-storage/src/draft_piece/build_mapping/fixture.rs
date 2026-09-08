use super::super::mutation::advance_budget::BuildAcquisition;
use super::{DraftPieceBuildMappingFamily, MappingContext, codec, model::*};
use crate::{DraftPieceBuildWorkV1, DraftPiecePrepareErrorV1, SyndicStorage, codec::Family};
use beryl_home_store::HomeStore;

#[derive(Clone, Debug)]
pub struct MappingWorkForTest {
    pub acquired: usize,
    pub emitted: usize,
    pub emission_bytes: u64,
    pub occupancy: Vec<(u8, usize)>,
    pub work: DraftPieceBuildWorkV1,
}

pub struct BuildMappingForTest {
    owner: [u8; 48],
    ordinal: u64,
    root: MapRoot,
}

impl BuildMappingForTest {
    pub fn new(source: u128) -> Self {
        Self {
            owner: [71; 48],
            ordinal: 1,
            root: MapRoot::initial(Measure {
                source,
                target: source,
            })
            .unwrap(),
        }
    }
    pub fn totals(&self) -> (u128, u128) {
        let m = self.root.measure();
        (m.source, m.target)
    }
    pub fn height(&self) -> u8 {
        match self.root {
            MapRoot::Stored(d) => d.height,
            _ => 0,
        }
    }
    pub fn root_bytes(&self) -> Vec<u8> {
        codec::encode_root(self.root).unwrap()
    }
    pub fn set_root_bytes(&mut self, bytes: &[u8]) -> bool {
        match codec::decode_root(bytes) {
            Ok(root) => {
                self.root = root;
                true
            }
            Err(_) => false,
        }
    }
    pub fn set_ordinal(&mut self, ordinal: u64) {
        self.ordinal = ordinal;
    }
    pub fn set_owner(&mut self, owner: [u8; 48]) {
        self.owner = owner;
    }
    pub fn authenticate(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        source: u128,
        target: u128,
    ) -> bool {
        let acquisition = BuildAcquisition::new(storage, store);
        MappingContext::new(&acquisition, self.owner, &mut self.ordinal)
            .authenticate(self.root, Measure { source, target })
            .is_ok()
    }
    pub fn source_cut(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        source: u128,
        copy: bool,
    ) -> Result<(u128, DraftPieceBuildWorkV1), DraftPiecePrepareErrorV1> {
        let acquisition = BuildAcquisition::new(storage, store);
        let result = MappingContext::new(&acquisition, self.owner, &mut self.ordinal)
            .source_cut(self.root, source, copy)?;
        Ok((result, acquisition.budget.work()))
    }
    pub fn insert(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        at: u128,
        length: u128,
    ) -> Result<MappingWorkForTest, DraftPiecePrepareErrorV1> {
        let acquisition = BuildAcquisition::new(storage, store);
        let mut context = MappingContext::new(&acquisition, self.owner, &mut self.ordinal);
        let root = context.insert(self.root, at, length)?;
        let work = persist(storage, store, &context);
        self.root = root;
        Ok(work)
    }
    pub fn delete_leaf(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        a: u128,
        end: u128,
    ) -> Result<(u128, MappingWorkForTest), DraftPiecePrepareErrorV1> {
        let acquisition = BuildAcquisition::new(storage, store);
        let mut context = MappingContext::new(&acquisition, self.owner, &mut self.ordinal);
        let (root, remaining) = context.delete_leaf(self.root, a, end)?;
        let work = persist(storage, store, &context);
        self.root = root;
        Ok((remaining, work))
    }
    pub fn seed_runs(&mut self, storage: &SyndicStorage, store: &HomeStore, runs: &[(u8, u128)]) {
        assert!(runs.len() <= 4096);
        let mut level = Vec::new();
        let group_count = runs.len().div_ceil(16);
        let base = runs.len() / group_count;
        let extra = runs.len() % group_count;
        let mut start = 0;
        for i in 0..group_count {
            let count = base + usize::from(i < extra);
            level.push(seed_node(
                storage,
                store,
                self.owner,
                &mut self.ordinal,
                1,
                Entries::Runs(
                    runs[start..start + count]
                        .iter()
                        .map(|(tag, n)| match tag {
                            0 => Run::Copy(*n),
                            1 => Run::Deleted(*n),
                            2 => Run::Inserted(*n),
                            _ => panic!("tag"),
                        })
                        .collect(),
                ),
            ));
            start += count;
        }
        let mut height = 2;
        while level.len() > 1 {
            let groups = level.len().div_ceil(16);
            let base = level.len() / groups;
            let extra = level.len() % groups;
            let mut next = Vec::new();
            let mut start = 0;
            for i in 0..groups {
                let count = base + usize::from(i < extra);
                next.push(seed_node(
                    storage,
                    store,
                    self.owner,
                    &mut self.ordinal,
                    height,
                    Entries::Children(level[start..start + count].to_vec()),
                ));
                start += count;
            }
            level = next;
            height += 1;
        }
        self.root = MapRoot::Stored(level[0]);
    }
    pub fn seed_tall_inserted(&mut self, storage: &SyndicStorage, store: &HomeStore, height: u8) {
        assert!((2..=22).contains(&height));
        let mut pair = [0, 1].map(|_| {
            seed_node(
                storage,
                store,
                self.owner,
                &mut self.ordinal,
                1,
                Entries::Runs(vec![Run::Inserted(1); 8]),
            )
        });
        for h in 2..height {
            pair = [0, 1].map(|_| {
                seed_node(
                    storage,
                    store,
                    self.owner,
                    &mut self.ordinal,
                    h,
                    Entries::Children((0..8).map(|i| pair[i % 2]).collect()),
                )
            });
        }
        self.root = MapRoot::Stored(seed_node(
            storage,
            store,
            self.owner,
            &mut self.ordinal,
            height,
            Entries::Children(pair.to_vec()),
        ));
    }
    pub fn root_node_bytes(&self, storage: &SyndicStorage, store: &HomeStore) -> Vec<u8> {
        let MapRoot::Stored(d) = self.root else {
            panic!("stored root")
        };
        let mut key = [0; 64];
        key[..48].copy_from_slice(&self.owner);
        key[48..].copy_from_slice(&d.id);
        let node = BuildAcquisition::new(storage, store)
            .point::<DraftPieceBuildMappingFamily>(key)
            .unwrap()
            .unwrap();
        DraftPieceBuildMappingFamily::encode_value(&node).unwrap()
    }
    pub fn substitute_value_identity(&self, storage: &SyndicStorage, store: &HomeStore) {
        let bytes = self.root_node_bytes(storage, store);
        let mut node = DraftPieceBuildMappingFamily::decode_value(&bytes).unwrap();
        let key = node.key;
        node.key[63] ^= 1;
        crate::test_faults::put_mapping_fixture_record::<DraftPieceBuildMappingFamily>(
            storage, store, &key, &node,
        );
    }
    pub fn exhausted_shared_allowance_rejects(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
    ) -> bool {
        let acquisition = BuildAcquisition::new(storage, store);
        acquisition.budget.encoded_effect(4_194_304, false).unwrap();
        let shared = acquisition.clone();
        let mut context = MappingContext::new(&shared, self.owner, &mut self.ordinal);
        context.insert(self.root, 0, 1).is_err() && context.emitted.is_empty()
    }
}

fn seed_node(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: [u8; 48],
    ordinal: &mut u64,
    height: u8,
    entries: Entries,
) -> Descriptor {
    let digest = codec::node_digest(&owner, height, &entries);
    let id = codec::record_id(&owner, *ordinal, &digest);
    *ordinal += 1;
    let mut key = [0; 64];
    key[..48].copy_from_slice(&owner);
    key[48..].copy_from_slice(&id);
    let node = Node {
        key,
        height,
        entries,
        digest,
    };
    crate::test_faults::put_mapping_fixture_record::<DraftPieceBuildMappingFamily>(
        storage, store, &key, &node,
    );
    node.descriptor().unwrap()
}

fn persist(
    storage: &SyndicStorage,
    store: &HomeStore,
    context: &MappingContext<'_, '_>,
) -> MappingWorkForTest {
    let mut emission_bytes = 0;
    for node in &context.emitted {
        emission_bytes += (64
            + DraftPieceBuildMappingFamily::encode_value(node)
                .unwrap()
                .len()) as u64;
        crate::test_faults::put_mapping_fixture_record::<DraftPieceBuildMappingFamily>(
            storage, store, &node.key, node,
        );
    }
    MappingWorkForTest {
        acquired: context.acquired,
        emitted: context.emitted.len(),
        emission_bytes,
        occupancy: context
            .emitted
            .iter()
            .map(|node| (node.height, node.entries.len()))
            .collect(),
        work: context.acquisition.budget.work(),
    }
}

pub fn mapping_node_codec_roundtrip(bytes: &[u8]) -> Option<Vec<u8>> {
    DraftPieceBuildMappingFamily::decode_value(bytes)
        .ok()
        .and_then(|node| DraftPieceBuildMappingFamily::encode_value(&node).ok())
}
pub fn mapping_root_codec_roundtrip(bytes: &[u8]) -> Option<Vec<u8>> {
    codec::decode_root(bytes)
        .ok()
        .and_then(|root| codec::encode_root(root).ok())
}
