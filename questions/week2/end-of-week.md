# End Of Week

## Q. A full-compaction task captures L0 files [5, 4] and bottom-level files [1, 2]. File 6 is flushed while the task writes its output. File 5 contains the newest entry for k, a tombstone; file 1 contains k -> old. What remains in L0 after installation, and does k appear in the compaction output?

L0 remains file 6 after installation.

K does not appear in the compaction output because the bottom-level is included in the compaction and the newest entry for k is a tombstone.

## Q. Order these events for a flush: write the SST, synchronize the SST, synchronize the directory entry, append and synchronize the manifest record. When may an obsolete input file be deleted?

Events Ordering:

1. Write the SST file to disk.
2. Synchronize (sync) the SST file.
3. Synchronize the parent directory entry.
4. Append and synchronize the manifest record.

The obsolete input file will be deleted after the synchronization of the manifest record.

## Q. Two policies report different amplification. What must you define before deciding which policy is better?
