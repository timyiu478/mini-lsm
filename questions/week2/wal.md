# WAL

## Q. The manifest contains NewMemtable(7) but no Flush(7), and 00007.wal contains k -> v. What should recovery construct? If Flush(7) is durable but the old WAL file was not deleted before the crash, should recovery replay it?

It should recover a memtable with id 7 and the memtable should contains k -> v.

The recovery should not replay the old WAL file the manifest contains Flush(7).
Otherwise the system will contain a memtable and a SST with the same ID and same content (Violation of state invariants: having active memtables whose IDs predate or equal flushed SST IDs).

## Q. When should you call fsync in your engine? What happens if you call fsync too often (i.e., on every put key request)?

When should you call fsync in your engine:

* Adding a new record to the manifest (to ensure metadata durability).
* Before the engine or WAL file is closed.
* When flushing a memtable to an SSTable (to ensure data hits disk before updating the manifest).

What happens if you call fsync too often (i.e., on every put key request):

Calling fsync on every write would force a blocking disk I/O operation, completely destroying write throughput. 

## Q. Experiment: Measure batched fsync latency and throughput on your own storage device. Record the operating system, filesystem, device, queue depth, and batch size; why is there no single hardware-independent answer?

* Storage Medium: HDDs rely on mechanical arm movement and rotational latency (spindle speed), whereas NVMe/SSD drives rely on internal flash translation layers (FTL), parallel flash channels, and controller queues.
* Filesystem Behavior: Filesystems (ext4, xfs, zfs) handle barrier flushing, journal commits, and metadata updates differently.
* Operating System & Kernel Stacks: I/O schedulers and system call implementations (fsync, fdatasync, syncfs) introduce variable overhead.

## Q. When can you tell the user that their modifications (put/delete) have been persisted?

An operation is durable only after it has been written to the WAL and the WAL has been explicitly synced to disk via fsync

## Q. Why must a new memtable be recorded in the manifest before a synchronized write to its WAL can be considered recoverable?

If a memtable/WAL is written to disk but never recorded in the manifest, the recovery process will ignore that WAL entirely during startup, rendering the data unrecoverable

## Q. Why should a flushed memtable’s WAL be deleted only after the manifest’s flush record is durable?

The manifest flush record confirms that the system has successfully transitioned the data from the memtable into an immutable SSTable. If you delete the WAL before this manifest update is synced and a crash occurs, the recovery process will look for the old memtable/WAL, find neither, and risk data inconsistency or failure.

## Q. Given WAL IDs 4 and 9 plus live SST IDs 3, 7, and 12, what ID should the next memtable use?

The next memtable should use ID 13.

## Q. How can you handle corrupted data in WAL?

## Q. Is it possible to design an LSM engine without WAL (i.e., use L0 as WAL)? What will be the implications of this design?

