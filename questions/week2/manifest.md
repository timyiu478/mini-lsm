# Manifest

## Q. The engine has written and synced a new SST but crashes before appending its flush record. What state should recovery produce, and what kind of file is left behind? Now reverse the unsafe order: the manifest is synced before the new SST’s directory entry. What can recovery attempt to open after a power loss?

The recovery should produce a state that all SSTs with flushed records are recovered. The new SST without a flush record is left behind.

If the manifest is synced before the new SST’s directory entry, a power loss in between the manifest is synced and the new SST’s directory entry can cause the system unable to recover correctly (file not found or corrupted/empty file).

## Q. When do you need to call fsync? Why do you need to fsync the directory?

When to call fsync:

* On newly built SST files: Before appending to the manifest.
* On the Manifest file: Immediately after writing a ManifestRecord so the state change is committed to disk.
* On the directory: Right after creating or deleting SST files.

Why fsync the directory?

* File contents and directory entries (dentries) are managed separately by the operating system. Writing data to an SST file puts the file content on disk, but the file's entry in the directory folder structure might still sit in the OS page cache.
* If a power crash occurs before the directory is synced, the SST file data might exist, but the filesystem will have no record of the filename in the directory, rendering the file unreachable.

## Q. What are the places you will need to write to the manifest?

* Memtable Flush: When an immutable memtable is written to disk as an L0 SST (ManifestRecord::Flush).
* Compaction: When a compaction task completes, creating new merged SSTs and retiring input SSTs (ManifestRecord::Compaction).

## Q. Why must newly created SSTs and their directory entries be synced before the manifest record that references them?

To prevent dangling references upon recovery.

## Q. Why is it safe for an obsolete SST to remain on disk after the compaction record is durable? Is it safe to delete the SST before that point?

Why it's safe to remain on disk: Once the compaction record is flushed and synced to the MANIFEST, the engine's logical state has officially moved forward. On recovery, the engine replays the manifest and constructs its live state without referencing the old SSTs. Leftover obsolete files are just harmless orphaned garbage on disk that can be cleaned up at any time (or during open()).


Why it's NOT safe to delete before: If you delete the old SST file before syncing the compaction manifest record, and the system crashes mid-operation:

* The MANIFEST on disk still points to the old state (expecting the old SST file to be there).
* Upon recovery, the system tries to load the old SST, finds it missing, and panics—resulting in data loss.

## Q. During recovery, why can leveled compaction results not be sorted by first key while manifest records are being replayed?

## Q. Construct a record sequence containing flushes and compactions, replay it by hand, and compute the next unused SST ID.

## Q. Consider an alternative implementation of an LSM engine that does not use a manifest file. Instead, it records the level/tier information in the header of each file, scans the storage directory every time it restarts, and recover the LSM state solely from the files present in the directory. Is it possible to correctly maintain the LSM state in this implementation and what might be the problems/challenges with that?

## Q. Currently, we create all SST/concat iterators before creating the merge iterator, which means that we have to load the first block of the first SST in all levels into memory before starting the scanning process. We have start/end key in the manifest, and is it possible to leverage this information to delay the loading of the data blocks and make the time to return the first key-value pair faster?

## Q. Is it possible not to store the tier/level information in the manifest? i.e., we only store the list of SSTs we have in the manifest without the level information, and rebuild the tier/level using the key range and timestamp information (SST metadata).

