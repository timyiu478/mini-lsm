# SST Optimization

## Q. A Bloom filter returns “may contain” for a key that is absent from an SST. What extra work occurs, and why is the final result still correct? Now consider a filter that incorrectly returns “definitely absent” for a present key. Which storage-engine guarantee is violated?

* Extra work: Point lookup / Block I/O read.
    * The engine must perform unnecessary disk/cache I/O to read and parse the SSTable's index block and data blocks to search for the key, only to discover it isn't there (a false positive).
* Why is the final result still correct: Bloom filters are a pure performance optimization.
    * A Bloom filter never produces false negatives. Saying "may contain" simply triggers the standard reading path (searching the SSTable itself), which performs an exact key lookup. Since the SSTable is **the source of truth**, reading it directly confirms the key is absent or not.
* Which storage-engine guarantee is violated: `get(key)` returns the most recently written valid value.

## Q. How does a Bloom filter help filter SSTs? Which claims can it make about a key: may not exist, may exist, must exist, or must not exist?

If a Bloom filter indicates that a key does not exist, it is guaranteed not to be in the SSTable. This allows the storage engine to completely skip reading unnecessary SSTs from disk.

Claims can be make about a key:

* may exist
* must not exist

## Q. Can Bloom filters help with scans?

No. Bloom filters are designed for point lookups (checking if a single specific key exists). During a range scan, the engine iterates over a sequence of keys sequentially, so Bloom filters provide no utility for predicting or skipping ranges.

## Q. What are the advantages and disadvantages of prefix-encoding each key relative to the previous key rather than the first key in the block?

Advantages:

* Minimizes disk and memory space by storing only the shared prefix length and the differing suffix.

Disadvantages:

* Increases CPU computation overhead because decoding a key requires sequentially processing preceding keys in the block.

## Q. Why must the first key in a block have an overlap length of zero? What malformed or circular representation could result otherwise?

The first key has no preceding key in the block to share a prefix with. If its overlap length were non-zero, it would incorrectly reference a non-existent base, leading to data loss.

## Q. Compare the encoded sizes of keys that share a long prefix, keys that share no prefix, and one key larger than the target block size. When does prefix encoding provide little or no benefit?

* Keys sharing a long prefix: Highly efficient; significantly reduces storage size.
* Keys sharing no prefix: Provides no benefit; in fact, it can slightly increase the overall size due to the storage overhead required for metadata (overlap length and suffix length indicators).

## Q. If we need a backward iterator, how does this key compression affect it?
