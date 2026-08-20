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

use std::collections::HashMap;
use std::fs::File;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use anyhow::Result;
use bytes::Bytes;
use farmhash;
use parking_lot::{Mutex, MutexGuard, RwLock};

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};
use crate::iterators::StorageIterator;
use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::txn::TxnIterator;
use crate::key::{KeySlice, TS_DEFAULT};
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::manifest::Manifest;
use crate::manifest::ManifestRecord;
use crate::mem_table::{MemTable, map_bound, map_key_bound_plus_ts};
use crate::mvcc::LsmMvccInner;
use crate::table::{FileObject, SsTable, SsTableBuilder, SsTableIterator};

pub type BlockCache = moka::sync::Cache<(usize, usize), Arc<Block>>;

/// Represents the state of the storage engine.
#[derive(Clone)]
pub struct LsmStorageState {
    /// The current memtable.
    pub memtable: Arc<MemTable>,
    /// Immutable memtables, from latest to earliest.
    pub imm_memtables: Vec<Arc<MemTable>>,
    /// L0 SSTs, from latest to earliest.
    pub l0_sstables: Vec<usize>,
    /// SsTables sorted by key range; L1 - L_max for leveled compaction, or tiers for tiered
    /// compaction.
    pub levels: Vec<(usize, Vec<usize>)>,
    /// SST objects.
    pub sstables: HashMap<usize, Arc<SsTable>>,
}

pub enum WriteBatchRecord<T: AsRef<[u8]>> {
    Put(T, T),
    Del(T),
}

impl LsmStorageState {
    fn create(options: &LsmStorageOptions) -> Self {
        let levels = match &options.compaction_options {
            CompactionOptions::Leveled(LeveledCompactionOptions { max_levels, .. })
            | CompactionOptions::Simple(SimpleLeveledCompactionOptions { max_levels, .. }) => (1
                ..=*max_levels)
                .map(|level| (level, Vec::new()))
                .collect::<Vec<_>>(),
            CompactionOptions::Tiered(_) => Vec::new(),
            CompactionOptions::NoCompaction => vec![(1, Vec::new())],
        };
        Self {
            memtable: Arc::new(MemTable::create(0)),
            imm_memtables: Vec::new(),
            l0_sstables: Vec::new(),
            levels,
            sstables: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LsmStorageOptions {
    // Block size in bytes
    pub block_size: usize,
    // SST size in bytes, also the approximate memtable capacity limit
    pub target_sst_size: usize,
    // Maximum number of memtables in memory, flush to L0 when exceeding this limit
    pub num_memtable_limit: usize,
    pub compaction_options: CompactionOptions,
    pub enable_wal: bool,
    pub serializable: bool,
}

impl LsmStorageOptions {
    pub fn default_for_week1_test() -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::NoCompaction,
            enable_wal: false,
            num_memtable_limit: 50,
            serializable: false,
        }
    }

    pub fn default_for_week1_day6_test() -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::NoCompaction,
            enable_wal: false,
            num_memtable_limit: 2,
            serializable: false,
        }
    }

    pub fn default_for_week2_test(compaction_options: CompactionOptions) -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 1 << 20, // 1MB
            compaction_options,
            enable_wal: false,
            num_memtable_limit: 2,
            serializable: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CompactionFilter {
    Prefix(Bytes),
}

/// The storage interface of the LSM tree.
pub(crate) struct LsmStorageInner {
    pub(crate) state: Arc<RwLock<Arc<LsmStorageState>>>,
    pub(crate) state_lock: Mutex<()>,
    path: PathBuf,
    pub(crate) block_cache: Arc<BlockCache>,
    next_sst_id: AtomicUsize,
    pub(crate) options: Arc<LsmStorageOptions>,
    pub(crate) compaction_controller: CompactionController,
    pub(crate) manifest: Option<Manifest>,
    pub(crate) mvcc: Option<LsmMvccInner>,
    pub(crate) compaction_filters: Arc<Mutex<Vec<CompactionFilter>>>,
}

/// A thin wrapper for `LsmStorageInner` and the user interface for MiniLSM.
pub struct MiniLsm {
    pub(crate) inner: Arc<LsmStorageInner>,
    /// Notifies the L0 flush thread to stop working. (In week 1 day 6)
    flush_notifier: crossbeam_channel::Sender<()>,
    /// The handle for the flush thread. (In week 1 day 6)
    flush_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Notifies the compaction thread to stop working. (In week 2)
    compaction_notifier: crossbeam_channel::Sender<()>,
    /// The handle for the compaction thread. (In week 2)
    compaction_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for MiniLsm {
    fn drop(&mut self) {
        self.compaction_notifier.send(()).ok();
        self.flush_notifier.send(()).ok();

        if let Some(thread) = self.compaction_thread.lock().take() {
            let _ = thread.join().map_err(|e| anyhow::anyhow!("{:?}", e));
        }
        if let Some(thread) = self.flush_thread.lock().take() {
            let _ = thread.join().map_err(|e| anyhow::anyhow!("{:?}", e));
        }
    }
}

impl MiniLsm {
    pub fn close(&self) -> Result<()> {
        if self.inner.options.enable_wal {
            return self.sync();
        }

        self.inner
            .force_freeze_memtable(&self.inner.state_lock.lock())?;

        loop {
            let state = self.inner.state.read();
            if state.imm_memtables.is_empty() {
                return Ok(());
            }
            drop(state);

            self.inner.force_flush_next_imm_memtable()?;
        }
    }

    /// Start the storage engine by either loading an existing directory or creating a new one if the directory does
    /// not exist.
    pub fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Arc<Self>> {
        let inner = Arc::new(LsmStorageInner::open(path, options)?);
        let (tx1, rx) = crossbeam_channel::unbounded();
        let compaction_thread = inner.spawn_compaction_thread(rx)?;
        let (tx2, rx) = crossbeam_channel::unbounded();
        let flush_thread = inner.spawn_flush_thread(rx)?;
        Ok(Arc::new(Self {
            inner,
            flush_notifier: tx2,
            flush_thread: Mutex::new(flush_thread),
            compaction_notifier: tx1,
            compaction_thread: Mutex::new(compaction_thread),
        }))
    }

    pub fn new_txn(&self) -> Result<()> {
        self.inner.new_txn()
    }

    pub fn write_batch<T: AsRef<[u8]>>(&self, batch: &[WriteBatchRecord<T>]) -> Result<()> {
        self.inner.write_batch(batch)
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        self.inner.add_compaction_filter(compaction_filter)
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key)
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner.put(key, value)
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.inner.delete(key)
    }

    pub fn sync(&self) -> Result<()> {
        self.inner.sync()
    }

    pub fn scan(
        self: &Arc<Self>,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<TxnIterator> {
        self.inner.scan(lower, upper)
    }

    /// Only call this in test cases due to race conditions
    pub fn force_flush(&self) -> Result<()> {
        if !self.inner.state.read().memtable.is_empty() {
            self.inner
                .force_freeze_memtable(&self.inner.state_lock.lock())?;
        }
        if !self.inner.state.read().imm_memtables.is_empty() {
            self.inner.force_flush_next_imm_memtable()?;
        }
        Ok(())
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        self.inner.force_full_compaction()
    }
}

impl LsmStorageInner {
    pub(crate) fn next_sst_id(&self) -> usize {
        self.next_sst_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn mvcc(&self) -> &LsmMvccInner {
        self.mvcc.as_ref().unwrap()
    }

    /// Start the storage engine by either loading an existing directory or creating a new one if the directory does
    /// not exist.
    pub(crate) fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Self> {
        let path = path.as_ref();
        let mut state = LsmStorageState::create(&options);
        let block_cache = Arc::new(BlockCache::new(1024));

        let compaction_controller = match &options.compaction_options {
            CompactionOptions::Leveled(options) => {
                CompactionController::Leveled(LeveledCompactionController::new(options.clone()))
            }
            CompactionOptions::Tiered(options) => {
                CompactionController::Tiered(TieredCompactionController::new(options.clone()))
            }
            CompactionOptions::Simple(options) => CompactionController::Simple(
                SimpleLeveledCompactionController::new(options.clone()),
            ),
            CompactionOptions::NoCompaction => CompactionController::NoCompaction,
        };

        let manifest_path = Self::path_of_manifest_static(path);

        let mut max_sst_id = 0;

        let (manifest, records) = if manifest_path.exists() {
            Manifest::recover(&manifest_path)?
        } else {
            if !path.exists() {
                std::fs::create_dir_all(path)?;
            }
            let manifest = Manifest::create(&manifest_path)?;
            (manifest, Vec::new())
        };

        let mut memtable_ids = Vec::new();

        // Replay records to compute the final, live SST IDs without opening the files yet
        for record in records {
            match record {
                ManifestRecord::Flush(id) => {
                    memtable_ids.retain(|&x| x != id);

                    if compaction_controller.flush_to_l0() {
                        state.l0_sstables.insert(0, id);
                    } else {
                        state.levels.insert(0, (id, vec![id]));
                    }
                    max_sst_id = max_sst_id.max(id);
                }
                ManifestRecord::Compaction(task, ids) => {
                    for id in &ids {
                        max_sst_id = max_sst_id.max(*id);
                    }

                    let (new_state, _) =
                        compaction_controller.apply_compaction_result(&state, &task, &ids, true);
                    state = new_state;
                }
                ManifestRecord::NewMemtable(id) => {
                    max_sst_id = max_sst_id.max(id);
                    memtable_ids.insert(0, id);
                }
            }
        }

        // Recover the memtables that are actually still alive
        if !memtable_ids.is_empty() {
            let active_id = memtable_ids[0];
            state.memtable = Arc::new(if options.enable_wal {
                MemTable::recover_from_wal(active_id, Self::path_of_wal_static(path, active_id))?
            } else {
                MemTable::create(active_id)
            });

            state.imm_memtables.clear();
            for &id in memtable_ids.iter().skip(1) {
                state.imm_memtables.push(Arc::new(if options.enable_wal {
                    MemTable::recover_from_wal(id, Self::path_of_wal_static(path, id))?
                } else {
                    MemTable::create(id)
                }));
            }
        }

        // Open ONLY the live files present in the final state
        let mut live_ssts = Vec::new();
        live_ssts.extend(state.l0_sstables.iter().copied());
        for (_, files) in &state.levels {
            live_ssts.extend(files.iter().copied());
        }

        for id in &live_ssts {
            let path_of_sst = Self::path_of_sst_static(path, *id);
            let file = FileObject::open(&path_of_sst)?;
            let sst = SsTable::open(*id, Some(block_cache.clone()), file)?;
            state.sstables.insert(*id, Arc::new(sst));
        }

        // Delete orphaned SST files
        if path.exists() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let file_path = entry.path();

                if file_path.extension().and_then(|s| s.to_str()) == Some("sst")
                    && let Some(file_name) = file_path.file_stem().and_then(|s| s.to_str())
                    && let Ok(sst_id) = file_name.parse::<usize>()
                    && !live_ssts.contains(&sst_id)
                {
                    std::fs::remove_file(&file_path)?;
                }
            }
        }

        match &compaction_controller {
            CompactionController::Leveled(_) | CompactionController::Simple(_) => {
                for (_level, files) in &mut state.levels {
                    files.sort_by(|a, b| {
                        state.sstables[a]
                            .first_key()
                            .cmp(state.sstables[b].first_key())
                    });
                }
            }
            _ => {}
        }

        let next_sst_id = max_sst_id + 1;

        // Add the first memtable to the manifest for a newly initialized DB
        if max_sst_id == 0 {
            state.memtable = Arc::new(if options.enable_wal {
                MemTable::create_with_wal(
                    state.memtable.id(),
                    Self::path_of_wal_static(path, state.memtable.id()),
                )?
            } else {
                MemTable::create(state.memtable.id())
            });
            manifest.add_record_when_init(ManifestRecord::NewMemtable(state.memtable.id()))?;
        }

        // TODO: fix timestamp
        let mvcc = LsmMvccInner::new(TS_DEFAULT);

        let storage = Self {
            state: Arc::new(RwLock::new(Arc::new(state))),
            state_lock: Mutex::new(()),
            path: path.to_path_buf(),
            block_cache,
            next_sst_id: AtomicUsize::new(next_sst_id + 1),
            compaction_controller,
            manifest: Some(manifest),
            options: options.into(),
            mvcc: Some(mvcc),
            compaction_filters: Arc::new(Mutex::new(Vec::new())),
        };

        Ok(storage)
    }

    pub fn sync(&self) -> Result<()> {
        self.state.read().memtable.sync_wal()
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        let mut compaction_filters = self.compaction_filters.lock();
        compaction_filters.push(compaction_filter);
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(self: &Arc<Self>, _key: &[u8]) -> Result<Option<Bytes>> {
        let txn = self.mvcc().new_txn(self.clone(), self.options.serializable);
        txn.get(_key)
    }

    pub fn get_with_ts(&self, key: &[u8], read_ts: u64) -> Result<Option<Bytes>> {
        let snapshot = self.state.read();
        let key_slice = KeySlice::from_slice(key, read_ts);

        // Helper function to check an iterator and early-stop
        fn check_iter(
            iter: impl for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
            key: &[u8],
        ) -> Result<Option<Option<Bytes>>> {
            if iter.is_valid() && iter.key().key_ref() == key {
                if iter.value().is_empty() {
                    return Ok(Some(None)); // Found deletion tombstone -> early stop
                }
                return Ok(Some(Some(Bytes::copy_from_slice(iter.value())))); // Found value -> early stop
            }
            Ok(None) // Not found in this source -> check next source
        }

        // 1. Check Active Memtable
        if let Some(res) = check_iter(
            snapshot
                .memtable
                .scan(Bound::Included(key_slice), Bound::Unbounded),
            key,
        )? {
            return Ok(res);
        }

        // 2. Check Immutable Memtables
        for imm in &snapshot.imm_memtables {
            if let Some(res) =
                check_iter(imm.scan(Bound::Included(key_slice), Bound::Unbounded), key)?
            {
                return Ok(res);
            }
        }

        let key_hash = farmhash::fingerprint32(key);

        // 3. Check L0 SSTables (newest to oldest)
        if self.compaction_controller.flush_to_l0() {
            for id in &snapshot.l0_sstables {
                let sst = snapshot.sstables[id].clone();
                if sst.first_key().key_ref() > key || sst.last_key().key_ref() < key {
                    continue;
                }
                if let Some(bloom) = &sst.bloom
                    && !bloom.may_contain(key_hash)
                {
                    continue;
                }
                let iter = SsTableIterator::create_and_seek_to_key(sst, key_slice)?;
                if let Some(res) = check_iter(iter, key)? {
                    return Ok(res);
                }
            }
        }

        // 4. Check All SSTables
        for i in 0..snapshot.levels.len() {
            let level_sst_ids = &snapshot.levels[i].1;

            if level_sst_ids.is_empty() {
                continue;
            }
            let ssts: Vec<_> = level_sst_ids
                .iter()
                .map(|id| snapshot.sstables[id].clone())
                .collect();

            let iter = SstConcatIterator::create_and_seek_to_key(ssts, key_slice)?;
            if let Some(res) = check_iter(iter, key)? {
                return Ok(res);
            }
        }

        Ok(None)
    }

    /// Write a batch of data into the storage. Implement in week 2 day 7.
    pub fn write_batch<T: AsRef<[u8]>>(&self, _batch: &[WriteBatchRecord<T>]) -> Result<()> {
        // Acquire MVCC write lock to make write_batch atomic
        let _lock = self.mvcc().write_lock.lock();

        // Allocate timestamp
        let commit_ts = self.mvcc().latest_commit_ts() + 1;

        for record in _batch {
            let state = self.state.read();

            match record {
                WriteBatchRecord::Del(k) => {
                    let key = KeySlice::from_slice(k.as_ref(), commit_ts);
                    state.memtable.put(key, b"")?;
                }
                WriteBatchRecord::Put(k, v) => {
                    let key = KeySlice::from_slice(k.as_ref(), commit_ts);
                    state.memtable.put(key, v.as_ref())?;
                }
            }

            if state.memtable.approximate_size() >= self.options.target_sst_size {
                // Drop read lock before calling force_freeze_memtable
                // because freeze needs a WRITE lock on self.state
                drop(state);
                let state_lock = self.state_lock.lock();
                let state = self.state.read();

                if state.memtable.approximate_size() >= self.options.target_sst_size {
                    drop(state);
                    let _ = self.force_freeze_memtable(&state_lock);
                }
            }
        }

        // Publication
        self.mvcc().update_commit_ts(commit_ts);

        Ok(())
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, _key: &[u8], _value: &[u8]) -> Result<()> {
        let batch = vec![WriteBatchRecord::Put(_key, _value)];

        self.write_batch(&batch)
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, _key: &[u8]) -> Result<()> {
        let batch = vec![WriteBatchRecord::Del(_key)];

        self.write_batch(&batch)
    }

    pub(crate) fn path_of_manifest_static(path: impl AsRef<Path>) -> PathBuf {
        path.as_ref().join("MANIFEST")
    }

    pub(crate) fn path_of_manifest(&self) -> PathBuf {
        Self::path_of_manifest_static(&self.path)
    }

    pub(crate) fn path_of_sst_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{:05}.sst", id))
    }

    pub(crate) fn path_of_sst(&self, id: usize) -> PathBuf {
        Self::path_of_sst_static(&self.path, id)
    }

    pub(crate) fn path_of_wal_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{:05}.wal", id))
    }

    pub(crate) fn path_of_wal(&self, id: usize) -> PathBuf {
        Self::path_of_wal_static(&self.path, id)
    }

    pub(super) fn sync_dir(&self) -> Result<()> {
        File::open(&self.path)?.sync_all()?;
        Ok(())
    }

    /// Force freeze the current memtable to an immutable memtable
    pub fn force_freeze_memtable(&self, _state_lock_observer: &MutexGuard<'_, ()>) -> Result<()> {
        let memtable_id = self.next_sst_id();

        let new_memtable = Arc::new(if self.options.enable_wal {
            MemTable::create_with_wal(memtable_id, self.path_of_wal(memtable_id))?
        } else {
            MemTable::create(memtable_id)
        });

        if let Some(manifest) = &self.manifest {
            manifest.add_record(
                _state_lock_observer,
                ManifestRecord::NewMemtable(memtable_id),
            )?;
        }

        {
            let mut guard = self.state.write();

            let mut snapshot = guard.as_ref().clone();
            let old_memtable = std::mem::replace(&mut snapshot.memtable, new_memtable);
            snapshot.imm_memtables.insert(0, old_memtable);
            *guard = Arc::new(snapshot);
        }

        Ok(())
    }

    /// Force flush the earliest-created immutable memtable to disk
    pub fn force_flush_next_imm_memtable(&self) -> Result<()> {
        let _state_lock = self.state_lock.lock();

        let memtable_to_flush = {
            let guard = self.state.read();
            let Some(memtable) = guard.imm_memtables.last() else {
                return Ok(());
            };
            memtable.clone()
        };

        // if the memtable is empty, don't build an SST file
        if memtable_to_flush.is_empty() {
            let mut guard = self.state.write();
            let mut snapshot = guard.as_ref().clone();
            snapshot.imm_memtables.pop();
            *guard = Arc::new(snapshot);
            return Ok(());
        }

        let mut sst_builder = SsTableBuilder::new(self.options.block_size);

        memtable_to_flush.flush(&mut sst_builder)?;

        let sst_id = memtable_to_flush.id();

        let path_of_sst = self.path_of_sst(sst_id);

        let sst = sst_builder.build(sst_id, Some(self.block_cache.clone()), path_of_sst)?;

        {
            let mut guard = self.state.write();
            let mut snapshot = guard.as_ref().clone();
            let removed = snapshot.imm_memtables.pop().unwrap();
            assert_eq!(removed.id(), sst_id);

            if self.compaction_controller.flush_to_l0() {
                snapshot.l0_sstables.insert(0, sst_id);
            } else {
                snapshot.levels.insert(0, (sst_id, vec![sst_id]));
            }

            snapshot.sstables.insert(sst_id, Arc::new(sst));
            *guard = Arc::new(snapshot);
        }

        self.sync_dir()?;

        if let Some(manifest) = &self.manifest {
            manifest.add_record(&_state_lock, ManifestRecord::Flush(sst_id))?;
        }

        if self.options.enable_wal {
            std::fs::remove_file(self.path_of_wal(sst_id)).ok();
        }

        Ok(())
    }

    pub fn new_txn(&self) -> Result<()> {
        // no-op
        Ok(())
    }

    /// Create an iterator over a range of keys.
    pub fn scan(
        self: &Arc<Self>,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
    ) -> Result<TxnIterator> {
        let txn = self.mvcc().new_txn(self.clone(), self.options.serializable);
        txn.scan(_lower, _upper)
    }

    pub(crate) fn scan_with_ts(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
        read_ts: u64,
    ) -> Result<FusedIterator<LsmIterator>> {
        // Helper function to determine whether [first_key, last_key] overlaps with [_lower, _upper]
        fn range_overlap(
            user_begin: Bound<&[u8]>,
            user_end: Bound<&[u8]>,
            table_begin: KeySlice,
            table_end: KeySlice,
        ) -> bool {
            match user_begin {
                Bound::Included(b) => {
                    if table_end.key_ref() < b {
                        return false;
                    }
                }
                Bound::Excluded(b) => {
                    if table_end.key_ref() <= b {
                        return false;
                    }
                }
                Bound::Unbounded => {}
            }
            match user_end {
                Bound::Included(e) => {
                    if table_begin.key_ref() > e {
                        return false;
                    }
                }
                Bound::Excluded(e) => {
                    if table_begin.key_ref() >= e {
                        return false;
                    }
                }
                Bound::Unbounded => {}
            }

            true
        }

        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };

        // Collect active memtable and all immutable memtables (newest to oldest)
        let mut memtables = Vec::with_capacity(snapshot.imm_memtables.len() + 1);
        memtables.push(snapshot.memtable.clone());
        memtables.extend(snapshot.imm_memtables.iter().cloned());

        // Map each memtable to a boxed scanner iterator
        let mut mem_iters = Vec::with_capacity(memtables.len());
        let (begin, end) = map_key_bound_plus_ts(lower, upper, read_ts);
        for memtable in memtables {
            mem_iters.push(Box::new(memtable.scan(begin, end)));
        }

        // Create L0 SST iterators
        let mut sst_iters = Vec::with_capacity(snapshot.l0_sstables.len());
        if self.compaction_controller.flush_to_l0() {
            for id in &snapshot.l0_sstables {
                let sst = snapshot.sstables[id].clone();

                if !range_overlap(
                    lower,
                    upper,
                    sst.first_key().as_key_slice(),
                    sst.last_key().as_key_slice(),
                ) {
                    continue;
                }

                let iter = match lower {
                    Bound::Included(l) | Bound::Excluded(l) => {
                        SsTableIterator::create_and_seek_to_key(
                            sst,
                            KeySlice::from_slice(l, read_ts),
                        )?
                    }
                    Bound::Unbounded => SsTableIterator::create_and_seek_to_first(sst)?,
                };
                sst_iters.push(Box::new(iter));
            }
        }

        // Create All low level SST iterators
        let mut low_sst_concat_iters = Vec::with_capacity(snapshot.levels.len());
        for i in 0..snapshot.levels.len() {
            let level_sst_ids = &snapshot.levels[i].1;

            if level_sst_ids.is_empty() {
                continue;
            }

            let l_ssts: Vec<_> = level_sst_ids
                .iter()
                .map(|id| snapshot.sstables[id].clone())
                .collect();

            let l_concat_iter = match lower {
                Bound::Included(b) | Bound::Excluded(b) => {
                    SstConcatIterator::create_and_seek_to_key(
                        l_ssts,
                        KeySlice::from_slice(b, read_ts),
                    )?
                }
                Bound::Unbounded => SstConcatIterator::create_and_seek_to_first(l_ssts)?,
            };

            low_sst_concat_iters.push(Box::new(l_concat_iter));
        }

        let mem_merge_iter = MergeIterator::create(mem_iters);
        let sst_merge_iter = MergeIterator::create(sst_iters);
        let l0_two_merge_iter = TwoMergeIterator::create(mem_merge_iter, sst_merge_iter)?;
        let low_merge_iter = MergeIterator::create(low_sst_concat_iters);
        let two_merge_iter = TwoMergeIterator::create(l0_two_merge_iter, low_merge_iter)?;

        let end_bound = map_bound(upper);

        let mut lsm_iter = LsmIterator::new(two_merge_iter, end_bound, read_ts)?;

        // Skip the key if it matches the excluded lower bound exactly
        if let Bound::Excluded(b) = lower {
            while lsm_iter.is_valid() && lsm_iter.key() == b {
                lsm_iter.next()?;
            }
        }

        Ok(FusedIterator::new(lsm_iter))
    }
}
