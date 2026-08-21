# Watermark and Gargbage Collection

## Q. Readers exist at timestamps 3, 3, and 7. Which drops advance the watermark? For k@8=v8, k@5=del, k@2=v2 with watermark 5, which versions survive a non-bottom compaction and a bottom-level compaction?

Two readers exist at timestamps 3 drop will advance the watermark.

versions survive a non-bottom compaction: k@8=v8, k@5=del

versions survive a bottom-level compaction: k@8=v8

## Q. Why do we need to store an Arc of Transaction inside a transaction iterator?

The transaction iterator is a two merge iterator of transaction local iterator and LSM iterator.

The transaction local iterator has a reference of the Transaction's Skipmap.

The Transaction owns the watermark registration. If the transaction handle were dropped while the iterator was still scanning, the watermark would advance prematurely, allowing concurrent compaction to garbage-collect data out from under the active LSM iterator.

To prevent the transaction local iterator outlive the skipmap and premature watermark advancement, we store an Arc of Transaction inside a transaction iterator.

## Q. What is the condition to fully remove a key from the SST file?

1. The latest version of the key is a tombstone and its timestamp <= watermark
2. A bottom-level compaction is triggered.

## Q. For now, we only remove a key when compacting to the bottom-most level. Is there any other prior time that we can remove the key? (Hint: you know the start/end key of each SST in all levels.)

By comparing the start/end key of each SST in all levels, we can conclude which levels contain the key.

If a compaction task covers all levels whose SST key ranges overlap with that key, no lower levels outside the task can possibly hold older versions. The tombstone can be safely dropped early without waiting for a full bottom-level compaction.

## Q. Consider the case that the user creates a long-running transaction and we could not garbage collect anything. The user keeps updating a single key. Eventually, there could be a key with thousands of versions in a single SST file. How would it affect performance, and how would you deal with it?

How would it affect performance:

* unnecessary I/O write amplification during compaction
* skipping thousands of stale key versions in memory => scan latency spike

how would you deal with it:

* set a time out of a transaction to prevent long-running transaction

## Q. Why must compaction keep one version at or below the watermark instead of deleting every version below it?

Because it is possible that there is no newer version of the key above the watermark.

If a key hasn't been updated recently, its latest valid value exists at or below the watermark. Deleting all versions at or below the watermark would erase the key entirely, causing data loss for future reads.

## Q. What race appears if a transaction reads the latest timestamp before registering itself with the watermark?

The exact race condition with compaction happens in this sequence:

1. Transaction Reads read_ts: A new transaction fetches read_ts = 10 from the commit TS counter.
2. Compaction Runs: Before the transaction registers 10 with the watermark tracker, background compaction checks the watermark. It sees the lowest registered reader is 20 (or none).
3. Compaction Garbage Collects: Compaction assumes no reader will ever read at ts <= 10 and purges old versions or tombstones at ts <= 10.
4. Late Registration: The transaction finally registers read_ts = 10 into the watermark structure.
5. Data Corruption: When the transaction attempts to read, the data versions it expects at read_ts = 10 have already been destroyed by compaction.

To prevent this race, timestamp allocation and watermark registration must happen atomically or register before reading the timestamp.

```rust
pub fn new_txn(&self, inner: Arc<LsmStorageInner>, serializable: bool) -> Arc<Transaction> {
    // let mut ts = self.ts.lock();
    let read_ts = ts.0;
    // race!! compaction runs
    ts.1.add_reader(read_ts);
    Arc::new(Transaction {
        inner,
        read_ts,
        local_storage: Arc::new(SkipMap::new()),
        committed: Arc::new(AtomicBool::new(false)),
        key_hashes: if serializable {
            Some(Mutex::new((HashSet::new(), HashSet::new())))
        } else {
            None
        },
    })
}
```

## Q. In our implementation, we manage watermarks by ourselves with the lifecycle of Transaction (so-called un-managed mode). If the user intends to manage key timestamps and the watermarks by themselves (i.e., when they have their own timestamp generator), what do you need to do in the write_batch/get/scan API to validate their requests? Is there any architectural assumption we had that might be hard to maintain in this case?
