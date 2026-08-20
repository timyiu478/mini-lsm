# Transaction API

## Q. For a@7=del, a@5=v5, a@2=v2, what should reads at timestamps 8, 6, 5, and 1 return? Which iterator transitions are needed to avoid resurrecting a@5 at timestamp 8?

what should reads at timestamps:

* 8: none
* 6: v5
* 5: v5
* 1: none

Which iterator transitions are needed to avoid resurrecting a@5 at timestamp 8:

* After deciding one user key, skip every remaining version of that key before returning the next result.

## Q. So far, we have assumed that our SST files use a monotonically increasing id as the file name. Is it okay to use <level>_<begin_key>_<end_key>_<max_ts>.sst as the SST file name? What might be the potential problems with that?

It is NOT ok.

The potential problems:

* the key may contains `_` so that we cant decode them correctly
* key is arbitrary length so that the SST file name length can exceed the OS file name length limit

## Q. Consider an alternative implementation of transaction/snapshot. In our implementation, we have read_ts in our iterators and transaction context, so that the user can always access a consistent view of one version of the database based on the timestamp. Is it viable to store the current LSM state directly in the transaction context in order to gain a consistent snapshot? (i.e., all SST ids, their level information, and all memtables + ts) What are the pros/cons with that? What if the engine does not have memtables? What if the engine is running on a distributed storage system like S3 object store?

It is viable. 

Pros:

* no need to worry compaction that will remove the necessary data because the transaction essentially "pins" the exact set of SST files and memtables it needs.

Cons:

* If you truly wanted a static snapshot without read_ts, you would have to freeze the active memtable and create a new one for every single read transaction, which would destroy write performance.


## Q. Consider that you are implementing a backup utility of the MVCC Mini-LSM engine. Is it enough to simply copy all SST files out without backing up the LSM state? Why or why not?

No. SST files are just part of the LSM state. We also need to backup the manifest file and memtables/WAL files.

Without the manifest file, we can know what are the live SSTs and memtables.

Without WAL files, we cannot recover the memtables and determine the largest committed timestamp.

## Q. Why does a tombstone selected at read_ts stop the search instead of allowing an older value to become visible?

Because the tombstone selected at the read_ts is the greatest version of the data <= read_ts.

Otherwise, we would resurrect deleted data.

## Q. Which object owns the lifetime of a scan’s read timestamp, and what could compaction reclaim if that object were dropped too early?

The Transaction (or the read guard it holds) owns the registration of the read_ts.

The Watermark is the centralized structure that tracks all these transactions.

If the Transaction is dropped too early, the Watermark removes that read_ts from its active list. This advances the global safe-to-reclaim timestamp, meaning compaction might prematurely delete an old version of a key that your iterator was still planning to read, leading to a missing key error or data corruption in your scan.

## Q. Is the maximum timestamp among current SST entries always a durable history of every timestamp ever allocated? What additional metadata would be needed if timestamps must never be reused after all records at the maximum timestamp are garbage-collected?

