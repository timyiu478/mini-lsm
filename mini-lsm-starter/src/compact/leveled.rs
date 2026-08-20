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
pub struct LeveledCompactionTask {
    // if upper_level is `None`, then it is L0 compaction
    pub upper_level: Option<usize>,
    pub upper_level_sst_ids: Vec<usize>,
    pub lower_level: usize,
    pub lower_level_sst_ids: Vec<usize>,
    pub is_lower_level_bottom_level: bool,
}

#[derive(Debug, Clone)]
pub struct LeveledCompactionOptions {
    pub level_size_multiplier: usize,
    pub level0_file_num_compaction_trigger: usize,
    pub max_levels: usize,
    pub base_level_size_mb: usize,
}

pub struct LeveledCompactionController {
    options: LeveledCompactionOptions,
}

impl LeveledCompactionController {
    pub fn new(options: LeveledCompactionOptions) -> Self {
        Self { options }
    }

    fn find_overlapping_ssts(
        &self,
        _snapshot: &LsmStorageState,
        _sst_ids: &[usize],
        _in_level: usize,
    ) -> Vec<usize> {
        // Determine the bounding key range [min_key, max_key] of the source sst_ids
        let mut min_key = None;
        let mut max_key = None;

        for id in _sst_ids {
            if let Some(sst) = _snapshot.sstables.get(id) {
                let first = sst.first_key();
                let last = sst.last_key();

                min_key = Some(min_key.map_or(first, |k| std::cmp::min(k, first)));
                max_key = Some(max_key.map_or(last, |k| std::cmp::max(k, last)));
            }
        }

        let (min_key, max_key) = match (min_key, max_key) {
            (Some(min), Some(max)) => (min, max),
            _ => return Vec::new(),
        };

        // Find all SSTs in `in_level` that overlap with [min_key, max_key]
        _snapshot
            .levels
            .iter()
            .find(|(l, _)| *l == _in_level)
            .map(|(_, target_sst_ids)| {
                target_sst_ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        if let Some(sst) = _snapshot.sstables.get(id) {
                            sst.first_key() <= max_key && sst.last_key() >= min_key
                        } else {
                            false
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_ssts_size(&self, snapshot: &LsmStorageState, level: usize) -> u64 {
        if level == 0 {
            snapshot
                .l0_sstables
                .iter()
                .filter_map(|id| snapshot.sstables.get(id))
                .map(|sst| sst.table_size())
                .sum()
        } else {
            snapshot
                .levels
                .iter()
                .find(|(l, _)| *l == level)
                .map(|(_, sst_ids)| {
                    sst_ids
                        .iter()
                        .filter_map(|id| snapshot.sstables.get(id))
                        .map(|sst| sst.table_size())
                        .sum()
                })
                .unwrap_or(0)
        }
    }

    fn compute_target_sizes(&self, snapshot: &LsmStorageState) -> Vec<u64> {
        let mut real_sizes = vec![0u64; self.options.max_levels + 1]; // 1-indexed
        for i in 1..=self.options.max_levels {
            real_sizes[i] = self.get_ssts_size(snapshot, i);
        }

        let base_level_size_bytes = self.options.base_level_size_mb as u64 * 1024 * 1024;
        let mut target_sizes = vec![0u64; self.options.max_levels + 1];

        // Bottom level target
        target_sizes[self.options.max_levels] =
            real_sizes[self.options.max_levels].max(base_level_size_bytes);

        for i in (1..self.options.max_levels).rev() {
            let lower_target = target_sizes[i + 1];
            if lower_target <= base_level_size_bytes {
                // At most one level below base_level_size_mb can have a positive target
                target_sizes[i] = 0;
            } else {
                target_sizes[i] = lower_target / self.options.level_size_multiplier as u64;
            }
        }

        target_sizes
    }

    fn compute_current_sizes(&self, snapshot: &LsmStorageState) -> Vec<u64> {
        let mut current_sizes = vec![0u64; self.options.max_levels + 1];
        for i in 1..=self.options.max_levels {
            current_sizes[i] = self.get_ssts_size(snapshot, i);
        }
        current_sizes
    }

    pub fn generate_compaction_task(
        &self,
        _snapshot: &LsmStorageState,
    ) -> Option<LeveledCompactionTask> {
        let target_sizes = self.compute_target_sizes(_snapshot);

        // 1. L0 Compaction Trigger
        if _snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger {
            let l_base = (1..=self.options.max_levels)
                .find(|&i| target_sizes[i] > 0)
                .unwrap_or(self.options.max_levels);

            // Include ALL L0 SSTs, not just the minimum one
            let upper_sst_ids = _snapshot.l0_sstables.clone();
            let lower_overlap_sst_ids =
                self.find_overlapping_ssts(_snapshot, &upper_sst_ids, l_base);

            return Some(LeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: upper_sst_ids,
                lower_level: l_base,
                lower_level_sst_ids: lower_overlap_sst_ids,
                is_lower_level_bottom_level: l_base == self.options.max_levels,
            });
        }

        // 2. Inter-Level Compaction Priorities
        let current_sizes = self.compute_current_sizes(_snapshot);
        let mut max_priority = 1.0f64;
        let mut selected_level: Option<usize> = None;

        for i in 1..self.options.max_levels {
            let target_size = target_sizes[i];
            let current_size = current_sizes[i];

            if target_size > 0 {
                let priority = current_size as f64 / target_size as f64;

                if priority > max_priority {
                    max_priority = priority;
                    selected_level = Some(i);
                }
            }
        }

        // 3. Generate Inter-Level Compaction Task
        if let Some(level) = selected_level {
            let level_ssts = _snapshot
                .levels
                .iter()
                .find(|(l, _)| *l == level)
                .map(|(_, ssts)| ssts)?;

            if level_ssts.is_empty() {
                return None;
            }

            // Select the oldest SSTable (smallest ID) as the candidate
            let chosen_sst = level_ssts.iter().copied().min().unwrap();
            let upper_sst_ids = vec![chosen_sst];
            let lower_overlap_sst_ids =
                self.find_overlapping_ssts(_snapshot, &upper_sst_ids, level + 1);

            return Some(LeveledCompactionTask {
                upper_level: Some(level),
                upper_level_sst_ids: upper_sst_ids,
                lower_level: level + 1,
                lower_level_sst_ids: lower_overlap_sst_ids,
                is_lower_level_bottom_level: level + 1 == self.options.max_levels,
            });
        }

        None
    }

    pub fn apply_compaction_result(
        &self,
        _snapshot: &LsmStorageState,
        _task: &LeveledCompactionTask,
        _output: &[usize],
        _in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot = _snapshot.clone();
        let mut obsolete_ssts = Vec::new();

        obsolete_ssts.extend(&_task.upper_level_sst_ids);
        obsolete_ssts.extend(&_task.lower_level_sst_ids);

        let remove_upper_set: std::collections::HashSet<_> =
            _task.upper_level_sst_ids.iter().copied().collect();
        let remove_lower_set: std::collections::HashSet<_> =
            _task.lower_level_sst_ids.iter().copied().collect();

        // Remove SSTs from the upper level
        match _task.upper_level {
            None => {
                snapshot
                    .l0_sstables
                    .retain(|id| !remove_upper_set.contains(id));
            }
            Some(lvl) => {
                if let Some((_, ssts)) = snapshot.levels.iter_mut().find(|(l, _)| *l == lvl) {
                    ssts.retain(|id| !remove_upper_set.contains(id));
                }
            }
        }

        // Remove old SSTs and append new output SSTs in lower level
        if let Some((_, ssts)) = snapshot
            .levels
            .iter_mut()
            .find(|(l, _)| *l == _task.lower_level)
        {
            ssts.retain(|id| !remove_lower_set.contains(id));
            ssts.extend(_output);

            // Sort lower level SSTs by first_key if not in recovery
            if !_in_recovery {
                ssts.sort_by_key(|id| snapshot.sstables.get(id).unwrap().first_key());
            }
        }

        (snapshot, obsolete_ssts)
    }
}
