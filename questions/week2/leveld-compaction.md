# Leveld Compaction

## Q. The selected upper SST covers [100, 200]; the lower level contains [50, 99], [100, 150], [151, 250], and [251, 300]. Which lower SSTs belong in the task? If the task is being replayed from the manifest before SST metadata is loaded, when can the output files be sorted by first key?

The lower SSTs [100, 150] and [151, 250] belong in the task because their key ranges cover [100, 200].

During manifest replay, the system only processes SST IDs to update state.levels and state.l0_sstables without opening the actual .sst files on disk. Because first_key metadata lives inside the SST files, sorting by first_key cannot happen during manifest replay. The exact timing to sort is right after live SST files are opened (SsTable::open) and loaded into state.sstables, but before LsmStorageInner::open() returns:

## Q. Why does L0 compaction take priority over a lower level with a larger size score?

Because L0 SSTs are non-disjoint, point lookups must check every single L0 file in worst-case scenarios (O(N) SSTs).

## Q. Construct an overlap example that fails if endpoint equality is treated as non-overlapping.

Example:

* SST A (Upper Level): Range [100, 200] (contains keys 100 and 200)
* SST B (Lower Level): Range [200, 300] (contains keys 200 and 300)

Why it fails:

1. If last_key(A) == first_key(B) (200 == 200) is treated as non-overlapping, the compaction task leaves SST B out of the merge process.
2. After compacting SST A down to the lower level without SST B, both SST A's output and SST B end up in the same level ($L$).
3. Now, key 200 exists in two different SSTs in the same level, violating the core Leveled Compaction invariant that levels $L \ge 1$ must have strictly disjoint key ranges. Binary search routing for point queries will fail.

## Q. Why must output SSTs be merged with untouched lower-level SSTs and sorted by first key?

1. A compaction task only rewrites a slice of the lower level's key space.
2. Merging new output SSTs and sorting by first_key preserves strict total ordering.

## Q. What information is unavailable while manifest records are being replayed, and what phase of recovery makes it available?

* Unavailable during replay: manifest records log only SST IDs without opening file headers/index blocks.
* They are available after replay and the SSTs are opened.

## Q. If a new L0 file appears while an L0-to-base-level task runs, how does result application retain it?

Taking a snapshot of L0 SST IDs before compaction begins isolates the exact set of SSTs to compact.

Using `snapshot.l0_sstables.retain(|id| !l0_sstables.contains(id))` guarantees set difference: only the SSTs involved in the compaction are removed, while any newly flushed L0 SSTs added by background flush threads during the compaction remain untouched.

## Q. Consider the case that the upper level has two tables of [100, 200], [201, 300] and the lower level has [50, 150], [151, 250], [251, 350]. In this case, do you still want to compact one file in the upper level at a time? Why?

No because it causes the [151, 250] table to rewrite twice.

* If you compact [100, 200] alone, it overlaps lower tables [50, 150] and [151, 250]. Table [151, 250] gets merged and rewritten.
* Next, when [201, 300] is compacted, it overlaps [151, 250] (its new replacement) and [251, 350]. This forces [151, 250] to be rewritten a second time.

## Q. Estimate write amplification under an explicit level multiplier, overlap fraction, and steady-state workload. Which term changes when compaction selects one upper SST instead of an entire level?

## Q. Estimate worst-case point-read amplification with no cache or Bloom-filter benefit. How do overlapping L0 files differ from the one candidate file per lower level?

## Q. Finding a good key split point for compaction may potentially reduce the write amplification, or it does not matter at all? (Consider that case that the user write keys beginning with some prefixes, 00 and 01. The number of keys under these two prefixes are different and their write patterns are different. If we can always split 00 and 01 into different SSTs…)

## Q. Imagine that a user was using tiered (universal) compaction before and wants to migrate to leveled compaction. What might be the challenges of this migration? And how to do the migration?

## Q. And if we do it reversely, what if the user wants to migrate from leveled compaction to tiered compaction?

## Q. What happens if compaction speed cannot keep up with the SST flushes for leveled compaction?

## Q. What must the system consider before scheduling multiple compaction tasks in parallel?

## Q. What is the peak storage usage for leveled compaction? Compared with universal compaction?

## Q. Is it true that with a lower level_size_multiplier, you can always get a lower write amplification?

## Q. What needs to be done if a user not using compaction at all decides to migrate to leveled compaction?

## Q. Some people propose to do intra-L0 compaction (compact L0 tables and still put them in L0) before pushing them to lower layers. What might be the benefits of doing so? (Might be related: PebblesDB SOSP’17)

