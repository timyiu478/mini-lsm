// Copyright (c) 2022-2025 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use tempfile::tempdir;

use crate::{
    compact::{
        CompactionOptions, TieredCompactionController, TieredCompactionOptions,
        TieredCompactionTask,
    },
    lsm_storage::{LsmStorageOptions, MiniLsm},
};

use super::harness::{check_compaction_ratio, compaction_bench};

#[test]
fn test_integration() {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Tiered(
            TieredCompactionOptions {
                num_tiers: 3,
                max_size_amplification_percent: 200,
                size_ratio: 1,
                min_merge_width: 2,
                max_merge_width: None,
            },
        )),
    )
    .unwrap();

    compaction_bench(storage.clone());
    check_compaction_ratio(storage.clone());
}

#[test]
fn test_reduce_sorted_runs_respects_max_merge_width() {
    let options = TieredCompactionOptions {
        num_tiers: 4,
        max_size_amplification_percent: 10_000,
        size_ratio: 10_000,
        min_merge_width: 2,
        max_merge_width: Some(2),
    };
    let controller = TieredCompactionController::new(options.clone());
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Tiered(options)),
    )
    .unwrap();
    let mut snapshot = storage.inner.state.read().as_ref().clone();
    snapshot.levels = vec![(4, vec![4]), (3, vec![3]), (2, vec![2]), (1, vec![1])];

    let task = controller.generate_compaction_task(&snapshot).unwrap();
    assert_eq!(task.tiers, snapshot.levels[..2]);
    assert!(!task.bottom_tier_included);
}

#[test]
fn test_tiered_compaction_accepts_empty_output() {
    let options = TieredCompactionOptions {
        num_tiers: 2,
        max_size_amplification_percent: 200,
        size_ratio: 1,
        min_merge_width: 2,
        max_merge_width: None,
    };
    let controller = TieredCompactionController::new(options.clone());
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Tiered(options)),
    )
    .unwrap();
    let mut snapshot = storage.inner.state.read().as_ref().clone();
    snapshot.levels = vec![(2, vec![2]), (1, vec![1])];
    let task = TieredCompactionTask {
        tiers: snapshot.levels.clone(),
        bottom_tier_included: true,
    };

    let (result, removed) = controller.apply_compaction_result(&snapshot, &task, &[]);
    assert!(result.levels.is_empty());
    assert_eq!(removed, vec![2, 1]);
}

#[test]
#[should_panic(expected = "should not add l0 ssts in tiered compaction")]
fn test_tiered_compaction_rejects_l0_ssts() {
    let options = TieredCompactionOptions {
        num_tiers: 2,
        max_size_amplification_percent: 200,
        size_ratio: 1,
        min_merge_width: 2,
        max_merge_width: None,
    };
    let controller = TieredCompactionController::new(options.clone());
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Tiered(options)),
    )
    .unwrap();
    let mut snapshot = storage.inner.state.read().as_ref().clone();
    snapshot.l0_sstables.push(1);

    controller.generate_compaction_task(&snapshot);
}

#[test]
fn test_tiered_compaction_apply_result_middle_tiers() {
    let options = TieredCompactionOptions {
        num_tiers: 5,
        max_size_amplification_percent: 200,
        size_ratio: 1,
        min_merge_width: 2,
        max_merge_width: None,
    };
    let controller = TieredCompactionController::new(options.clone());
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Tiered(options)),
    )
    .unwrap();
    let mut snapshot = storage.inner.state.read().as_ref().clone();

    // Simulate 5 existing tiers, from newest (5) to oldest (1)
    snapshot.levels = vec![
        (5, vec![5]), 
        (4, vec![4]), 
        (3, vec![3]), 
        (2, vec![2]), 
        (1, vec![1])
    ];

    // Simulate a compaction task that merges middle tiers 4, 3, and 2.
    let task = TieredCompactionTask {
        tiers: vec![(4, vec![4]), (3, vec![3]), (2, vec![2])],
        bottom_tier_included: false,
    };

    // Apply the result, assuming the compaction engine outputted a new tier with SST ID 6
    let (new_state, mut removed) = controller.apply_compaction_result(&snapshot, &task, &[6]);

    // Rigorous State Transition Checks:
    // 1. The new tier (6) MUST replace the exact slice of compacted tiers.
    // 2. The newer tier (5) MUST remain above.
    // 3. The older tier (1) MUST remain below.
    assert_eq!(
        new_state.levels.len(), 
        3, 
        "State should have exactly 3 tiers left"
    );
    assert_eq!(
        new_state.levels[0], 
        (5, vec![5]), 
        "Tier 5 should remain unaffected at the top"
    );
    assert_eq!(
        new_state.levels[1], 
        (6, vec![6]), 
        "New tier 6 should be spliced exactly into the middle"
    );
    assert_eq!(
        new_state.levels[2], 
        (1, vec![1]), 
        "Tier 1 should remain unaffected at the bottom"
    );

    // 4. Ensure the correct SSTs are returned for cleanup
    removed.sort();
    assert_eq!(
        removed, 
        vec![2, 3, 4], 
        "SSTs 2, 3, and 4 should be returned for removal"
    );
}
