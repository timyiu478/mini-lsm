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

use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use ouroboros::self_referencing;

use crate::iterators::StorageIterator;
use crate::key::{KeyBytes, KeySlice, TS_DEFAULT};
use crate::table::SsTableBuilder;
use crate::wal::Wal;

/// A basic mem-table based on crossbeam-skiplist.
///
/// An initial implementation of memtable is part of week 1, day 1. It will be incrementally implemented in other
/// chapters of week 1 and week 2.
pub struct MemTable {
    map: Arc<SkipMap<KeyBytes, Bytes>>,
    wal: Option<Wal>,
    id: usize,
    approximate_size: Arc<AtomicUsize>, // the estimated total size of data stored in memory
}

/// Create a bound of `Bytes` from a bound of `&[u8]`.
pub(crate) fn map_bound(bound: Bound<&[u8]>) -> Bound<Bytes> {
    match bound {
        Bound::Included(x) => Bound::Included(Bytes::copy_from_slice(x)),
        Bound::Excluded(x) => Bound::Excluded(Bytes::copy_from_slice(x)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

/// Create a bound of `Bytes` from a bound of `KeySlice`.
pub(crate) fn map_key_bound(bound: Bound<KeySlice>) -> Bound<KeyBytes> {
    match bound {
        Bound::Included(x) => Bound::Included(KeyBytes::from_bytes_with_ts(
            Bytes::copy_from_slice(x.key_ref()),
            x.ts(),
        )),
        Bound::Excluded(x) => Bound::Excluded(KeyBytes::from_bytes_with_ts(
            Bytes::copy_from_slice(x.key_ref()),
            x.ts(),
        )),
        Bound::Unbounded => Bound::Unbounded,
    }
}

/// Create a bound of `KeySlice` from a bound of `&[u8]`.
pub(crate) fn map_key_bound_plus_ts<'a>(
    lower: Bound<&'a [u8]>,
    upper: Bound<&'a [u8]>,
    ts: u64,
) -> (Bound<KeySlice<'a>>, Bound<KeySlice<'a>>) {
    (
        match lower {
            Bound::Included(x) => Bound::Included(KeySlice::from_slice(x, ts)),
            Bound::Excluded(x) => Bound::Excluded(KeySlice::from_slice(x, TS_RANGE_END)),
            Bound::Unbounded => Bound::Unbounded,
        },
        match upper {
            Bound::Included(x) => {
                Bound::Included(KeySlice::from_slice(x, TS_RANGE_END))
            }
            Bound::Excluded(x) => Bound::Excluded(KeySlice::from_slice(x, TS_RANGE_BEGIN)),
            Bound::Unbounded => Bound::Unbounded,
        },
    )
}

impl MemTable {
    /// Create a new mem-table.
    pub fn create(_id: usize) -> Self {
        MemTable {
            id: _id,
            map: Arc::new(SkipMap::new()),
            wal: None,
            approximate_size: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a new mem-table with WAL
    pub fn create_with_wal(_id: usize, _path: impl AsRef<Path>) -> Result<Self> {
        let wal = Wal::create(_path)?;

        Ok(MemTable {
            id: _id,
            map: Arc::new(SkipMap::new()),
            wal: Some(wal),
            approximate_size: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Create a memtable from WAL
    pub fn recover_from_wal(_id: usize, _path: impl AsRef<Path>) -> Result<Self> {
        let map = Arc::new(SkipMap::new());

        let wal = Wal::recover(_path, &map)?;

        let size = map.iter().fold(0, |acc, entry| {
            acc + entry.key().len() + entry.value().len()
        });

        Ok(Self {
            id: _id,
            map,
            wal: Some(wal),
            approximate_size: Arc::new(AtomicUsize::new(size)),
        })
    }

    pub fn for_testing_put_slice(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put(key, value)
    }

    pub fn for_testing_get_slice(&self, key: &[u8]) -> Option<Bytes> {
        self.get(key)
    }

    pub fn for_testing_scan_slice(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> MemTableIterator {
        // This function is only used in week 1 tests, so during the week 3 key-ts refactor, you do
        // not need to consider the bound exclude/include logic. Simply provide `DEFAULT_TS` as the
        // timestamp for the key-ts pair.
        self.scan(lower, upper)
    }

    /// Get a value by key.
    pub fn get(&self, _key: KeySlice) -> Option<Bytes> {
        let raw_key = _key.key_ref();
        
        // SAFETY: The static slice is created strictly for a synchronous lookup inside `get`.
        // The `Bytes` object constructed from it never escapes this function body.
        let static_key = unsafe { std::slice::from_raw_parts(raw_key.as_ptr(), raw_key.len()) };
        let bytes = Bytes::from_static(static_key);
        let key_bytes = KeyBytes::from_bytes_with_ts(bytes, _key.ts());

        self.map.get(&key_bytes).map(|e| e.value().clone())
    }

    /// Put a key-value pair into the mem-table.
    ///
    /// In week 1, day 1, simply put the key-value pair into the skipmap.
    /// In week 2, day 6, also flush the data to WAL.
    /// In week 3, day 5, route this through the batch WAL implementation.
    pub fn put(&self, _key: KeySlice, _value: &[u8]) -> Result<()> {
        let key_bytes = _key.to_key_vec().into_key_bytes();
        let val_bytes = Bytes::copy_from_slice(_value);

        if let Some(wal) = &self.wal {
            wal.put(_key, &val_bytes)?;
        }

        let added_size = _key.key_len() + _value.len();
        self.approximate_size
            .fetch_add(added_size, Ordering::Relaxed);

        self.map.insert(key_bytes, val_bytes);

        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, _data: &[(KeySlice, &[u8])]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync_wal(&self) -> Result<()> {
        if let Some(ref wal) = self.wal {
            wal.sync()?;
        }
        Ok(())
    }

    /// Get an iterator over a range of keys.
    pub fn scan(&self, _lower: Bound<KeySlice>, _upper: Bound<KeySlice>) -> MemTableIterator {
        let (lower, upper) = (map_key_bound(_lower), map_key_bound(_upper));

        let mut iter = MemTableIteratorBuilder {
            map: self.map.clone(),
            iter_builder: |map| { map.range((lower, upper)) },
            item: (KeyBytes::new(), Bytes::new()),
            valid: false,
        }
        .build();

        let _ = iter.next();

        iter
    }

    /// Flush the mem-table to SSTable. Implement in week 1 day 6.
    pub fn flush(&self, _builder: &mut SsTableBuilder) -> Result<()> {
        for entry in self.map.iter() {
            let key = entry.key();
            let value = entry.value();

            _builder.add(key.as_key_slice(), &value[..]);
        }

        Ok(())
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn approximate_size(&self) -> usize {
        self.approximate_size
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Only use this function when closing the database
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, KeyBytes, (Bound<KeyBytes>, Bound<KeyBytes>), KeyBytes, Bytes>;

/// An iterator over a range of `SkipMap`. This is a self-referential structure and please refer to week 1, day 2
/// chapter for more information.
///
/// This is part of week 1, day 2.
#[self_referencing]
pub struct MemTableIterator {
    /// Stores a reference to the skipmap.
    map: Arc<SkipMap<KeyBytes, Bytes>>,
    /// Stores a skipmap iterator that refers to the lifetime of `MemTableIterator` itself.
    #[borrows(map)]
    #[not_covariant]
    iter: SkipMapRangeIter<'this>,
    /// Stores the current key-value pair.
    item: (KeyBytes, Bytes),
    valid: bool,
}

impl StorageIterator for MemTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn value(&self) -> &[u8] {
        self.borrow_item().1.as_ref()
    }

    fn key(&self) -> KeySlice<'_> {
        self.borrow_item().0.as_key_slice()
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
            self.with_item_mut(|item| *item = (KeyBytes::new(), Bytes::new()));
            self.with_valid_mut(|valid| *valid = false);
        }

        Ok(())
    }
}
