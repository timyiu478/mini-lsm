# Transaction Workplace and Atomic Commit

## Q. The snapshot contains a=old,b=old; the local workspace contains a=new,b=del,c=new. What should an unbounded scan return? If the process stops after writing only part of the transaction’s WAL record, how many of those updates may recovery expose?

* What should an unbounded scan return: a=new, b=none, c=new
* how many of those updates may recovery expose: 0

## Q. Compare this chapter’s guarantees with classic snapshot isolation as defined in the Week 3 overview. Which first-committer-wins rule is not enforced if two concurrent transactions blindly write the same key? Does allowing both commits necessarily make that particular history non-serializable?

The first-committer-wins rule is not enforced: the write set of the committing transaction does not overlap with the "concurrent" committed transactions (between read_ts and commit_ts)

Allowing both commits DOES NOT necessarily make that particular history non-serializable. Allowing concurrent blind writes to succeed bypasses SI's rule, but it does not violate serializability.

## Q. What if the user wants to batch import data (i.e., 1TB?) If they use the transaction API to do that, will you give them some advice? Is there any opportunity to optimize for this case?

Problem:

* cause out-of-memory error because the engine has to store 1TB data in the skipmap
* it will block other transactions commit

Avdice: Chunk the import into smaller transaction batches (e.g., 32MB–64MB per commit).

Optimization: use SST injection offline

* https://rocksdb.org/blog/2017/02/17/bulkoad-ingest-sst-file.html

## Q. Why do a shared commit timestamp and delayed publication provide atomic visibility, while a validated WAL frame provides crash atomicity? Where does sync establish durability?

Readers only observe mutations where commit_ts <= read_ts, delayed commit_ts publication after the writes can keep uncommitted or in-flight writes hidden.

On startup, recovery parsers reject incomplete or corrupt frames, guaranteeing all-or-nothing replay.

Where does sync establish durability: sync() issues an fsync system call to flush kernel page caches to non-volatile physical disk.

## Q. Why must WAL append happen before memtable publication? What should the caller observe if either step fails?

Why WAL First: Publishing to memory before logging to disk lets readers observe unpersisted updates that could be lost in a crash.

Failure States:

* WAL fails: Engine returns Err, memory remains untouched, transaction is safely discarded.
* MemTable fails after WAL succeeds: Engine returns Err, but on restart, recovery replays the complete WAL frame—committing a transaction the caller was told failed. (Engine insertions after WAL write must be infallible).

## Q. When can the engine safely check the memtable size and freeze it without splitting the transaction?

Immediately after the complete batch is written to the active MemTable and commit_ts is published, before releasing the write lock.

This prevents a single atomic transaction batch from being fragmented across multiple MemTables.

If you freeze a MemTable in the middle of a transaction, you are also forced to rotate the WAL in the middle of that transaction. This splits the transaction across two WAL files, leading directly to the threat of partial recovery:

* The Torn Transaction: If the database crashes, WAL 1 might be safely synced to disk, but WAL 2 might be lost in the OS cache or corrupted.
* Crash Atomicity Failure: On restart, the engine will successfully replay WAL 1, committing the first half of the transaction. Because WAL 2 is missing, the second half is lost.

This violates the "all-or-nothing" guarantee of transactions.

## Q. Should an empty transaction allocate a commit timestamp? What are the tradeoffs? This checkpoint follows the existing batch-write behavior.

Recommendation: Assign only a read_ts. Do not allocate a commit_ts.

Tradeoffs: Assigning a commit_ts to an empty transaction wastes sequence numbers, triggers unnecessary lock contention on the global timestamp generator, and stalls concurrent writers.

## Q. Day 6 uses optimistic concurrency control: it checks for conflicts at commit instead of preventing them while the transaction runs. What locks or blocking behavior would a pessimistic design add, and how would that change abort rates and concurrency?
