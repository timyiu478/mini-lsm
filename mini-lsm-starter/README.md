My implementation of an LSM-tree storage engine in Rust ^_^

Implementation Tips:

* If you implement [Key Prefix Encoding + Decoding](https://skyzh.github.io/mini-lsm/week1-07-sst-optimizations.html#task-3-key-prefix-encoding--decoding), you may also need to update your block decoding validation logic in block.rs.
* When implementing Tiered Compaction, ensure your `LsmStorageInner::open()` recovery logic does not push SSTables into state.l0_sstables.
