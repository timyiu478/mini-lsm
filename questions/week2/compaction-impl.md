# Compaction Implementation

## Q.  A full-compaction task captures L0 files [5, 4] and L1 files [1, 2]. While it is writing output SSTs, file 6 is flushed to the front of L0. Which files should remain in L0 after the result is installed? If the newest value of k in file 5 is a tombstone and an older value is in file 1, should either entry appear in the output?

The file 6 should remain in L0 after the result is installed.

The entry of key k should be removed in the output.

## Q.  How does your implementation retain an L0 SST flushed while compaction is in progress?

Taking a snapshot of L0 SST IDs before compaction begins isolates the exact set of SSTs to compact.

Using `snapshot.l0_sstables.retain(|id| !l0_sstables.contains(id))` guarantees set difference: only the SSTs involved in the compaction are removed, while any newly flushed L0 SSTs added by background flush threads during the compaction remain untouched.

## Q.  Can a reader using an older state snapshot finish after the input filenames are unlinked? On Unix-like systems, an open file remains accessible until its final handle is closed.

Yes because the file object of the older state snapshot is opened. 

## Q.  Construct the smallest input in which reversing L0 iterator priority preserves a stale value. Then construct one in which it resurrects a deleted value.

L0: sst 2 { a->2 }, sst 1 { a->1 }

If we reverse the order of L0 iterator, the stale value a->1 will be preserved.

L0: sst 2 { a->"" }, sst 1 { a->1 }

If we reverse the order of L0 iterator, the deleted value a->1 will be resurrects.

## Q.  Why is it safe to discard tombstones during this chapter’s full compaction? Give a counterexample showing why the same rule is unsafe when compacting into a non-bottom level.

Because it covers all older versions of the key so that it ensures the deleted value will not be resurrected.

why the same rule is unsafe when compacting into a non-bottom level:

L0: 3: { a->"" } 
L1: 2: { a->"b" }
L2: 1: { a->"c" }

If we compact L0 and L1 into L1 (with L2 existing below) and discard the tombstone a->"", the new L1 will contain no entry for a. When a user reads a, the engine checks L0 (empty), L1 (not found), and then hits L2, resurrecting a->"c"

## Q.  What should apply_compaction_result do when compaction produces no SSTs?

remove the input SSTables from the state for saving space.

## Q.  What ordering and non-overlap properties must hold before SstConcatIterator is safe to use?

* ordering:
    * keys are sorted in ascending order for each SST
    * last key of SST i < first key of SST i+1
* non-overlap: each SST keys are non-overlap

## Q.  What are the definitions of read/write/space amplifications?

* read amplification: actaul bytes read (from memtable, SSTs) / logical bytes of the read request
* write amplification: actual bytes write (commpaction -> rewrite SSTs) / logical bytes of the write request 
* space amplification: actual bytes stored on disk and memory (duplicated key-value paris) / logical bytes of active live data

## Q.  What are the ways to accurately compute the read/write/space amplifications, and what are the ways to estimate them?

## Q.  Is it correct that a key will take some storage space even if a user requests to delete it?

In an LSM-tree, deletes are logical, non-in-place operations.

Until a compaction process merges all levels containing the key and cleans up the tombstone, the key exists in multiple places: the original key-value version(s) plus the new tombstone record.

## Q.  Because compaction consumes read and write bandwidth, should the engine postpone or pause it during heavy foreground traffic? What new problem could that create? Read SILK: Preventing Latency Spikes in Log-Structured Merge Key-Value Stores.


## Q.  Is it a good idea to use/fill the block cache for compactions? Or is it better to fully bypass the block cache when compaction?


## Q.  Does it make sense to have a struct ConcatIterator<I: StorageIterator> in the system?

A generic ConcatIterator<I> fails because:

Semantically: Most storage streams in LSM (Memtable, L0) overlap and cannot be concatenated.
Mechanically: It breaks lazy iterator creation, forcing expensive I/O to open all underlying iterators at once.

## Q.  Some researchers/engineers propose to offload compaction to a remote server or a serverless lambda function. What are the benefits, and what might be the potential challenges and performance impacts of doing remote compaction? (Think of the point when a compaction completes and what happens to the block cache on the next read request…)

