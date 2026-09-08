#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

use sha2::{Digest, Sha256};
use std::num::NonZeroU64;
use syndic_storage::test_faults::{
    draft_marker_program_snapshot_for_test, marker_removal_bounds_fixture,
};
use syndic_storage::*;

#[path = "marker_continuation_bounds/inventory.rs"]
mod inventory;
#[path = "marker_continuation_bounds/publishing.rs"]
mod publishing;
#[path = "draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;
#[path = "draft_marker_writer_admission/support.rs"]
mod support;

use readiness_support::owner;

#[test]
fn complete_tree_locator_skips_a_text_endpoint_before_later_anchor_marker() {
    let (_home, store, storage, thread) = fixture("marker-locator-endpoint", 93);
    let result = syndic_storage::test_faults::marker_locator_bounds_fixture(
        &storage,
        &store,
        current(&storage, &store, thread).draft().id(),
        marker(94, 0, 1),
    );
    assert_eq!(result.exact_rank_and_ordinal, Some((2, 1)));
    assert_eq!(result.insertion_rank_and_ordinal, Some((2, 1)));
    assert_eq!(result.removed_population, Some(3));
    assert_eq!(result.inserted_population, Some(5));
    for work in result.work {
        inventory::ceilings()[4].assert_contains(work);
    }
}

#[test]
fn every_marker_structure_constructor_reserves_before_retaining_output() {
    let (_home, store, storage, thread) = fixture("marker-constructor-reservation", 91);
    assert_eq!(
        syndic_storage::test_faults::marker_constructor_reservation_fixture(
            &storage,
            &store,
            current(&storage, &store, thread).draft().id(),
            marker(92, 0, 1)
        ),
        [true; 6]
    );
}

#[test]
fn authenticated_sparse_marker_paths_measure_height_forty_five_and_u64_edge() {
    for height in [45, 63, 64] {
        let (_home, store, storage, thread) = fixture("marker-sparse-bounds", height);
        let draft = current(&storage, &store, thread).draft().id();
        let result = marker_removal_bounds_fixture(
            &storage,
            &store,
            draft,
            marker(7, 0, 1),
            height,
            2,
            false,
        );
        assert!(
            !result.complete_closure,
            "opaque descendants are not complete-tree evidence"
        );
        let branching_height = height.min(63);
        assert_eq!(result.input_population, 1_u64 << branching_height);
        assert_eq!(result.output_populations, [result.input_population - 1; 3]);
        assert_eq!(
            result.acquired,
            [u64::from(2 * branching_height + u8::from(height == 64)); 3]
        );
        assert_eq!(result.output_heights, [branching_height - 1; 3]);
        for index in 0..3 {
            let work = result.work[index];
            assert_eq!(work.point_attempts(), result.acquired[index]);
            assert_eq!(
                work.stored_structure_records(),
                result.acquired[index] + result.emitted[index] as u64
            );
            assert!(
                result.emitted_internal_children[index]
                    .iter()
                    .all(|n| (2..=128).contains(n))
            );
            assert!(work.stored_structure_records() <= 193);
            assert!(work.encoded_bytes() <= 3_126_650);
            assert!(work.peak_encoded_bytes() <= 4_194_304);
        }
        if height == 45 {
            assert_eq!(result.acquired.iter().sum::<u64>(), 270);
        }
    }
}

#[test]
fn complete_marker_trees_normalize_merge_redistribution_and_leaf_root_retention() {
    for (height, fanout) in [(1, 2), (2, 2), (2, 128)] {
        let (_home, store, storage, thread) = fixture("marker-complete-bounds", fanout as u8);
        let result = marker_removal_bounds_fixture(
            &storage,
            &store,
            current(&storage, &store, thread).draft().id(),
            marker(8, 0, 1),
            height,
            fanout,
            true,
        );
        assert!(result.complete_closure);
        assert_eq!(result.output_populations, [result.input_population - 1; 3]);
        for index in 0..3 {
            assert!(
                result.emitted_internal_children[index]
                    .iter()
                    .all(|n| (2..=128).contains(n) || (height == 1 && *n == 1))
            );
            assert_eq!(
                result.work[index].stored_structure_records(),
                result.acquired[index] + result.emitted[index] as u64
            );
        }
        if height == 1 {
            assert_eq!(result.output_heights, [0, 1, 1]);
            assert_eq!(result.emitted, [0, 1, 1]);
        } else if fanout == 2 {
            assert_eq!(result.output_heights, [1; 3]);
            assert_eq!(result.emitted, [1; 3]);
        } else {
            assert_eq!(result.output_heights, [2; 3]);
            assert_eq!(result.measured_max_internal_bytes, [23_913, 11_372, 8_299]);
            for mut widths in result.emitted_internal_children {
                widths.sort_unstable();
                assert_eq!(widths, [2, 64, 65]);
            }
        }
    }
}
