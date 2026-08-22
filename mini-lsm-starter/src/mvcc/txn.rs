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

use std::{
    collections::HashSet,
    ops::Bound,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use ouroboros::self_referencing;
use parking_lot::Mutex;

use crate::{
    iterators::{StorageIterator, two_merge_iterator::TwoMergeIterator},
    lsm_iterator::{FusedIterator, LsmIterator},
    lsm_storage::{LsmStorageInner, WriteBatchRecord},
    mem_table::map_bound,
};

pub struct Transaction {
    pub(crate) read_ts: u64,
    pub(crate) inner: Arc<LsmStorageInner>,
    pub(crate) local_storage: Arc<SkipMap<Bytes, Bytes>>,
    pub(crate) committed: Arc<AtomicBool>,
    /// Write set and read set
    pub(crate) key_hashes: Option<Mutex<(HashSet<u32>, HashSet<u32>)>>,
}

impl Transaction {
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        if self.committed.load(Ordering::SeqCst) {
            panic!("Cannot operate on committed txn");
        }
        if let Some(guard) = &self.key_hashes {
            let mut guard = guard.lock();
            let (_, read_set) = &mut *guard;
            read_set.insert(farmhash::hash32(key));
        }
        if let Some(entry) = self.local_storage.get(key) {
            if entry.value().is_empty() {
                return Ok(None);
            } else {
                return Ok(Some(entry.value().clone()));
            }
        }
        self.inner.get_with_ts(key, self.read_ts)
    }

    pub fn scan(self: &Arc<Self>, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> Result<TxnIterator> {
        if self.committed.load(Ordering::SeqCst) {
            panic!("Cannot operate on committed txn");
        }

        let lower_bytes = map_bound(lower);
        let upper_bytes = map_bound(upper);

        let mut local_iter = TxnLocalIteratorBuilder {
            map: self.local_storage.clone(),
            iter_builder: |map| map.range((lower_bytes, upper_bytes)),
            item: (Bytes::new(), Bytes::new()),
            valid: false,
        }
        .build();

        local_iter.next()?;

        let fused_iter = self.inner.scan_with_ts(lower, upper, self.read_ts)?;

        let two_merge_iterator = TwoMergeIterator::create(local_iter, fused_iter)?;

        TxnIterator::create(self.clone(), two_merge_iterator)
    }

    pub fn put(&self, key: &[u8], value: &[u8]) {
        if self.committed.load(Ordering::SeqCst) {
            panic!("Cannot operate on committed txn");
        }
        if let Some(guard) = &self.key_hashes {
            let mut guard = guard.lock();
            let (write_set, _) = &mut *guard;
            write_set.insert(farmhash::hash32(key));
        }

        let key_bytes = Bytes::copy_from_slice(key);
        let val_bytes = Bytes::copy_from_slice(value);

        self.local_storage.insert(key_bytes, val_bytes);
    }

    pub fn delete(&self, key: &[u8]) {
        if self.committed.load(Ordering::SeqCst) {
            panic!("Cannot operate on committed txn");
        }
        if let Some(guard) = &self.key_hashes {
            let mut guard = guard.lock();
            let (write_set, _) = &mut *guard;
            write_set.insert(farmhash::hash32(key));
        }

        let key_bytes = Bytes::copy_from_slice(key);

        self.local_storage.insert(key_bytes, Bytes::new());
    }

    pub fn commit(&self) -> Result<()> {
        if self.committed.load(Ordering::SeqCst) {
            panic!("Cannot operate on committed txn");
        }

        let mut batch = Vec::new();
        let mut local_entries = Vec::new();

        for entry in self.local_storage.iter() {
            local_entries.push((entry.key().clone(), entry.value().clone()));
        }

        for (k, v) in &local_entries {
            if v.is_empty() {
                batch.push(WriteBatchRecord::Del(k.as_ref()));
            } else {
                batch.push(WriteBatchRecord::Put(k.as_ref(), v.as_ref()));
            }
        }

        self.inner.write_batch(&batch)?;

        self.committed.swap(true, Ordering::SeqCst);

        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.inner.mvcc().ts.lock().1.remove_reader(self.read_ts)
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, Bytes, (Bound<Bytes>, Bound<Bytes>), Bytes, Bytes>;

#[self_referencing]
pub struct TxnLocalIterator {
    /// Stores a reference to the skipmap.
    map: Arc<SkipMap<Bytes, Bytes>>,
    /// Stores a skipmap iterator that refers to the lifetime of `TxnLocalIterator` itself.
    #[borrows(map)]
    #[not_covariant]
    iter: SkipMapRangeIter<'this>,
    /// Stores the current key-value pair.
    item: (Bytes, Bytes),
    valid: bool,
}

impl StorageIterator for TxnLocalIterator {
    type KeyType<'a> = &'a [u8];

    fn value(&self) -> &[u8] {
        self.borrow_item().1.as_ref()
    }

    fn key(&self) -> &[u8] {
        self.borrow_item().0.as_ref()
    }

    fn is_valid(&self) -> bool {
        self.with_valid(|valid| *valid)
    }

    fn next(&mut self) -> Result<()> {
        let entry = self.with_iter_mut(|iter| {
            iter.next()
                .map(|entry| (entry.key().clone(), entry.value().clone()))
        });

        if let Some((key, value)) = entry {
            self.with_item_mut(|item| *item = (key, value));
            self.with_valid_mut(|valid| *valid = true);
        } else {
            self.with_item_mut(|item| *item = (Bytes::new(), Bytes::new()));
            self.with_valid_mut(|valid| *valid = false);
        }

        Ok(())
    }
}

pub struct TxnIterator {
    _txn: Arc<Transaction>,
    iter: TwoMergeIterator<TxnLocalIterator, FusedIterator<LsmIterator>>,
}

impl TxnIterator {
    pub fn create(
        txn: Arc<Transaction>,
        iter: TwoMergeIterator<TxnLocalIterator, FusedIterator<LsmIterator>>,
    ) -> Result<Self> {
        let mut txn_iter = TxnIterator { _txn: txn, iter };
        txn_iter.skip_deletes()?;
        Ok(txn_iter)
    }

    fn skip_deletes(&mut self) -> Result<()> {
        while self.iter.is_valid() && self.iter.value().is_empty() {
            self.iter.next()?;
        }

        if self.iter.is_valid() {
            if let Some(guard) = &self._txn.key_hashes {
                let mut guard = guard.lock();
                let (_, read_set) = &mut *guard;
                let key = self.iter.key();
                read_set.insert(farmhash::hash32(key));
            }
        }

        Ok(())
    }
}

impl StorageIterator for TxnIterator {
    type KeyType<'a>
        = &'a [u8]
    where
        Self: 'a;

    fn value(&self) -> &[u8] {
        self.iter.value()
    }

    fn key(&self) -> Self::KeyType<'_> {
        self.iter.key()
    }

    fn is_valid(&self) -> bool {
        self.iter.is_valid()
    }

    fn next(&mut self) -> Result<()> {
        self.iter.next()?;
        self.skip_deletes()?;
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.iter.num_active_iterators()
    }
}
