// Copyright (c) 2022-2026 Alex Chi Z
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

use serde::{Deserialize, Serialize};

use crate::lsm_storage::LsmStorageState;

#[derive(Debug, Serialize, Deserialize)]
pub struct TieredCompactionTask {
    pub tiers: Vec<(usize, Vec<usize>)>,
    pub bottom_tier_included: bool,
}

#[derive(Debug, Clone)]
pub struct TieredCompactionOptions {
    pub num_tiers: usize,
    pub max_size_amplification_percent: usize,
    pub size_ratio: usize,
    pub min_merge_width: usize,
    pub max_merge_width: Option<usize>,
}

pub struct TieredCompactionController {
    options: TieredCompactionOptions,
}

impl TieredCompactionController {
    pub fn new(options: TieredCompactionOptions) -> Self {
        Self { options }
    }

    pub fn generate_compaction_task(
        &self,
        _snapshot: &LsmStorageState,
    ) -> Option<TieredCompactionTask> {
        assert!(
            _snapshot.l0_sstables.is_empty(),
            "should not add l0 ssts in tiered compaction"
        );

        let levels = &_snapshot.levels;

        // 1. Guard: Check if we have enough tiers to trigger compaction
        if levels.len() < self.options.num_tiers {
            return None;
        }

        // 2. Space Amplification Ratio Trigger
        // Safe from panics: map extracts the size, unwrap_or handles empty states gracefully
        let bottom_tier_size = levels.last().map(|(_, ids)| ids.len()).unwrap_or(0);
        let total_size: usize = levels.iter().map(|(_, ids)| ids.len()).sum();
        let upper_tier_size = total_size.saturating_sub(bottom_tier_size);

        if upper_tier_size * 100 >= bottom_tier_size * self.options.max_size_amplification_percent {
            return Some(TieredCompactionTask {
                tiers: levels.clone(),
                bottom_tier_included: true,
            });
        }

        // 3. Size Ratio Trigger
        let threshold = (100.0 + self.options.size_ratio as f64) / 100.0;
        let mut running_sum = levels.first().map(|(_, ids)| ids.len()).unwrap_or(0);

        // Destructure cleanly: `i` acts as our exact prefix length, `ids` gives us the SSTs
        for (i, (_, ids)) in levels.iter().enumerate().skip(1) {
            let current_tier_size = ids.len();
            let prefix_len = i;

            let ratio = current_tier_size as f64 / running_sum as f64;

            if ratio > threshold && prefix_len >= self.options.min_merge_width {
                return Some(TieredCompactionTask {
                    tiers: levels[..prefix_len].to_vec(),
                    bottom_tier_included: false,
                });
            }

            running_sum += current_tier_size;
        }

        // 4. Reduce Sorted Runs Trigger
        let max_merge_width = self.options.max_merge_width.unwrap_or(levels.len());
        let num_to_merge = levels.len().min(max_merge_width);

        Some(TieredCompactionTask {
            tiers: levels[..num_to_merge].to_vec(),
            bottom_tier_included: num_to_merge == levels.len(),
        })
    }

    pub fn apply_compaction_result(
        &self,
        _snapshot: &LsmStorageState,
        _task: &TieredCompactionTask,
        _output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot = _snapshot.clone();

        let task_tier_ids: std::collections::HashSet<usize> =
            _task.tiers.iter().map(|(id, _)| *id).collect();

        // Find the index where the compacted tiers started (where the new tier will go)
        let insert_idx = snapshot
            .levels
            .iter()
            .position(|(id, _)| task_tier_ids.contains(id))
            .unwrap_or(0);

        snapshot
            .levels
            .retain(|(id, _)| !task_tier_ids.contains(id));

        // Insert the new tier (using the first output SST ID as the Tier ID)
        if let Some(&first_sst_id) = _output.first() {
            snapshot
                .levels
                .insert(insert_idx, (first_sst_id, _output.to_vec()));
        }

        let obsolete_ssts = _task
            .tiers
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();

        (snapshot, obsolete_ssts)
    }
}
