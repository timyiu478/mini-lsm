## Q. Merge iter1 = [b->delete, c->4] with iter2 = [a->1, b->2, c->3], where iter1 is newer. Write the internal merged stream first, including tombstones, and then the user-visible stream. What breaks if tombstones are removed before duplicate versions are resolved?

If we prematurely filter out tombstones from iter1 before merging/deduplicating with older iterators, here is what goes wrong:

1. strip b -> delete from iter1, leaving iter1 = [c -> 4].
2. now merge iter1 = [c -> 4] with iter2 = [a -> 1, b -> 2, c -> 3].
3. Because b -> delete is gone, the merge iterator sees b -> 2 in iter2 and add it. 
4. Result: The merged output becomes [a -> 1, b -> 2, c -> 4].

## Q. If a key is removed (there is a delete tombstone), do you need to return it to the user? Where did you handle this logic?

No. The lsm iterator will skip the removed key.

```rust
fn next(&mut self) -> Result<()> {
    self.inner.next()?;
    self.move_to_non_delete()
}

fn move_to_non_delete(&mut self) -> Result<()> {
    while self.inner.is_valid() && self.inner.value().is_empty() {
        self.inner.next()?;
    }
    Ok(())
}
```

## Q. If a key has multiple versions, will the user see all of them? Where did you handle this logic?

No, the user will only see the latest valid version of the key. This is handled in two stage:

1. Merge Iterator's Next function: it deduplicates keys by draining the older versions from the sibing iterators.
1. LSM Iterator's Next function: it filters out the tombstones so that the deleted key is invisible to user.


## Q. What happens if your key comparator cannot give the binary heap implementation a stable order?

It will break the invariants of the binary heap that the top of the heap is the smallest latest version of the key and can cause the duplication logic fails.

## Q. Why must the merge iterator resolve duplicate keys according to iterator construction order?

Because the iterator construction order represents the order of the data from latest to oldest (active memtable -> immutable memtable -> SST).
Since each individual iterator emits keys in strictly sorted order, an iterator will never yield the same key twice.
Thus, the tie-breaking duplicate keys using the iterator construction order ensures the latest version of the key will always precedence over the older versions.

## Q. Construct a minimal input that produces a duplicate key if MergeIterator::next advances only the currently visible child and not every child positioned at that key.

Minimal Input

```
current: Iter 1: a -> b
heap: Iter 2: a -> c, Iter 3: a -> d
```

Execution Trace when calling next():

1. Peeks Iter 2 and finds a key match (a == a) $\rightarrow$ advances Iter 2 to key c.
2. Advances Iter 1 (current) to key b.Pushes Iter 1 back into the heap.
3. Pops the top iterator from the heap $\rightarrow$
4. Iter 3 is popped because it is still sitting at key a (BUG!).

Final Heap & Current State:

```
current: Iter 3: a -> d
heap: Iter 1: b, Iter 2: c
```

## Q. Why do we need a self-referential struct for the memtable iterator?

In Rust, an iterator borrowing from a SkipMap requires a lifetime parameter (e.g., SkipMapIter<'a>). If the iterator wants to own the underlying Arc<SkipMap> to ensure **the memtable isn't dropped while iterating**, it creates a self-referential structure.

## Q. If we replace the self-referential struct with a lifetime on the memtable iterator—for example, MemTableIterator<'a>, where 'a is tied to a memtable or LsmStorageInner—can we still implement scan?

 
## Q. Could you implement a Rust-style iterator—for example, one with next(&mut self) -> Option<(Key, Value)>—for LSM iterators? What are the advantages and disadvantages?

Yes, it is possible.

Advantages:

* It seamlessly integrates with Rust’s stdlib Iterator trait. Users can use the functional methods such as map, filter, and for (k, v) in iter.
* Cleaner ownership model: returns the owned key value pair (avoids lifetime annotation on the pair).

Disadvantages:

* Heap Comparator & Peeking: A standard BinaryHeap requires inspecting the current key at the top of the heap without consuming or advancing the iterator. With a standard next() model, you can't "peek" at the current key unless every child iterator eagerly fetches and caches its current (Key, Value) pair in memory.
* Loss of Error Propagation: Disk I/O failures, checksum corruptions, or network errors occurring mid-scan would either have to be hidden/swallowed (returning None), cause a panic!.


## Q. The scan interface resembles fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>). How could you make it accept Rust range syntax such as key_a..key_b? If you implement this API, try passing the full range .. and observe what happens.

## Q. The starter code provides the merge iterator interface to store Box<I> instead of I. What might be the reason behind that?

It allows the merge iterator seamlessly combine different iterator types (memtable iterator, sst iterator) in the same heap.

## Q. What are the time and space complexities of building and advancing your merge iterator in terms of the number of input iterators?

Let n to be the number of input iterators and k to be the number of duplicate keys across iterators at the current position.

Building the merge iterator:

* time Complexity: O(n)
    * heaplify the list of iterators is O(n)
* space Complexity: O(n)

Advancing your merge iterator:

* time Complexity: O(klogn)
    * worst case is O(nlogn): all n iterators contain the exact same key at the current position
        * the next() must pop/advance all n iterators to deduplicate them.
        * each heap rebalance takes O(logn), leading to O(nlogn) total work.
* space Complexity: O(1)


## Q. Suppose that (1) you create an iterator over the skiplist memtable and (2) another thread inserts keys into that memtable. Will the iterator see the new keys? Design a small experiment rather than relying only on the type signature.

Yes, the iterator will see the new keys.

See the test_concurrent_memtable_iterator_visibility test in the week1_day2.rs
