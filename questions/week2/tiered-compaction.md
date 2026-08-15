# Tiered Compaction

## Q. Suppose the tiers from newest to oldest have sizes [1, 1, 3, 20], and the reduce-sorted-runs trigger is capped at max_merge_width = 2. Which tiers are selected? Does the task include the bottom tier, and may it discard a tombstone whose older value could be in the 20-file tier?

* Tiers 0 and 1 are selected. Because their sizes are 1 which is lower than the max_merge_width constraint.
* NO, it doesnot include the bottom tier.
* No, it CANNOT discard the tombstone. A tombstone can only be safely dropped if the compaction includes the bottom-most (oldest) tier.

## Q. Why must every task select adjacent tiers from the newest end of the state? Construct a counterexample for a task that merges two non-adjacent tiers and places the output incorrectly.

levels: [T2: {a->"", c->"d"}, T1: {a->"b"}, T0: {a->"c"}]

If we merge T0 and T2, 

1. the new tier T3: {c->"d"} is created
2. and the levels come [T3: {c->"d"}, T1: {a->"b"}] (the tombstone of a is cleared)

We can user get(a), the system returns the stale value b instead of not found!

## Q. When max_merge_width limits a task to only the newest tiers, why must bottom_tier_included be false?

Because the absolute oldest tier in the LSM state is not included in this compaction task.

## Q. What should the new LSM state contain if a bottom-tier compaction produces no output because every surviving entry is a tombstone?

1. remove the obsolete SST
2. no new SST is created

## Q. Construct a tier-size sequence for which the space-amplification trigger wins, and another for which the size-ratio trigger wins.

* space-amplification ratio: 4
* size-ratio: 2
* min_merge_width: 3

the space-amplification trigger wins

```
Tier 3: 1
Tier 2: 3
Tier 1: 1 
```

the size-ratio trigger wins:

```
Tier 4: 1
Tier 3: 2 ; size-ratio = 2 / 1 = 2, width = 2
Tier 2: 6 ; size-ratio = 6 / (2+1) = 2, width = 3, space-amplification ratio = (1+2+6) / 3 = 3 // the size-ratio trigger wins
Tier 1: 3 
```

## Q. If a new tier is flushed while a task is running, where should the compacted output be inserted relative to that tier?

The newly flushed tier goes to index 0 (the absolute newest).
The compacted output must be inserted after it in the array (e.g., at index 1), which represents an older position.

## Q. What must the system consider before scheduling multiple compaction tasks in parallel?

The compaction scheduler must **"lock"** or reserve the contiguous slice of tiers being compacted so that a parallel background thread doesn't pick them up simultaneously, leading to a race condition when applying the result to the LSM state.

## Q. What happens if compaction speed cannot keep up with the SST flushes for tiered compaction?

Read Amplification Spikes: The number of active tiers grows uncontrollably. Because tiered compaction requires checking tiers from newest to oldest, get and scan operations will slow down drastically as they iterate through an excessive number of SSTables.

Space Amplification Increases: Overwritten keys and tombstones are not merged out fast enough. Stale data accumulates, rapidly consuming disk space.

Write Stalls/Throttling: To prevent the system from completely collapsing or exceeding OS file descriptor limits, the LSM engine will eventually trigger a "write stall." It will artificially throttle or completely block incoming writes until background compaction tasks can catch up and reduce the tier count.

## Q. Estimate write amplification for one concrete tier-size sequence. Begin by ignoring the final reduce sorted runs trigger, and count physical SST bytes written divided by flushed bytes.

## Q. Estimate worst-case point-read amplification for a state with N tiers and no cache or Bloom-filter benefit. Which policy parameter bounds that value?

## Q. What are the advantages and disadvantages of universal compaction compared with leveled compaction?

## Q. How much temporary free storage does one universal-compaction task require? Answer for a concrete set of input-tier sizes and distinguish peak task space from steady-state space amplification.

## Q. SSDs also write its own logs (basically it is a log-structured storage). If the SSD has a write amplification of 2x, what is the end-to-end write amplification of the whole system? Related: ZNS: Avoiding the Block Interface Tax for Flash-based SSDs.

## Q. Consider the case that the user chooses to keep a large number of sorted runs (i.e., 300) for tiered compaction. To make the read path faster, is it a good idea to keep some data structure that helps reduce the time complexity (i.e., to O(log n)) of finding SSTs to read in each layer for some key ranges? Note that normally, you will need to do a binary search in each sorted run to find the key ranges that you will need to read. (Check out Neon’s layer map implementation!)

