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

use std::sync::Arc;

use tempfile::tempdir;

use crate::{
    lsm_storage::{LsmStorageInner, LsmStorageOptions},
    mem_table::MemTable,
};

#[test]
fn test_task1_memtable_get() {
    let memtable = MemTable::create(0);
    memtable.for_testing_put_slice(b"key1", b"value1").unwrap();
    memtable.for_testing_put_slice(b"key2", b"value2").unwrap();
    memtable.for_testing_put_slice(b"key3", b"value3").unwrap();
    assert_eq!(
        &memtable.for_testing_get_slice(b"key1").unwrap()[..],
        b"value1"
    );
    assert_eq!(
        &memtable.for_testing_get_slice(b"key2").unwrap()[..],
        b"value2"
    );
    assert_eq!(
        &memtable.for_testing_get_slice(b"key3").unwrap()[..],
        b"value3"
    );
}

#[test]
fn test_task1_memtable_overwrite() {
    let memtable = MemTable::create(0);
    memtable.for_testing_put_slice(b"key1", b"value1").unwrap();
    memtable.for_testing_put_slice(b"key2", b"value2").unwrap();
    memtable.for_testing_put_slice(b"key3", b"value3").unwrap();
    memtable.for_testing_put_slice(b"key1", b"value11").unwrap();
    memtable.for_testing_put_slice(b"key2", b"value22").unwrap();
    memtable.for_testing_put_slice(b"key3", b"value33").unwrap();
    assert_eq!(
        &memtable.for_testing_get_slice(b"key1").unwrap()[..],
        b"value11"
    );
    assert_eq!(
        &memtable.for_testing_get_slice(b"key2").unwrap()[..],
        b"value22"
    );
    assert_eq!(
        &memtable.for_testing_get_slice(b"key3").unwrap()[..],
        b"value33"
    );
}

#[test]
fn test_task2_storage_integration() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorageInner::open(dir.path(), LsmStorageOptions::default_for_week1_test()).unwrap(),
    );
    assert_eq!(&storage.get(b"0").unwrap(), &None);
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"233");
    assert_eq!(&storage.get(b"2").unwrap().unwrap()[..], b"2333");
    assert_eq!(&storage.get(b"3").unwrap().unwrap()[..], b"23333");
    storage.delete(b"2").unwrap();
    assert!(storage.get(b"2").unwrap().is_none());
    storage.delete(b"0").unwrap(); // should NOT report any error
}

#[test]
fn test_task3_storage_integration() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorageInner::open(dir.path(), LsmStorageOptions::default_for_week1_test()).unwrap(),
    );
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    assert_eq!(storage.state.read().imm_memtables.len(), 1);
    let previous_approximate_size = storage.state.read().imm_memtables[0].approximate_size();
    assert!(previous_approximate_size >= 15);
    storage.put(b"1", b"2333").unwrap();
    storage.put(b"2", b"23333").unwrap();
    storage.put(b"3", b"233333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    assert_eq!(storage.state.read().imm_memtables.len(), 2);
    assert!(
        storage.state.read().imm_memtables[1].approximate_size() == previous_approximate_size,
        "wrong order of memtables?"
    );
    assert!(storage.state.read().imm_memtables[0].approximate_size() > previous_approximate_size);
}

#[test]
fn test_task3_freeze_on_capacity() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week1_test();
    options.target_sst_size = 1024;
    options.num_memtable_limit = 1000;
    let storage = Arc::new(LsmStorageInner::open(dir.path(), options).unwrap());
    for _ in 0..1000 {
        storage.put(b"1", b"2333").unwrap();
    }
    let num_imm_memtables = storage.state.read().imm_memtables.len();
    assert!(num_imm_memtables >= 1, "no memtable frozen?");
    for _ in 0..1000 {
        storage.delete(b"1").unwrap();
    }
    assert!(
        storage.state.read().imm_memtables.len() > num_imm_memtables,
        "no more memtable frozen?"
    );
}

#[test]
fn test_task4_storage_integration() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorageInner::open(dir.path(), LsmStorageOptions::default_for_week1_test()).unwrap(),
    );
    assert_eq!(&storage.get(b"0").unwrap(), &None);
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.delete(b"1").unwrap();
    storage.delete(b"2").unwrap();
    storage.put(b"3", b"2333").unwrap();
    storage.put(b"4", b"23333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"1", b"233333").unwrap();
    storage.put(b"3", b"233333").unwrap();
    assert_eq!(storage.state.read().imm_memtables.len(), 2);
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"233333");
    assert_eq!(&storage.get(b"2").unwrap(), &None);
    assert_eq!(&storage.get(b"3").unwrap().unwrap()[..], b"233333");
    assert_eq!(&storage.get(b"4").unwrap().unwrap()[..], b"23333");
}

#[test]
fn test_task3_concurrent_put_and_freeze() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorageInner::open(dir.path(), LsmStorageOptions::default_for_week1_test()).unwrap(),
    );

    let num_threads = 4;
    let ops_per_thread = 200;

    // 1. Spawn threads to concurrently PUT data
    let mut handles = Vec::new();
    for thread_id in 0..num_threads {
        let storage_clone = storage.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = format!("key_{}_{}", thread_id, i);
                let val = format!("val_{}_{}", thread_id, i);
                // Keep the read lock while writing, preventing writes from
                // landing in a frozen memtable!
                storage_clone.put(key.as_bytes(), val.as_bytes()).unwrap();
            }
        });
        handles.push(handle);
    }

    // 2. Spawn a background thread to concurrently FREEZE memtables
    let storage_freeze = storage.clone();
    let freeze_handle = std::thread::spawn(move || {
        for _ in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let lock = storage_freeze.state_lock.lock();
            let _ = storage_freeze.force_freeze_memtable(&lock);
        }
    });

    // 3. Wait for all threads to finish
    for handle in handles {
        handle.join().unwrap();
    }
    freeze_handle.join().unwrap();

    // 4. Verify no data was lost during the concurrent freezes
    let state = storage.state.read();
    assert!(
        state.imm_memtables.len() >= 1,
        "Expected at least one memtable to be frozen"
    );

    for thread_id in 0..num_threads {
        for i in 0..ops_per_thread {
            let key = format!("key_{}_{}", thread_id, i);
            let expected_val = format!("val_{}_{}", thread_id, i);
            let mut found = false;

            // Search the current active memtable
            if let Some(val) = state.memtable.get(key.as_bytes()) {
                assert_eq!(val.as_ref(), expected_val.as_bytes());
                found = true;
            }

            // Search all frozen immutable memtables
            if !found {
                for imm_memtable in &state.imm_memtables {
                    if let Some(val) = imm_memtable.get(key.as_bytes()) {
                        assert_eq!(val.as_ref(), expected_val.as_bytes());
                        found = true;
                        break;
                    }
                }
            }

            assert!(
                found,
                "Key {} was completely lost during concurrent freeze!",
                key
            );
        }
    }
}
