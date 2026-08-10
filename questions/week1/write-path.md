# Write Path

## Q. Suppose imm_memtables contains IDs [7, 6, 5] from newest to oldest and l0_sstables contains [4, 3]. Write both vectors after one correct flush. Which assertions would detect flushing the wrong memtable or installing the wrong SST ID?

* imm_memtables: [7, 6]
* l0_sstables: [5, 4, 3]
* assertions: l0_sstables[0] > l0_sstables[1] if l0_sstables.len() > 1 && imm_memtables[-1] > l0_sstables[0] if imm_memtables.len() > 0

## Q. What happens if a user requests to delete a key twice?

The engine inserts two deletion tombstones for the same key into the memtable (or successive memtables). This cause the system does more work and consumes 1 key-value pair space to store the redundant tombstone until background compaction eventually drops it.

But the semantic result to the user is the same (the key is deleted).

## Q. Why must the state update verify that the memtable removed from imm_memtables has the ID used to build the SST?

It ensures that no other process modified imm_memtables or popped the last element while state locks were released (such as during disk I/O). This prevents future code refactorings from silently breaking the ordering invariant and dropping the wrong memtable.

## Q. Construct an interleaving that would corrupt the state if two flushes selected the same oldest memtable without state_lock.

| Time | Thread A (Flush Task 1) | Thread B (Flush Task 2) | System State Risk |
| --- | --- | --- | --- |
| **1** | Reads state, targets oldest memtable (`ID=1`). |  |  |
| **2** |  | Reads state, targets oldest memtable (`ID=1`). | Both threads are building an SST for `ID=1`. |
| **3** | Writes SST for `ID=1` to disk. | Writes SST for `ID=1` to disk (duplicate work). |  |
| **4** | Obtains write lock, pops oldest memtable (`ID=1`). |  | `imm_memtables` now has `ID=2` as the oldest. |
| **5** |  | Obtains write lock, pops oldest memtable (`ID=2`). | **PANIC** because pop id != id  |

## Q. For each combination of included, excluded, and unbounded scan bounds, state the condition under which an SST range can be safely excluded.

## Q. How much memory, or how many blocks, are loaded at the same time when an iterator is initialized? Measure num_active_iterators during a scan and explain why it changes.

