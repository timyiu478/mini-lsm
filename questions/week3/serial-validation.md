# Serializable Validation

## Q. Two transactions begin at timestamp 10. T1 reads b and writes a; T2 reads a and writes b. T1 commits at 11. Which set intersection must abort T2? How would the answer change if T2’s dependency came only from an empty range scan?

T2 read set intersect with T1 write set.

T2 read set will not intersect with T1 write set if T2’s dependency came only from an empty range scan.

## Q. If you have some experience with building a relational database, you may think about the following question: assume that we build a database based on Mini-LSM where we store each row in the relation table as a key-value pair (key: primary key, value: serialized row) and enable serializable verification, does the database system directly gain ANSI serializable isolation level capability? Why or why not?

No. The system only tracks point-key hashes in its read set. It cannot track logical range scans.
Therefore, it fails to prevent Phantom Reads when concurrent transactions insert into a scanned range, which violates ANSI Serializable requirements

## Q. The point-key rule is related to write snapshot isolation (see A critique of snapshot isolation): it aborts on any relevant read-after-snapshot write conflict instead of detecting only cycles. Construct a serializable execution that this conservative rule still rejects.

Execution History:

1. T1: read x
2. T2: write x
3. T2: commit
4. T1: write y
5. T1: commit

This history is serializable because it is equivalent to T1 <- T2.

However, T1 will be aborted because T1 read set intersects with T2 write set.

## Q. Why must commit_lock cover both validation and publication? Construct an interleaving that fails if the lock is released between them.

Time-of-Check to Time-of-Use (TOCTOU) race condition:

1. T1: read x
2. T1: write x
3. T2: read x
4. T2: write x
5. T1: validation (no overlap)
6. T2: validation (no overlap)
7. T1: commit and submit write set
8. T2: commit and submit write set

## Q. Why can two transactions that only write the same key both commit without violating serializability?

Because there is no dependency cycle between two transactions that only write the same key.

## Q. Which committed transaction records are safe to garbage-collect at a given watermark?

committed transaction records that their committed timestamps are strictly below the given watermark are safe to garbage-collect.

## Q. There are databases that claim they have serializable snapshot isolation support by only tracking the keys accessed in gets and scans (instead of key range). Do they really prevent write skews caused by phantoms? (Okay… Actually, I’m talking about BadgerDB.)
