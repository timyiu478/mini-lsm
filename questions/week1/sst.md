# Sorted String Table

## Q. An SST contains block 1 with [a, b, c] and block 2 with [e, f, g]. If block selection uses only first-key metadata, which block is initially selected by seek(d)? What must the iterator do next to satisfy lower-bound seek semantics?

1. The index binary search looks for the last block whose first_key <= "d" which is block 1.
2. The SST iterator creates a BlockIterator for Block 1 [a, b, c] and calls seek_to_key("d"). Since all keys inside block 1 are < "d". The iterator will become invalid.
3. The SST iterator creates a BlockIterator for Blcok 2 [e, f, g] and calls seek_to_key("d"). The iterator will land on "e".

Only using first-key metadata makes the SST iterator enter a block that does not contain the search key, and requires fallback step to advance to the next block.

## Q. Inspect the block-reading path after the tests pass. Explain when a disk read occurs, what remains resident because of the iterator, and what remains reachable through the cache. These lifetimes are related but not identical.

* When does a disk read occur: a cache miss inside the read_block_cached function
* What remains resident because of the iterator: block iterator, Arc<Block>
* What remains reachable through the cache: the block entries (sst id, block idx) managed by the moka cache

## Q. What is the time complexity of seeking a key in the SST?

O(logn) where n is number of block.

Because the blocks are stored in sorted order which the binary search algorithm can be used to seek the target block of the key.

## Q. Where does the cursor stop when you seek a non-existent key in your implementation?

It will stop to the block that its first key < non-existent key and this first key is larger than all other first keys that smaller than non-existent key.

## Q. Is it possible (or necessary) to do in-place updates of SST files?

No. SST files are immutable.

It is not necessary to do in-place updates of SST files. Only the read path needs SST files.

## Q. An SST is usually large—for example, 256 MB—so repeatedly copying or growing its Vec can be expensive. Does your implementation reserve enough space for the SST builder in advance? How?

Our SST builder DOES NOT pre-allocate the block_size memory.

But the size of the data is bounded by the block_size.


## Q. Looking at the moka block cache, why does it return Arc<Error> instead of the original Error?

Because the method guarantees that concurrent calls on the same not-existing key are coalesced into one evaluation of the init closure (as long as these closures return the same error type). Only one of the calls evaluates its closure, and other calls wait for that closure to complete.

However, the error type usually does not implement the clone. So, Moka wraps the error inside an Arc. This allows Moka to cheaply share and distribute a single ownership reference of the failure across all the threads that were waiting on that lookup.

https://docs.rs/moka/latest/moka/sync/struct.Cache.html#method.try_get_with

## Q. Does using a block cache guarantee that at most a fixed number of blocks exist in memory? For example, with a 4 GB moka cache and 4 KiB blocks, can more than 4 GB / 4 KiB blocks be alive at once? Account for references held outside the cache.

## Q. Can an LSM engine store columnar data, such as a table with 100 integer columns? Would the current SST format still be a good choice?

No, the row-oriented SST format(1 key, 1 row) would not be a good choice.

If a query only needs to read or update a 1 column out of 100, the enginee is still need to read/write entire row, 100 columns from/to the disk, causing read/write amplification.

## Q. Suppose the LSM engine uses an object-storage service such as S3. How would you adapt the SST format, its parameters, and the block cache to suit that environment?

## Q. For now, we load the metadata for every SST into memory. If 16 GB is reserved for this metadata, estimate the maximum database size under explicit assumptions for average key length, block size, metadata bytes per block, and SST utilization. Which assumption dominates? This limitation motivates an index cache.

