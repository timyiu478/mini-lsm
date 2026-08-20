# Snapshot Read - Memtables and Timestamps

## Q. A batch writes a and b at timestamp 7, while the memtable already contains a@6. What is their internal order? For the user range (a, b], which timestamp sentinels exclude every version of a while including every version of b?

Internal order: a@7, a@6, b@7

Timestamp sentinels:

* lower bound: Excluded(a@0)
* upper bound: Included(b@0)

## Q. What is the difference of get in the MVCC engine and the engine you built in week 2?

In Week 2, memtable get was a hash map/skiplist point lookup on UserKey.

In MVCC, keys are tuple-encoded as (UserKey, Timestamp) with timestamps sorted descending.
You can no longer do an exact match lookup; you must seek/scan the range (key, read_ts) down to (key, 0) to locate the newest version where $\text{ts} \le \text{read\_ts}$.


## Q. In week 2, you stop at the first memtable/level where a key is found when get. Can you do the same in the MVCC version?

You can still stop at the first memtable/level where you find a version with $\text{ts} \le \text{read\_ts}$.

Because writes are assigned strictly increasing timestamps, any valid version found in a newer memtable or level is guaranteed to have a higher timestamp than any version of that key stored in older levels. 

## Q. Why must the write lock cover both timestamp selection and batch insertion?

Serialize the commit order. Ensure reader either see all writes or nothing.

Stale Read can happen if without using a write lock to cover both timestamp selection and batch insertion:

```
Thread A (Write item1)           Thread B (Write item2)           Reader Thread
----------------------           ----------------------           -------------
1. Assigns `ts = 10` for
   `item1 = "v10"`

                                 2. Assigns `ts = 11` for
                                    `item2 = "v11"`
                                 3. Inserts `item2` into Memtable
                                 4. PUBLISHES `latest_commit_ts = 11`

                                                                  5. Fetches `read_ts = 11`
                                                                  6. Queries `item1` at `ts <= 11`
                                                                     -> Returns "v0" (STALE!)

7. Inserts `item1 = "v10"`
   into Memtable (Delayed)

                                                                  8. Queries `item1` at `ts <= 11`
                                                                     -> Returns "v10" (NEW!)
```


## Q. Why does an excluded lower bound use the opposite timestamp sentinel from an included lower bound?

Internal keys are ordered by UserKey (ascending) then Timestamp (descending).

Included(key) uses key@MAX_TS: MAX_TS sorts before all other timestamps, making (key, MAX_TS) the very first internal record for key.

Excluded(key) uses key@0: 0 sorts after all valid timestamps, making (key, 0) the very last internal record for key.

Seeking past Excluded((key, 0)) **skips all versions of key entirely** and lands directly on the first version of the next user key.

## Q. How do you convert KeySlice into a temporary KeyBytes lookup key? Which lifetime condition makes the unsafe conversion sound?

KeyBytes contains a bytes::Bytes struct, which requires a 'static reference when initialized without heap allocation (e.g., via Bytes::from_static). Because a lookup KeySlice<'a> has a bounded lifetime 'a, converting it to KeyBytes zero-copy requires using unsafe to artificially extend its lifetime to KeySlice<'static>.

This unsafe lifetime extension is sound because

1. the synthesized KeyBytes object is used strictly within the synchronous execution of get => never escape the get stack frame
1. the underlying memory buffer referenced by KeySlice<'a> remains valid and unmodified for the entire duration of the lookup operation.

## Q. What observable failure occurs if compaction splits two versions of one user key across SSTs in the same level?

The invariant of SSTs in L1+ levels are user key non overlap (sst1.last_user_key < ss2.begin_user_key) will be broken if compaction splits two versions of one user key across SSTs in the same level.

This can cause **stale read** if we use leveled compaction.

1. The Bad Cut (How v2 and v1 get split in L1)

Internal MVCC keys are sorted by UserKey (ascending), then Timestamp (descending).

When generating SSTables for Level 1, the iterator processes keyA:

It yields keyA@v2 (newer) first. The builder puts this into SST 10.

2. The Bug Triggers: An SST size limit forces a file cut immediately after keyA@v2.

It yields keyA@v1 (older) second. The builder puts this into SST 11.

Both files are placed in Level 1:

* SST 10 contains keyA@v2
* SST 11 contains keyA@v1

3. Partial Compaction (Level Inversion)

Leveled compaction moves data one file at a time. To pick a candidate file in Level 1 to compact down to Level 2, the engine selects the file with the smallest ID (the oldest file in the level): SST 10.

The engine merges SST 10 into Level 2.

SST 10 is removed from Level 1, and its newly compacted output is written to Level 2.

SST 11 is untouched and remains behind in Level 1.

The Inverted Hierarchy:

* Level 1 holds keyA@v1 (Older version)
* Level 2 holds keyA@v2 (Newer version)

4. The Stale Read

A reader comes in with read_ts = 2 looking for keyA (expecting v2).

```
Read Request: get("keyA") @ read_ts = 2
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────────┐
│ Level 1: Inspects SST 11                                             │
│ ──> Finds `keyA@v1`                                                  │
│ ──> Checks condition: ts(1) <= read_ts(2)? TRUE                      │
│ ──> EARLY STOPPING TRIGGERED! Returns "v1"                           │
└──────────────────────────────────────────────────────────────────────┘
                                   │
              (Level 2 is NEVER searched!)
                                   │
                                   X
┌──────────────────────────────────────────────────────────────────────┐
│ Level 2: Holds `keyA@v2` (Completely missed)                         │
└──────────────────────────────────────────────────────────────────────┘
```
