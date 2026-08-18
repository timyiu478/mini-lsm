# MVCC: End of the week

## Q. The internal stream contains k@9=delete, k@7=v7, k@3=v3. What does a read return at timestamps 10, 8, 7, and 2?

## Q. For the same versions and watermark 7, which versions must a non-bottom compaction retain? What may a bottom-level compaction do after the watermark advances to 9?

## Q. One transaction writes a and b. Name what one shared commit timestamp guarantees, what one framed and checksummed WAL batch guarantees, and what sync adds. Do any of these alone prevent write skew?

## Q. T1 and T2 begin at timestamp 10. T1 reads b and writes a; T2 reads a and writes b. T1 commits first. Why should T2 abort? Why can the same key-only scheme miss an insert into an empty scan range?
