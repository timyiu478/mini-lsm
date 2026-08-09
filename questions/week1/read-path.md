# Read Path

## Q. The mutable memtable contains b -> delete and d -> 4; an immutable memtable contains a -> 1 and b -> 2; the newest L0 SST contains a -> 0, c -> 3, and d -> 3. What should get(a), get(b), and a scan with both a and d included return? For each result, identify the source that wins.

* get(a) returns 1. The immutable memtable wins.
* get(b) returns not found. The mmutable memtable wins.
* a scan with both a and d included returns a->1, c->3, d->4.
    * The source of a is the immutable memtable, the source of c is the L0 SST, and the source of c is the mmutable memtable.

## Q. Construct the smallest state in which continuing to search after finding a tombstone resurrects a deleted key.

* active memtable: a->""
* immmutable memtable 0: a->"1"

## Q. A seek for b lands on c. Which explicit comparison prevents get(b) from returning c’s value?

The key comparison prevents `sst_merge_iter.key() == KeySlice::from_slice(_key)` get(b) from returning c’s value

* Related code: https://github.com/timyiu478/mini-lsm/blob/read-path/mini-lsm-starter/src/lsm_storage.rs#L335

Why a seek for b lands on c? Because b does not exist.

## Q. Where are included and excluded upper bounds enforced? Write a boundary test that would fail if the implementation used < for both variants.

The included and excluded upper bounds is enfored in Lsm storage's is valid function.

* Related code: https://github.com/timyiu478/mini-lsm/blob/read-path/mini-lsm-starter/src/lsm_iterator.rs#L53-L61

Boundary Test:

* active memtable: a->"1", b->"2"
* scan(Bound::Unbounded, Bound::Included(b"b"))

## Q. Suppose a user creates an iterator over the entire 1 TB storage engine, and the scan takes about an hour. What problems could this cause? We will revisit this question at several points in the course.

* The iterator clones an Arc of the engine's state snapshot (Arc<LsmStorageState>) can cause the following problems => Resource pinning -> Prevent memtables eviction
* Scan 1 TB -> read data blocks sequentially -> block cache pollution (evict the hot cache)

## Q. Some LSM-tree storage engines provide a multi-get, or vectored-get, interface. The caller supplies a list of keys and receives a value for each one; for example, multi_get(vec!["a", "b", "c", "d"]) -> a=1,b=2,c=3,d=4. The simplest implementation performs one get per key. How would you implement multi-get, and what could you optimize? Hint: some work in the get path needs to be performed only once for the entire batch. You can also consider an improved disk-I/O interface designed for multi-get.

