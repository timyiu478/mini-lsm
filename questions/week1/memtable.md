# Memtable

## Q. Why doesn't the memtable provide a delete API?

In Mini-LSM, a key associated with an empty value represents a deletion.

If we actually remove the key entry from the skip map, the older verions of the key stored in old verions of memtables or SST can become visible in the read query. 

## Q. Does it make sense for the memtable to store all write operations instead of only the latest version of a key? For example, the user puts a->1, a->2, and a->3 into the same memtable.

* No, it does not make sense.
* We only need the latest version of a key. When a->1, a->2, and a->3 are written, any subsequent read for key a only cares about the value 3. Storing a->1 or a->2 inside the same memtable provides zero value to point lookups or range scans.
* Overwriting or random access in memory is fast and safe. Unlike physical disk SST files (which are strictly immutable and append-only), in-memory data structures like concurrent SkipLists allow fast, $O(\log N)$ in-place updates/replaces.

## Q. Is it possible to use other data structures as the memtable in LSM? What are the pros/cons of using the skiplist?

Yes, it is possible. The AVL-tree and B-tree also can be used to implement memtable.

Pros of using the skiplist:

* In-Order Iteration / Easy SST Flush: the key value pairs are ordered by the key. So that we can change it the SST without sorting.
* O(logn) read/write
* Lock-Free Concurrency: no complex rebalancing operation that requires lock a portion of the tree.

Cons of using the skiplist:

* Probabilistic Performance Guarantees: SkipList heights are determined by a random number generator.
* High memory overhead per entry: each skiplist node stores multiple forward pointers.

## Q. Why do we need a combination of state and state_lock? Can we only use state.read() and state.write()?

* The use of state: make sure NO thread that can hold an old LSM-state snapshot write to that now-immutable memtable
* The use of state_lock: coordinate the memtable freezing process to avoid the race between freezers

If we were to rely only on state.write() without state_lock, we would face two bad choices:

1. Hold state.write() during the entire freeze process (Bad for Latency): Creating a new MemTable (and initializing its WAL on disk) involves slow file system I/O. If we do this inside state.write(), all reads (get) and all writes (put/delete) across the entire storage engine are completely blocked for several milliseconds while disk I/O completes.

2. Perform I/O outside the lock, then take state.write() (Races between Freezers): If two threads concurrently notice the active memtable is full and both try to prepare a new MemTable outside state.write() to avoid blocking readers/writers.

* Thread A and Thread B both see approximate_size >= target_sst_size.
* Both threads generate a new SST ID (e.g., ID 5 and ID 6).
* Both threads spend time creating WAL files on disk.
* Both threads race to acquire state.write() and push their new memtable.
* Result: Duplicate concurrent freezes, wasted I/O, skipped or out-of-order memtable IDs, and corrupt imm_memtables history.

## Q. Construct the smallest example in which probing memtables in the wrong order returns a stale value. Then construct one in which it resurrects a deleted value.

Wrong order: probing from oldest to newest instead of newest to oldest.

Scenario 1: Returning a Stale Value

* setup:
    * Key: "k1"
    * Memtable 0 (Oldest / Frozen): Stores ("k1", "v1")
    * Memtable 1 (Newest / Active): Stores ("k1", "v2")
* read k1:
    * Inspects Memtable 0 first.
    * Finds "k1" with value "v1".
    * Immediately returns "v1".

Scenario 2: Resurrects a deleted value

* setup:
    * Key: "k1"
    * Memtable 0 (Oldest / Frozen): Stores ("k1", "v1")
    * Memtable 1 (Newest / Active): Stores ("k1", "")
* read k1:
    * Inspects Memtable 0 first.
    * Finds "k1" with value "v1".
    * Immediately returns "v1".

## Q. After a memtable is frozen, could a thread that still holds an old LSM-state snapshot write to that now-immutable memtable? How does your solution prevent this?

No, a thread cannot write to an immutable memtable after it is frozen.

Our solution prevents this race condition through the state RWLock scoping:

1. Active Writes Hold state.read(): When a thread executes read, put or delete, it acquires a read lock (state.read()) before writing to state.memtable, and holds this lock for the entire duration of the call.

2. Freezing Requires state.write(): To freeze a memtable, force_freeze_memtable must acquire an exclusive write lock (state.write()) on self.state to replace state.memtable with a new one and move the old one to imm_memtables.

3. Mutual Exclusion: Because a write lock cannot be acquired while any read lock is active:

* If a write operation starts first: The freeze thread is blocked until the active write finishes writing to the current mutable memtable.
* If a freeze operation starts first: Any incoming write threads are blocked from acquiring state.read() until the swap finishes. Once unblocked, the write thread reads the newly updated state, obtaining a handle to the new mutable memtable.

Therefore, a thread can never hold a reference to the active memtable while a freeze is actively executing, ensuring writes can never land on a frozen memtable.

## Q. In several places, you might acquire a state read lock, release it, and then acquire a write lock. The two operations may occur in different functions that call one another. How does this differ from directly upgrading a read lock to a write lock? Is an upgrade necessary, and what does it cost?

Related Code:

```rust
pub fn put(&self, _key: &[u8], _value: &[u8]) -> Result<()> {
    let state = self.state.read();

    state.memtable.put(_key, _value)?;

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

    Ok(())
}
```

Lock upgrade vs Lock reacquisition:

* Lock reacquisition: other threads can acquire the lock in the gap of release the state read lock and then acquire the state write lock. This can cause multiple threads both see approximate_size >= target_sst_size can and prepare the new memtable.
* Lock upgrade: elimate the gap of lock reacquisition

The upgrade is unnecessary because we can use the state_lock mutex lock to ensure only one thread see approximate_size >= target_sst_size.

The cost of lock upgrade is deadlock:

1. Thread A and Thread B holds read lock
2. Thread A waits for thread B for upgrading to write lock
3. Thread B waits for thread A for upgrading to write lock

## Q. Documentation check: Read parking_lot’s RwLock fairness section. What might happen to readers waiting to acquire the lock when a writer is already waiting for the current readers to release it? How does eventual fairness differ from strict first-in, first-out service?



## Q. Is the memtable’s memory layout efficient? Does it have good data locality? Consider how Bytes is implemented and stored in the skiplist. How could you optimize the memtable’s layout?


# Reference

https://skyzh.github.io/mini-lsm/week1-01-memtable.html
