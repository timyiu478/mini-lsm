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

mod leveled;
mod simple_leveled;
mod tiered;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
use serde::{Deserialize, Serialize};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

use crate::iterators::StorageIterator;
use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::key::KeySlice;
use crate::lsm_storage::{LsmStorageInner, LsmStorageState};
use crate::manifest::ManifestRecord;
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

#[derive(Debug, Serialize, Deserialize)]
pub enum CompactionTask {
    Leveled(LeveledCompactionTask),
    Tiered(TieredCompactionTask),
    Simple(SimpleLeveledCompactionTask),
    ForceFullCompaction {
        l0_sstables: Vec<usize>,
        l1_sstables: Vec<usize>,
    },
}

impl CompactionTask {
    fn compact_to_bottom_level(&self) -> bool {
        match self {
            CompactionTask::ForceFullCompaction { .. } => true,
            CompactionTask::Leveled(task) => task.is_lower_level_bottom_level,
            CompactionTask::Simple(task) => task.is_lower_level_bottom_level,
            CompactionTask::Tiered(task) => task.bottom_tier_included,
        }
    }
}

pub(crate) enum CompactionController {
    Leveled(LeveledCompactionController),
    Tiered(TieredCompactionController),
    Simple(SimpleLeveledCompactionController),
    NoCompaction,
}

impl CompactionController {
    pub fn generate_compaction_task(&self, snapshot: &LsmStorageState) -> Option<CompactionTask> {
        match self {
            CompactionController::Leveled(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Leveled),
            CompactionController::Simple(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Simple),
            CompactionController::Tiered(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Tiered),
            CompactionController::NoCompaction => unreachable!(),
        }
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &CompactionTask,
        output: &[usize],
        in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        match (self, task) {
            (CompactionController::Leveled(ctrl), CompactionTask::Leveled(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output, in_recovery)
            }
            (CompactionController::Simple(ctrl), CompactionTask::Simple(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output)
            }
            (CompactionController::Tiered(ctrl), CompactionTask::Tiered(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output)
            }
            _ => unreachable!(),
        }
    }
}

impl CompactionController {
    pub fn flush_to_l0(&self) -> bool {
        matches!(
            self,
            Self::Leveled(_) | Self::Simple(_) | Self::NoCompaction
        )
    }
}

#[derive(Debug, Clone)]
pub enum CompactionOptions {
    /// Leveled compaction with partial compaction + dynamic level support (= RocksDB's Leveled
    /// Compaction)
    Leveled(LeveledCompactionOptions),
    /// Tiered compaction (= RocksDB's universal compaction)
    Tiered(TieredCompactionOptions),
    /// Simple leveled compaction
    Simple(SimpleLeveledCompactionOptions),
    /// In no compaction mode (week 1), always flush to L0
    NoCompaction,
}

impl LsmStorageInner {
    /// a generic helper for building new compacted SST(s)
    fn build_ssts_from_iter<I>(
        &self,
        mut iter: I,
        drop_tombstones: bool,
    ) -> Result<Vec<Arc<SsTable>>>
    where
        I: for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
    {
        let mut compact_ssts = Vec::new();
        let mut sst_builder = SsTableBuilder::new(self.options.block_size);
        let mut has_key = false;

        while iter.is_valid() {
            if drop_tombstones && iter.value().is_empty() {
                iter.next()?;
                continue;
            }
            has_key = true;
            sst_builder.add(iter.key(), iter.value());

            if sst_builder.estimated_size() >= self.options.target_sst_size {
                let sst_id = self.next_sst_id();
                let compact_sst = sst_builder.build(
                    sst_id,
                    Some(self.block_cache.clone()),
                    self.path_of_sst(sst_id),
                )?;
                compact_ssts.push(Arc::new(compact_sst));
                sst_builder = SsTableBuilder::new(self.options.block_size);
                has_key = false;
            }
            iter.next()?;
        }

        if has_key {
            let sst_id = self.next_sst_id();
            let compact_sst = sst_builder.build(
                sst_id,
                Some(self.block_cache.clone()),
                self.path_of_sst(sst_id),
            )?;
            compact_ssts.push(Arc::new(compact_sst));
        }

        Ok(compact_ssts)
    }
    fn compact(&self, _task: &CompactionTask) -> Result<Vec<Arc<SsTable>>> {
        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };

        match _task {
            CompactionTask::Tiered(tiered_task) => {
                let mut sst_iters = Vec::with_capacity(tiered_task.tiers.len());

                for (_, ids) in &tiered_task.tiers {
                    let ssts: Vec<_> = ids.iter().map(|id| snapshot.sstables[id].clone()).collect();

                    let iter = SstConcatIterator::create_and_seek_to_first(ssts)?;
                    sst_iters.push(Box::new(iter));
                }

                let merge_iter = MergeIterator::create(sst_iters);
                self.build_ssts_from_iter(merge_iter, tiered_task.bottom_tier_included)
            },
            CompactionTask::Leveled(_) => {
                // TODO
                Ok(vec![])
            },
            CompactionTask::Simple(simple_task) => {
                let upper_ssts: Vec<_> = simple_task
                    .upper_level_sst_ids
                    .iter()
                    .map(|id| snapshot.sstables[id].clone())
                    .collect();
                let lower_ssts: Vec<_> = simple_task
                    .lower_level_sst_ids
                    .iter()
                    .map(|id| snapshot.sstables[id].clone())
                    .collect();

                let lower_iter = SstConcatIterator::create_and_seek_to_first(lower_ssts)?;

                if simple_task.upper_level.is_none() {
                    let mut sst_iters = Vec::with_capacity(upper_ssts.len());
                    for sst in upper_ssts {
                        sst_iters.push(Box::new(SsTableIterator::create_and_seek_to_first(sst)?));
                    }
                    let upper_iter = MergeIterator::create(sst_iters);
                    let iter = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.build_ssts_from_iter(iter, simple_task.is_lower_level_bottom_level)
                } else {
                    let upper_iter = SstConcatIterator::create_and_seek_to_first(upper_ssts)?;
                    let iter = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.build_ssts_from_iter(iter, simple_task.is_lower_level_bottom_level)
                }
            },
            CompactionTask::ForceFullCompaction {
                l0_sstables,
                l1_sstables,
            } => {
                let mut ssts = Vec::with_capacity(l0_sstables.len() + l1_sstables.len());
                ssts.extend(l0_sstables.iter().map(|id| snapshot.sstables[id].clone()));
                ssts.extend(l1_sstables.iter().map(|id| snapshot.sstables[id].clone()));

                let mut sst_iters = Vec::with_capacity(ssts.len());
                for sst in ssts {
                    sst_iters.push(Box::new(SsTableIterator::create_and_seek_to_first(sst)?));
                }

                let iter = MergeIterator::create(sst_iters);

                self.build_ssts_from_iter(iter, true)
            },
        }
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };

        let l0_sstables = snapshot.l0_sstables.clone();
        let l1_sstables = snapshot.levels[0].1.clone();

        let full_compaction_task = CompactionTask::ForceFullCompaction {
            l0_sstables: l0_sstables.clone(),
            l1_sstables: l1_sstables.clone(),
        };

        let compact_ssts = self.compact(&full_compaction_task)?;

        {
            // prevent concurrent compaction/flush
            let _state_lock = self.state_lock.lock();

            let mut guard = self.state.write();
            let mut snapshot = guard.as_ref().clone();

            snapshot.l0_sstables.retain(|id| !l0_sstables.contains(id));
            snapshot.levels[0].1.clear();

            for id in l0_sstables.iter().chain(l1_sstables.iter()) {
                snapshot.sstables.remove(id);
            }

            let sst_ids = compact_ssts.iter().map(|s| s.sst_id()).collect();

            for sst in compact_ssts {
                snapshot.levels[0].1.push(sst.sst_id());
                snapshot.sstables.insert(sst.sst_id(), sst);
            }

            *guard = Arc::new(snapshot);

            self.sync_dir()?;

            let record = ManifestRecord::Compaction(full_compaction_task, sst_ids);

            if let Some(manifest) = &self.manifest {
                manifest.add_record(&_state_lock, record)?;
            }
        }

        for id in l0_sstables.iter().chain(l1_sstables.iter()) {
            std::fs::remove_file(self.path_of_sst(*id))?;
        }

        self.sync_dir()?;

        Ok(())
    }
    fn trigger_compaction(&self) -> Result<()> {
        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };

        let Some(task) = self
            .compaction_controller
            .generate_compaction_task(&snapshot)
        else {
            return Ok(());
        };

        let new_ssts = self.compact(&task)?;
        let new_sst_ids: Vec<usize> = new_ssts.iter().map(|s| s.sst_id()).collect();

        let _state_lock = self.state_lock.lock();
        let obsolete_ssts = {
            let mut guard = self.state.write();

            let (mut new_snapshot, obsolete_ssts) = self
                .compaction_controller
                .apply_compaction_result(guard.as_ref(), &task, &new_sst_ids, false);

            for sst in new_ssts {
                new_snapshot.sstables.insert(sst.sst_id(), sst);
            }

            for id in &obsolete_ssts {
                new_snapshot.sstables.remove(id);
            }

            *guard = Arc::new(new_snapshot);
            obsolete_ssts
        };

        self.sync_dir()?;

        let record = ManifestRecord::Compaction(task, new_sst_ids);

        if let Some(manifest) = &self.manifest {
            manifest.add_record(&_state_lock, record)?;
        }

        drop(_state_lock);

        for id in obsolete_ssts {
            let path = self.path_of_sst(id);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }

        self.sync_dir()?;

        Ok(())
    }

    pub(crate) fn spawn_compaction_thread(
        self: &Arc<Self>,
        rx: crossbeam_channel::Receiver<()>,
    ) -> Result<Option<std::thread::JoinHandle<()>>> {
        if let CompactionOptions::Leveled(_)
        | CompactionOptions::Simple(_)
        | CompactionOptions::Tiered(_) = self.options.compaction_options
        {
            let this = self.clone();
            let handle = std::thread::spawn(move || {
                let ticker = crossbeam_channel::tick(Duration::from_millis(50));
                loop {
                    crossbeam_channel::select! {
                        recv(ticker) -> _ => if let Err(e) = this.trigger_compaction() {
                            eprintln!("compaction failed: {}", e);
                        },
                        recv(rx) -> _ => return
                    }
                }
            });
            return Ok(Some(handle));
        }
        Ok(None)
    }

    fn trigger_flush(&self) -> Result<()> {
        let state = self.state.read();
        let has_imm = !state.imm_memtables.is_empty();
        drop(state);

        if has_imm {
            self.force_flush_next_imm_memtable()
        } else {
            Ok(())
        }
    }

    pub(crate) fn spawn_flush_thread(
        self: &Arc<Self>,
        rx: crossbeam_channel::Receiver<()>,
    ) -> Result<Option<std::thread::JoinHandle<()>>> {
        let this = self.clone();
        let handle = std::thread::spawn(move || {
            let ticker = crossbeam_channel::tick(Duration::from_millis(50));
            loop {
                crossbeam_channel::select! {
                    recv(ticker) -> _ => if let Err(e) = this.trigger_flush() {
                        eprintln!("flush failed: {}", e);
                    },
                    recv(rx) -> _ => return
                }
            }
        });
        Ok(Some(handle))
    }
}
