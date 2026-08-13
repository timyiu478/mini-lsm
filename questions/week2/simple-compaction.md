# Simple Compaction

## Q. With size_ratio_percent = 200, suppose L1 has two files, L2 has three, and L3 has eight. Which adjacent pair should be compacted next? If a new L0 file is flushed while that task runs, should applying the result change L0?

The pair L1 and L2 should be compacted next because 3(# of files in L2)/2(# of files in L1) < 2  and 8/3 > 2.

No result change on L0 if a new L0 file is flushed while that task runs because the compaction task does not involve L0. It involves L1 and L2 only.

## Q. For a duplicate key present in both levels of a task, why must the upper-level iterator win? What stale result appears if the priority is reversed?

* Why upper-level wins: In an LSM tree, lower level numbers represent chronologically newer data (e.g., $L_1$ is newer than $L_2$). When compacting $L_k$ into $L_{k+1}$, $L_k$ is the upper level and contains the more recent modifications.
* Stale result if reversed: If $L_{k+1}$ (lower level) takes priority, the system will keep the outdated value of an overwritten key. Even worse, if $L_k$ contains a tombstone (deletion) and $L_{k+1}$ contains an older value, reversing priority will ignore the tombstone and resurrect a deleted key.

## Q. Why may a bottom-level compaction discard a tombstone while an L1-to-L2 compaction might need to retain it?

* L1-to-L2 Compaction: Older versions of the key might still exist in lower levels (e.g., $L_3, L_4$). If you drop the tombstone during an $L_1 \rightarrow L_2$ compaction, those older values in $L_3+$ will be exposed and resurrected on subsequent reads.
* Bottom-Level Compaction: By definition, no data exists below the bottom-most level. Since there are no older versions left in the entire LSM tree for the tombstone to hide, the tombstone has fulfilled its purpose and can be safely garbage-collected (purged).

## Q. What state must be rechecked when a background compaction finishes, and which concurrent change is expected rather than an error?

* Expected concurrent change: Flushes to $L_0$ (l0_sstables). While background compaction runs on lower levels, the active memtable may freeze and flush new SSTables to $L_0$. The compaction thread must merge its results into LsmStorageState without overwriting or dropping these newly prepended $L_0$ SSTables.

## Q. Can you merge L1 and L3 directly if there are SST files in L2? Does it still produce the correct result?

No, it does not produce the correct result. Skipping $L_2$ breaks the fundamental LSM read hierarchy ($L_1 \rightarrow L_2 \rightarrow L_3$).

Failure Scenario:

1. Suppose Key $K$ exists in three levels:$L_1$: $K = V_3$ (Newest)$L_2$: $K = V_2$ (Middle)$L_3$: $K = V_1$ (Oldest)
2. If you compact $L_1$ directly into $L_3$, $V_3$ merges with $V_1$ in $L_3$, resulting in $K = V_3$ residing in $L_3$.
3. Now the levels hold: $L_2$ has $V_2$, and $L_3$ has $V_3$.
4. When a user queries Key $K$, the read path searches $L_1$ (miss) $\rightarrow$ $L_2$ (finds $V_2$) and returns $V_2$.
5. The engine returns the stale value $V_2$ instead of $V_3$ because $L_2$ shadows $L_3$.

## Q. Is it correct that a key will only be purged from the LSM tree if the user requests to delete it and it has been compacted in the bottom-most level?

No, this statement is inaccurate. There are two reasons a key/version can be purged:

1. Deletions (Tombstones): A tombstone itself is indeed only dropped (purged) when compacted at the bottom-most level.
2. Overwrites (Superceded Versions): If a user updates an existing key ($K = V_1 \rightarrow K = V_2$), the older version $V_1$ is purged immediately during any compaction level where both versions meet, regardless of whether it is the bottom-most level.

## Q. Construct a level-size configuration that causes an implementation with reversed ratio operands to select the wrong task.

## Q. Estimate write amplification for a steady-state overwrite workload. State the level count, size ratio, and how much of each lower level a task rewrites; explain why the chapter’s full-level policy differs from partial leveled compaction.

## Q. Estimate worst-case point-read amplification with no cache or Bloom-filter benefit. State how you count L0 and each non-empty sorted run.

## Q. Is it a good strategy to periodically do a full compaction on the LSM tree? Why or why not?

## Q. Actively choosing some old files/levels to compact even if they do not violate the level amplifier would be a good choice, is it true? (Look at the Lethe paper!)

## Q. If the storage device can achieve a sustainable 1GB/s write throughput and the write amplification of the LSM tree is 10x, how much throughput can the user get from the LSM key-value interfaces?

## Q. So far, SST filenames have used monotonically increasing IDs. What problems might arise from naming a file <level>_<begin_key>_<end_key>.sst instead? Revisit this question in Week 3.

