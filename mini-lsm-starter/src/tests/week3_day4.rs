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

use std::time::Duration;

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord},
    mvcc::watermark::Watermark,
};

use super::harness::{
    check_iter_result_by_key, construct_merge_iterator_over_storage, dump_files_in_dir,
};

#[test]
fn test_force_flush_empty_imm_memtable() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options).unwrap();

    storage.inner.force_flush_next_imm_memtable().unwrap();

    let state = storage.inner.state.read();
    assert!(state.imm_memtables.is_empty());
    assert!(state.l0_sstables.is_empty());
}

#[test]
fn test_task3_compaction_keeps_versions_together() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options).unwrap();
    let _txn = storage.new_txn().unwrap();
    for i in 0..=20000 {
        storage
            .put(b"0", format!("{:02000}", i).as_bytes())
            .unwrap();
    }
    std::thread::sleep(Duration::from_secs(1));
    while {
        let snapshot = storage.inner.state.read();
        !snapshot.imm_memtables.is_empty()
    } {
        storage.inner.force_flush_next_imm_memtable().unwrap();
    }
    assert!(storage.inner.state.read().l0_sstables.len() > 1);
    storage.force_full_compaction().unwrap();
    storage.dump_structure();
    dump_files_in_dir(&dir);
    assert!(storage.inner.state.read().l0_sstables.is_empty());
    assert_eq!(storage.inner.state.read().levels.len(), 1);
    assert_eq!(storage.inner.state.read().levels[0].1.len(), 1);

    for i in 0..=100 {
        storage
            .put(b"1", format!("{:02000}", i).as_bytes())
            .unwrap();
    }
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    while {
        let snapshot = storage.inner.state.read();
        !snapshot.imm_memtables.is_empty()
    } {
        storage.inner.force_flush_next_imm_memtable().unwrap();
    }
    storage.force_full_compaction().unwrap();
    storage.dump_structure();
    dump_files_in_dir(&dir);
    assert!(storage.inner.state.read().l0_sstables.is_empty());
    assert_eq!(storage.inner.state.read().levels.len(), 1);
    assert_eq!(storage.inner.state.read().levels[0].1.len(), 2);
}

#[test]
fn test_task1_watermark() {
    let mut watermark = Watermark::new();
    watermark.add_reader(0);
    for i in 1..=1000 {
        watermark.add_reader(i);
        assert_eq!(watermark.watermark(), Some(0));
        assert_eq!(watermark.num_retained_snapshots(), i as usize + 1);
    }
    let mut cnt = 1001;
    for i in 0..500 {
        watermark.remove_reader(i);
        assert_eq!(watermark.watermark(), Some(i + 1));
        cnt -= 1;
        assert_eq!(watermark.num_retained_snapshots(), cnt);
    }
    for i in (501..=1000).rev() {
        watermark.remove_reader(i);
        assert_eq!(watermark.watermark(), Some(500));
        cnt -= 1;
        assert_eq!(watermark.num_retained_snapshots(), cnt);
    }
    watermark.remove_reader(500);
    assert_eq!(watermark.watermark(), None);
    assert_eq!(watermark.num_retained_snapshots(), 0);
    watermark.add_reader(2000);
    watermark.add_reader(2000);
    watermark.add_reader(2001);
    assert_eq!(watermark.num_retained_snapshots(), 2);
    assert_eq!(watermark.watermark(), Some(2000));
    watermark.remove_reader(2000);
    assert_eq!(watermark.num_retained_snapshots(), 2);
    assert_eq!(watermark.watermark(), Some(2000));
    watermark.remove_reader(2000);
    assert_eq!(watermark.num_retained_snapshots(), 1);
    assert_eq!(watermark.watermark(), Some(2001));
}

#[test]
fn test_task2_snapshot_watermark() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();
    storage.put(b"233", b"23333").unwrap();
    let txn3 = storage.new_txn().unwrap();
    assert_eq!(storage.inner.mvcc().watermark(), txn1.read_ts);
    drop(txn1);
    assert_eq!(storage.inner.mvcc().watermark(), txn2.read_ts);
    drop(txn2);
    assert_eq!(storage.inner.mvcc().watermark(), txn3.read_ts);
    drop(txn3);
    assert_eq!(
        storage.inner.mvcc().watermark(),
        storage.inner.mvcc().latest_commit_ts()
    );
}

#[test]
fn test_task3_mvcc_compaction() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let snapshot0 = storage.new_txn().unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"1"),
            WriteBatchRecord::Put(b"b", b"1"),
        ])
        .unwrap();
    let snapshot1 = storage.new_txn().unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"2"),
            WriteBatchRecord::Put(b"d", b"2"),
        ])
        .unwrap();
    let snapshot2 = storage.new_txn().unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"3"),
            WriteBatchRecord::Del(b"d"),
        ])
        .unwrap();
    let snapshot3 = storage.new_txn().unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"c", b"4"),
            WriteBatchRecord::Del(b"a"),
        ])
        .unwrap();

    storage.force_flush().unwrap();
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
            (Bytes::from("d"), Bytes::new()),
            (Bytes::from("d"), Bytes::from("2")),
        ],
    );

    drop(snapshot0);
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
            (Bytes::from("d"), Bytes::new()),
            (Bytes::from("d"), Bytes::from("2")),
        ],
    );

    drop(snapshot1);
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
            (Bytes::from("d"), Bytes::new()),
            (Bytes::from("d"), Bytes::from("2")),
        ],
    );

    drop(snapshot2);
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
        ],
    );

    drop(snapshot3);
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
        ],
    );
}

#[test]
fn test_duplicate_oldest_timestamp_drops() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options).unwrap();

    // Create two transactions at the same timestamp (no writes in between)
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();

    assert_eq!(
        txn1.read_ts, txn2.read_ts,
        "Transactions should share the same read_ts"
    );
    let shared_ts = txn1.read_ts;

    assert_eq!(storage.inner.mvcc().watermark(), shared_ts);

    // Drop only one transaction. The watermark must NOT advance.
    drop(txn1);
    assert_eq!(
        storage.inner.mvcc().watermark(),
        shared_ts,
        "Watermark must not advance while txn2 is still alive"
    );

    // Drop the second transaction. The watermark should now advance.
    drop(txn2);
    assert_eq!(
        storage.inner.mvcc().watermark(),
        storage.inner.mvcc().latest_commit_ts(),
        "Watermark should advance to latest commit ts after all readers drop"
    );
}

#[test]
fn test_successive_watermarks_and_user_reads() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options).unwrap();

    // Write batch 1 (Commits at TS 1)
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"1"),
            WriteBatchRecord::Put(b"b", b"1"),
        ])
        .unwrap();
    let txn_ts1 = storage.new_txn().unwrap(); // Captures read_ts = 1

    // Write batch 2 (Commits at TS 2)
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"2"),
            WriteBatchRecord::Put(b"d", b"2"),
        ])
        .unwrap();
    let txn_ts2 = storage.new_txn().unwrap(); // Captures read_ts = 2

    // Write batch 3 (Commits at TS 3)
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"3"),
            WriteBatchRecord::Del(b"d"),
        ])
        .unwrap();
    let txn_ts3 = storage.new_txn().unwrap(); // Captures read_ts = 3

    // Write batch 4 (Commits at TS 4)
    storage
        .write_batch(&[
            WriteBatchRecord::Del(b"a"),
            WriteBatchRecord::Put(b"c", b"4"),
        ])
        .unwrap();
    let txn_ts4 = storage.new_txn().unwrap(); // Captures read_ts = 4

    storage.force_flush().unwrap();

    // Watermark is currently 1 (txn_ts1).
    storage.force_full_compaction().unwrap();
    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
            (Bytes::from("c"), Bytes::from("4")),
            (Bytes::from("d"), Bytes::new()),
            (Bytes::from("d"), Bytes::from("2")),
        ],
    );

    // Advance watermark to 3 (Drop txn1, txn2)
    drop(txn_ts1);
    drop(txn_ts2);
    storage.force_full_compaction().unwrap();

    // Verify User-Visible Reads (Unchanged for txn3 and txn4)
    assert_eq!(txn_ts3.get(b"a").unwrap(), Some(Bytes::from("3")));
    assert_eq!(txn_ts3.get(b"b").unwrap(), Some(Bytes::from("1")));
    assert_eq!(txn_ts4.get(b"a").unwrap(), None); // Tombstoned
    assert_eq!(txn_ts4.get(b"c").unwrap(), Some(Bytes::from("4")));

    // Verify Internal State at Watermark 3
    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::new()),     // a@4=del (kept, > watermark)
            (Bytes::from("a"), Bytes::from("3")), // a@3=3   (kept, latest <= watermark)
            (Bytes::from("b"), Bytes::from("1")), // b@1=1   (kept, latest <= watermark)
            (Bytes::from("c"), Bytes::from("4")), // c@4=4   (kept, > watermark)
                                                  // d is entirely dropped because d@3 is a tombstone <= watermark at the bottom level
        ],
    );
}
