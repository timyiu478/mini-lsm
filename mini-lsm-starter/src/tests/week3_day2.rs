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

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord},
};

use super::harness::{
    check_iter_result_by_key_and_ts, check_lsm_iter_result_by_key,
    construct_merge_iterator_over_storage,
};

#[test]
fn test_timestamped_batches_and_latest_reads() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options).unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"1"),
            WriteBatchRecord::Put(b"b", b"1"),
        ])
        .unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"2"),
            WriteBatchRecord::Del(b"b"),
        ])
        .unwrap();
    storage.force_flush().unwrap();

    let mut raw_iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key_and_ts(
        &mut raw_iter,
        vec![
            ((Bytes::from("a"), 2), Bytes::from("2")),
            ((Bytes::from("a"), 1), Bytes::from("1")),
            ((Bytes::from("b"), 2), Bytes::new()),
            ((Bytes::from("b"), 1), Bytes::from("1")),
        ],
    );
    assert_eq!(storage.get(b"a").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"b").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![(Bytes::from("a"), Bytes::from("2"))],
    );
}

#[test]
fn test_scan_bounds_multi_version() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options).unwrap();

    storage.write_batch(&[
        WriteBatchRecord::Put(b"a", b"1"),
        WriteBatchRecord::Put(b"b", b"1"),
        WriteBatchRecord::Put(b"c", b"1"),
    ]).unwrap();
    storage.write_batch(&[WriteBatchRecord::Put(b"b", b"2")]).unwrap();
    storage.write_batch(&[WriteBatchRecord::Put(b"b", b"3")]).unwrap();

    // Included("b") to Included("c") -> returns ["b" (3), "c" (1)]
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Included(b"b"), Bound::Included(b"c")).unwrap(),
        vec![(Bytes::from("b"), Bytes::from("3")), (Bytes::from("c"), Bytes::from("1"))],
    );

    // Excluded("b") to Unbounded -> MUST skip all versions of "b", returning ["c"]
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Excluded(b"b"), Bound::Unbounded).unwrap(),
        vec![(Bytes::from("c"), Bytes::from("1"))],
    );

    // Unbounded to Excluded("b") -> MUST stop before "b", returning ["a"]
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Excluded(b"b")).unwrap(),
        vec![(Bytes::from("a"), Bytes::from("1"))],
    );
}

#[test]
fn test_sst_boundary_preserves_user_key_history() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.target_sst_size = 128; // Small SST size to force splits
    let storage = MiniLsm::open(&dir, options).unwrap();

    // Write enough versions of "heavy_key" to breach target SST size
    for i in 0..50 {
        storage.write_batch(&[WriteBatchRecord::Put(
            b"heavy_key".as_slice(),
            format!("val_{}", i).as_bytes(),
        )]).unwrap();
    }
    storage.write_batch(&[WriteBatchRecord::Put(b"next_key".as_slice(), b"v0".as_slice())]).unwrap();

    let raw_iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    let ssts = storage.inner.build_ssts_from_iter(raw_iter, false).unwrap();

    // Ensure user keys never span across adjacent SST boundaries
    for window in ssts.windows(2) {
        let prev_last_user_key = window[0].last_key().key_ref();
        let next_first_user_key = window[1].first_key().key_ref();

        assert_ne!(
            prev_last_user_key, next_first_user_key,
            "History for key {:?} was split across SST boundaries",
            String::from_utf8_lossy(prev_last_user_key)
        );
    }
}
