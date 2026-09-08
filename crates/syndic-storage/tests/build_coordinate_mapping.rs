#![cfg(feature = "test-faults")]

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use syndic_storage::{
    SyndicStorage,
    test_faults::{
        BuildMappingForTest, MappingWorkForTest, mapping_node_codec_roundtrip,
        mapping_root_codec_roundtrip,
    },
};

#[path = "build_coordinate_mapping/codec.rs"]
mod codec;

struct Home(std::path::PathBuf);
impl Home {
    fn open(name: &str) -> (Self, HomeStore, SyndicStorage) {
        let path =
            std::env::temp_dir().join(format!("beryl-build-mapping-{name}-{}", std::process::id()));
        assert!(!path.exists());
        std::fs::create_dir_all(&path).unwrap();
        let mut store =
            HomeStore::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT)).unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        (Self(path), store, storage)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let resolved = self.0.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("beryl-build-mapping-")
        );
        assert!(
            std::process::Command::new("cleanup-dir.exe")
                .args(["-rf"])
                .arg(&resolved)
                .status()
                .unwrap()
                .success()
        );
    }
}

fn bounded(work: &MappingWorkForTest, insertion: bool) {
    assert!(work.acquired <= if insertion { 22 } else { 43 });
    assert!(work.emitted <= 45);
    assert!(work.work.stored_structure_records() <= (work.acquired + work.emitted) as u64);
    assert!(work.work.point_attempts() <= (work.acquired + work.emitted) as u64);
    assert!(work.work.encoded_bytes() >= work.emission_bytes + 64 * work.emitted as u64);
    for (height, count) in &work.occupancy {
        assert!((1..=22).contains(height));
        assert!((1..=16).contains(count));
    }
}

struct Oracle {
    cuts: Vec<u128>,
    target: Vec<Option<usize>>,
}
impl Oracle {
    fn new(n: usize) -> Self {
        Self {
            cuts: (0..=n as u128).collect(),
            target: (0..n).map(Some).collect(),
        }
    }
    fn insert(&mut self, at: usize, n: usize) {
        for cut in &mut self.cuts {
            if *cut >= at as u128 {
                *cut += n as u128;
            }
        }
        self.target.splice(at..at, vec![None; n]);
    }
    fn delete(&mut self, a: usize, end: usize) {
        for cut in &mut self.cuts {
            *cut = if *cut <= a as u128 {
                *cut
            } else if *cut < end as u128 {
                a as u128
            } else {
                *cut - (end - a) as u128
            };
        }
        self.target.drain(a..end);
    }
    fn compare(&self, map: &mut BuildMappingForTest, storage: &SyndicStorage, store: &HomeStore) {
        assert_eq!(
            map.totals(),
            ((self.cuts.len() - 1) as u128, self.target.len() as u128)
        );
        for (source, expected) in self.cuts.iter().enumerate() {
            assert_eq!(
                map.source_cut(storage, store, source as u128, false)
                    .unwrap()
                    .0,
                *expected,
                "source cut {source}"
            );
            let copy = map
                .source_cut(storage, store, source as u128, true)
                .ok()
                .map(|x| x.0);
            let original = self
                .target
                .iter()
                .position(|unit| *unit == Some(source))
                .map(|x| x as u128);
            assert_eq!(copy, original, "original unit {source}");
        }
    }
}

#[test]
fn independent_composition_and_original_unit_oracle() {
    let (_home, store, storage) = Home::open("oracle");
    let mut map = BuildMappingForTest::new(32);
    let mut oracle = Oracle::new(32);
    let mut random = 831u64;
    for step in 0..160 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let at = (random >> 24) as usize % (oracle.target.len() + 1);
        if step % 3 != 0 || at == oracle.target.len() {
            let n = 1 + (random >> 48) as usize % 3;
            bounded(
                &map.insert(&storage, &store, at as u128, n as u128).unwrap(),
                true,
            );
            oracle.insert(at, n);
        } else {
            let end = (at + 1 + (random >> 48) as usize % 15).min(oracle.target.len());
            let mut remaining = end as u128;
            while remaining > at as u128 {
                let (next, work) = map
                    .delete_leaf(&storage, &store, at as u128, remaining)
                    .unwrap();
                assert!(next < remaining);
                bounded(&work, false);
                remaining = next;
            }
            oracle.delete(at, end);
        }
        oracle.compare(&mut map, &storage, &store);
    }
}

#[test]
fn insertion_precedes_deleted_plateau_and_never_restores_copy_authority() {
    let (_home, store, storage) = Home::open("plateau");
    let mut map = BuildMappingForTest::new(0);
    let mut runs = vec![(0, 1); 16];
    runs.extend(vec![(1, 1); 48]);
    runs.extend(vec![(0, 1); 16]);
    map.seed_runs(&storage, &store, &runs);
    bounded(&map.insert(&storage, &store, 16, 5).unwrap(), true);
    for x in 16..=64 {
        assert_eq!(map.source_cut(&storage, &store, x, false).unwrap().0, 21);
    }
    for x in 16..64 {
        assert!(map.source_cut(&storage, &store, x, true).is_err());
    }
    assert_eq!(map.source_cut(&storage, &store, 64, true).unwrap().0, 21);
    let (next, work) = map.delete_leaf(&storage, &store, 16, 21).unwrap();
    assert_eq!(next, 16);
    bounded(&work, false);
    for x in 16..=64 {
        assert_eq!(map.source_cut(&storage, &store, x, false).unwrap().0, 16);
    }
}

#[test]
fn repeated_insertions_split_and_multileaf_deletion_collapses_to_empty() {
    let (_home, store, storage) = Home::open("normalization");
    let mut map = BuildMappingForTest::new(0);
    let mut split = false;
    for n in 0..300 {
        let work = map.insert(&storage, &store, n, 1).unwrap();
        split |= work.emitted >= 3;
        bounded(&work, true);
    }
    assert!(split);
    assert!(map.height() >= 3);
    let mut end = 300;
    let mut steps = 0;
    while end != 0 {
        let (next, work) = map.delete_leaf(&storage, &store, 0, end).unwrap();
        assert!(next < end);
        bounded(&work, false);
        end = next;
        steps += 1;
    }
    assert!(steps > 16);
    assert_eq!(map.root_bytes(), vec![0]);
    assert_eq!(map.totals(), (0, 0));
}

#[test]
fn deletion_redistributes_with_preferred_left_sibling() {
    let (_home, store, storage) = Home::open("redistribution");
    let mut map = BuildMappingForTest::new(0);
    map.seed_runs(&storage, &store, &vec![(2, 1); 32]);
    let (next, work) = map.delete_leaf(&storage, &store, 16, 26).unwrap();
    assert_eq!(next, 16);
    bounded(&work, false);
    assert_eq!(work.occupancy, vec![(1, 11), (1, 11), (2, 2)]);
    assert_eq!(map.totals(), (0, 22));
}

#[test]
fn partial_copy_deletion_produces_eighteen_runs_and_splits_the_root() {
    let (_home, store, storage) = Home::open("partial-copy");
    let mut map = BuildMappingForTest::new(0);
    map.seed_runs(&storage, &store, &vec![(0, 10); 16]);
    let (next, work) = map.delete_leaf(&storage, &store, 1, 159).unwrap();
    assert_eq!(next, 1);
    bounded(&work, false);
    assert_eq!(work.occupancy, vec![(1, 9), (1, 9), (2, 2)]);
    assert_eq!(map.totals(), (160, 2));
    assert_eq!(map.source_cut(&storage, &store, 0, true).unwrap().0, 0);
    for x in 1..159 {
        assert_eq!(map.source_cut(&storage, &store, x, false).unwrap().0, 1);
        assert!(map.source_cut(&storage, &store, x, true).is_err());
    }
    assert_eq!(map.source_cut(&storage, &store, 159, true).unwrap().0, 1);
}

#[test]
fn occupied_keys_and_key_value_substitution_fail_closed() {
    let (_home, store, storage) = Home::open("custody");
    let mut map = BuildMappingForTest::new(0);
    map.insert(&storage, &store, 0, 1).unwrap();
    let root = map.root_bytes();
    assert!(map.set_root_bytes(&[0]));
    map.set_ordinal(1);
    assert!(map.insert(&storage, &store, 0, 1).is_err());
    assert_eq!(map.root_bytes(), vec![0]);
    assert!(map.set_root_bytes(&root));
    map.substitute_value_identity(&storage, &store);
    assert!(!map.authenticate(&storage, &store, 0, 1));
    assert!(map.source_cut(&storage, &store, 0, false).is_err());
}

#[test]
fn maximum_height_complete_shared_closure_charges_actual_reads_and_probes() {
    let (_home, store, storage) = Home::open("height");
    let mut map = BuildMappingForTest::new(0);
    map.seed_tall_inserted(&storage, &store, 22);
    assert_eq!(map.totals(), (0, 1u128 << 64));
    assert!(map.authenticate(&storage, &store, 0, 1u128 << 64));
    let (next, work) = map.delete_leaf(&storage, &store, 0, 8).unwrap();
    assert_eq!(next, 0);
    bounded(&work, false);
    assert_eq!(work.acquired, 43);
    assert_eq!(work.emitted, 21);
    assert_eq!(work.work.stored_structure_records(), 64);
    assert_eq!(work.work.point_attempts(), 64);
    assert_eq!(work.emission_bytes, 27_685);
    assert_eq!(work.work.encoded_bytes(), 62_328);
    assert_eq!(work.work.peak_encoded_bytes(), 62_344);
    assert_eq!(map.height(), 21);
    eprintln!("mapping height 22 deletion: {work:?}");
    assert_eq!(map.totals(), (0, (1u128 << 64) - 8));
}

#[test]
fn huge_identity_extents_and_shared_allowance_reject_before_emission() {
    let (_home, store, storage) = Home::open("huge");
    let max = 2 * u64::MAX as u128;
    let mut map = BuildMappingForTest::new(max);
    assert!(map.authenticate(&storage, &store, max, max));
    assert_eq!(
        map.source_cut(&storage, &store, max, false)
            .unwrap()
            .1
            .point_attempts(),
        0
    );
    assert!(map.insert(&storage, &store, 0, 1).is_err());
    let (next, work) = map.delete_leaf(&storage, &store, max - 1, max).unwrap();
    assert_eq!(next, max - 1);
    bounded(&work, false);
    assert_eq!(work.work.encoded_bytes(), work.emission_bytes + 64);
    assert_eq!(work.work.point_attempts(), 1);
    assert!(map.source_cut(&storage, &store, max - 1, true).is_err());
    assert!(map.exhausted_shared_allowance_rejects(&storage, &store));
    map.set_ordinal(u64::MAX);
    assert!(map.insert(&storage, &store, 0, 1).is_err());
}
